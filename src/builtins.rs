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
// Mathematical formulas read more naturally with x/y as variable
// names; `many_single_char_names` is noisy for the Num arithmetic.
#![allow(clippy::many_single_char_names)]
// num_add/sub/mul/div consume their operands; clippy's
// `needless_pass_by_value` would flip the signatures to references and
// force the call sites to clone instead of move. Numbers can carry
// BigInts so moving is meaningfully cheaper.
#![allow(clippy::needless_pass_by_value)]

use std::rc::Rc;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, One, Signed, ToPrimitive, Zero};

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
    install_chars(env);
    install_strings(env);
    install_symbols(env);
    install_vectors(env);
    install_bytevectors(env);
    crate::io::install_io(env);
    eval_source(BOOTSTRAP, env.clone())?;
    eval_source(crate::io::CURRENT_PORTS_BOOTSTRAP, env.clone())?;
    Ok(())
}

fn define(env: &EnvRef, name: &'static str, arity: Arity, body: PrimitiveFn) {
    let p = Procedure::Primitive { name, arity, body };
    env.define(Symbol::intern(name), Value::Procedure(Rc::new(p)));
}

// ---------------------------------------------------------------------
// Numeric helper
// ---------------------------------------------------------------------

/// Helper representation for numeric ops. R7RS exact/inexact rules:
/// mixing exact and inexact produces inexact; otherwise stay in the
/// exact tower.
#[derive(Clone, Debug)]
enum Num {
    Int(i64),
    Big(BigInt),
    Rat(BigRational),
    Float(f64),
}

impl Num {
    fn from_value(v: &Value) -> Result<Self, RuntimeError> {
        match v {
            Value::Int(n) => Ok(Self::Int(*n)),
            Value::BigInt(b) => Ok(Self::Big((**b).clone())),
            Value::Rational(r) => Ok(Self::Rat((**r).clone())),
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
            Self::Big(b) => bigint_to_value(b),
            Self::Rat(r) => rational_to_value(r),
            Self::Float(f) => Value::Float(f),
        }
    }

    fn to_f64(&self) -> f64 {
        match self {
            #[allow(clippy::cast_precision_loss)]
            Self::Int(n) => *n as f64,
            Self::Big(b) => b.to_f64().unwrap_or(f64::NAN),
            Self::Rat(r) => r.to_f64().unwrap_or(f64::NAN),
            Self::Float(f) => *f,
        }
    }

    fn is_inexact(&self) -> bool {
        matches!(self, Self::Float(_))
    }

    /// Promote an exact number to a common exact level.
    fn to_rational(&self) -> BigRational {
        match self {
            Self::Int(n) => BigRational::from_i64(*n).expect("i64 to BigRational"),
            Self::Big(b) => BigRational::from_integer(b.clone()),
            Self::Rat(r) => r.clone(),
            Self::Float(_) => unreachable!("to_rational on inexact"),
        }
    }
}

/// Collapse a `BigInt` back to `Int` if it fits.
fn bigint_to_value(b: BigInt) -> Value {
    if let Some(n) = b.to_i64() {
        Value::Int(n)
    } else {
        Value::BigInt(Rc::new(b))
    }
}

/// Collapse a `BigRational` with denom 1 down through bigint to fixnum
/// if possible.
fn rational_to_value(r: BigRational) -> Value {
    if One::is_one(r.denom()) {
        bigint_to_value(r.numer().clone())
    } else {
        Value::Rational(Rc::new(r))
    }
}

// Arithmetic combinators on Num that follow R7RS promotion.

fn num_add(a: Num, b: Num) -> Num {
    if a.is_inexact() || b.is_inexact() {
        return Num::Float(a.to_f64() + b.to_f64());
    }
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => match x.checked_add(y) {
            Some(n) => Num::Int(n),
            None => Num::Big(BigInt::from(x) + BigInt::from(y)),
        },
        (a, b) => Num::Rat(a.to_rational() + b.to_rational()),
    }
}

fn num_sub(a: Num, b: Num) -> Num {
    if a.is_inexact() || b.is_inexact() {
        return Num::Float(a.to_f64() - b.to_f64());
    }
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => match x.checked_sub(y) {
            Some(n) => Num::Int(n),
            None => Num::Big(BigInt::from(x) - BigInt::from(y)),
        },
        (a, b) => Num::Rat(a.to_rational() - b.to_rational()),
    }
}

fn num_mul(a: Num, b: Num) -> Num {
    if a.is_inexact() || b.is_inexact() {
        return Num::Float(a.to_f64() * b.to_f64());
    }
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => match x.checked_mul(y) {
            Some(n) => Num::Int(n),
            None => Num::Big(BigInt::from(x) * BigInt::from(y)),
        },
        (a, b) => Num::Rat(a.to_rational() * b.to_rational()),
    }
}

