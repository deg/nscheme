//! R7RS-small base-library primitives.
//!
//! This module installs a curated subset of the procedures from the
//! `(scheme base)` library — enough for the REPL (`nscheme-qkh`) to
//! support real programs. Bigger or specialized libraries land in their
//! own beads:
//!
//! - Numeric tower (bignum / rational / full type ladder) — T12
//!   (`nscheme-c92`).
//! - String / char / vector / bytevector ops beyond the minimum — T13
//!   (`nscheme-d7c`).
//! - I/O and ports — T14 (`nscheme-wcy`).
//!
//! Entry point: [`install_base`] adds the primitives and a small
//! bootstrap of higher-order procedures (`map`, `for-each`) defined in
//! Scheme on top of the primitives.

// install_* functions are intentionally long — each is a flat
// registration table for a category of primitives. Splitting them
// further would just multiply boilerplate.
#![allow(clippy::too_many_lines)]

use std::rc::Rc;

use crate::env::EnvRef;
use crate::eval::{EvalError, eval_source};
use crate::value::{
    Arity, PrimitiveFn, Procedure, RuntimeError, Symbol, Value, eq as v_eq, equal as v_equal,
    eqv as v_eqv,
};

/// Install the base-library primitives plus a Scheme bootstrap. Call
/// this on a fresh [`crate::env::Env::new_global`] before evaluating
/// user code.
pub fn install_base(env: &EnvRef) -> Result<(), EvalError> {
    install_arithmetic(env);
    install_comparison(env);
    install_predicates(env);
    install_equality(env);
    install_list_ops(env);
    install_misc(env);
    eval_source(BOOTSTRAP, env.clone())?;
    Ok(())
}

fn define(env: &EnvRef, name: &'static str, arity: Arity, body: PrimitiveFn) {
    let p = Procedure::Primitive { name, arity, body };
    env.define(Symbol::intern(name), Value::Procedure(Rc::new(p)));
}

// ---------------------------------------------------------------------
// Numeric helper
// ---------------------------------------------------------------------

