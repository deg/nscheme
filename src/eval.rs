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
use crate::value::{ErrorKind, ErrorObject, Pair, Procedure, RuntimeError, Symbol, Value};

/// Convert a [`RuntimeError`] (raised by a primitive) into a Scheme
/// error-object Value, so it can flow through the R7RS exception
/// mechanism (with-exception-handler / guard). The error kind is
/// preserved so `file-error?` and `read-error?` work on the
/// resulting value.
fn runtime_error_to_value(e: RuntimeError) -> Value {
    let kind = match &e {
        RuntimeError::FileError(_) => ErrorKind::File,
        RuntimeError::ReadError(_) => ErrorKind::Read,
        _ => ErrorKind::User,
    };
    Value::ErrorObject(Rc::new(ErrorObject {
        message: format!("{e}"),
        irritants: Vec::new(),
        kind,
    }))
}

/// Errors raised during evaluation. Wraps [`RuntimeError`] (primitive /
/// lookup failures) and [`ParseError`] for convenience APIs that go
/// straight from source to result.
#[derive(Clone, Debug, Error)]
pub enum EvalError {
    #[error("{0}")]
    Runtime(#[from] RuntimeError),

    #[error("{0}")]
    Parse(#[from] ParseError),

    #[error("malformed special form `{form}`: {message}")]
    MalformedForm { form: &'static str, message: String },

    /// An uncaught `raise` — bubbled out past the outermost handler.
    /// The payload is whatever value was passed to `raise`.
    #[error("unhandled raise: {0}")]
    Raised(Value),
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
    /// Replace the evaluator's frame stack with the captured one and
    /// resume by returning `value`. Used to invoke a continuation
    /// captured by `call/cc`.
    InvokeContinuation(Vec<Frame>, Value),
    /// Propagate an exception up the frame stack until an
    /// [`Frame::ExceptionHandler`] catches it. If none catches, the
    /// `eval()` function returns an `EvalError::Raised(value)`. The
    /// boolean is `true` for `raise-continuable`: when set, a handler
    /// that returns normally substitutes its result for the raise
    /// expression. Otherwise the handler's result is re-raised.
    Raise(Value, bool),
}

/// Pending work that resumes when a sub-evaluation returns.
///
/// Every variant carries an `env` so it can resume in the correct
/// lexical scope.
#[derive(Clone)]
pub enum Frame {
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
    /// `(cond (t b…) … )` — currently evaluating the test of `clause`;
    /// `remaining_clauses` holds the as-yet-unexamined clauses.
    CondClause {
        clause: Value,
        remaining_clauses: Vec<Value>,
        env: EnvRef,
    },
    /// `((test => proc-expr))` — the test was truthy and produced
    /// `test_value`; `proc-expr` is currently evaluating.
    CondArrow { test_value: Value, env: EnvRef },
    /// `(and e1 … en)` — short-circuit AND. The currently-evaluating
    /// expression is non-last (we never push this frame for the tail
    /// expression).
    AndNext { remaining: Vec<Value>, env: EnvRef },
    /// `(or e1 … en)` — short-circuit OR. The currently-evaluating
    /// expression is non-last.
    OrNext { remaining: Vec<Value>, env: EnvRef },
    /// `(apply proc a1 ... arglist)` — evaluating arg expressions.
    /// `evaluated[0]` is the proc once it has been evaluated. The
    /// last evaluated arg is spread when remaining becomes empty.
    ApplySpread {
        evaluated: Vec<Value>,
        remaining: Vec<Value>,
        env: EnvRef,
    },
    /// An exception handler installed by `with-exception-handler`.
    /// When a `Step::Raise(v, _)` propagates up, the handler is
    /// invoked with `v`. Whether the handler's return value is
    /// substituted (for `raise-continuable`) or re-raised (for plain
    /// `raise`) is decided by the `continuable` flag carried by the
    /// raise itself, not the handler.
    ExceptionHandler { handler: Value, env: EnvRef },
    /// Helper frame: when a non-continuable handler returns, re-raise
    /// the value to the next outer handler.
    ReRaise,
    /// Pending raise. When the operand of `raise` /
    /// `raise-continuable` finishes evaluating, this frame fires
    /// `Step::Raise(value, continuable)`.
    RaiseAfter { continuable: bool },
    /// `(eval datum env-spec)` post-evaluation step: when the
    /// expression argument finishes evaluating (giving us a *datum*),
    /// re-evaluate that datum as code in the captured env.
    EvalAfter { env: EnvRef },
    /// Helper for `with-exception-handler`: when the handler
    /// expression has finished evaluating, install it as an
    /// `ExceptionHandler` frame and call the thunk.
    InstallHandler { thunk_expr: Value, env: EnvRef },
    /// Restore parameter values when a `parameterize` form's body
    /// completes (or unwinds via raise — see `Step::Raise` handling).
    ParameterRestore {
        saved: Vec<(Rc<crate::value::ParameterCell>, Value)>,
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
            Step::InvokeContinuation(saved, value) => {
                frames = saved;
                Step::Return(value)
            }
            Step::Raise(value, continuable) => {
                // Walk the frame stack looking for an ExceptionHandler.
                // For raise-continuable we preserve the frames between
                // the raise and the handler so the handler's return
                // value substitutes for the raise expression. For plain
                // raise we discard them and push ReRaise so the
                // handler's return is re-raised. ParameterRestore
                // frames fire as we unwind so parameterize's old
                // values survive uncaught raises.
                let mut popped: Vec<Frame> = Vec::new();
                let mut handler_found = None;
                while let Some(frame) = frames.pop() {
                    if let Frame::ExceptionHandler { handler, env: _ } = frame {
                        handler_found = Some(handler);
                        break;
                    }
                    if let Frame::ParameterRestore { saved } = &frame {
                        for (cell, old) in saved {
                            *cell.value.borrow_mut() = old.clone();
                        }
                    }
                    popped.push(frame);
                }
                match handler_found {
                    Some(handler) => {
                        if continuable {
                            // Restore the frames between handler and
                            // raise so the handler's result resumes at
                            // the raise expression's position.
                            for f in popped.into_iter().rev() {
                                frames.push(f);
                            }
                        } else {
                            frames.push(Frame::ReRaise);
                        }
                        Step::Apply(handler, vec![value])
                    }
                    None => return Err(EvalError::Raised(value)),
                }
            }
        };
    }
}

/// Convenience: lex, parse, and evaluate every top-level datum in
/// `source` in `env`. Returns the value of the last expression (or
/// [`Value::Unspecified`] if `source` is empty).
///
/// All top-level datums share a single evaluator loop (wrapped in an
/// implicit `begin`) so that continuations captured by `call/cc` can
/// span across top-level forms — invoking a saved continuation jumps
/// back to whatever was about to happen at capture time, even if a
/// later top-level form has since started evaluating.
pub fn eval_source(source: &str, env: EnvRef) -> Result<Value, EvalError> {
    let datums = parse_program(source)?;
    if datums.is_empty() {
        return Ok(Value::Unspecified);
    }
    let mut begin_items = vec![Value::Symbol(Symbol::intern("begin"))];
    begin_items.extend(datums);
    eval(Value::list_from(begin_items), env)
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
        | Value::BigInt(_)
        | Value::Rational(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::Vector(_)
        | Value::Bytevector(_)
        | Value::Eof
        | Value::Unspecified
        | Value::Procedure(_)
        | Value::Port(_)
        | Value::Macro(_)
        | Value::ErrorObject(_)
        | Value::Promise(_)
        | Value::Values(_)
        | Value::Record { .. } => return Ok(Step::Return(expr)),
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
            // Derived forms (T8 nscheme-i4h):
            "let" => return step_let(tail, env, frames),
            "let*" => return step_let_star(tail, env, frames),
            "letrec" | "letrec*" => return step_letrec(tail, env, frames),
            "cond" => return step_cond(tail, env, frames),
            "case" => return step_case(tail, env, frames),
            "and" => return step_and(tail, env, frames),
            "or" => return step_or(tail, env, frames),
            "when" => return step_when(tail, env, frames),
            "unless" => return step_unless(tail, env, frames),
            "do" => return step_do(tail, env, frames),
            "quasiquote" => return step_quasiquote(&tail, env),
            "define-syntax" => return step_define_syntax(tail, env),
            "let-syntax" | "letrec-syntax" => return step_let_syntax(tail, env, frames),
            "define-library" => return step_define_library(tail, env),
            "import" => return step_import(tail, env),
            "cond-expand" => return step_cond_expand(tail, env, frames),
            "call/cc" | "call-with-current-continuation" => {
                return step_call_cc(tail, env, frames);
            }
            "apply" => return step_apply_form(tail, env, frames),
            "delay" | "delay-force" | "lazy" => return step_delay(tail, env),
            "case-lambda" => return step_case_lambda(tail, env),
            "define-values" => return step_define_values(tail, env),
            "define-record-type" => return step_define_record_type(tail, env),
            "eval" => return step_eval_form(tail, env, frames),
            "let-values" => return step_let_values(tail, env, frames, false),
            "let*-values" => return step_let_values(tail, env, frames, true),
            "parameterize" => return step_parameterize(tail, env, frames),
            "raise" => return step_raise_with_frames(tail, env, false, frames),
            "raise-continuable" => return step_raise_with_frames(tail, env, true, frames),
            "with-exception-handler" => {
                return step_with_exception_handler_real(tail, env, frames);
            }
            "guard" => return step_guard_real(tail, env, frames),
            _ => { /* fall through to procedure call or macro */ }
        }
        // Macro application: if the head symbol resolves to a Macro
        // value, expand and re-evaluate.
        if let Some(Value::Macro(rules)) = env.lookup(sym) {
            let args = collect_list(&tail)
                .map_err(|()| EvalError::malformed("macro", "argument list must be proper"))?;
            // Reconstruct the full call form (head + args) for matching.
            let call_form = Value::cons(Value::Symbol(sym.clone()), Value::list_from(args));
            let expanded = crate::macros::expand(&rules, &call_form)?;
            return Ok(Step::Eval(expanded, env));
        }
    }

    step_call(head, tail, env, frames)
}

/// `(define-syntax name (syntax-rules ...))` — install a macro.
fn step_define_syntax(tail: Value, env: EnvRef) -> Result<Step, EvalError> {
    let (name_val, rest) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("define-syntax", "expected name and rules"))?;
    let Value::Symbol(name) = name_val else {
        return Err(EvalError::malformed(
            "define-syntax",
            "name must be a symbol",
        ));
    };
    let mut iter = ListIter::new(rest);
    let rules_form = iter
        .next()
        .ok_or_else(|| EvalError::malformed("define-syntax", "expected a syntax-rules form"))??;
    let rules = crate::macros::parse_syntax_rules(&rules_form)?;
    env.define(name, Value::Macro(Rc::new(rules)));
    Ok(Step::Return(Value::Unspecified))
}

