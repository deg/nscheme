//! Tree-walking evaluator with an explicit step-loop.
//!
//! See [`docs/0001-tree-walking-interpreter.md`](../../docs/0001-tree-walking-interpreter.md)
//! for the architectural rationale. In short: this is **not** a
//! recursive `fn eval(expr, env) -> Value`. It is an iterative loop
//! over a `Step` state with a `Vec<Frame>` continuation stack.
//!
//! That shape is what makes tail-call optimization (R7RS §3.5) free —
//! tail positions transition `Step::Eval(tail_expr, env)` *without*
//! pushing a frame — and what makes `call-with-current-continuation`
//! a `frames.clone()` later (T17 / `nscheme-0xn`).
//!
//! ## Tail positions (the discipline this module enforces)
//!
//! - The chosen branch of `if` is in tail position.
//! - The last expression of `begin` (and of a `lambda` body) is in tail
//!   position; earlier expressions are not.
//! - The body expression of a procedure call is in tail position with
//!   respect to its caller.
//!
//! Argument evaluation is *not* in tail position. `(f (g x))` evaluates
//! `(g x)` under a frame so the value can be collected as an argument
//! to `f`.
//!
//! ## Special forms recognized here (T7)
//!
//! `quote`, `if`, `lambda`, `define`, `set!`, `begin`. Derived forms
//! (`let`, `let*`, `letrec`, `cond`, `case`, `and`, `or`, `when`,
//! `unless`, `do`, `parameterize`, `quasiquote`-expansion) are added in
//! T8 (`nscheme-i4h`).

// Several internal helpers take `Value`/`EnvRef` by value so they can
// consume and decompose them without forcing the caller to clone. The
// alternative — taking `&Value` — would push the clones into the
// special-form code where they aren't any cheaper. Allow the lint at
// module scope rather than spraying `#[allow]` on each helper.
#![allow(clippy::needless_pass_by_value)]
// step_eval is a single dispatch table per the architecture in
// docs/0001; splitting it further to satisfy clippy's line-count cap
// would obscure the dispatch.
#![allow(clippy::too_many_lines)]

use std::rc::Rc;

use thiserror::Error;

use crate::env::{Env, EnvRef};
use crate::parse::{ParseError, parse_program};
use crate::value::{Pair, Procedure, RuntimeError, Symbol, Value};