fn num_div(a: Num, b: Num) -> Result<Num, RuntimeError> {
    if a.is_inexact() || b.is_inexact() {
        let bf = b.to_f64();
        if bf == 0.0 {
            return Err(RuntimeError::DivisionByZero);
        }
        return Ok(Num::Float(a.to_f64() / bf));
    }
    let br = b.to_rational();
    if br.is_zero() {
        return Err(RuntimeError::DivisionByZero);
    }
    Ok(Num::Rat(a.to_rational() / br))
}

fn num_neg(a: Num) -> Num {
    match a {
        Num::Int(n) => match n.checked_neg() {
            Some(m) => Num::Int(m),
            None => Num::Big(-BigInt::from(n)),
        },
        Num::Big(b) => Num::Big(-b),
        Num::Rat(r) => Num::Rat(-r),
        Num::Float(f) => Num::Float(-f),
    }
}

fn num_is_zero(n: &Num) -> bool {
    match n {
        Num::Int(x) => *x == 0,
        Num::Big(b) => b.is_zero(),
        Num::Rat(r) => r.is_zero(),
        Num::Float(f) => *f == 0.0,
    }
}

fn is_integer_value(v: &Value) -> bool {
    match v {
        Value::Int(_) | Value::BigInt(_) => true,
        Value::Rational(r) => One::is_one(r.denom()),
        Value::Float(f) => f.is_finite() && f.fract() == 0.0,
        _ => false,
    }
}

/// Coerce a numeric `Value` to a `BigInt`. Rejects rationals,
/// non-integral floats, and non-numbers.
fn value_to_bigint(v: &Value) -> Result<BigInt, RuntimeError> {
    match v {
        Value::Int(n) => Ok(BigInt::from(*n)),
        Value::BigInt(b) => Ok((**b).clone()),
        Value::Rational(r) if One::is_one(r.denom()) => Ok(r.numer().clone()),
        Value::Float(f) if f.fract() == 0.0 && f.is_finite() => {
            BigInt::from_f64(*f).ok_or(RuntimeError::Type {
                expected: "integer".into(),
                got: "non-integer float".into(),
            })
        }
        other => Err(RuntimeError::Type {
            expected: "integer".into(),
            got: other.type_name().into(),
        }),
    }
}