/// Helper representation for numeric ops: either a fixnum or a float.
/// Promotion follows R7RS: mixing exact and inexact produces inexact.
#[derive(Clone, Copy, Debug)]
enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    fn from_value(v: &Value) -> Result<Self, RuntimeError> {
        match v {
            Value::Int(n) => Ok(Self::Int(*n)),
            Value::Float(f) => Ok(Self::Float(*f)),
            other => Err(RuntimeError::Type {
                expected: "number".into(),
                got: other.type_name().into(),
            }),
        }
    }

    fn into_value(self) -> Value {
        match self {
            Self::Int(n) => Value::Int(n),
            Self::Float(f) => Value::Float(f),
        }
    }

    fn to_f64(self) -> f64 {
        match self {
            #[allow(clippy::cast_precision_loss)]
            Self::Int(n) => n as f64,
            Self::Float(f) => f,
        }
    }

    /// Promote two numbers to a common shape. Returns `(Int, Int)` only
    /// if both are exact.
    fn promote(a: Self, b: Self) -> (Self, Self) {
        match (a, b) {
            (Self::Int(_), Self::Int(_)) => (a, b),
            _ => (Self::Float(a.to_f64()), Self::Float(b.to_f64())),
        }
    }

    fn add(a: Self, b: Self) -> Result<Self, RuntimeError> {
        Ok(match Self::promote(a, b) {
            (Self::Int(x), Self::Int(y)) => {
                Self::Int(x.checked_add(y).ok_or(RuntimeError::Overflow { op: "+" })?)
            }
            (Self::Float(x), Self::Float(y)) => Self::Float(x + y),
            _ => unreachable!(),
        })
    }

    fn sub(a: Self, b: Self) -> Result<Self, RuntimeError> {
        Ok(match Self::promote(a, b) {
            (Self::Int(x), Self::Int(y)) => {
                Self::Int(x.checked_sub(y).ok_or(RuntimeError::Overflow { op: "-" })?)
            }
            (Self::Float(x), Self::Float(y)) => Self::Float(x - y),
            _ => unreachable!(),
        })
    }

    fn mul(a: Self, b: Self) -> Result<Self, RuntimeError> {
        Ok(match Self::promote(a, b) {
            (Self::Int(x), Self::Int(y)) => {
                Self::Int(x.checked_mul(y).ok_or(RuntimeError::Overflow { op: "*" })?)
            }
            (Self::Float(x), Self::Float(y)) => Self::Float(x * y),
            _ => unreachable!(),
        })
    }

    fn div(a: Self, b: Self) -> Result<Self, RuntimeError> {
        match Self::promote(a, b) {
            (Self::Int(_), Self::Int(0)) | (Self::Float(_), Self::Float(0.0)) => {
                Err(RuntimeError::DivisionByZero)
            }
            (Self::Int(x), Self::Int(y)) => {
                if x % y == 0 {
                    Ok(Self::Int(x / y))
                } else {
                    // R7RS: integer division that doesn't divide
                    // evenly produces an exact rational. We don't have
                    // rationals in v1, so promote to inexact float.
                    // The numeric-tower bead (nscheme-c92) will fix.
                    #[allow(clippy::cast_precision_loss)]
                    Ok(Self::Float(x as f64 / y as f64))
                }
            }
            (Self::Float(x), Self::Float(y)) => Ok(Self::Float(x / y)),
            _ => unreachable!(),
        }
    }

    fn cmp(a: Self, b: Self) -> std::cmp::Ordering {
        let (a, b) = Self::promote(a, b);
        match (a, b) {
            (Self::Int(x), Self::Int(y)) => x.cmp(&y),
            (Self::Float(x), Self::Float(y)) => {
                x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Less)
            }
            _ => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------
// Arithmetic
// ---------------------------------------------------------------------

fn install_arithmetic(env: &EnvRef) {
    define(env, "+", Arity::AtLeast(0), |args| {
        if args.is_empty() {
            return Ok(Value::Int(0));
        }
        let mut acc = Num::from_value(&args[0])?;
        for a in &args[1..] {
            acc = Num::add(acc, Num::from_value(a)?)?;
        }
        Ok(acc.into_value())
    });
    define(env, "-", Arity::AtLeast(1), |args| {
        let first = Num::from_value(&args[0])?;
        if args.len() == 1 {
            // Unary negation.
            return Ok(match first {
                Num::Int(n) => Num::Int(n.checked_neg().ok_or(RuntimeError::Overflow { op: "-" })?),
                Num::Float(f) => Num::Float(-f),
            }
            .into_value());
        }
        let mut acc = first;
        for a in &args[1..] {
            acc = Num::sub(acc, Num::from_value(a)?)?;
        }
        Ok(acc.into_value())
    });
    define(env, "*", Arity::AtLeast(0), |args| {
        if args.is_empty() {
            return Ok(Value::Int(1));
        }
        let mut acc = Num::from_value(&args[0])?;
        for a in &args[1..] {
            acc = Num::mul(acc, Num::from_value(a)?)?;
        }
        Ok(acc.into_value())
    });
    define(env, "/", Arity::AtLeast(1), |args| {
        let first = Num::from_value(&args[0])?;
        if args.len() == 1 {
            return Num::div(Num::Int(1), first).map(Num::into_value);
        }
        let mut acc = first;
        for a in &args[1..] {
            acc = Num::div(acc, Num::from_value(a)?)?;
        }
        Ok(acc.into_value())
    });
    define(env, "quotient", Arity::Exact(2), |args| {
        let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else {
            return Err(RuntimeError::Type {
                expected: "two integers".into(),
                got: format!("{}, {}", args[0].type_name(), args[1].type_name()),
            });
        };
        if *b == 0 {
            return Err(RuntimeError::DivisionByZero);
        }
        // R7RS quotient: truncation toward zero (Rust's / on i64 does this).
        Ok(Value::Int(a / b))
    });
    define(env, "remainder", Arity::Exact(2), |args| {
        let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else {
            return Err(RuntimeError::Type {
                expected: "two integers".into(),
                got: format!("{}, {}", args[0].type_name(), args[1].type_name()),
            });
        };
        if *b == 0 {
            return Err(RuntimeError::DivisionByZero);
        }
        // R7RS remainder: same sign as dividend (Rust's % on i64).
        Ok(Value::Int(a % b))
    });
    define(env, "modulo", Arity::Exact(2), |args| {
        let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else {
            return Err(RuntimeError::Type {
                expected: "two integers".into(),
                got: format!("{}, {}", args[0].type_name(), args[1].type_name()),
            });
        };
        if *b == 0 {
            return Err(RuntimeError::DivisionByZero);
        }
        // R7RS modulo: same sign as divisor.
        let r = a % b;
        let m = if (r != 0) && ((r < 0) != (*b < 0)) {
            r + b
        } else {
            r
        };
        Ok(Value::Int(m))
    });
    define(env, "abs", Arity::Exact(1), |args| match &args[0] {
        Value::Int(n) => Ok(Value::Int(
            n.checked_abs()
                .ok_or(RuntimeError::Overflow { op: "abs" })?,
        )),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "min", Arity::AtLeast(1), |args| {
        let mut best = Num::from_value(&args[0])?;
        for a in &args[1..] {
            let n = Num::from_value(a)?;
            if Num::cmp(n, best) == std::cmp::Ordering::Less {
                best = n;
            }
        }
        Ok(best.into_value())
    });
    define(env, "max", Arity::AtLeast(1), |args| {
        let mut best = Num::from_value(&args[0])?;
        for a in &args[1..] {
            let n = Num::from_value(a)?;
            if Num::cmp(n, best) == std::cmp::Ordering::Greater {
                best = n;
            }
        }
        Ok(best.into_value())
    });
}

// ---------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------

fn check_numeric_chain(
    args: &[Value],
    pass: impl Fn(std::cmp::Ordering) -> bool,
) -> Result<Value, RuntimeError> {
    let mut prev = Num::from_value(&args[0])?;
    for a in &args[1..] {
        let cur = Num::from_value(a)?;
        if !pass(Num::cmp(prev, cur)) {
            return Ok(Value::Bool(false));
        }
        prev = cur;
    }
    Ok(Value::Bool(true))
}

fn install_comparison(env: &EnvRef) {
    use std::cmp::Ordering::{Equal, Greater, Less};
    define(env, "=", Arity::AtLeast(2), |args| {
        check_numeric_chain(args, |o| o == Equal)
    });
    define(env, "<", Arity::AtLeast(2), |args| {
        check_numeric_chain(args, |o| o == Less)
    });
    define(env, ">", Arity::AtLeast(2), |args| {
        check_numeric_chain(args, |o| o == Greater)
    });
    define(env, "<=", Arity::AtLeast(2), |args| {
        check_numeric_chain(args, |o| o != Greater)
    });
    define(env, ">=", Arity::AtLeast(2), |args| {
        check_numeric_chain(args, |o| o != Less)
    });
}

// ---------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------

fn install_predicates(env: &EnvRef) {
    define(env, "null?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_null()))
    });
    define(env, "pair?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_pair()))
    });
    define(env, "boolean?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_boolean()))
    });
    define(env, "symbol?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_symbol()))
    });
    define(env, "number?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_number()))
    });
    define(env, "integer?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Int(_))))
    });
    define(env, "real?", Arity::Exact(1), |a| {
        // In v1 the numeric tower is Int + Float, so any number is real.
        Ok(Value::Bool(a[0].is_number()))
    });
    define(env, "exact?", Arity::Exact(1), |a| match &a[0] {
        Value::Int(_) => Ok(Value::Bool(true)),
        Value::Float(_) => Ok(Value::Bool(false)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "inexact?", Arity::Exact(1), |a| match &a[0] {
        Value::Float(_) => Ok(Value::Bool(true)),
        Value::Int(_) => Ok(Value::Bool(false)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "zero?", Arity::Exact(1), |a| match &a[0] {
        Value::Int(n) => Ok(Value::Bool(*n == 0)),
        Value::Float(f) => Ok(Value::Bool(*f == 0.0)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "positive?", Arity::Exact(1), |a| match &a[0] {
        Value::Int(n) => Ok(Value::Bool(*n > 0)),
        Value::Float(f) => Ok(Value::Bool(*f > 0.0)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "negative?", Arity::Exact(1), |a| match &a[0] {
        Value::Int(n) => Ok(Value::Bool(*n < 0)),
        Value::Float(f) => Ok(Value::Bool(*f < 0.0)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "string?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_string()))
    });
    define(env, "vector?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_vector()))
    });
    define(env, "char?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Char(_))))
    });
    define(env, "procedure?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_procedure()))
    });
    define(env, "eof-object?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Eof)))
    });
    define(env, "not", Arity::Exact(1), |a| {
        Ok(Value::Bool(!a[0].is_truthy()))
    });
}

// ---------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------

fn install_equality(env: &EnvRef) {
    define(env, "eq?", Arity::Exact(2), |a| {
        Ok(Value::Bool(v_eq(&a[0], &a[1])))
    });
    define(env, "eqv?", Arity::Exact(2), |a| {
        Ok(Value::Bool(v_eqv(&a[0], &a[1])))
    });
    define(env, "equal?", Arity::Exact(2), |a| {
        Ok(Value::Bool(v_equal(&a[0], &a[1])))
    });
}

// ---------------------------------------------------------------------
// List operations
// ---------------------------------------------------------------------

fn install_list_ops(env: &EnvRef) {
    define(env, "cons", Arity::Exact(2), |a| {
        Ok(Value::cons(a[0].clone(), a[1].clone()))
    });
    define(env, "car", Arity::Exact(1), |a| match &a[0] {
        Value::Pair(p) => Ok(p.borrow().car.clone()),
        other => Err(RuntimeError::Type {
            expected: "pair".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "cdr", Arity::Exact(1), |a| match &a[0] {
        Value::Pair(p) => Ok(p.borrow().cdr.clone()),
        other => Err(RuntimeError::Type {
            expected: "pair".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "set-car!", Arity::Exact(2), |a| match &a[0] {
        Value::Pair(p) => {
            p.borrow_mut().car = a[1].clone();
            Ok(Value::Unspecified)
        }
        other => Err(RuntimeError::Type {
            expected: "pair".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "set-cdr!", Arity::Exact(2), |a| match &a[0] {
        Value::Pair(p) => {
            p.borrow_mut().cdr = a[1].clone();
            Ok(Value::Unspecified)
        }
        other => Err(RuntimeError::Type {
            expected: "pair".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "list", Arity::AtLeast(0), |a| {
        Ok(Value::list_from(a.iter().cloned()))
    });
    define(env, "length", Arity::Exact(1), |a| {
        match a[0].list_length() {
            Some(n) =>
            {
                #[allow(clippy::cast_possible_wrap)]
                Ok(Value::Int(n as i64))
            }
            None => Err(RuntimeError::Type {
                expected: "proper list".into(),
                got: a[0].type_name().into(),
            }),
        }
    });
    define(env, "reverse", Arity::Exact(1), |a| {
        let mut items: Vec<Value> = Vec::new();
        let mut cur = a[0].clone();
        loop {
            match cur {
                Value::Null => break,
                Value::Pair(p) => {
                    let pair = p.borrow();
                    items.push(pair.car.clone());
                    cur = pair.cdr.clone();
                }
                _ => {
                    return Err(RuntimeError::Type {
                        expected: "proper list".into(),
                        got: a[0].type_name().into(),
                    });
                }
            }
        }
        items.reverse();
        Ok(Value::list_from(items))
    });
    define(env, "append", Arity::AtLeast(0), |args| {
        if args.is_empty() {
            return Ok(Value::Null);
        }
        // append all but the last as proper lists; the last is used as-is.
        let mut out: Vec<Value> = Vec::new();
        for v in &args[..args.len() - 1] {
            let mut cur = v.clone();
            loop {
                match cur {
                    Value::Null => break,
                    Value::Pair(p) => {
                        let pair = p.borrow();
                        out.push(pair.car.clone());
                        cur = pair.cdr.clone();
                    }
                    _ => {
                        return Err(RuntimeError::Type {
                            expected: "proper list".into(),
                            got: v.type_name().into(),
                        });
                    }
                }
            }
        }
        // Build list from out, with the final arg as the tail.
        let mut acc = args[args.len() - 1].clone();
        for item in out.into_iter().rev() {
            acc = Value::cons(item, acc);
        }
        Ok(acc)
    });
    define(env, "list-ref", Arity::Exact(2), |a| {
        let Value::Int(idx) = a[1] else {
            return Err(RuntimeError::Type {
                expected: "integer index".into(),
                got: a[1].type_name().into(),
            });
        };
        if idx < 0 {
            return Err(RuntimeError::Other(format!(
                "list-ref: negative index {idx}"
            )));
        }
        let mut cur = a[0].clone();
        for _ in 0..idx {
            match cur {
                Value::Pair(p) => cur = p.borrow().cdr.clone(),
                _ => return Err(RuntimeError::Other("list-ref: index out of range".into())),
            }
        }
        match cur {
            Value::Pair(p) => Ok(p.borrow().car.clone()),
            _ => Err(RuntimeError::Other("list-ref: index out of range".into())),
        }
    });
    define(env, "memq", Arity::Exact(2), |a| {
        member_with(&a[0], &a[1], v_eq)
    });
    define(env, "memv", Arity::Exact(2), |a| {
        member_with(&a[0], &a[1], v_eqv)
    });
    define(env, "member", Arity::Exact(2), |a| {
        member_with(&a[0], &a[1], v_equal)
    });
    define(env, "assq", Arity::Exact(2), |a| {
        assoc_with(&a[0], &a[1], v_eq)
    });
    define(env, "assv", Arity::Exact(2), |a| {
        assoc_with(&a[0], &a[1], v_eqv)
    });
    define(env, "assoc", Arity::Exact(2), |a| {
        assoc_with(&a[0], &a[1], v_equal)
    });
}

fn member_with(
    needle: &Value,
    haystack: &Value,
    eq: fn(&Value, &Value) -> bool,
) -> Result<Value, RuntimeError> {
    let mut cur = haystack.clone();
    loop {
        let (matched, next) = match &cur {
            Value::Null => return Ok(Value::Bool(false)),
            Value::Pair(p) => {
                let pair = p.borrow();
                (eq(needle, &pair.car), pair.cdr.clone())
            }
            _ => {
                return Err(RuntimeError::Type {
                    expected: "proper list".into(),
                    got: haystack.type_name().into(),
                });
            }
        };
        if matched {
            return Ok(cur);
        }
        cur = next;
    }
}

fn assoc_with(
    needle: &Value,
    alist: &Value,
    eq: fn(&Value, &Value) -> bool,
) -> Result<Value, RuntimeError> {
    let mut cur = alist.clone();
    loop {
        match cur {
            Value::Null => return Ok(Value::Bool(false)),
            Value::Pair(outer) => {
                let outer = outer.borrow();
                let entry = outer.car.clone();
                if let Value::Pair(p) = &entry
                    && eq(needle, &p.borrow().car)
                {
                    return Ok(entry);
                }
                cur = outer.cdr.clone();
            }
            _ => {
                return Err(RuntimeError::Type {
                    expected: "association list".into(),
                    got: alist.type_name().into(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------
// Miscellaneous
// ---------------------------------------------------------------------

fn install_misc(env: &EnvRef) {
    define(env, "exact->inexact", Arity::Exact(1), |a| match &a[0] {
        #[allow(clippy::cast_precision_loss)]
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::Float(f) => Ok(Value::Float(*f)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "inexact->exact", Arity::Exact(1), |a| match &a[0] {
        #[allow(clippy::cast_possible_truncation)]
        Value::Float(f) => Ok(Value::Int(*f as i64)),
        Value::Int(n) => Ok(Value::Int(*n)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
}

// ---------------------------------------------------------------------
// Scheme-level bootstrap of higher-order procedures
// ---------------------------------------------------------------------
//
// Some "primitives" are easier (and more honest) to express in Scheme
// itself once enough of the language is wired up. Anything in this
// string can assume every Rust-installed primitive is already in scope.

const BOOTSTRAP: &str = r"
(define (map f xs)
  (if (null? xs)
      '()
      (cons (f (car xs)) (map f (cdr xs)))))

(define (for-each f xs)
  (if (null? xs)
      (if #f #f)
      (begin (f (car xs)) (for-each f (cdr xs)))))

(define (caar p) (car (car p)))
(define (cadr p) (car (cdr p)))
(define (cdar p) (cdr (car p)))
(define (cddr p) (cdr (cdr p)))
(define (caddr p) (car (cdr (cdr p))))
(define (cadddr p) (car (cdr (cdr (cdr p)))))
";

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;
    use crate::value::equal;

    fn run(source: &str) -> Result<Value, EvalError> {
        let env = Env::new_global();
        install_base(&env).expect("install_base");
        eval_source(source, env)
    }

    // -- arithmetic --------------------------------------------------

    #[test]
    fn arithmetic_identities() {
        assert!(equal(&run("(+)").unwrap(), &Value::Int(0)));
        assert!(equal(&run("(*)").unwrap(), &Value::Int(1)));
    }

    #[test]
    fn addition_chain() {
        assert!(equal(&run("(+ 1 2 3 4 5)").unwrap(), &Value::Int(15)));
    }

    #[test]
    fn negation() {
        assert!(equal(&run("(- 7)").unwrap(), &Value::Int(-7)));
    }

    #[test]
    fn subtraction_chain() {
        assert!(equal(&run("(- 10 1 2 3)").unwrap(), &Value::Int(4)));
    }

    #[test]
    fn integer_division_promotes_to_float_when_inexact() {
        // 1/3 isn't exact in our v1 numeric tower, so it becomes a float.
        let v = run("(/ 1 3)").unwrap();
        assert!(matches!(v, Value::Float(f) if (f - 0.333_333_333_333_333_3).abs() < 1e-9));
    }

    #[test]
    fn exact_division_stays_int() {
        assert!(equal(&run("(/ 12 3)").unwrap(), &Value::Int(4)));
    }

    #[test]
    fn division_by_zero_errors() {
        assert!(matches!(
            run("(/ 1 0)"),
            Err(EvalError::Runtime(RuntimeError::DivisionByZero))
        ));
    }

    #[test]
    fn quotient_remainder_modulo() {
        assert!(equal(&run("(quotient 13 4)").unwrap(), &Value::Int(3)));
        assert!(equal(&run("(quotient -13 4)").unwrap(), &Value::Int(-3)));
        assert!(equal(&run("(remainder 13 4)").unwrap(), &Value::Int(1)));
        assert!(equal(&run("(remainder -13 4)").unwrap(), &Value::Int(-1)));
        assert!(equal(&run("(modulo 13 4)").unwrap(), &Value::Int(1)));
        assert!(equal(&run("(modulo -13 4)").unwrap(), &Value::Int(3))); // sign of divisor
    }

    #[test]
    fn abs_min_max() {
        assert!(equal(&run("(abs -7)").unwrap(), &Value::Int(7)));
        assert!(equal(&run("(min 3 1 4 1 5)").unwrap(), &Value::Int(1)));
        assert!(equal(&run("(max 3 1 4 1 5)").unwrap(), &Value::Int(5)));
    }

    // -- comparison --------------------------------------------------

    #[test]
    fn equality_chain() {
        assert!(equal(&run("(= 3 3 3)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(= 3 3 4)").unwrap(), &Value::Bool(false)));
    }

    #[test]
    fn less_than_chain() {
        assert!(equal(&run("(< 1 2 3 4)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(< 1 2 2 3)").unwrap(), &Value::Bool(false)));
    }

    #[test]
    fn mixed_exact_inexact_comparison() {
        assert!(equal(&run("(= 3 3.0)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(< 3 3.5)").unwrap(), &Value::Bool(true)));
    }

    // -- predicates --------------------------------------------------

    #[test]
    fn type_predicates() {
        assert!(equal(&run("(null? '())").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(null? '(1))").unwrap(), &Value::Bool(false)));
        assert!(equal(&run("(pair? '(1))").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(symbol? 'foo)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(number? 1)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(number? 1.5)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(integer? 1)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(integer? 1.5)").unwrap(), &Value::Bool(false)));
        assert!(equal(&run("(exact? 1)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(exact? 1.5)").unwrap(), &Value::Bool(false)));
        assert!(equal(&run("(string? \"x\")").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(procedure? +)").unwrap(), &Value::Bool(true)));
    }

    #[test]
    fn not_inverts_truthiness() {
        assert!(equal(&run("(not #f)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(not #t)").unwrap(), &Value::Bool(false)));
        assert!(equal(&run("(not 0)").unwrap(), &Value::Bool(false))); // 0 is truthy
        assert!(equal(&run("(not '())").unwrap(), &Value::Bool(false))); // () is truthy
    }

    // -- equality ----------------------------------------------------

    #[test]
    fn eq_eqv_equal() {
        assert!(equal(&run("(eq? 'a 'a)").unwrap(), &Value::Bool(true)));
        assert!(equal(&run("(eqv? 1.5 1.5)").unwrap(), &Value::Bool(true)));
        assert!(equal(
            &run("(equal? '(1 2 3) (list 1 2 3))").unwrap(),
            &Value::Bool(true)
        ));
        assert!(equal(
            &run("(equal? \"hi\" \"hi\")").unwrap(),
            &Value::Bool(true)
        ));
    }

    // -- list ops ----------------------------------------------------

    #[test]
    fn cons_car_cdr() {
        let v = run("(cons 1 2)").unwrap();
        let expected = Value::cons(Value::Int(1), Value::Int(2));
        assert!(equal(&v, &expected));
        assert!(equal(&run("(car '(1 2 3))").unwrap(), &Value::Int(1)));
        let cdr = run("(cdr '(1 2 3))").unwrap();
        assert!(equal(
            &cdr,
            &Value::list_from([Value::Int(2), Value::Int(3)])
        ));
    }

    #[test]
    fn length_and_reverse() {
        assert!(equal(&run("(length '(1 2 3 4))").unwrap(), &Value::Int(4)));
        let v = run("(reverse '(1 2 3))").unwrap();
        let expected = Value::list_from([Value::Int(3), Value::Int(2), Value::Int(1)]);
        assert!(equal(&v, &expected));
    }

    #[test]
    fn append_lists() {
        let v = run("(append '(1 2) '(3 4) '(5))").unwrap();
        let expected = Value::list_from((1..=5).map(Value::Int));
        assert!(equal(&v, &expected));
        // append with non-list tail produces an improper list.
        let v = run("(append '(1 2) 3)").unwrap();
        let expected = Value::cons(Value::Int(1), Value::cons(Value::Int(2), Value::Int(3)));
        assert!(equal(&v, &expected));
    }

    #[test]
    fn list_ref() {
        assert!(equal(
            &run("(list-ref '(a b c d) 2)").unwrap(),
            &Value::Symbol(Symbol::intern("c"))
        ));
    }

    #[test]
    fn member_assoc_families() {
        // memq with a symbol that's eq?
        let v = run("(memq 'b '(a b c))").unwrap();
        let expected = Value::list_from([
            Value::Symbol(Symbol::intern("b")),
            Value::Symbol(Symbol::intern("c")),
        ]);
        assert!(equal(&v, &expected));
        // member with structural equality
        let v = run("(member '(2) '((1) (2) (3)))").unwrap();
        let expected = Value::list_from([
            Value::list_from([Value::Int(2)]),
            Value::list_from([Value::Int(3)]),
        ]);
        assert!(equal(&v, &expected));
        // assq
        let v = run("(assq 'b '((a 1) (b 2) (c 3)))").unwrap();
        let expected = Value::list_from([Value::Symbol(Symbol::intern("b")), Value::Int(2)]);
        assert!(equal(&v, &expected));
        // assoc returns #f when missing
        assert!(equal(
            &run("(assoc 'z '((a 1) (b 2)))").unwrap(),
            &Value::Bool(false)
        ));
    }

    #[test]
    fn set_car_and_cdr() {
        let src = "(define p (cons 1 2)) (set-car! p 99) (set-cdr! p 100) p";
        let v = run(src).unwrap();
        let expected = Value::cons(Value::Int(99), Value::Int(100));
        assert!(equal(&v, &expected));
    }

    // -- bootstrap (map/for-each from Scheme) ------------------------

    #[test]
    fn map_via_bootstrap() {
        let v = run("(map (lambda (x) (* x x)) '(1 2 3 4))").unwrap();
        let expected =
            Value::list_from([Value::Int(1), Value::Int(4), Value::Int(9), Value::Int(16)]);
        assert!(equal(&v, &expected));
    }

    #[test]
    fn for_each_via_bootstrap() {
        // for-each is used for side effects; verify counter behavior.
        let src = "(define s 0)
                   (for-each (lambda (x) (set! s (+ s x))) '(1 2 3 4))
                   s";
        assert!(equal(&run(src).unwrap(), &Value::Int(10)));
    }

    #[test]
    fn cxr_shortcuts() {
        assert!(equal(&run("(cadr '(1 2 3))").unwrap(), &Value::Int(2)));
        assert!(equal(&run("(caddr '(1 2 3 4))").unwrap(), &Value::Int(3)));
        assert!(equal(
            &run("(cddr '(1 2 3 4))").unwrap(),
            &Value::list_from([Value::Int(3), Value::Int(4)])
        ));
    }
}