/// Errors raised during evaluation. Wraps [`RuntimeError`] (primitive /
/// lookup failures) and [`ParseError`] for convenience APIs that go
/// straight from source to result.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum EvalError {
    #[error("{0}")]
    Runtime(#[from] RuntimeError),

    #[error("{0}")]
    Parse(#[from] ParseError),

    #[error("malformed special form `{form}`: {message}")]
    MalformedForm { form: &'static str, message: String },
}

impl EvalError {
    fn malformed(form: &'static str, message: impl Into<String>) -> Self {
        Self::MalformedForm {
            form,
            message: message.into(),
        }
    }
}

// ---------------------------------------------------------------------
// Step + Frame state
// ---------------------------------------------------------------------

/// One transition of the evaluator loop.
enum Step {
    /// Evaluate this expression in this environment.
    Eval(Value, EnvRef),
    /// Apply this procedure to these argument values.
    Apply(Value, Vec<Value>),
    /// A sub-evaluation produced this value; resume the next frame.
    Return(Value),
}

/// Pending work that resumes when a sub-evaluation returns.
///
/// Every variant carries an `env` so it can resume in the correct
/// lexical scope.
enum Frame {
    /// `(if test conseq alt)` — `test` is currently evaluating.
    IfBranch {
        conseq: Value,
        alt: Option<Value>,
        env: EnvRef,
    },
    /// `(begin e1 e2 … en)` — `e_i` is currently evaluating, and the
    /// remaining (non-empty) tail expressions are `rest`. The last
    /// expression is reached by *not* pushing this frame for it.
    BeginRest { rest: Vec<Value>, env: EnvRef },
    /// `(define name <expr>)` — `<expr>` is currently evaluating.
    DefineBind { name: Symbol, env: EnvRef },
    /// `(set! name <expr>)` — `<expr>` is currently evaluating.
    SetBind { name: Symbol, env: EnvRef },
    /// `(op a1 a2 …)` — the operator `op` is currently evaluating;
    /// `args` holds the unevaluated argument expressions.
    CallOp { args: Vec<Value>, env: EnvRef },
    /// `(op a1 a2 …)` — the operator is `proc`, we've already
    /// evaluated `evaluated`, and `remaining[0]` is currently
    /// evaluating; the rest of `remaining` is queued.
    CallArg {
        proc: Value,
        evaluated: Vec<Value>,
        remaining: Vec<Value>,
        env: EnvRef,
    },
}

// ---------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------

/// Evaluate one expression in `env`. The expression is a runtime
/// [`Value`] produced by [`crate::parse::parse_one`] or constructed
/// programmatically.
pub fn eval(expr: Value, env: EnvRef) -> Result<Value, EvalError> {
    let mut state = Step::Eval(expr, env);
    let mut frames: Vec<Frame> = Vec::new();
    loop {
        state = match state {
            Step::Eval(expr, env) => step_eval(expr, env, &mut frames)?,
            Step::Apply(proc, args) => step_apply(proc, args, &mut frames)?,
            Step::Return(value) => match frames.pop() {
                Some(frame) => resume(frame, value, &mut frames)?,
                None => return Ok(value),
            },
        };
    }
}

/// Convenience: lex, parse, and evaluate every top-level datum in
/// `source` in `env`. Returns the value of the last expression (or
/// [`Value::Unspecified`] if `source` is empty).
pub fn eval_source(source: &str, env: EnvRef) -> Result<Value, EvalError> {
    let datums = parse_program(source)?;
    let mut last = Value::Unspecified;
    for d in datums {
        last = eval(d, env.clone())?;
    }
    Ok(last)
}

// ---------------------------------------------------------------------
// step_eval
// ---------------------------------------------------------------------

fn step_eval(expr: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    // Self-evaluating values.
    match &expr {
        Value::Bool(_)
        | Value::Char(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::Vector(_)
        | Value::Bytevector(_)
        | Value::Eof
        | Value::Unspecified
        | Value::Procedure(_)
        | Value::Port(_) => return Ok(Step::Return(expr)),
        Value::Symbol(sym) => {
            let v = env.lookup(sym).ok_or_else(|| {
                EvalError::Runtime(RuntimeError::Undefined(sym.name().to_string()))
            })?;
            return Ok(Step::Return(v));
        }
        Value::Null => {
            // `()` is not a valid expression in R7RS; evaluating it
            // is an error.
            return Err(EvalError::malformed(
                "application",
                "() is not a procedure call",
            ));
        }
        Value::Pair(_) => { /* fall through */ }
    }

    // Compound expression: a pair. Inspect the head.
    let (head, tail) = expr.as_pair().expect("must be a pair");
    if let Value::Symbol(sym) = &head {
        match sym.name() {
            "quote" => return step_quote(&tail),
            "if" => return step_if(tail, env, frames),
            "lambda" => return step_lambda(tail, env, None),
            "define" => return step_define(tail, env, frames),
            "set!" => return step_set(tail, env, frames),
            "begin" => return step_begin(tail, env, frames),
            _ => { /* fall through to procedure call */ }
        }
    }

    step_call(head, tail, env, frames)
}

// ---------------------------------------------------------------------
// Special forms
// ---------------------------------------------------------------------

fn step_quote(tail: &Value) -> Result<Step, EvalError> {
    let mut iter = ListIter::new(tail.clone());
    let datum = iter
        .next()
        .ok_or_else(|| EvalError::malformed("quote", "expected one operand"))??;
    if iter.next().is_some() {
        return Err(EvalError::malformed(
            "quote",
            "expected exactly one operand",
        ));
    }
    Ok(Step::Return(datum))
}

fn step_if(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let mut iter = ListIter::new(tail);
    let test = iter
        .next()
        .ok_or_else(|| EvalError::malformed("if", "expected at least 2 operands"))??;
    let conseq = iter
        .next()
        .ok_or_else(|| EvalError::malformed("if", "expected at least 2 operands"))??;
    let alt = match iter.next() {
        Some(Ok(v)) => Some(v),
        Some(Err(e)) => return Err(e),
        None => None,
    };
    if iter.next().is_some() {
        return Err(EvalError::malformed("if", "expected at most 3 operands"));
    }
    frames.push(Frame::IfBranch {
        conseq,
        alt,
        env: env.clone(),
    });
    Ok(Step::Eval(test, env))
}

fn step_lambda(tail: Value, env: EnvRef, name: Option<String>) -> Result<Step, EvalError> {
    let (params_form, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("lambda", "expected parameters and body"))?;
    let (params, rest) = parse_formals(&params_form)?;
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("lambda", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed(
            "lambda",
            "body must contain at least one expression",
        ));
    }
    let closure = Procedure::Closure {
        params,
        rest,
        body,
        env,
        name,
    };
    Ok(Step::Return(Value::Procedure(Rc::new(closure))))
}

fn step_define(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    // (define name <value>)           - bind name to value
    // (define (name . formals) body)  - sugar for (define name (lambda formals body))
    let (head, rest) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("define", "expected name and value"))?;
    match head {
        Value::Symbol(name) => {
            let mut value_iter = ListIter::new(rest);
            let value_expr = value_iter
                .next()
                .ok_or_else(|| EvalError::malformed("define", "expected a value expression"))??;
            if value_iter.next().is_some() {
                return Err(EvalError::malformed(
                    "define",
                    "expected exactly one value expression",
                ));
            }
            frames.push(Frame::DefineBind {
                name,
                env: env.clone(),
            });
            Ok(Step::Eval(value_expr, env))
        }
        Value::Pair(_) => {
            // (define (name . formals) body...)
            let (name_val, formals) = head
                .as_pair()
                .ok_or_else(|| EvalError::malformed("define", "expected (name . formals)"))?;
            let Value::Symbol(name_sym) = name_val else {
                return Err(EvalError::malformed("define", "name must be a symbol"));
            };
            // Construct (lambda formals body...) and evaluate it.
            let lambda_tail = Value::cons(formals, rest);
            let closure_step =
                step_lambda(lambda_tail, env.clone(), Some(name_sym.name().to_string()))?;
            // step_lambda returns Step::Return(closure); bind it.
            match closure_step {
                Step::Return(closure) => {
                    env.define(name_sym, closure);
                    Ok(Step::Return(Value::Unspecified))
                }
                _ => unreachable!("step_lambda always returns Step::Return"),
            }
        }
        _ => Err(EvalError::malformed(
            "define",
            "target must be a symbol or (name . formals)",
        )),
    }
}

fn step_set(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (head, rest) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("set!", "expected name and value"))?;
    let Value::Symbol(name) = head else {
        return Err(EvalError::malformed("set!", "name must be a symbol"));
    };
    let mut value_iter = ListIter::new(rest);
    let value_expr = value_iter
        .next()
        .ok_or_else(|| EvalError::malformed("set!", "expected a value expression"))??;
    if value_iter.next().is_some() {
        return Err(EvalError::malformed(
            "set!",
            "expected exactly one value expression",
        ));
    }
    frames.push(Frame::SetBind {
        name,
        env: env.clone(),
    });
    Ok(Step::Eval(value_expr, env))
}