/// `(let-syntax ((name (syntax-rules ...)) ...) body...)`. Binds the
/// macros in a fresh child env and evaluates the body.
/// `letrec-syntax` is treated the same — `syntax-rules` can't reference
/// other macros being defined in the same form anyway.
fn step_let_syntax(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (bindings_form, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("let-syntax", "expected bindings and body"))?;
    let bindings = collect_list(&bindings_form)
        .map_err(|()| EvalError::malformed("let-syntax", "bindings must be a proper list"))?;
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("let-syntax", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed("let-syntax", "body must not be empty"));
    }
    let scope = Env::extend(env);
    for b in bindings {
        let parts = collect_list(&b)
            .map_err(|()| EvalError::malformed("let-syntax", "each binding must be a list"))?;
        if parts.len() != 2 {
            return Err(EvalError::malformed(
                "let-syntax",
                "binding must be (name (syntax-rules ...))",
            ));
        }
        let Value::Symbol(name) = parts[0].clone() else {
            return Err(EvalError::malformed(
                "let-syntax",
                "binding name must be a symbol",
            ));
        };
        let rules = crate::macros::parse_syntax_rules(&parts[1])?;
        scope.define(name, Value::Macro(Rc::new(rules)));
    }
    Ok(eval_sequence(body, scope, frames))
}

/// `(define-library (name) decl...)`. Each `decl` is one of:
/// `(export …)`, `(import …)`, `(begin …)`, `(include "file")`,
/// `(cond-expand …)`.
fn step_define_library(tail: Value, env: EnvRef) -> Result<Step, EvalError> {
    use std::collections::HashMap;
    let (name_form, decls_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("define-library", "expected name and decls"))?;
    let lib_name = crate::library::parse_library_name(&name_form)?;
    let decls = collect_list(&decls_tail)
        .map_err(|()| EvalError::malformed("define-library", "decls must be a proper list"))?;
    let lib_env = crate::env::Env::extend(env.clone());
    let mut exports: Vec<Symbol> = Vec::new();
    process_library_decls(&decls, &lib_env, &mut exports)?;
    // Collect exported bindings into a registry entry.
    let mut bindings: HashMap<Symbol, Value> = HashMap::new();
    for name in exports {
        let v = lib_env.lookup(&name).ok_or_else(|| {
            EvalError::malformed("define-library", "exported name was not defined")
        })?;
        bindings.insert(name, v);
    }
    crate::library::register_library(lib_name, bindings);
    Ok(Step::Return(Value::Unspecified))
}

fn process_library_decls(
    decls: &[Value],
    lib_env: &EnvRef,
    exports: &mut Vec<Symbol>,
) -> Result<(), EvalError> {
    for decl in decls {
        let parts = collect_list(decl)
            .map_err(|()| EvalError::malformed("define-library", "decl must be a list"))?;
        if parts.is_empty() {
            continue;
        }
        let Value::Symbol(head) = &parts[0] else {
            return Err(EvalError::malformed(
                "define-library",
                "decl must start with an identifier",
            ));
        };
        match head.name() {
            "export" => {
                for e in &parts[1..] {
                    if let Value::Symbol(s) = e {
                        exports.push(s.clone());
                    } else {
                        return Err(EvalError::malformed("export", "expected identifier"));
                    }
                }
            }
            "import" => {
                for lib_form in &parts[1..] {
                    import_one(lib_form, lib_env)?;
                }
            }
            "begin" => {
                for body in &parts[1..] {
                    eval(body.clone(), lib_env.clone())?;
                }
            }
            "include" => {
                for path_val in &parts[1..] {
                    let Value::String(p) = path_val else {
                        return Err(EvalError::malformed("include", "argument must be a string"));
                    };
                    let source = std::fs::read_to_string(&*p.borrow()).map_err(|e| {
                        EvalError::malformed("include", format!("read {}: {e}", p.borrow()))
                    })?;
                    eval_source(&source, lib_env.clone())?;
                }
            }
            "cond-expand" => {
                let chosen = pick_cond_expand_branch(&parts[1..])?;
                if let Some(branch_decls) = chosen {
                    process_library_decls(&branch_decls, lib_env, exports)?;
                }
            }
            other => {
                return Err(EvalError::malformed(
                    "define-library",
                    format!("unknown library declaration: {other}"),
                ));
            }
        }
    }
    Ok(())
}

fn step_import(tail: Value, env: EnvRef) -> Result<Step, EvalError> {
    let libs = collect_list(&tail)
        .map_err(|()| EvalError::malformed("import", "import list must be proper"))?;
    for lib_form in libs {
        import_one(&lib_form, &env)?;
    }
    Ok(Step::Return(Value::Unspecified))
}

fn import_one(lib_form: &Value, target: &EnvRef) -> Result<(), EvalError> {
    let lib_name = crate::library::parse_library_name(lib_form)?;
    if crate::library::is_builtin_library(&lib_name) {
        // Bindings already installed by install_base.
        return Ok(());
    }
    let bindings = crate::library::library_bindings(&lib_name).ok_or_else(|| {
        EvalError::malformed(
            "import",
            format!("unknown library: ({})", lib_name.join(" ")),
        )
    })?;
    for (name, value) in bindings {
        target.define(name, value);
    }
    Ok(())
}

/// `(apply proc a1 a2 ... arglist)` — spreads `arglist` as the trailing
/// arguments and applies `proc`. Implemented as a special form so we
/// don't need a primitive-side trampoline.
fn step_apply_form(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let items = collect_list(&tail)
        .map_err(|()| EvalError::malformed("apply", "argument list must be proper"))?;
    if items.is_empty() {
        return Err(EvalError::malformed(
            "apply",
            "expected at least proc and arglist",
        ));
    }
    let proc_expr = items[0].clone();
    let inner_args = items[1..].to_vec();
    if inner_args.is_empty() {
        // (apply proc) with no further args is a corner case;
        // R7RS requires at least one arg-list. Treat (apply proc) as
        // applying proc to (). This is permissive but harmless.
        frames.push(Frame::ApplySpread {
            evaluated: Vec::new(),
            remaining: Vec::new(),
            env: env.clone(),
        });
        return Ok(Step::Eval(proc_expr, env));
    }
    // Push an ApplySpread frame that will, after evaluating the proc
    // and each arg, treat the last evaluated arg as the spread list.
    frames.push(Frame::ApplySpread {
        evaluated: Vec::new(),
        remaining: inner_args,
        env: env.clone(),
    });
    Ok(Step::Eval(proc_expr, env))
}

/// `(parameterize ((param value) ...) body...)` — dynamically rebind
/// each `param` to `value` for the dynamic extent of `body`. Uses
/// synchronous evaluation for the binding expressions (the typical
/// case — they're short — and avoids growing the frame enum further
/// with intermediate states).
fn step_parameterize(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    use std::rc::Rc as StdRc;
    let (bindings_form, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("parameterize", "expected bindings and body"))?;
    let bindings = collect_list(&bindings_form)
        .map_err(|()| EvalError::malformed("parameterize", "bindings must be a proper list"))?;
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("parameterize", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed(
            "parameterize",
            "body must not be empty",
        ));
    }

    let mut saved: Vec<(StdRc<crate::value::ParameterCell>, Value)> = Vec::new();
    for binding in bindings {
        let parts = collect_list(&binding)
            .map_err(|()| EvalError::malformed("parameterize", "binding must be a list"))?;
        if parts.len() != 2 {
            return Err(EvalError::malformed(
                "parameterize",
                "binding must be (param value)",
            ));
        }
        // Evaluate the param expression synchronously.
        let param_val = eval(parts[0].clone(), env.clone())?;
        let cell = match &param_val {
            Value::Procedure(p) => match &**p {
                Procedure::Parameter { cell } => cell.clone(),
                _ => {
                    return Err(EvalError::malformed(
                        "parameterize",
                        "first item of each binding must be a parameter object",
                    ));
                }
            },
            _ => {
                return Err(EvalError::malformed(
                    "parameterize",
                    "first item of each binding must be a parameter object",
                ));
            }
        };
        let new_val = eval(parts[1].clone(), env.clone())?;
        // Save current value, install new.
        let old = cell.value.borrow().clone();
        *cell.value.borrow_mut() = new_val;
        saved.push((cell, old));
    }

    frames.push(Frame::ParameterRestore { saved });
    Ok(eval_sequence(body, env, frames))
}