fn num_cmp(a: &Num, b: &Num) -> std::cmp::Ordering {
    if a.is_inexact() || b.is_inexact() {
        let x = a.to_f64();
        let y = b.to_f64();
        return x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Less);
    }
    a.to_rational().cmp(&b.to_rational())
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
            acc = num_add(acc, Num::from_value(a)?);
        }
        Ok(acc.into_value())
    });
    define(env, "-", Arity::AtLeast(1), |args| {
        let first = Num::from_value(&args[0])?;
        if args.len() == 1 {
            return Ok(num_neg(first).into_value());
        }
        let mut acc = first;
        for a in &args[1..] {
            acc = num_sub(acc, Num::from_value(a)?);
        }
        Ok(acc.into_value())
    });
    define(env, "*", Arity::AtLeast(0), |args| {
        if args.is_empty() {
            return Ok(Value::Int(1));
        }
        let mut acc = Num::from_value(&args[0])?;
        for a in &args[1..] {
            acc = num_mul(acc, Num::from_value(a)?);
        }
        Ok(acc.into_value())
    });
    define(env, "/", Arity::AtLeast(1), |args| {
        let first = Num::from_value(&args[0])?;
        if args.len() == 1 {
            return Ok(num_div(Num::Int(1), first)?.into_value());
        }
        let mut acc = first;
        for a in &args[1..] {
            acc = num_div(acc, Num::from_value(a)?)?;
        }
        Ok(acc.into_value())
    });
    define(env, "quotient", Arity::Exact(2), |args| {
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        // R7RS quotient: truncation toward zero.
        Ok(bigint_to_value(a / b))
    });
    define(env, "remainder", Arity::Exact(2), |args| {
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        // R7RS remainder: same sign as dividend.
        Ok(bigint_to_value(a % b))
    });
    define(env, "modulo", Arity::Exact(2), |args| {
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        let r = &a % &b;
        // R7RS modulo: same sign as divisor.
        let m = if !r.is_zero() && (r.sign() != b.sign()) {
            r + b
        } else {
            r
        };
        Ok(bigint_to_value(m))
    });
    define(env, "abs", Arity::Exact(1), |args| {
        let n = Num::from_value(&args[0])?;
        Ok(match n {
            Num::Int(x) => match x.checked_abs() {
                Some(m) => Num::Int(m),
                None => Num::Big(BigInt::from(x).abs()),
            },
            Num::Big(b) => Num::Big(b.abs()),
            Num::Rat(r) => Num::Rat(r.abs()),
            Num::Float(f) => Num::Float(f.abs()),
        }
        .into_value())
    });
    define(env, "min", Arity::AtLeast(1), |args| {
        let mut best = Num::from_value(&args[0])?;
        for a in &args[1..] {
            let n = Num::from_value(a)?;
            if num_cmp(&n, &best) == std::cmp::Ordering::Less {
                best = n;
            }
        }
        Ok(best.into_value())
    });
    define(env, "max", Arity::AtLeast(1), |args| {
        let mut best = Num::from_value(&args[0])?;
        for a in &args[1..] {
            let n = Num::from_value(a)?;
            if num_cmp(&n, &best) == std::cmp::Ordering::Greater {
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
        if !pass(num_cmp(&prev, &cur)) {
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
    define(env, "list?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].list_length().is_some()))
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
        Ok(Value::Bool(is_integer_value(&a[0])))
    });
    define(env, "real?", Arity::Exact(1), |a| {
        // In v1 (no complex), every number is real.
        Ok(Value::Bool(a[0].is_number()))
    });
    define(env, "rational?", Arity::Exact(1), |a| {
        Ok(Value::Bool(
            matches!(a[0], Value::Int(_) | Value::BigInt(_) | Value::Rational(_))
                || matches!(a[0], Value::Float(f) if f.is_finite()),
        ))
    });
    define(env, "exact?", Arity::Exact(1), |a| match &a[0] {
        Value::Int(_) | Value::BigInt(_) | Value::Rational(_) => Ok(Value::Bool(true)),
        Value::Float(_) => Ok(Value::Bool(false)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "inexact?", Arity::Exact(1), |a| match &a[0] {
        Value::Float(_) => Ok(Value::Bool(true)),
        Value::Int(_) | Value::BigInt(_) | Value::Rational(_) => Ok(Value::Bool(false)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "zero?", Arity::Exact(1), |a| {
        let n = Num::from_value(&a[0])?;
        Ok(Value::Bool(num_is_zero(&n)))
    });
    define(env, "positive?", Arity::Exact(1), |a| {
        let n = Num::from_value(&a[0])?;
        Ok(Value::Bool(
            num_cmp(&n, &Num::Int(0)) == std::cmp::Ordering::Greater,
        ))
    });
    define(env, "negative?", Arity::Exact(1), |a| {
        let n = Num::from_value(&a[0])?;
        Ok(Value::Bool(
            num_cmp(&n, &Num::Int(0)) == std::cmp::Ordering::Less,
        ))
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
    define(env, "exact->inexact", Arity::Exact(1), |a| {
        let n = Num::from_value(&a[0])?;
        Ok(Value::Float(n.to_f64()))
    });
    define(env, "inexact->exact", Arity::Exact(1), |a| match &a[0] {
        Value::Float(f) => {
            if !f.is_finite() {
                return Err(RuntimeError::Other(format!("cannot convert {f} to exact")));
            }
            let r = BigRational::from_f64(*f)
                .ok_or_else(|| RuntimeError::Other(format!("cannot convert {f} to exact")))?;
            Ok(rational_to_value(r))
        }
        Value::Int(_) | Value::BigInt(_) | Value::Rational(_) => Ok(a[0].clone()),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "exact", Arity::Exact(1), |a| {
        // R7RS `exact` is the modern alias for `inexact->exact`.
        match &a[0] {
            Value::Float(f) => {
                if !f.is_finite() {
                    return Err(RuntimeError::Other(format!("cannot convert {f} to exact")));
                }
                let r = BigRational::from_f64(*f)
                    .ok_or_else(|| RuntimeError::Other(format!("cannot convert {f} to exact")))?;
                Ok(rational_to_value(r))
            }
            Value::Int(_) | Value::BigInt(_) | Value::Rational(_) => Ok(a[0].clone()),
            other => Err(RuntimeError::Type {
                expected: "number".into(),
                got: other.type_name().into(),
            }),
        }
    });
    define(env, "inexact", Arity::Exact(1), |a| {
        let n = Num::from_value(&a[0])?;
        Ok(Value::Float(n.to_f64()))
    });
    define(env, "numerator", Arity::Exact(1), |a| match &a[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::BigInt(b) => Ok(Value::BigInt(b.clone())),
        Value::Rational(r) => Ok(bigint_to_value(r.numer().clone())),
        other => Err(RuntimeError::Type {
            expected: "rational".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "denominator", Arity::Exact(1), |a| match &a[0] {
        Value::Int(_) | Value::BigInt(_) => Ok(Value::Int(1)),
        Value::Rational(r) => Ok(bigint_to_value(r.denom().clone())),
        other => Err(RuntimeError::Type {
            expected: "rational".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "features", Arity::Exact(0), |_| {
        Ok(Value::list_from(
            crate::library::features()
                .into_iter()
                .map(|s| Value::Symbol(Symbol::intern(s))),
        ))
    });
    define(env, "error-object?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::ErrorObject(_))))
    });
    define(env, "error-object-message", Arity::Exact(1), |a| {
        match &a[0] {
            Value::ErrorObject(e) => Ok(Value::string(e.message.clone())),
            other => Err(RuntimeError::Type {
                expected: "error-object".into(),
                got: other.type_name().into(),
            }),
        }
    });
    define(
        env,
        "error-object-irritants",
        Arity::Exact(1),
        |a| match &a[0] {
            Value::ErrorObject(e) => Ok(Value::list_from(e.irritants.iter().cloned())),
            other => Err(RuntimeError::Type {
                expected: "error-object".into(),
                got: other.type_name().into(),
            }),
        },
    );
    define(env, "read-error?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(
            &a[0],
            Value::ErrorObject(e) if e.kind == crate::value::ErrorKind::Read
        )))
    });
    define(env, "file-error?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(
            &a[0],
            Value::ErrorObject(e) if e.kind == crate::value::ErrorKind::File
        )))
    });
    // `error` constructs an ErrorObject from a message string + irritants.
    // It does NOT itself raise — the user calls (raise (error "msg" ...))
    // or wraps with our convenience `error/raise` (the bootstrap defines
    // an `error` that raises directly).
    define(env, "values", Arity::AtLeast(0), |args| match args.len() {
        // R7RS: (values v) ≡ v (a single value, not a singleton packet).
        1 => Ok(args[0].clone()),
        _ => Ok(Value::Values(Rc::new(args.to_vec()))),
    });
    define(env, "values-packet?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Values(_))))
    });
    define(env, "values->list", Arity::Exact(1), |a| match &a[0] {
        Value::Values(vs) => Ok(Value::list_from(vs.iter().cloned())),
        // Single value: wrap in a one-element list, so the bootstrap
        // call-with-values can apply consumer to any producer result
        // uniformly.
        other => Ok(Value::list_from([other.clone()])),
    });
    define(env, "make-parameter", Arity::Exact(1), |a| {
        let cell = Rc::new(crate::value::ParameterCell {
            value: std::cell::RefCell::new(a[0].clone()),
        });
        Ok(Value::Procedure(Rc::new(Procedure::Parameter { cell })))
    });
    define(env, "promise?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Promise(_))))
    });
    define(env, "make-promise", Arity::Exact(1), |a| {
        // (make-promise obj) builds an already-forced promise.
        let state = std::cell::RefCell::new(crate::value::PromiseState::Forced(a[0].clone()));
        Ok(Value::Promise(Rc::new(state)))
    });
    define(env, "force", Arity::Exact(1), |a| {
        let mut cur = a[0].clone();
        // R7RS: force resolves chains of promises, so (force (delay
        // (delay 42))) returns 42, not a promise.
        loop {
            let Value::Promise(p) = &cur else {
                return Ok(cur);
            };
            // Snapshot the state — if it's Forced, return; if
            // Pending, evaluate and cache.
            let snapshot = {
                let borrowed = p.borrow();
                match &*borrowed {
                    crate::value::PromiseState::Forced(v) => Some(v.clone()),
                    crate::value::PromiseState::Pending { .. } => None,
                }
            };
            if let Some(v) = snapshot {
                cur = v;
                continue;
            }
            // Pending: evaluate. We need expr/env from the promise,
            // but we can't hold borrow across eval (eval may
            // re-enter and mutate). Take a clone first.
            let (expr, env) = {
                let borrowed = p.borrow();
                if let crate::value::PromiseState::Pending { expr, env } = &*borrowed {
                    (expr.clone(), env.clone())
                } else {
                    unreachable!()
                }
            };
            let v = crate::eval::eval(expr, env)
                .map_err(|e| RuntimeError::Other(format!("force: {e}")))?;
            *p.borrow_mut() = crate::value::PromiseState::Forced(v.clone());
            cur = v;
        }
    });
    define(env, "make-error-object", Arity::AtLeast(1), |args| {
        let msg = match &args[0] {
            Value::String(s) => s.borrow().clone(),
            Value::Symbol(s) => s.name().to_string(),
            other => {
                return Err(RuntimeError::Type {
                    expected: "string".into(),
                    got: other.type_name().into(),
                });
            }
        };
        let irritants: Vec<Value> = args.iter().skip(1).cloned().collect();
        Ok(Value::ErrorObject(Rc::new(crate::value::ErrorObject {
            message: msg,
            irritants,
            kind: crate::value::ErrorKind::User,
        })))
    });
}

// ---------------------------------------------------------------------
// Characters
// ---------------------------------------------------------------------

fn install_chars(env: &EnvRef) {
    define(env, "char->integer", Arity::Exact(1), |a| match &a[0] {
        Value::Char(c) =>
        {
            #[allow(clippy::cast_lossless)]
            Ok(Value::Int(u32::from(*c) as i64))
        }
        other => Err(RuntimeError::Type {
            expected: "char".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "integer->char", Arity::Exact(1), |a| {
        let n = value_to_bigint(&a[0])?;
        let code = n.to_u32().ok_or_else(|| {
            RuntimeError::Other("integer->char: value out of Unicode range".into())
        })?;
        char::from_u32(code).map(Value::Char).ok_or_else(|| {
            RuntimeError::Other(format!(
                "integer->char: {code} is not a valid Unicode scalar"
            ))
        })
    });
    define(env, "char-alphabetic?", Arity::Exact(1), |a| {
        char_predicate(&a[0], char::is_alphabetic)
    });
    define(env, "char-numeric?", Arity::Exact(1), |a| {
        char_predicate(&a[0], char::is_numeric)
    });
    define(env, "char-whitespace?", Arity::Exact(1), |a| {
        char_predicate(&a[0], char::is_whitespace)
    });
    define(env, "char-upper-case?", Arity::Exact(1), |a| {
        char_predicate(&a[0], char::is_uppercase)
    });
    define(env, "char-lower-case?", Arity::Exact(1), |a| {
        char_predicate(&a[0], char::is_lowercase)
    });
    define(env, "char-upcase", Arity::Exact(1), |a| match &a[0] {
        Value::Char(c) => {
            // char::to_uppercase yields an iterator; R7RS char-upcase
            // returns one char, so take the first.
            Ok(Value::Char(c.to_uppercase().next().unwrap_or(*c)))
        }
        other => Err(type_err("char", other)),
    });
    define(env, "char-downcase", Arity::Exact(1), |a| match &a[0] {
        Value::Char(c) => Ok(Value::Char(c.to_lowercase().next().unwrap_or(*c))),
        other => Err(type_err("char", other)),
    });
    // Comparison ops.
    define(env, "char=?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x == y)
    });
    define(env, "char<?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x < y)
    });
    define(env, "char>?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x > y)
    });
    define(env, "char<=?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x <= y)
    });
    define(env, "char>=?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x >= y)
    });
}

fn char_predicate(v: &Value, f: fn(char) -> bool) -> Result<Value, RuntimeError> {
    match v {
        Value::Char(c) => Ok(Value::Bool(f(*c))),
        other => Err(type_err("char", other)),
    }
}

fn char_chain(args: &[Value], pass: fn(char, char) -> bool) -> Result<Value, RuntimeError> {
    let mut prev = match &args[0] {
        Value::Char(c) => *c,
        other => return Err(type_err("char", other)),
    };
    for a in &args[1..] {
        let cur = match a {
            Value::Char(c) => *c,
            other => return Err(type_err("char", other)),
        };
        if !pass(prev, cur) {
            return Ok(Value::Bool(false));
        }
        prev = cur;
    }
    Ok(Value::Bool(true))
}

fn type_err(expected: &str, got: &Value) -> RuntimeError {
    RuntimeError::Type {
        expected: expected.into(),
        got: got.type_name().into(),
    }
}

// ---------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------

fn install_strings(env: &EnvRef) {
    define(env, "make-string", Arity::Range { min: 1, max: 2 }, |a| {
        let len = value_to_usize(&a[0], "make-string")?;
        let fill = if a.len() == 2 {
            match &a[1] {
                Value::Char(c) => *c,
                other => return Err(type_err("char", other)),
            }
        } else {
            ' '
        };
        Ok(Value::string(fill.to_string().repeat(len)))
    });
    define(env, "string", Arity::AtLeast(0), |args| {
        let mut s = String::with_capacity(args.len());
        for a in args {
            match a {
                Value::Char(c) => s.push(*c),
                other => return Err(type_err("char", other)),
            }
        }
        Ok(Value::string(s))
    });
    define(env, "string-length", Arity::Exact(1), |a| match &a[0] {
        Value::String(s) =>
        {
            #[allow(clippy::cast_possible_wrap)]
            Ok(Value::Int(s.borrow().chars().count() as i64))
        }
        other => Err(type_err("string", other)),
    });
    define(env, "string-ref", Arity::Exact(2), |a| match &a[0] {
        Value::String(s) => {
            let idx = value_to_usize(&a[1], "string-ref")?;
            s.borrow()
                .chars()
                .nth(idx)
                .map(Value::Char)
                .ok_or_else(|| RuntimeError::Other("string-ref: index out of range".into()))
        }
        other => Err(type_err("string", other)),
    });
    define(
        env,
        "substring",
        Arity::Range { min: 2, max: 3 },
        |a| match &a[0] {
            Value::String(s) => {
                let start = value_to_usize(&a[1], "substring")?;
                let total = s.borrow().chars().count();
                let end = if a.len() == 3 {
                    value_to_usize(&a[2], "substring")?
                } else {
                    total
                };
                if start > end || end > total {
                    return Err(RuntimeError::Other(
                        "substring: indices out of range".into(),
                    ));
                }
                let sub: String = s.borrow().chars().skip(start).take(end - start).collect();
                Ok(Value::string(sub))
            }
            other => Err(type_err("string", other)),
        },
    );
    define(env, "string-append", Arity::AtLeast(0), |args| {
        let mut out = String::new();
        for a in args {
            match a {
                Value::String(s) => out.push_str(&s.borrow()),
                other => return Err(type_err("string", other)),
            }
        }
        Ok(Value::string(out))
    });
    define(env, "string->list", Arity::Exact(1), |a| match &a[0] {
        Value::String(s) => {
            let items: Vec<Value> = s.borrow().chars().map(Value::Char).collect();
            Ok(Value::list_from(items))
        }
        other => Err(type_err("string", other)),
    });
    define(env, "list->string", Arity::Exact(1), |a| {
        let mut out = String::new();
        let mut cur = a[0].clone();
        loop {
            match cur {
                Value::Null => break,
                Value::Pair(p) => {
                    let pair = p.borrow();
                    match &pair.car {
                        Value::Char(c) => out.push(*c),
                        other => return Err(type_err("char", other)),
                    }
                    cur = pair.cdr.clone();
                }
                _ => return Err(type_err("proper list of chars", &a[0])),
            }
        }
        Ok(Value::string(out))
    });
    define(env, "string-copy", Arity::AtLeast(1), |a| match &a[0] {
        Value::String(s) => Ok(Value::string(s.borrow().clone())),
        other => Err(type_err("string", other)),
    });
    // Comparison ops.
    define(env, "string=?", Arity::AtLeast(2), |a| {
        string_chain(a, |x, y| x == y)
    });
    define(env, "string<?", Arity::AtLeast(2), |a| {
        string_chain(a, |x, y| x < y)
    });
    define(env, "string>?", Arity::AtLeast(2), |a| {
        string_chain(a, |x, y| x > y)
    });
    define(env, "string<=?", Arity::AtLeast(2), |a| {
        string_chain(a, |x, y| x <= y)
    });
    define(env, "string>=?", Arity::AtLeast(2), |a| {
        string_chain(a, |x, y| x >= y)
    });
    define(
        env,
        "string->number",
        Arity::Range { min: 1, max: 2 },
        |a| {
            let Value::String(s) = &a[0] else {
                return Err(type_err("string", &a[0]));
            };
            let s = s.borrow().clone();
            // Optional radix (2, 8, 10, 16). Default 10.
            let _radix = if a.len() == 2 {
                value_to_usize(&a[1], "string->number")?
            } else {
                10
            };
            // Quick parse via the existing parser (handles all numeric
            // forms). If the parse fails OR parses to a non-number,
            // R7RS says return #f.
            match crate::parse::parse_one(&s) {
                Ok(v) if v.is_number() => Ok(v),
                _ => Ok(Value::Bool(false)),
            }
        },
    );
    define(
        env,
        "number->string",
        Arity::Range { min: 1, max: 2 },
        |a| {
            // We render via Display for radix=10; other radices not yet
            // supported beyond integers.
            match &a[0] {
                Value::Int(_) | Value::BigInt(_) | Value::Rational(_) | Value::Float(_) => {
                    Ok(Value::string(format!("{}", a[0])))
                }
                other => Err(type_err("number", other)),
            }
        },
    );
}

fn string_chain(args: &[Value], pass: fn(&str, &str) -> bool) -> Result<Value, RuntimeError> {
    let mut prev = match &args[0] {
        Value::String(s) => s.borrow().clone(),
        other => return Err(type_err("string", other)),
    };
    for a in &args[1..] {
        let cur = match a {
            Value::String(s) => s.borrow().clone(),
            other => return Err(type_err("string", other)),
        };
        if !pass(&prev, &cur) {
            return Ok(Value::Bool(false));
        }
        prev = cur;
    }
    Ok(Value::Bool(true))
}

fn value_to_usize(v: &Value, ctx: &'static str) -> Result<usize, RuntimeError> {
    let big = value_to_bigint(v)?;
    big.to_usize()
        .ok_or_else(|| RuntimeError::Other(format!("{ctx}: integer out of usize range")))
}

// ---------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------

fn install_symbols(env: &EnvRef) {
    define(env, "symbol->string", Arity::Exact(1), |a| match &a[0] {
        Value::Symbol(s) => Ok(Value::string(s.name())),
        other => Err(type_err("symbol", other)),
    });
    define(env, "string->symbol", Arity::Exact(1), |a| match &a[0] {
        Value::String(s) => Ok(Value::Symbol(Symbol::intern(&s.borrow()))),
        other => Err(type_err("string", other)),
    });
    define(env, "symbol=?", Arity::AtLeast(2), |args| {
        let first = match &args[0] {
            Value::Symbol(s) => s.clone(),
            other => return Err(type_err("symbol", other)),
        };
        for a in &args[1..] {
            match a {
                Value::Symbol(s) if s == &first => {}
                Value::Symbol(_) => return Ok(Value::Bool(false)),
                other => return Err(type_err("symbol", other)),
            }
        }
        Ok(Value::Bool(true))
    });
}

// ---------------------------------------------------------------------
// Vectors
// ---------------------------------------------------------------------

fn install_vectors(env: &EnvRef) {
    define(env, "vector", Arity::AtLeast(0), |args| {
        Ok(Value::vector(args.to_vec()))
    });
    define(env, "make-vector", Arity::Range { min: 1, max: 2 }, |a| {
        let len = value_to_usize(&a[0], "make-vector")?;
        let fill = if a.len() == 2 {
            a[1].clone()
        } else {
            Value::Unspecified
        };
        Ok(Value::vector(vec![fill; len]))
    });
    define(env, "vector-length", Arity::Exact(1), |a| match &a[0] {
        Value::Vector(v) =>
        {
            #[allow(clippy::cast_possible_wrap)]
            Ok(Value::Int(v.borrow().len() as i64))
        }
        other => Err(type_err("vector", other)),
    });
    define(env, "vector-ref", Arity::Exact(2), |a| match &a[0] {
        Value::Vector(v) => {
            let idx = value_to_usize(&a[1], "vector-ref")?;
            v.borrow()
                .get(idx)
                .cloned()
                .ok_or_else(|| RuntimeError::Other("vector-ref: index out of range".into()))
        }
        other => Err(type_err("vector", other)),
    });
    define(env, "vector-set!", Arity::Exact(3), |a| match &a[0] {
        Value::Vector(v) => {
            let idx = value_to_usize(&a[1], "vector-set!")?;
            let mut vec_ref = v.borrow_mut();
            if idx >= vec_ref.len() {
                return Err(RuntimeError::Other(
                    "vector-set!: index out of range".into(),
                ));
            }
            vec_ref[idx] = a[2].clone();
            Ok(Value::Unspecified)
        }
        other => Err(type_err("vector", other)),
    });
    define(env, "vector->list", Arity::Exact(1), |a| match &a[0] {
        Value::Vector(v) => Ok(Value::list_from(v.borrow().iter().cloned())),
        other => Err(type_err("vector", other)),
    });
    define(env, "list->vector", Arity::Exact(1), |a| {
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
                _ => return Err(type_err("proper list", &a[0])),
            }
        }
        Ok(Value::vector(items))
    });
    define(env, "vector-fill!", Arity::Exact(2), |a| match &a[0] {
        Value::Vector(v) => {
            let mut vec_ref = v.borrow_mut();
            for slot in vec_ref.iter_mut() {
                *slot = a[1].clone();
            }
            Ok(Value::Unspecified)
        }
        other => Err(type_err("vector", other)),
    });
    define(env, "vector-copy", Arity::AtLeast(1), |a| match &a[0] {
        Value::Vector(v) => Ok(Value::vector(v.borrow().clone())),
        other => Err(type_err("vector", other)),
    });
}

// ---------------------------------------------------------------------
// Bytevectors
// ---------------------------------------------------------------------

fn install_bytevectors(env: &EnvRef) {
    define(env, "bytevector", Arity::AtLeast(0), |args| {
        let mut bytes = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Value::Int(n) if (0..=255).contains(n) => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    bytes.push(*n as u8);
                }
                _ => {
                    return Err(RuntimeError::Other(
                        "bytevector element must be integer in 0..=255".into(),
                    ));
                }
            }
        }
        Ok(Value::bytevector(bytes))
    });
    define(
        env,
        "make-bytevector",
        Arity::Range { min: 1, max: 2 },
        |a| {
            let len = value_to_usize(&a[0], "make-bytevector")?;
            let fill = if a.len() == 2 {
                match &a[1] {
                    Value::Int(n) if (0..=255).contains(n) => {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        {
                            *n as u8
                        }
                    }
                    _ => {
                        return Err(RuntimeError::Other(
                            "make-bytevector fill must be integer in 0..=255".into(),
                        ));
                    }
                }
            } else {
                0
            };
            Ok(Value::bytevector(vec![fill; len]))
        },
    );
    define(env, "bytevector-length", Arity::Exact(1), |a| match &a[0] {
        Value::Bytevector(b) =>
        {
            #[allow(clippy::cast_possible_wrap)]
            Ok(Value::Int(b.borrow().len() as i64))
        }
        other => Err(type_err("bytevector", other)),
    });
    define(env, "bytevector-u8-ref", Arity::Exact(2), |a| match &a[0] {
        Value::Bytevector(b) => {
            let idx = value_to_usize(&a[1], "bytevector-u8-ref")?;
            b.borrow()
                .get(idx)
                .map(|n| Value::Int(i64::from(*n)))
                .ok_or_else(|| RuntimeError::Other("bytevector-u8-ref: index out of range".into()))
        }
        other => Err(type_err("bytevector", other)),
    });
    define(env, "bytevector-u8-set!", Arity::Exact(3), |a| {
        match &a[0] {
            Value::Bytevector(b) => {
                let idx = value_to_usize(&a[1], "bytevector-u8-set!")?;
                let Value::Int(n) = a[2] else {
                    return Err(RuntimeError::Other(
                        "bytevector-u8-set!: value must be integer in 0..=255".into(),
                    ));
                };
                if !(0..=255).contains(&n) {
                    return Err(RuntimeError::Other(
                        "bytevector-u8-set!: value out of range".into(),
                    ));
                }
                let mut bs = b.borrow_mut();
                if idx >= bs.len() {
                    return Err(RuntimeError::Other(
                        "bytevector-u8-set!: index out of range".into(),
                    ));
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    bs[idx] = n as u8;
                }
                Ok(Value::Unspecified)
            }
            other => Err(type_err("bytevector", other)),
        }
    });
    define(env, "utf8->string", Arity::AtLeast(1), |a| match &a[0] {
        Value::Bytevector(b) => {
            let s = String::from_utf8(b.borrow().clone())
                .map_err(|e| RuntimeError::Other(format!("utf8->string: {e}")))?;
            Ok(Value::string(s))
        }
        other => Err(type_err("bytevector", other)),
    });
    define(env, "string->utf8", Arity::AtLeast(1), |a| match &a[0] {
        Value::String(s) => Ok(Value::bytevector(s.borrow().as_bytes().to_vec())),
        other => Err(type_err("string", other)),
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

;; Simple dynamic-wind. R7RS requires before/after to also fire on
;; continuation jumps in/out of the protected region. The continuation
;; bead (nscheme-0xn) documents that v1 only guarantees the linear
;; case (no escape).
(define (dynamic-wind before thunk after)
  (before)
  (let ((result (thunk)))
    (after)
    result))

;; (error msg arg ...) — build an error-object and raise it.
(define (error msg . irritants)
  (raise (apply make-error-object msg irritants)))

;; (call-with-values producer consumer) — calls (producer), then
;; applies consumer to whatever values producer returned. The producer
;; may use `values` to return zero, one, or many values; values->list
;; normalises both cases into a list that apply can spread.
(define (call-with-values producer consumer)
  (apply consumer (values->list (producer))))
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
    fn factorial_30_uses_bignum() {
        // 30! overflows i64. Verify the result is a BigInt with the
        // canonical 30! value.
        let src = "(define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))
                   (fact 30)";
        let v = run(src).unwrap();
        let expected = "265252859812191058636308480000000";
        match v {
            Value::BigInt(b) => assert_eq!(b.to_string(), expected),
            other => panic!("expected BigInt, got {other:?}"),
        }
    }

    #[test]
    fn rational_arithmetic_stays_exact() {
        // (+ 1/2 1/3) = 5/6
        let v = run("(+ 1/2 1/3)").unwrap();
        match v {
            Value::Rational(r) => {
                assert_eq!(r.numer().to_string(), "5");
                assert_eq!(r.denom().to_string(), "6");
            }
            other => panic!("expected Rational, got {other:?}"),
        }
    }

    #[test]
    fn mixing_exact_and_inexact_yields_inexact() {
        // (+ 1/2 0.5) = 1.0 (inexact)
        let v = run("(+ 1/2 0.5)").unwrap();
        assert!(matches!(v, Value::Float(f) if (f - 1.0).abs() < 1e-12));
    }

    #[test]
    fn integer_division_produces_exact_rational() {
        // 1/3 is an exact rational with the full numeric tower.
        let v = run("(/ 1 3)").unwrap();
        match v {
            Value::Rational(r) => {
                assert_eq!(r.numer().to_string(), "1");
                assert_eq!(r.denom().to_string(), "3");
            }
            other => panic!("expected Rational, got {other:?}"),
        }
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