fn step_begin(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let exprs = collect_list(&tail)
        .map_err(|()| EvalError::malformed("begin", "body must be a proper list"))?;
    if exprs.is_empty() {
        // (begin) — R7RS leaves this unspecified; we return Unspecified.
        return Ok(Step::Return(Value::Unspecified));
    }
    Ok(eval_sequence(exprs, env, frames))
}

/// Evaluate a non-empty sequence of expressions, returning the value of
/// the last. The last expression is in tail position — no frame is
/// pushed for it; intermediate expressions are queued under a
/// [`Frame::BeginRest`].
fn eval_sequence(exprs: Vec<Value>, env: EnvRef, frames: &mut Vec<Frame>) -> Step {
    assert!(!exprs.is_empty(), "eval_sequence called with empty list");
    if exprs.len() == 1 {
        let only = exprs.into_iter().next().unwrap();
        return Step::Eval(only, env);
    }
    let mut iter = exprs.into_iter();
    let first = iter.next().unwrap();
    let rest: Vec<Value> = iter.collect();
    frames.push(Frame::BeginRest {
        rest,
        env: env.clone(),
    });
    Step::Eval(first, env)
}

// ---------------------------------------------------------------------
// Procedure call
// ---------------------------------------------------------------------

fn step_call(
    head: Value,
    tail: Value,
    env: EnvRef,
    frames: &mut Vec<Frame>,
) -> Result<Step, EvalError> {
    let args = collect_list(&tail)
        .map_err(|()| EvalError::malformed("application", "argument list must be proper"))?;
    frames.push(Frame::CallOp {
        args,
        env: env.clone(),
    });
    Ok(Step::Eval(head, env))
}