/// `(let-values (((vars...) expr) ...) body...)` /
/// `(let*-values ...)`. Desugars to nested
/// `call-with-values + lambda`. `sequential = true` for `let*-values`
/// just means each binding sees the bindings before it, which our
/// nested-lambda desugaring gets for free.
fn step_let_values(
    tail: Value,
    env: EnvRef,
    frames: &mut Vec<Frame>,
    sequential: bool,
) -> Result<Step, EvalError> {
    let _ = sequential; // both forms use the same nested desugaring
    let (bindings_form, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("let-values", "expected bindings and body"))?;
    let bindings = collect_list(&bindings_form)
        .map_err(|()| EvalError::malformed("let-values", "bindings must be a proper list"))?;
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("let-values", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed("let-values", "body must not be empty"));
    }

    let mksym = |s: &str| Value::Symbol(Symbol::intern(s));
    let body_expr = if body.len() == 1 {
        body.into_iter().next().unwrap()
    } else {
        let mut begin = vec![mksym("begin")];
        begin.extend(body);
        Value::list_from(begin)
    };

    // Build nested call-with-values, innermost first.
    let mut acc = body_expr;
    for binding in bindings.into_iter().rev() {
        // Each binding is ((vars...) producer-expr)
        let parts = collect_list(&binding)
            .map_err(|()| EvalError::malformed("let-values", "binding must be a list"))?;
        if parts.len() != 2 {
            return Err(EvalError::malformed(
                "let-values",
                "binding must be (formals producer)",
            ));
        }
        let formals = parts[0].clone();
        let producer_expr = parts[1].clone();
        // (lambda () producer-expr)
        let producer_thunk = Value::list_from([mksym("lambda"), Value::Null, producer_expr]);
        // (lambda formals acc)
        let consumer = Value::list_from([mksym("lambda"), formals, acc]);
        // (call-with-values producer-thunk consumer)
        acc = Value::list_from([mksym("call-with-values"), producer_thunk, consumer]);
    }
    let _ = frames;
    Ok(Step::Eval(acc, env))
}

/// `(define-record-type NAME (CTOR ARG-FIELDS...) PRED FIELD-SPECS...)`
/// — R7RS §5.5. Defines five-ish bindings:
///   - `CTOR`: constructor; takes the arg-fields in order, returns a fresh record.
///   - `PRED`: type predicate.
///   - one accessor (and optionally one mutator) per `FIELD-SPEC`.
fn step_define_record_type(tail: Value, env: EnvRef) -> Result<Step, EvalError> {
    let parts = collect_list(&tail)
        .map_err(|()| EvalError::malformed("define-record-type", "expected proper list"))?;
    if parts.len() < 3 {
        return Err(EvalError::malformed(
            "define-record-type",
            "expected name, constructor spec, predicate, and fields",
        ));
    }
    // Name
    let Value::Symbol(type_name) = parts[0].clone() else {
        return Err(EvalError::malformed(
            "define-record-type",
            "record type name must be a symbol",
        ));
    };
    // Constructor spec: (ctor-name arg-fields...)
    let ctor_parts = collect_list(&parts[1]).map_err(|()| {
        EvalError::malformed("define-record-type", "constructor spec must be a list")
    })?;
    if ctor_parts.is_empty() {
        return Err(EvalError::malformed(
            "define-record-type",
            "constructor spec must include a name",
        ));
    }
    let Value::Symbol(ctor_name) = ctor_parts[0].clone() else {
        return Err(EvalError::malformed(
            "define-record-type",
            "constructor name must be a symbol",
        ));
    };
    let ctor_args: Vec<Symbol> = ctor_parts[1..]
        .iter()
        .map(|v| match v {
            Value::Symbol(s) => Ok(s.clone()),
            _ => Err(EvalError::malformed(
                "define-record-type",
                "constructor field name must be a symbol",
            )),
        })
        .collect::<Result<_, _>>()?;
    // Predicate name
    let Value::Symbol(pred_name) = parts[2].clone() else {
        return Err(EvalError::malformed(
            "define-record-type",
            "predicate name must be a symbol",
        ));
    };
    // Field specs
    let mut all_fields: Vec<Symbol> = Vec::new();
    let mut accessors: Vec<(Symbol, Symbol)> = Vec::new();
    let mut mutators: Vec<(Symbol, Symbol)> = Vec::new();
    for spec in &parts[3..] {
        let s = collect_list(spec).map_err(|()| {
            EvalError::malformed("define-record-type", "field spec must be a list")
        })?;
        if s.len() < 2 || s.len() > 3 {
            return Err(EvalError::malformed(
                "define-record-type",
                "field spec must be (name accessor) or (name accessor mutator)",
            ));
        }
        let Value::Symbol(field_name) = s[0].clone() else {
            return Err(EvalError::malformed(
                "define-record-type",
                "field name must be a symbol",
            ));
        };
        let Value::Symbol(acc_name) = s[1].clone() else {
            return Err(EvalError::malformed(
                "define-record-type",
                "accessor name must be a symbol",
            ));
        };
        all_fields.push(field_name.clone());
        accessors.push((field_name.clone(), acc_name));
        if let Some(mut_form) = s.get(2) {
            let Value::Symbol(mut_name) = mut_form.clone() else {
                return Err(EvalError::malformed(
                    "define-record-type",
                    "mutator name must be a symbol",
                ));
            };
            mutators.push((field_name, mut_name));
        }
    }

    // Resolve constructor args to field indices.
    let ctor_field_indices: Vec<usize> = ctor_args
        .iter()
        .map(|arg| {
            all_fields.iter().position(|f| f == arg).ok_or_else(|| {
                EvalError::malformed(
                    "define-record-type",
                    format!("constructor field `{}` not declared", arg.name()),
                )
            })
        })
        .collect::<Result<_, _>>()?;

    let type_id = Rc::new(crate::value::RecordTypeId {
        name: type_name.name().to_string(),
    });
    let field_count = all_fields.len();

    // Constructor
    env.define(
        ctor_name,
        Value::Procedure(Rc::new(Procedure::RecordConstructor {
            type_id: type_id.clone(),
            field_count,
            ctor_field_indices,
        })),
    );
    // Predicate
    env.define(
        pred_name,
        Value::Procedure(Rc::new(Procedure::RecordPredicate {
            type_id: type_id.clone(),
        })),
    );
    // Accessors
    for (field_name, acc_name) in accessors {
        let idx = all_fields.iter().position(|f| f == &field_name).unwrap();
        env.define(
            acc_name,
            Value::Procedure(Rc::new(Procedure::RecordAccessor {
                type_id: type_id.clone(),
                field_index: idx,
            })),
        );
    }
    // Mutators
    for (field_name, mut_name) in mutators {
        let idx = all_fields.iter().position(|f| f == &field_name).unwrap();
        env.define(
            mut_name,
            Value::Procedure(Rc::new(Procedure::RecordMutator {
                type_id: type_id.clone(),
                field_index: idx,
            })),
        );
    }
    Ok(Step::Return(Value::Unspecified))
}

/// `(eval datum env-spec)` — evaluate `datum` in the env identified
/// by `env-spec` (an `(environment ...)` spec or just `#t` for the
/// global env in this v1).
///
/// In nscheme v1 the env-spec argument is largely ignored: we always
/// evaluate against the current call-site env. Proper R7RS semantics
/// would distinguish between (environment '(scheme base)), the
/// interaction-environment, and so on. Documented limitation.
fn step_eval_form(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let parts = collect_list(&tail)
        .map_err(|()| EvalError::malformed("eval", "expected (eval expr env-spec)"))?;
    if parts.is_empty() {
        return Err(EvalError::malformed(
            "eval",
            "expected at least one operand",
        ));
    }
    // Evaluate the FIRST operand (the expression-as-data) and the
    // optional env-spec; then re-evaluate the resulting datum.
    let expr_to_eval = parts[0].clone();
    let _env_spec = parts.get(1).cloned();
    // Two-step: evaluate the data argument, then in the resume push
    // the value as a new datum to evaluate. Use a small frame.
    frames.push(Frame::EvalAfter { env: env.clone() });
    Ok(Step::Eval(expr_to_eval, env))
}

/// `(define-values (formals) expr)` — evaluate `expr` (which must
/// produce zero, one, or many values) and bind the resulting values
/// to the names in `formals` in the current env.
fn step_define_values(tail: Value, env: EnvRef) -> Result<Step, EvalError> {
    let (formals_form, rest) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("define-values", "expected formals and value"))?;
    let (params, rest_param) = parse_formals(&formals_form)?;
    let mut iter = ListIter::new(rest);
    let value_expr = iter
        .next()
        .ok_or_else(|| EvalError::malformed("define-values", "expected one value expression"))??;
    if iter.next().is_some() {
        return Err(EvalError::malformed(
            "define-values",
            "expected exactly one value expression",
        ));
    }
    // Evaluate synchronously. The expression typically calls `values`
    // and we destructure into the formals.
    let result = eval(value_expr, env.clone())?;
    let values: Vec<Value> = match result {
        Value::Values(vs) => (*vs).clone(),
        single => vec![single],
    };
    let provided = values.len();
    let arity_ok = match &rest_param {
        None => provided == params.len(),
        Some(_) => provided >= params.len(),
    };
    if !arity_ok {
        return Err(EvalError::Runtime(RuntimeError::Arity {
            procedure: "define-values".into(),
            expected: if rest_param.is_some() {
                format!("at least {}", params.len())
            } else {
                format!("exactly {}", params.len())
            },
            got: provided,
        }));
    }
    let mut iter = values.into_iter();
    for p in &params {
        env.define(p.clone(), iter.next().unwrap());
    }
    if let Some(rest_sym) = rest_param {
        let leftover: Vec<Value> = iter.collect();
        env.define(rest_sym, Value::list_from(leftover));
    }
    Ok(Step::Return(Value::Unspecified))
}

/// `(case-lambda ((formals1) body1 ...) ((formals2) body2 ...) ...)`
/// constructs a multi-arity dispatch procedure (R7RS §4.2.9).
fn step_case_lambda(tail: Value, env: EnvRef) -> Result<Step, EvalError> {
    let clause_forms = collect_list(&tail)
        .map_err(|()| EvalError::malformed("case-lambda", "clause list must be proper"))?;
    let mut clauses: Vec<crate::value::CaseLambdaClause> = Vec::new();
    for c in clause_forms {
        let (formals_form, body_tail) = c.as_pair().ok_or_else(|| {
            EvalError::malformed("case-lambda", "each clause must be (formals body...)")
        })?;
        let (params, rest) = parse_formals(&formals_form)?;
        let body = collect_list(&body_tail)
            .map_err(|()| EvalError::malformed("case-lambda", "body must be a proper list"))?;
        if body.is_empty() {
            return Err(EvalError::malformed(
                "case-lambda",
                "clause body must not be empty",
            ));
        }
        clauses.push(crate::value::CaseLambdaClause { params, rest, body });
    }
    Ok(Step::Return(Value::Procedure(Rc::new(
        Procedure::CaseLambda {
            clauses,
            env,
            name: None,
        },
    ))))
}