// ---------------------------------------------------------------------
// step_apply
// ---------------------------------------------------------------------

fn step_apply(
    proc_value: Value,
    args: Vec<Value>,
    frames: &mut Vec<Frame>,
) -> Result<Step, EvalError> {
    let proc_rc = match proc_value {
        Value::Procedure(p) => p,
        other => {
            return Err(EvalError::Runtime(RuntimeError::NotProcedure(format!(
                "{other}"
            ))));
        }
    };
    match &*proc_rc {
        Procedure::Primitive { name, arity, body } => {
            if !arity.matches(args.len()) {
                return Err(EvalError::Runtime(RuntimeError::Arity {
                    procedure: (*name).to_string(),
                    expected: format!("{arity}"),
                    got: args.len(),
                }));
            }
            let result = body(&args).map_err(EvalError::Runtime)?;
            Ok(Step::Return(result))
        }
        Procedure::Closure {
            params,
            rest,
            body,
            env,
            name,
        } => {
            let provided = args.len();
            let arity_ok = match rest {
                None => provided == params.len(),
                Some(_) => provided >= params.len(),
            };
            if !arity_ok {
                return Err(EvalError::Runtime(RuntimeError::Arity {
                    procedure: name.clone().unwrap_or_else(|| "lambda".into()),
                    expected: if rest.is_some() {
                        format!("at least {}", params.len())
                    } else {
                        format!("exactly {}", params.len())
                    },
                    got: provided,
                }));
            }
            let call_env = Env::extend(env.clone());
            let mut args_iter = args.into_iter();
            for p in params {
                call_env.define(p.clone(), args_iter.next().unwrap());
            }
            if let Some(rest_sym) = rest {
                let leftover: Vec<Value> = args_iter.collect();
                call_env.define(rest_sym.clone(), Value::list_from(leftover));
            }
            // Body: evaluate in sequence, last in tail position.
            Ok(eval_sequence(body.clone(), call_env, frames))
        }
    }
}

// ---------------------------------------------------------------------
// resume
// ---------------------------------------------------------------------

fn resume(frame: Frame, value: Value, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    match frame {
        Frame::IfBranch { conseq, alt, env } => {
            let chosen = if value.is_truthy() {
                conseq
            } else {
                match alt {
                    Some(a) => a,
                    None => return Ok(Step::Return(Value::Unspecified)),
                }
            };
            // Tail position: do NOT push a new frame.
            Ok(Step::Eval(chosen, env))
        }
        Frame::BeginRest { mut rest, env } => {
            // `value` was an intermediate expression's result; discard.
            let _ = value;
            // `rest` is non-empty by construction. If it has one expr
            // left, that's the tail position — Eval without re-pushing.
            // If more, re-push BeginRest with the tail.
            let first = rest.remove(0);
            if !rest.is_empty() {
                frames.push(Frame::BeginRest {
                    rest,
                    env: env.clone(),
                });
            }
            Ok(Step::Eval(first, env))
        }
        Frame::DefineBind { name, env } => {
            env.define(name, value);
            Ok(Step::Return(Value::Unspecified))
        }
        Frame::SetBind { name, env } => {
            env.set(&name, value).map_err(EvalError::Runtime)?;
            Ok(Step::Return(Value::Unspecified))
        }
        Frame::CallOp { args, env } => {
            // `value` is the operator. Begin argument evaluation, or
            // jump straight to Apply if there are no args.
            if args.is_empty() {
                return Ok(Step::Apply(value, Vec::new()));
            }
            let mut remaining = args;
            let first = remaining.remove(0);
            frames.push(Frame::CallArg {
                proc: value,
                evaluated: Vec::new(),
                remaining,
                env: env.clone(),
            });
            Ok(Step::Eval(first, env))
        }
        Frame::CallArg {
            proc,
            mut evaluated,
            mut remaining,
            env,
        } => {
            evaluated.push(value);
            if remaining.is_empty() {
                // All args evaluated — apply. Apply itself is the tail
                // operation: no frame pushed here.
                return Ok(Step::Apply(proc, evaluated));
            }
            let next = remaining.remove(0);
            frames.push(Frame::CallArg {
                proc,
                evaluated,
                remaining,
                env: env.clone(),
            });
            Ok(Step::Eval(next, env))
        }
    }
}

// ---------------------------------------------------------------------
// Helpers: parsing formals + list manipulation
// ---------------------------------------------------------------------

/// Parse the formal-parameter portion of a `lambda` form.
///
/// Returns `(positional, rest)`:
/// - `(lambda (a b c) ...)`        -> ([a, b, c], None)
/// - `(lambda (a b . rest) ...)`   -> ([a, b], Some(rest))
/// - `(lambda args ...)`           -> ([], Some(args))
/// - `(lambda () ...)`             -> ([], None)
fn parse_formals(form: &Value) -> Result<(Vec<Symbol>, Option<Symbol>), EvalError> {
    match form {
        Value::Null => Ok((Vec::new(), None)),
        Value::Symbol(s) => Ok((Vec::new(), Some(s.clone()))),
        Value::Pair(_) => {
            let mut positional = Vec::new();
            let mut cur = form.clone();
            loop {
                match cur {
                    Value::Null => return Ok((positional, None)),
                    Value::Symbol(s) => return Ok((positional, Some(s))),
                    Value::Pair(p) => {
                        let pair = p.borrow();
                        let head = pair.car.clone();
                        let tail = pair.cdr.clone();
                        drop(pair);
                        match head {
                            Value::Symbol(s) => positional.push(s),
                            other => {
                                return Err(EvalError::malformed(
                                    "lambda",
                                    format!("parameter must be a symbol, got {other}"),
                                ));
                            }
                        }
                        cur = tail;
                    }
                    other => {
                        return Err(EvalError::malformed(
                            "lambda",
                            format!("malformed formals near {other}"),
                        ));
                    }
                }
            }
        }
        other => Err(EvalError::malformed(
            "lambda",
            format!("formals must be a list or symbol, got {other}"),
        )),
    }
}

/// Iterator over a proper-list `Value`. Yields `Err` on improper-list
/// tails so callers can distinguish a malformed list from a normal
/// end-of-iteration.
struct ListIter {
    current: Value,
    done: bool,
}

impl ListIter {
    fn new(v: Value) -> Self {
        Self {
            current: v,
            done: false,
        }
    }
}