/// `(delay expr)` — construct a promise that, when forced, will
/// evaluate `expr` in the current env (and cache the result).
fn step_delay(tail: Value, env: EnvRef) -> Result<Step, EvalError> {
    let mut iter = ListIter::new(tail);
    let expr = iter
        .next()
        .ok_or_else(|| EvalError::malformed("delay", "expected one operand"))??;
    if iter.next().is_some() {
        return Err(EvalError::malformed(
            "delay",
            "expected exactly one operand",
        ));
    }
    let state = std::cell::RefCell::new(crate::value::PromiseState::Pending {
        expr,
        env: env.clone(),
    });
    Ok(Step::Return(Value::Promise(Rc::new(state))))
}

/// `(raise expr)` / `(raise-continuable expr)` — evaluate `expr`, then
/// propagate it up the frame stack as an exception. The boolean
/// `continuable` is `true` for `raise-continuable` (R7RS §6.11), in
/// which case a handler that returns substitutes its return value for
/// the `raise` expression rather than re-raising.
fn step_raise_with_frames(
    tail: Value,
    env: EnvRef,
    continuable: bool,
    frames: &mut Vec<Frame>,
) -> Result<Step, EvalError> {
    let mut iter = ListIter::new(tail);
    let expr = iter
        .next()
        .ok_or_else(|| EvalError::malformed("raise", "expected one operand"))??;
    if iter.next().is_some() {
        return Err(EvalError::malformed(
            "raise",
            "expected exactly one operand",
        ));
    }
    frames.push(Frame::RaiseAfter { continuable });
    Ok(Step::Eval(expr, env))
}

/// `(with-exception-handler handler thunk)` — install `handler` for
/// the dynamic extent of `(thunk)`. On normal return from `thunk`,
/// the handler is discarded. On `raise`, the handler is invoked with
/// the raised value.
///
/// Strategy: synthesize a small lambda `((lambda (h t) ...) handler
/// thunk)` so handler and thunk get evaluated first. Then in the body
/// we push an `ExceptionHandler` frame holding `h` and apply `t`.
/// For clarity we do this more directly: evaluate handler, then via a
/// helper frame install + call.
fn step_with_exception_handler_real(
    tail: Value,
    env: EnvRef,
    frames: &mut Vec<Frame>,
) -> Result<Step, EvalError> {
    let mut iter = ListIter::new(tail);
    let handler_expr = iter.next().ok_or_else(|| {
        EvalError::malformed("with-exception-handler", "expected handler and thunk")
    })??;
    let thunk_expr = iter.next().ok_or_else(|| {
        EvalError::malformed("with-exception-handler", "expected handler and thunk")
    })??;
    if iter.next().is_some() {
        return Err(EvalError::malformed(
            "with-exception-handler",
            "expected exactly two operands",
        ));
    }
    // Push InstallHandler frame; first child eval is the handler.
    frames.push(Frame::InstallHandler {
        thunk_expr,
        env: env.clone(),
    });
    Ok(Step::Eval(handler_expr, env))
}

/// `(guard (cond-var clause...) body...)` — structured exception
/// handling. Desugars to:
///   (call/cc
///     (lambda (k)
///       (with-exception-handler
///         (lambda (var) (k (cond clause... (else (raise var)))))
///         (lambda () body...))))
///
/// On normal return from `body`, call/cc just returns the body's
/// value. On a raise, the handler runs the cond — its result is
/// passed back through `k` to escape the handler's dynamic extent
/// (so a re-raise via the else clause goes to the next outer handler,
/// not back into ourselves).
fn step_guard_real(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (header, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("guard", "expected (var clause...) and body"))?;
    let header_parts = collect_list(&header)
        .map_err(|()| EvalError::malformed("guard", "header must be a proper list"))?;
    if header_parts.is_empty() {
        return Err(EvalError::malformed(
            "guard",
            "header must start with a variable",
        ));
    }
    let Value::Symbol(var) = header_parts[0].clone() else {
        return Err(EvalError::malformed(
            "guard",
            "guard variable must be a symbol",
        ));
    };
    let clauses: Vec<Value> = header_parts.into_iter().skip(1).collect();
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("guard", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed("guard", "body must not be empty"));
    }

    let mksym = |s: &str| Value::Symbol(Symbol::intern(s));
    let var_v = Value::Symbol(var);

    // (cond clause... (else (raise var)))
    let mut cond_items = vec![mksym("cond")];
    cond_items.extend(clauses);
    cond_items.push(Value::list_from([
        mksym("else"),
        Value::list_from([mksym("raise"), var_v.clone()]),
    ]));
    let cond_form = Value::list_from(cond_items);

    // Body as a single expression.
    let body_expr = if body.len() == 1 {
        body.into_iter().next().unwrap()
    } else {
        let mut begin = vec![mksym("begin")];
        begin.extend(body);
        Value::list_from(begin)
    };

    let k_sym = mksym("$guard-k");
    let var_param = Value::list_from([var_v.clone()]);
    // (lambda (var) (k (cond ...)))
    let handler_lambda = Value::list_from([
        mksym("lambda"),
        var_param,
        Value::list_from([k_sym.clone(), cond_form]),
    ]);
    // (lambda () body)
    let thunk_lambda = Value::list_from([mksym("lambda"), Value::Null, body_expr]);
    // (with-exception-handler handler thunk)
    let weh = Value::list_from([
        mksym("with-exception-handler"),
        handler_lambda,
        thunk_lambda,
    ]);
    // (lambda (k) weh)
    let outer_lambda = Value::list_from([mksym("lambda"), Value::list_from([k_sym]), weh]);
    // (call/cc outer-lambda)
    let call_cc = Value::list_from([mksym("call/cc"), outer_lambda]);
    let _ = frames;
    Ok(Step::Eval(call_cc, env))
}

/// `(call/cc proc)` — capture the current continuation and apply
/// `proc` to it. The captured continuation is exactly the frame stack
/// at the moment of the call/cc; invoking the continuation later
/// replaces the live frame stack with the saved one.
fn step_call_cc(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let mut iter = ListIter::new(tail);
    let proc_expr = iter
        .next()
        .ok_or_else(|| EvalError::malformed("call/cc", "expected one operand"))??;
    if iter.next().is_some() {
        return Err(EvalError::malformed(
            "call/cc",
            "expected exactly one operand",
        ));
    }
    // Capture the frame stack BEFORE we push our CallOp. The
    // continuation represents "what was going to happen after call/cc
    // returned"; invoking it should make that happen.
    let saved_frames = frames.clone();
    let cont = Value::Procedure(Rc::new(Procedure::Continuation {
        frames: saved_frames,
    }));
    frames.push(Frame::CallOp {
        args: vec![cont],
        env: env.clone(),
    });
    Ok(Step::Eval(proc_expr, env))
}

fn step_cond_expand(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let clauses = collect_list(&tail)
        .map_err(|()| EvalError::malformed("cond-expand", "clause list must be proper"))?;
    if let Some(body) = pick_cond_expand_branch(&clauses)? {
        if body.is_empty() {
            return Ok(Step::Return(Value::Unspecified));
        }
        return Ok(eval_sequence(body, env, frames));
    }
    Ok(Step::Return(Value::Unspecified))
}

/// Walk cond-expand clauses, return the first matching clause's body
/// (as a `Vec<Value>`) or `None` if none match.
fn pick_cond_expand_branch(clauses: &[Value]) -> Result<Option<Vec<Value>>, EvalError> {
    for clause in clauses {
        let parts = collect_list(clause)
            .map_err(|()| EvalError::malformed("cond-expand", "each clause must be a list"))?;
        if parts.is_empty() {
            return Err(EvalError::malformed("cond-expand", "empty clause"));
        }
        if matches!(&parts[0], Value::Symbol(s) if s.name() == "else") {
            return Ok(Some(parts.into_iter().skip(1).collect()));
        }
        if eval_feature_req(&parts[0])? {
            return Ok(Some(parts.into_iter().skip(1).collect()));
        }
    }
    Ok(None)
}