impl Iterator for ListIter {
    type Item = Result<Value, EvalError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let cur = std::mem::replace(&mut self.current, Value::Null);
        match cur {
            Value::Null => {
                self.done = true;
                None
            }
            Value::Pair(p) => {
                let Pair { car, cdr } = {
                    let cell = p.borrow();
                    Pair {
                        car: cell.car.clone(),
                        cdr: cell.cdr.clone(),
                    }
                };
                self.current = cdr;
                Some(Ok(car))
            }
            other => {
                self.done = true;
                Some(Err(EvalError::malformed(
                    "application",
                    format!("improper list: {other}"),
                )))
            }
        }
    }
}

/// Collect a proper list into a `Vec`. Returns `Err(())` if `v` is not
/// a proper list. Used where the caller already wants a structured
/// error.
fn collect_list(v: &Value) -> Result<Vec<Value>, ()> {
    let mut out = Vec::new();
    let mut cur = v.clone();
    loop {
        match cur {
            Value::Null => return Ok(out),
            Value::Pair(p) => {
                let pair = p.borrow();
                out.push(pair.car.clone());
                cur = pair.cdr.clone();
            }
            _ => return Err(()),
        }
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Arity, equal};

    fn run(source: &str) -> Result<Value, EvalError> {
        let env = base_env();
        eval_source(source, env)
    }

    fn intern_def(env: &EnvRef, name: &'static str, arity: Arity, body: crate::value::PrimitiveFn) {
        let p = Procedure::Primitive { name, arity, body };
        env.define(Symbol::intern(name), Value::Procedure(Rc::new(p)));
    }

    /// Tiny test environment with just `+`, `-`, `*`, `=`, `<`, and
    /// `list` so the evaluator tests don't depend on T9.
    fn base_env() -> EnvRef {
        let env = Env::new_global();
        intern_def(&env, "+", Arity::AtLeast(0), |args| {
            let mut acc: i64 = 0;
            for a in args {
                match a {
                    Value::Int(n) => {
                        acc = acc
                            .checked_add(*n)
                            .ok_or_else(|| RuntimeError::Other("integer overflow in +".into()))?
                    }
                    other => {
                        return Err(RuntimeError::Type {
                            expected: "integer".into(),
                            got: other.type_name().into(),
                        });
                    }
                }
            }
            Ok(Value::Int(acc))
        });
        intern_def(&env, "-", Arity::AtLeast(1), |args| {
            let mut iter = args.iter();
            let first = match iter.next().unwrap() {
                Value::Int(n) => *n,
                other => {
                    return Err(RuntimeError::Type {
                        expected: "integer".into(),
                        got: other.type_name().into(),
                    });
                }
            };
            if args.len() == 1 {
                return Ok(Value::Int(-first));
            }
            let mut acc = first;
            for a in iter {
                match a {
                    Value::Int(n) => {
                        acc = acc
                            .checked_sub(*n)
                            .ok_or_else(|| RuntimeError::Other("integer overflow in -".into()))?
                    }
                    other => {
                        return Err(RuntimeError::Type {
                            expected: "integer".into(),
                            got: other.type_name().into(),
                        });
                    }
                }
            }
            Ok(Value::Int(acc))
        });
        intern_def(&env, "*", Arity::AtLeast(0), |args| {
            let mut acc: i64 = 1;
            for a in args {
                match a {
                    Value::Int(n) => {
                        acc = acc
                            .checked_mul(*n)
                            .ok_or_else(|| RuntimeError::Other("integer overflow in *".into()))?
                    }
                    other => {
                        return Err(RuntimeError::Type {
                            expected: "integer".into(),
                            got: other.type_name().into(),
                        });
                    }
                }
            }
            Ok(Value::Int(acc))
        });
        intern_def(&env, "=", Arity::AtLeast(2), |args| {
            let first = match &args[0] {
                Value::Int(n) => *n,
                other => {
                    return Err(RuntimeError::Type {
                        expected: "integer".into(),
                        got: other.type_name().into(),
                    });
                }
            };
            for a in &args[1..] {
                match a {
                    Value::Int(n) if *n == first => {}
                    Value::Int(_) => return Ok(Value::Bool(false)),
                    other => {
                        return Err(RuntimeError::Type {
                            expected: "integer".into(),
                            got: other.type_name().into(),
                        });
                    }
                }
            }
            Ok(Value::Bool(true))
        });
        intern_def(&env, "<", Arity::AtLeast(2), |args| {
            let mut prev = match &args[0] {
                Value::Int(n) => *n,
                other => {
                    return Err(RuntimeError::Type {
                        expected: "integer".into(),
                        got: other.type_name().into(),
                    });
                }
            };
            for a in &args[1..] {
                let cur = match a {
                    Value::Int(n) => *n,
                    other => {
                        return Err(RuntimeError::Type {
                            expected: "integer".into(),
                            got: other.type_name().into(),
                        });
                    }
                };
                if prev >= cur {
                    return Ok(Value::Bool(false));
                }
                prev = cur;
            }
            Ok(Value::Bool(true))
        });
        intern_def(&env, "list", Arity::AtLeast(0), |args| {
            Ok(Value::list_from(args.iter().cloned()))
        });
        env
    }

    // -- self-evaluating + lookup -----------------------------------

    #[test]
    fn integers_self_evaluate() {
        assert!(equal(&run("42").unwrap(), &Value::Int(42)));
    }

    #[test]
    fn strings_self_evaluate() {
        assert!(equal(&run(r#""hi""#).unwrap(), &Value::string("hi")));
    }

    #[test]
    fn booleans_self_evaluate() {
        assert!(equal(&run("#t").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("#f").unwrap(), &Value::Bool(false)));
    }

    #[test]
    fn symbol_lookup_undefined_errors() {
        let err = run("nope").unwrap_err();
        assert!(matches!(
            err,
            EvalError::Runtime(RuntimeError::Undefined(name)) if name == "nope"
        ));
    }

    // -- special forms ----------------------------------------------

    #[test]
    fn quote_returns_datum_unevaluated() {
        let v = run("'(1 2 3)").unwrap();
        let expected = Value::list_from([Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert!(equal(&v, &expected));
    }

    #[test]
    fn quote_symbol() {
        let v = run("'foo").unwrap();
        assert!(equal(&v, &Value::Symbol(Symbol::intern("foo"))));
    }

    #[test]
    fn if_true_branch() {
        assert!(equal(&run("(if #t 1 2)").unwrap(), &Value::Int(1)));
    }

    #[test]
    fn if_false_branch() {
        assert!(equal(&run("(if #f 1 2)").unwrap(), &Value::Int(2)));
    }

    #[test]
    fn if_truthy_anything_but_false() {
        // R7RS: only #f is falsy.
        assert!(equal(
            &run("(if 0 'yes 'no)").unwrap(),
            &Value::Symbol(Symbol::intern("yes"))
        ));
        assert!(equal(
            &run("(if '() 'yes 'no)").unwrap(),
            &Value::Symbol(Symbol::intern("yes"))
        ));
    }

    #[test]
    fn if_without_alt_returns_unspecified_on_false() {
        let v = run("(if #f 1)").unwrap();
        assert!(matches!(v, Value::Unspecified));
    }

    #[test]
    fn begin_returns_last_value() {
        assert!(equal(&run("(begin 1 2 3)").unwrap(), &Value::Int(3)));
    }

    #[test]
    fn define_then_lookup() {
        let env = base_env();
        eval_source("(define x 42)", env.clone()).unwrap();
        assert!(equal(&eval_source("x", env).unwrap(), &Value::Int(42)));
    }

    #[test]
    fn set_existing_variable() {
        let env = base_env();
        eval_source("(define x 1) (set! x 99)", env.clone()).unwrap();
        assert!(equal(&eval_source("x", env).unwrap(), &Value::Int(99)));
    }

    #[test]
    fn set_undefined_variable_errors() {
        let env = base_env();
        let err = eval_source("(set! x 1)", env).unwrap_err();
        assert!(matches!(
            err,
            EvalError::Runtime(RuntimeError::Undefined(name)) if name == "x"
        ));
    }

    // -- lambda + call ----------------------------------------------

    #[test]
    fn lambda_identity() {
        assert!(equal(&run("((lambda (x) x) 7)").unwrap(), &Value::Int(7)));
    }

    #[test]
    fn lambda_closes_over_environment() {
        let v = run("(define x 10) ((lambda (y) (+ x y)) 5)").unwrap();
        assert!(equal(&v, &Value::Int(15)));
    }

    #[test]
    fn lambda_rest_argument() {
        let v = run("((lambda args args) 1 2 3)").unwrap();
        let expected = Value::list_from([Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert!(equal(&v, &expected));
    }

    #[test]
    fn lambda_dotted_rest() {
        let v = run("((lambda (a . rest) (list a rest)) 1 2 3)").unwrap();
        let inner_rest = Value::list_from([Value::Int(2), Value::Int(3)]);
        let expected = Value::list_from([Value::Int(1), inner_rest]);
        assert!(equal(&v, &expected));
    }

    #[test]
    fn lambda_arity_error() {
        let err = run("((lambda (x y) x) 1)").unwrap_err();
        assert!(matches!(
            err,
            EvalError::Runtime(RuntimeError::Arity { .. })
        ));
    }

    #[test]
    fn define_shorthand() {
        let v = run("(define (square x) (* x x)) (square 5)").unwrap();
        assert!(equal(&v, &Value::Int(25)));
    }

    #[test]
    fn define_named_closure_keeps_name() {
        let env = base_env();
        eval_source("(define (foo) 1)", env.clone()).unwrap();
        let v = eval_source("foo", env).unwrap();
        match v {
            Value::Procedure(p) => assert_eq!(p.name(), "foo"),
            other => panic!("expected procedure, got {other}"),
        }
    }

    // -- recursion ---------------------------------------------------

    #[test]
    fn recursive_factorial() {
        let v = run("(define (fact n) (if (= n 0) 1 (* n (fact (- n 1))))) (fact 5)").unwrap();
        assert!(equal(&v, &Value::Int(120)));
    }

    // -- TCO ---------------------------------------------------------

    #[test]
    fn deep_tail_recursion_does_not_overflow() {
        // 100k iterations is enough to overflow a recursive Rust eval;
        // with proper TCO the frame stack stays O(1).
        let src = "(define (loop n) (if (= n 0) 'done (loop (- n 1)))) (loop 100000)";
        let v = run(src).unwrap();
        assert!(equal(&v, &Value::Symbol(Symbol::intern("done"))));
    }

    #[test]
    fn tail_position_inside_begin() {
        // The recursive call is the LAST expression of begin — tail position.
        let src = "(define (loop n) (begin (if #t 'noop 'noop) (if (= n 0) 'done (loop (- n 1))))) (loop 10000)";
        let v = run(src).unwrap();
        assert!(equal(&v, &Value::Symbol(Symbol::intern("done"))));
    }

    #[test]
    fn tail_position_inside_if() {
        let src = "(define (loop n) (if (= n 0) 'done (if #t (loop (- n 1)) 'never))) (loop 10000)";
        let v = run(src).unwrap();
        assert!(equal(&v, &Value::Symbol(Symbol::intern("done"))));
    }

    // -- error paths -------------------------------------------------

    #[test]
    fn call_non_procedure_errors() {
        let err = run("(42)").unwrap_err();
        assert!(matches!(
            err,
            EvalError::Runtime(RuntimeError::NotProcedure(_))
        ));
    }

    #[test]
    fn empty_application_errors() {
        let err = run("()").unwrap_err();
        assert!(matches!(err, EvalError::MalformedForm { .. }));
    }

    #[test]
    fn malformed_define_errors() {
        let err = run("(define)").unwrap_err();
        assert!(matches!(
            err,
            EvalError::MalformedForm { form: "define", .. }
        ));
    }
}