/// Evaluate a cond-expand feature requirement.
/// Allowed forms: identifier, (library NAME), (and …), (or …), (not …).
fn eval_feature_req(req: &Value) -> Result<bool, EvalError> {
    match req {
        Value::Symbol(s) => Ok(crate::library::features().contains(&s.name())),
        Value::Pair(_) => {
            let parts = collect_list(req).map_err(|()| {
                EvalError::malformed("cond-expand", "feature must be a proper list")
            })?;
            let Value::Symbol(head) = &parts[0] else {
                return Err(EvalError::malformed(
                    "cond-expand",
                    "feature head must be identifier",
                ));
            };
            match head.name() {
                "and" => {
                    for p in &parts[1..] {
                        if !eval_feature_req(p)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                "or" => {
                    for p in &parts[1..] {
                        if eval_feature_req(p)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
                "not" => {
                    if parts.len() != 2 {
                        return Err(EvalError::malformed("cond-expand", "not takes one operand"));
                    }
                    Ok(!eval_feature_req(&parts[1])?)
                }
                "library" => {
                    if parts.len() != 2 {
                        return Err(EvalError::malformed(
                            "cond-expand",
                            "library takes one operand",
                        ));
                    }
                    let name = crate::library::parse_library_name(&parts[1])?;
                    Ok(crate::library::is_builtin_library(&name)
                        || crate::library::library_exists(&name))
                }
                other => Err(EvalError::malformed(
                    "cond-expand",
                    format!("unknown feature head: {other}"),
                )),
            }
        }
        _ => Err(EvalError::malformed(
            "cond-expand",
            "feature must be an identifier or list",
        )),
    }
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
// Derived forms (T8)
// ---------------------------------------------------------------------

/// `(let ((v e) ...) body...)` desugars to `((lambda (v ...) body...) e ...)`.
/// `(let name ((v e) ...) body...)` is named let: desugars to
/// `((letrec ((name (lambda (v ...) body...))) name) e ...)`.
fn step_let(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (first, after_first) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("let", "expected bindings and body"))?;
    if let Value::Symbol(name) = &first {
        // Named let.
        let name = name.clone();
        let (bindings_form, body_tail) = after_first
            .as_pair()
            .ok_or_else(|| EvalError::malformed("let", "named let needs bindings and body"))?;
        let bindings = parse_bindings(&bindings_form, "let")?;
        let body = collect_list(&body_tail)
            .map_err(|()| EvalError::malformed("let", "body must be a proper list"))?;
        if body.is_empty() {
            return Err(EvalError::malformed("let", "body must not be empty"));
        }
        let (vars, inits): (Vec<Symbol>, Vec<Value>) = bindings.into_iter().unzip();

        // Build (letrec ((name (lambda (vars...) body...))) (name inits...))
        let lambda_form = list_from_parts(&[
            sym("lambda"),
            Value::list_from(vars.iter().cloned().map(Value::Symbol)),
        ])
        .into_proper(body);
        let letrec_bind = Value::list_from([Value::Symbol(name.clone()), lambda_form]);
        let letrec_binds = Value::list_from([letrec_bind]);
        let call_inits: Vec<Value> = std::iter::once(Value::Symbol(name)).chain(inits).collect();
        let letrec_body = Value::list_from(call_inits);
        let letrec_form = Value::list_from([sym("letrec"), letrec_binds, letrec_body]);
        return Ok(Step::Eval(letrec_form, env));
    }
    // Plain let.
    let bindings = parse_bindings(&first, "let")?;
    let body = collect_list(&after_first)
        .map_err(|()| EvalError::malformed("let", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed("let", "body must not be empty"));
    }
    let (vars, inits): (Vec<Symbol>, Vec<Value>) = bindings.into_iter().unzip();
    // Build ((lambda (vars...) body...) inits...).
    let formals = Value::list_from(vars.into_iter().map(Value::Symbol));
    let lambda_head = list_from_parts(&[sym("lambda"), formals]).into_proper(body);
    let mut call_items = vec![lambda_head];
    call_items.extend(inits);
    let call = Value::list_from(call_items);
    // Drop frames param — we're delegating to the normal eval.
    let _ = frames;
    Ok(Step::Eval(call, env))
}

/// `(let* () body) ⇒ (let () body)`.
/// `(let* ((v e) rest...) body) ⇒ (let ((v e)) (let* (rest...) body))`.
fn step_let_star(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (bindings_form, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("let*", "expected bindings and body"))?;
    let bindings = parse_bindings(&bindings_form, "let*")?;
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("let*", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed("let*", "body must not be empty"));
    }
    // Build nested lets right-to-left.
    let inner = if body.len() == 1 {
        body.into_iter().next().unwrap()
    } else {
        // Wrap body in (begin ...).
        let mut items = vec![sym("begin")];
        items.extend(body);
        Value::list_from(items)
    };
    let mut acc = inner;
    for (v, e) in bindings.into_iter().rev() {
        let one_bind = Value::list_from([Value::Symbol(v), e]);
        let binds = Value::list_from([one_bind]);
        acc = Value::list_from([sym("let"), binds, acc]);
    }
    let _ = frames;
    Ok(Step::Eval(acc, env))
}

/// `(letrec ((v e) ...) body) ⇒
///  (let ((v <unspec>) ...) (set! v e) ... body)`.
/// `letrec*` is treated the same here: the bindings are evaluated in
/// source order, which is permitted by R7RS for both forms.
fn step_letrec(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (bindings_form, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("letrec", "expected bindings and body"))?;
    let bindings = parse_bindings(&bindings_form, "letrec")?;
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("letrec", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed("letrec", "body must not be empty"));
    }
    // Outer (let ((v <unspec>) ...) ...). The placeholder is
    // (quote <undefined-letrec-binding>) — a unique symbol so any code
    // that observes it before the set! ran raises a clear error.
    let placeholder = Value::list_from([sym("quote"), sym("<undefined-letrec-binding>")]);
    let undef_binds: Vec<Value> = bindings
        .iter()
        .map(|(v, _)| Value::list_from([Value::Symbol(v.clone()), placeholder.clone()]))
        .collect();
    let undef_binds_list = Value::list_from(undef_binds);

    // Body: (set! v e) ... body...
    let mut sets: Vec<Value> = bindings
        .into_iter()
        .map(|(v, e)| Value::list_from([sym("set!"), Value::Symbol(v), e]))
        .collect();
    sets.extend(body);
    let outer_body = sets;

    let mut let_items = vec![sym("let"), undef_binds_list];
    let_items.extend(outer_body);
    let let_form = Value::list_from(let_items);
    let _ = frames;
    Ok(Step::Eval(let_form, env))
}

/// `(when test body...) ⇒ (if test (begin body...))`. The if-without-alt
/// returns `Unspecified` on `#f`, matching R7RS §4.2.1.
fn step_when(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (test, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("when", "expected test and body"))?;
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("when", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed("when", "body must not be empty"));
    }
    let mut begin_items = vec![sym("begin")];
    begin_items.extend(body);
    let begin_form = Value::list_from(begin_items);
    let if_form = Value::list_from([sym("if"), test, begin_form]);
    let _ = frames;
    Ok(Step::Eval(if_form, env))
}

/// `(unless test body...) ⇒ (if test (if #f #f) (begin body...))`.
/// `(if #f #f)` is the conventional Scheme "unspecified" — both arms
/// fall through, so the if returns Unspecified.
fn step_unless(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (test, body_tail) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("unless", "expected test and body"))?;
    let body = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("unless", "body must be a proper list"))?;
    if body.is_empty() {
        return Err(EvalError::malformed("unless", "body must not be empty"));
    }
    let mut begin_items = vec![sym("begin")];
    begin_items.extend(body);
    let begin_form = Value::list_from(begin_items);
    let unspec_form = Value::list_from([sym("if"), Value::Bool(false), Value::Bool(false)]);
    let if_form = Value::list_from([sym("if"), test, unspec_form, begin_form]);
    let _ = frames;
    Ok(Step::Eval(if_form, env))
}

/// `(cond clause...)`. Walks clauses one at a time, evaluating each
/// test under a [`Frame::CondClause`].
fn step_cond(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let clauses = collect_list(&tail)
        .map_err(|()| EvalError::malformed("cond", "clause list must be proper"))?;
    cond_dispatch(clauses, env, frames)
}

fn cond_dispatch(
    mut clauses: Vec<Value>,
    env: EnvRef,
    frames: &mut Vec<Frame>,
) -> Result<Step, EvalError> {
    if clauses.is_empty() {
        return Ok(Step::Return(Value::Unspecified));
    }
    let clause = clauses.remove(0);
    let parts = collect_list(&clause)
        .map_err(|()| EvalError::malformed("cond", "clause must be a list"))?;
    if parts.is_empty() {
        return Err(EvalError::malformed("cond", "empty clause"));
    }
    // (else body...)
    if matches!(&parts[0], Value::Symbol(s) if s.name() == "else") {
        if parts.len() == 1 {
            return Err(EvalError::malformed("cond", "else clause needs a body"));
        }
        let body = parts.into_iter().skip(1).collect::<Vec<_>>();
        return Ok(eval_sequence(body, env, frames));
    }
    // Normal clause: evaluate the test, then dispatch.
    let test = parts[0].clone();
    frames.push(Frame::CondClause {
        clause: Value::list_from(parts),
        remaining_clauses: clauses,
        env: env.clone(),
    });
    Ok(Step::Eval(test, env))
}

/// `(case key clause...)`. Each clause's keys are matched with `eqv?`
/// against the value of `key`. The `else` clause runs if no key
/// matches; clauses may also use `=>` like `cond`.
fn step_case(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (key_expr, rest) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("case", "expected key and clauses"))?;
    let clauses = collect_list(&rest)
        .map_err(|()| EvalError::malformed("case", "clause list must be proper"))?;
    // Evaluate the key by wrapping it in a let so we only evaluate
    // it once, then desugar to a cond. The key value is bound to a
    // private symbol that the user cannot reference.
    let key_var = Symbol::intern("$case-key");
    let key_ref = Value::Symbol(key_var.clone());
    let mut cond_clauses: Vec<Value> = Vec::new();
    for clause in clauses {
        let parts = collect_list(&clause)
            .map_err(|()| EvalError::malformed("case", "clause must be a list"))?;
        if parts.is_empty() {
            return Err(EvalError::malformed("case", "empty clause"));
        }
        // R7RS allows `=> proc` in both regular and else clauses.
        // For case the procedure is applied to the *key* (not the
        // test result as in cond) — so we rewrite the clause body
        // to `(proc $case-key)` rather than relying on cond's =>
        // machinery.
        let is_arrow =
            parts.len() == 3 && matches!(&parts[1], Value::Symbol(s) if s.name() == "=>");
        if matches!(&parts[0], Value::Symbol(s) if s.name() == "else") {
            if is_arrow {
                let proc = parts[2].clone();
                let call = Value::list_from([proc, key_ref.clone()]);
                cond_clauses.push(Value::list_from([sym("else"), call]));
            } else {
                cond_clauses.push(Value::list_from(parts));
            }
            continue;
        }
        let keys = collect_list(&parts[0])
            .map_err(|()| EvalError::malformed("case", "clause keys must be a list"))?;
        // Build (or (eqv? $case-key k1) (eqv? $case-key k2) ...)
        let mut or_items = vec![sym("or")];
        for k in keys {
            // Each key is a literal datum, so wrap with quote.
            let quoted = Value::list_from([sym("quote"), k]);
            or_items.push(Value::list_from([sym("eqv?"), key_ref.clone(), quoted]));
        }
        let test = Value::list_from(or_items);
        let mut new_clause = vec![test];
        if is_arrow {
            let proc = parts[2].clone();
            new_clause.push(Value::list_from([proc, key_ref.clone()]));
        } else {
            new_clause.extend(parts.into_iter().skip(1));
        }
        cond_clauses.push(Value::list_from(new_clause));
    }
    let mut cond_items = vec![sym("cond")];
    cond_items.extend(cond_clauses);
    let cond_form = Value::list_from(cond_items);

    let bind = Value::list_from([Value::Symbol(key_var), key_expr]);
    let binds = Value::list_from([bind]);
    let let_form = Value::list_from([sym("let"), binds, cond_form]);
    let _ = frames;
    Ok(Step::Eval(let_form, env))
}

/// `(and e1 ... en)` — short-circuit. Last expression is in tail
/// position.
fn step_and(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let exprs = collect_list(&tail)
        .map_err(|()| EvalError::malformed("and", "operand list must be proper"))?;
    if exprs.is_empty() {
        return Ok(Step::Return(Value::Bool(true)));
    }
    if exprs.len() == 1 {
        let only = exprs.into_iter().next().unwrap();
        return Ok(Step::Eval(only, env));
    }
    let mut iter = exprs.into_iter();
    let first = iter.next().unwrap();
    let remaining: Vec<Value> = iter.collect();
    frames.push(Frame::AndNext {
        remaining,
        env: env.clone(),
    });
    Ok(Step::Eval(first, env))
}

/// `(or e1 ... en)` — short-circuit. Last expression is in tail
/// position.
fn step_or(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let exprs = collect_list(&tail)
        .map_err(|()| EvalError::malformed("or", "operand list must be proper"))?;
    if exprs.is_empty() {
        return Ok(Step::Return(Value::Bool(false)));
    }
    if exprs.len() == 1 {
        let only = exprs.into_iter().next().unwrap();
        return Ok(Step::Eval(only, env));
    }
    let mut iter = exprs.into_iter();
    let first = iter.next().unwrap();
    let remaining: Vec<Value> = iter.collect();
    frames.push(Frame::OrNext {
        remaining,
        env: env.clone(),
    });
    Ok(Step::Eval(first, env))
}

/// `(do ((var init step) ...) (test result...) body...)`. Desugars to
/// a `letrec` loop:
/// `(letrec ((loop (lambda (v...)
///                   (if test (begin result...)
///                            (begin body... (loop step...))))))
///    (loop init...))`
fn step_do(tail: Value, env: EnvRef, frames: &mut Vec<Frame>) -> Result<Step, EvalError> {
    let (bindings_form, after_bindings) = tail
        .as_pair()
        .ok_or_else(|| EvalError::malformed("do", "expected bindings, test, body"))?;
    let (test_clause, body_tail) = after_bindings
        .as_pair()
        .ok_or_else(|| EvalError::malformed("do", "expected test clause"))?;

    // Parse var/init/step bindings.
    let binding_list = collect_list(&bindings_form)
        .map_err(|()| EvalError::malformed("do", "bindings must be a proper list"))?;
    let mut vars: Vec<Symbol> = Vec::new();
    let mut inits: Vec<Value> = Vec::new();
    let mut steps: Vec<Value> = Vec::new();
    for b in binding_list {
        let parts =
            collect_list(&b).map_err(|()| EvalError::malformed("do", "binding must be a list"))?;
        if parts.len() < 2 || parts.len() > 3 {
            return Err(EvalError::malformed(
                "do",
                "binding must be (var init) or (var init step)",
            ));
        }
        let Value::Symbol(v) = parts[0].clone() else {
            return Err(EvalError::malformed("do", "binding name must be a symbol"));
        };
        vars.push(v.clone());
        inits.push(parts[1].clone());
        let step_expr = if parts.len() == 3 {
            parts[2].clone()
        } else {
            Value::Symbol(v)
        };
        steps.push(step_expr);
    }

    // Parse test clause: (test result...).
    let test_parts = collect_list(&test_clause)
        .map_err(|()| EvalError::malformed("do", "test clause must be a proper list"))?;
    if test_parts.is_empty() {
        return Err(EvalError::malformed(
            "do",
            "test clause must include a test",
        ));
    }
    let test_expr = test_parts[0].clone();
    let result_exprs: Vec<Value> = test_parts.into_iter().skip(1).collect();
    let result_body = if result_exprs.is_empty() {
        Value::Unspecified
    } else if result_exprs.len() == 1 {
        result_exprs.into_iter().next().unwrap()
    } else {
        let mut begin = vec![sym("begin")];
        begin.extend(result_exprs);
        Value::list_from(begin)
    };

    // Parse body.
    let body_exprs = collect_list(&body_tail)
        .map_err(|()| EvalError::malformed("do", "body must be a proper list"))?;

    // Build loop body: (begin body... (loop step...))
    let loop_name = Symbol::intern("$do-loop");
    let recur_call: Vec<Value> = std::iter::once(Value::Symbol(loop_name.clone()))
        .chain(steps)
        .collect();
    let recur = Value::list_from(recur_call);
    let mut alt_items = vec![sym("begin")];
    alt_items.extend(body_exprs);
    alt_items.push(recur);
    let alt_body = if alt_items.len() == 2 {
        // Just (begin recur) — simplify to just recur.
        alt_items.pop().unwrap()
    } else {
        Value::list_from(alt_items)
    };

    let if_form = Value::list_from([sym("if"), test_expr, result_body, alt_body]);
    let formals = Value::list_from(vars.iter().cloned().map(Value::Symbol));
    let lambda_form = Value::list_from([sym("lambda"), formals, if_form]);
    let letrec_bind = Value::list_from([Value::Symbol(loop_name.clone()), lambda_form]);
    let letrec_binds = Value::list_from([letrec_bind]);
    let init_call: Vec<Value> = std::iter::once(Value::Symbol(loop_name))
        .chain(inits)
        .collect();
    let letrec_body = Value::list_from(init_call);
    let letrec_form = Value::list_from([sym("letrec"), letrec_binds, letrec_body]);
    let _ = frames;
    Ok(Step::Eval(letrec_form, env))
}

/// `(quasiquote template)` — expand the template, treating `unquote`
/// and `unquote-splicing` as escape hatches. v1 supports single-level
/// quasiquote only (no nested `` ` ``).
fn step_quasiquote(template: &Value, env: EnvRef) -> Result<Step, EvalError> {
    let mut iter = ListIter::new(template.clone());
    let body = iter
        .next()
        .ok_or_else(|| EvalError::malformed("quasiquote", "expected one template"))??;
    if iter.next().is_some() {
        return Err(EvalError::malformed(
            "quasiquote",
            "expected exactly one template",
        ));
    }
    let value = qq_expand(&body, &env)?;
    Ok(Step::Return(value))
}

fn qq_expand(template: &Value, env: &EnvRef) -> Result<Value, EvalError> {
    match template {
        Value::Pair(_) => qq_expand_pair(template, env),
        // Vectors quasiquote element-by-element.
        Value::Vector(items) => {
            let items = items.borrow().clone();
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                // Vector elements don't support unquote-splicing per R7RS.
                out.push(qq_expand(&item, env)?);
            }
            Ok(Value::vector(out))
        }
        _ => Ok(template.clone()),
    }
}

fn qq_expand_pair(template: &Value, env: &EnvRef) -> Result<Value, EvalError> {
    // Check for (unquote x) as the whole template.
    if let Some((head, tail)) = template.as_pair()
        && let Value::Symbol(s) = &head
        && s.name() == "unquote"
    {
        let mut iter = ListIter::new(tail);
        let expr = iter
            .next()
            .ok_or_else(|| EvalError::malformed("unquote", "expected one expression"))??;
        return eval(expr, env.clone());
    }
    // Otherwise, walk car and cdr.
    let (head, tail) = template.as_pair().expect("pair");
    // Check if head is (unquote-splicing x) — then splice into the
    // result of recursing on tail.
    if let Some((h_head, h_tail)) = head.as_pair()
        && let Value::Symbol(s) = &h_head
        && s.name() == "unquote-splicing"
    {
        let mut iter = ListIter::new(h_tail);
        let expr = iter
            .next()
            .ok_or_else(|| EvalError::malformed("unquote-splicing", "expected one expression"))??;
        let spliced = eval(expr, env.clone())?;
        let tail_expanded = qq_expand(&tail, env)?;
        return append_lists(spliced, tail_expanded);
    }
    let head_expanded = qq_expand(&head, env)?;
    let tail_expanded = qq_expand(&tail, env)?;
    Ok(Value::cons(head_expanded, tail_expanded))
}

fn append_lists(front: Value, back: Value) -> Result<Value, EvalError> {
    // Walk `front` and replace its tail with `back`. front must be a
    // proper list.
    let mut items: Vec<Value> = Vec::new();
    let mut cur = front;
    loop {
        match cur {
            Value::Null => break,
            Value::Pair(p) => {
                let pair = p.borrow();
                items.push(pair.car.clone());
                cur = pair.cdr.clone();
            }
            _ => {
                return Err(EvalError::malformed(
                    "unquote-splicing",
                    "spliced value must be a proper list",
                ));
            }
        }
    }
    let mut acc = back;
    for item in items.into_iter().rev() {
        acc = Value::cons(item, acc);
    }
    Ok(acc)
}

/// Parse a binding list `((v1 e1) (v2 e2) ...)`.
fn parse_bindings(form: &Value, context: &'static str) -> Result<Vec<(Symbol, Value)>, EvalError> {
    let bindings = collect_list(form)
        .map_err(|()| EvalError::malformed(context, "binding list must be a proper list"))?;
    let mut out = Vec::with_capacity(bindings.len());
    for b in bindings {
        let parts = collect_list(&b)
            .map_err(|()| EvalError::malformed(context, "each binding must be a list"))?;
        if parts.len() != 2 {
            return Err(EvalError::malformed(
                context,
                "each binding must be (name value)",
            ));
        }
        let Value::Symbol(name) = parts[0].clone() else {
            return Err(EvalError::malformed(
                context,
                "binding name must be a symbol",
            ));
        };
        out.push((name, parts[1].clone()));
    }
    Ok(out)
}

/// Helper: shortcut for building a `Symbol` value.
fn sym(name: &str) -> Value {
    Value::Symbol(Symbol::intern(name))
}

/// Helper: build a list from a slice of items, returning a constructor
/// that lets you add a body tail via `.into_proper(body_items)`.
struct ListBuilder {
    head_items: Vec<Value>,
}

impl ListBuilder {
    fn into_proper(self, body: Vec<Value>) -> Value {
        let mut items = self.head_items;
        items.extend(body);
        Value::list_from(items)
    }
}

fn list_from_parts(items: &[Value]) -> ListBuilder {
    ListBuilder {
        head_items: items.to_vec(),
    }
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
                // Arity errors are R7RS conditions too — raise so
                // handlers can catch.
                let err = RuntimeError::Arity {
                    procedure: (*name).to_string(),
                    expected: format!("{arity}"),
                    got: args.len(),
                };
                return Ok(Step::Raise(runtime_error_to_value(err), false));
            }
            match body(&args) {
                Ok(v) => Ok(Step::Return(v)),
                // R7RS §6.11: any "error" from a primitive should be
                // catchable by with-exception-handler / guard. Convert
                // the RuntimeError into an error-object value and
                // raise it.
                Err(e) => Ok(Step::Raise(runtime_error_to_value(e), false)),
            }
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
        Procedure::Continuation { frames: saved } => {
            // Invoking a captured continuation: replace the current
            // frame stack and resume by returning the supplied value.
            // R7RS allows multi-value continuations; we take the first
            // arg (or Unspecified for 0-arg invocation).
            let value = args.into_iter().next().unwrap_or(Value::Unspecified);
            Ok(Step::InvokeContinuation(saved.clone(), value))
        }
        Procedure::CaseLambda {
            clauses,
            env,
            name: _,
        } => {
            // Pick the first clause whose arity matches the call.
            let provided = args.len();
            for clause in clauses {
                let arity_ok = match &clause.rest {
                    None => provided == clause.params.len(),
                    Some(_) => provided >= clause.params.len(),
                };
                if !arity_ok {
                    continue;
                }
                let call_env = Env::extend(env.clone());
                let mut args_iter = args.into_iter();
                for p in &clause.params {
                    call_env.define(p.clone(), args_iter.next().unwrap());
                }
                if let Some(rest_sym) = &clause.rest {
                    let leftover: Vec<Value> = args_iter.collect();
                    call_env.define(rest_sym.clone(), Value::list_from(leftover));
                }
                return Ok(eval_sequence(clause.body.clone(), call_env, frames));
            }
            Err(EvalError::Runtime(RuntimeError::Arity {
                procedure: "case-lambda".into(),
                expected: "any matching clause".into(),
                got: provided,
            }))
        }
        Procedure::RecordConstructor {
            type_id,
            field_count,
            ctor_field_indices,
        } => {
            if args.len() != ctor_field_indices.len() {
                return Err(EvalError::Runtime(RuntimeError::Arity {
                    procedure: format!("{}-constructor", type_id.name),
                    expected: format!("exactly {}", ctor_field_indices.len()),
                    got: args.len(),
                }));
            }
            // Initialize all fields to Unspecified, then place each
            // arg at its mapped field index.
            let fields: Vec<std::cell::RefCell<Value>> = (0..*field_count)
                .map(|_| std::cell::RefCell::new(Value::Unspecified))
                .collect();
            for (arg, &idx) in args.into_iter().zip(ctor_field_indices.iter()) {
                *fields[idx].borrow_mut() = arg;
            }
            Ok(Step::Return(Value::Record {
                type_id: type_id.clone(),
                fields: Rc::new(fields),
            }))
        }
        Procedure::RecordPredicate { type_id } => {
            if args.len() != 1 {
                return Err(EvalError::Runtime(RuntimeError::Arity {
                    procedure: format!("{}?", type_id.name),
                    expected: "exactly 1".into(),
                    got: args.len(),
                }));
            }
            let matches_type = matches!(
                &args[0],
                Value::Record { type_id: tid, .. } if Rc::ptr_eq(tid, type_id)
            );
            Ok(Step::Return(Value::Bool(matches_type)))
        }
        Procedure::RecordAccessor {
            type_id,
            field_index,
        } => {
            if args.len() != 1 {
                return Err(EvalError::Runtime(RuntimeError::Arity {
                    procedure: format!("{}-accessor", type_id.name),
                    expected: "exactly 1".into(),
                    got: args.len(),
                }));
            }
            match &args[0] {
                Value::Record {
                    type_id: tid,
                    fields,
                } if Rc::ptr_eq(tid, type_id) => {
                    Ok(Step::Return(fields[*field_index].borrow().clone()))
                }
                other => Err(EvalError::Runtime(RuntimeError::Type {
                    expected: format!("record of type {}", type_id.name),
                    got: other.type_name().into(),
                })),
            }
        }
        Procedure::RecordMutator {
            type_id,
            field_index,
        } => {
            if args.len() != 2 {
                return Err(EvalError::Runtime(RuntimeError::Arity {
                    procedure: format!("{}-mutator", type_id.name),
                    expected: "exactly 2".into(),
                    got: args.len(),
                }));
            }
            let new_val = args[1].clone();
            match &args[0] {
                Value::Record {
                    type_id: tid,
                    fields,
                } if Rc::ptr_eq(tid, type_id) => {
                    *fields[*field_index].borrow_mut() = new_val;
                    Ok(Step::Return(Value::Unspecified))
                }
                other => Err(EvalError::Runtime(RuntimeError::Type {
                    expected: format!("record of type {}", type_id.name),
                    got: other.type_name().into(),
                })),
            }
        }
        Procedure::Parameter { cell } => {
            // R7RS: (param) returns the current value; (param new)
            // sets it (parameterize uses this internally but it's
            // also the public setter).
            match args.len() {
                0 => Ok(Step::Return(cell.value.borrow().clone())),
                1 => {
                    *cell.value.borrow_mut() = args.into_iter().next().unwrap();
                    Ok(Step::Return(Value::Unspecified))
                }
                n => Err(EvalError::Runtime(RuntimeError::Arity {
                    procedure: "parameter".into(),
                    expected: "0 or 1".into(),
                    got: n,
                })),
            }
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
        Frame::CondClause {
            clause,
            remaining_clauses,
            env,
        } => {
            // `value` is the result of evaluating the clause's test.
            let parts = collect_list(&clause)
                .map_err(|()| EvalError::malformed("cond", "clause must be a list"))?;
            // parts[0] is the test, parts[1..] is the body (or => proc).
            if value.is_truthy() {
                if parts.len() == 1 {
                    // Single-test clause: return the test value.
                    return Ok(Step::Return(value));
                }
                // (test => proc-expr)
                if parts.len() == 3 && matches!(&parts[1], Value::Symbol(s) if s.name() == "=>") {
                    let proc_expr = parts[2].clone();
                    frames.push(Frame::CondArrow {
                        test_value: value,
                        env: env.clone(),
                    });
                    return Ok(Step::Eval(proc_expr, env));
                }
                // (test body...)
                let body: Vec<Value> = parts.into_iter().skip(1).collect();
                return Ok(eval_sequence(body, env, frames));
            }
            // Falsy — try the next clause.
            cond_dispatch(remaining_clauses, env, frames)
        }
        Frame::CondArrow { test_value, env } => {
            // `value` is the => procedure. Apply it to the test value.
            // Apply is in tail position: no frame pushed here.
            let _ = env;
            Ok(Step::Apply(value, vec![test_value]))
        }
        Frame::AndNext { mut remaining, env } => {
            // Short-circuit: if the just-evaluated expression is #f,
            // return #f without evaluating the rest.
            if !value.is_truthy() {
                return Ok(Step::Return(value));
            }
            // Otherwise: continue. If only one expr left, that's tail
            // position — Eval without re-pushing.
            if remaining.is_empty() {
                // Shouldn't happen — step_and never pushes AndNext when
                // there are 0 remaining. But guard anyway.
                return Ok(Step::Return(value));
            }
            let next = remaining.remove(0);
            if !remaining.is_empty() {
                frames.push(Frame::AndNext {
                    remaining,
                    env: env.clone(),
                });
            }
            Ok(Step::Eval(next, env))
        }
        Frame::OrNext { mut remaining, env } => {
            // Short-circuit: if truthy, return the value immediately.
            if value.is_truthy() {
                return Ok(Step::Return(value));
            }
            if remaining.is_empty() {
                return Ok(Step::Return(value));
            }
            let next = remaining.remove(0);
            if !remaining.is_empty() {
                frames.push(Frame::OrNext {
                    remaining,
                    env: env.clone(),
                });
            }
            Ok(Step::Eval(next, env))
        }
        Frame::ExceptionHandler { .. } => {
            // The protected expression returned normally; discard the
            // handler.
            Ok(Step::Return(value))
        }
        Frame::ReRaise => Ok(Step::Raise(value, false)),
        Frame::RaiseAfter { continuable } => Ok(Step::Raise(value, continuable)),
        Frame::EvalAfter { env } => {
            // `value` is the datum produced by evaluating eval's
            // first argument; re-evaluate it as code in the captured
            // env. No new frame: this is in tail position relative
            // to the caller of `eval`.
            Ok(Step::Eval(value, env))
        }
        Frame::ParameterRestore { saved } => {
            // Body returned; restore the saved parameter values and
            // pass the body's value through.
            for (cell, old) in saved {
                *cell.value.borrow_mut() = old;
            }
            Ok(Step::Return(value))
        }
        Frame::InstallHandler { thunk_expr, env } => {
            // `value` is the evaluated handler. Install it and call
            // (thunk).
            frames.push(Frame::ExceptionHandler {
                handler: value,
                env: env.clone(),
            });
            // Construct (thunk) as a call form.
            let call_form = Value::list_from([thunk_expr]);
            Ok(Step::Eval(call_form, env))
        }
        Frame::ApplySpread {
            mut evaluated,
            mut remaining,
            env,
        } => {
            // First time we hit this frame, `value` is the procedure;
            // subsequent times it's an arg. Push the just-arrived
            // value onto `evaluated`.
            evaluated.push(value);
            if remaining.is_empty() {
                // All exprs have been evaluated. evaluated[0] is the
                // procedure; evaluated[1..n-1] are leading args;
                // evaluated[n-1] is the list to spread.
                let mut final_args: Vec<Value> = evaluated.iter().skip(1).cloned().collect();
                // If there's nothing to spread (only proc), just apply
                // to empty args.
                if let Some(last) = final_args.pop() {
                    // Walk the list and append its elements.
                    let mut cur = last;
                    loop {
                        match cur {
                            Value::Null => break,
                            Value::Pair(p) => {
                                let pair = p.borrow();
                                final_args.push(pair.car.clone());
                                cur = pair.cdr.clone();
                            }
                            _ => {
                                return Err(EvalError::malformed(
                                    "apply",
                                    "last argument must be a proper list",
                                ));
                            }
                        }
                    }
                }
                let proc = evaluated.into_iter().next().unwrap();
                return Ok(Step::Apply(proc, final_args));
            }
            let next = remaining.remove(0);
            frames.push(Frame::ApplySpread {
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
                            .ok_or_else(|| RuntimeError::Other("integer overflow in +".into()))?;
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
                            .ok_or_else(|| RuntimeError::Other("integer overflow in -".into()))?;
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
                            .ok_or_else(|| RuntimeError::Other("integer overflow in *".into()))?;
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
        intern_def(&env, "eqv?", Arity::Exact(2), |args| {
            Ok(Value::Bool(crate::value::eqv(&args[0], &args[1])))
        });
        intern_def(&env, "car", Arity::Exact(1), |args| match &args[0] {
            Value::Pair(p) => Ok(p.borrow().car.clone()),
            other => Err(RuntimeError::Type {
                expected: "pair".into(),
                got: other.type_name().into(),
            }),
        });
        intern_def(&env, "cdr", Arity::Exact(1), |args| match &args[0] {
            Value::Pair(p) => Ok(p.borrow().cdr.clone()),
            other => Err(RuntimeError::Type {
                expected: "pair".into(),
                got: other.type_name().into(),
            }),
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

    // -- T8 derived forms -------------------------------------------

    // -- let, let*, letrec ------------------------------------------

    #[test]
    fn let_binds_locally() {
        assert!(equal(
            &run("(let ((x 1) (y 2)) (+ x y))").unwrap(),
            &Value::Int(3)
        ));
    }

    #[test]
    fn let_bindings_evaluated_in_outer_scope() {
        // `y` in the binding for `b` refers to the outer `y`, not the let's `y`.
        let src = "(define y 10) (let ((y 99) (z y)) z)";
        assert!(equal(&run(src).unwrap(), &Value::Int(10)));
    }

    #[test]
    fn let_star_sees_earlier_bindings() {
        let src = "(let* ((x 1) (y (+ x 1)) (z (+ y 1))) z)";
        assert!(equal(&run(src).unwrap(), &Value::Int(3)));
    }

    #[test]
    fn letrec_mutual_recursion() {
        let src = "(letrec ((even? (lambda (n) (if (= n 0) #t (odd? (- n 1)))))
                            (odd?  (lambda (n) (if (= n 0) #f (even? (- n 1))))))
                     (even? 6))";
        assert!(equal(&run(src).unwrap(), &Value::Bool(true)));
    }

    #[test]
    fn named_let_loop() {
        // (let loop ((i 0) (acc 0)) (if (= i 5) acc (loop (+ i 1) (+ acc i))))
        let src = "(let loop ((i 0) (acc 0)) (if (= i 5) acc (loop (+ i 1) (+ acc i))))";
        // 0+1+2+3+4 = 10
        assert!(equal(&run(src).unwrap(), &Value::Int(10)));
    }

    // -- cond -------------------------------------------------------

    #[test]
    fn cond_picks_first_truthy_branch() {
        assert!(equal(
            &run("(cond (#f 1) (#t 2) (else 3))").unwrap(),
            &Value::Int(2),
        ));
    }

    #[test]
    fn cond_falls_through_to_else() {
        assert!(equal(
            &run("(cond (#f 1) (#f 2) (else 99))").unwrap(),
            &Value::Int(99),
        ));
    }

    #[test]
    fn cond_returns_unspecified_when_no_match() {
        let v = run("(cond (#f 1) (#f 2))").unwrap();
        assert!(matches!(v, Value::Unspecified));
    }

    #[test]
    fn cond_single_test_clause_returns_test_value() {
        assert!(equal(
            &run("(cond ((+ 1 2)) (else 99))").unwrap(),
            &Value::Int(3),
        ));
    }

    #[test]
    fn cond_arrow_applies_proc_to_test_value() {
        // (cond ((list 1 2) => cdr) (else 'no))
        // cdr of (1 2) is (2)
        let v = run("(cond ((list 1 2) => cdr) (else 'no))").unwrap();
        let expected = Value::list_from([Value::Int(2)]);
        assert!(equal(&v, &expected));
    }

    #[test]
    fn cond_test_evaluated_only_once_for_arrow() {
        // If the test was evaluated twice, the counter would be 2.
        let src = "(define counter 0)
                   (define (bump) (set! counter (+ counter 1)) (list counter))
                   (cond ((bump) => car) (else 'no))
                   counter";
        // bump increments counter to 1, returns (1). The => applies car to (1) → 1.
        // counter at end should be 1, not 2.
        assert!(equal(&run(src).unwrap(), &Value::Int(1)));
    }

    // -- case -------------------------------------------------------

    #[test]
    fn case_matches_first_key_list() {
        assert!(equal(
            &run("(case 2 ((1 2 3) 'low) ((4 5) 'high) (else 'other))").unwrap(),
            &Value::Symbol(Symbol::intern("low")),
        ));
    }

    #[test]
    fn case_falls_through_to_else() {
        assert!(equal(
            &run("(case 99 ((1 2) 'a) ((3 4) 'b) (else 'other))").unwrap(),
            &Value::Symbol(Symbol::intern("other")),
        ));
    }

    #[test]
    fn case_uses_eqv_not_equal() {
        // (case (list 1 2 3) (((1 2 3)) 'a) (else 'b))
        // The clause key is the literal list (1 2 3) which is NOT eqv?
        // to a freshly-allocated list. So we should hit else.
        let v = run("(case (list 1 2 3) (((1 2 3)) 'a) (else 'b))").unwrap();
        assert!(equal(&v, &Value::Symbol(Symbol::intern("b"))));
    }

    // -- and / or ---------------------------------------------------

    #[test]
    fn empty_and_is_true() {
        assert!(equal(&run("(and)").unwrap(), &Value::Bool(true)));
    }

    #[test]
    fn empty_or_is_false() {
        assert!(equal(&run("(or)").unwrap(), &Value::Bool(false)));
    }

    #[test]
    fn and_returns_last_truthy_value() {
        // (and 1 2 3) -> 3
        assert!(equal(&run("(and 1 2 3)").unwrap(), &Value::Int(3)));
    }

    #[test]
    fn and_short_circuits_on_false() {
        assert!(equal(&run("(and 1 #f 3)").unwrap(), &Value::Bool(false)));
    }

    #[test]
    fn or_returns_first_truthy_value() {
        assert!(equal(&run("(or #f #f 7)").unwrap(), &Value::Int(7)));
    }

    #[test]
    fn or_returns_last_falsy_when_all_false() {
        assert!(equal(&run("(or #f #f)").unwrap(), &Value::Bool(false)));
    }

    // -- when / unless ----------------------------------------------

    #[test]
    fn when_runs_body_when_true() {
        let src = "(define x 0) (when #t (set! x 99)) x";
        assert!(equal(&run(src).unwrap(), &Value::Int(99)));
    }

    #[test]
    fn when_skips_body_when_false() {
        let src = "(define x 0) (when #f (set! x 99)) x";
        assert!(equal(&run(src).unwrap(), &Value::Int(0)));
    }

    #[test]
    fn unless_inverse_of_when() {
        let src = "(define x 0) (unless #f (set! x 99)) x";
        assert!(equal(&run(src).unwrap(), &Value::Int(99)));
        let src = "(define x 0) (unless #t (set! x 99)) x";
        assert!(equal(&run(src).unwrap(), &Value::Int(0)));
    }

    // -- do ---------------------------------------------------------

    #[test]
    fn do_sums_first_n() {
        let src = "(do ((i 0 (+ i 1)) (acc 0 (+ acc i)))
                       ((= i 10) acc))";
        // sum 0..9 = 45
        assert!(equal(&run(src).unwrap(), &Value::Int(45)));
    }

    #[test]
    fn do_with_no_step_keeps_var() {
        // step omitted defaults to var itself (unchanged).
        let src = "(do ((i 5) (n 0 (+ n 1)))
                       ((= n 3) i))";
        assert!(equal(&run(src).unwrap(), &Value::Int(5)));
    }

    // -- quasiquote -------------------------------------------------

    #[test]
    fn quasiquote_with_no_unquote_is_just_quote() {
        let v = run("`(1 2 3)").unwrap();
        let expected = Value::list_from([Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert!(equal(&v, &expected));
    }

    #[test]
    fn quasiquote_unquote_evaluates_inserted_value() {
        let v = run("(define x 99) `(1 ,x 3)").unwrap();
        let expected = Value::list_from([Value::Int(1), Value::Int(99), Value::Int(3)]);
        assert!(equal(&v, &expected));
    }

    #[test]
    fn quasiquote_unquote_splicing_appends_list() {
        let v = run("`(1 ,@(list 2 3) 4)").unwrap();
        let expected =
            Value::list_from([Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)]);
        assert!(equal(&v, &expected));
    }
}
