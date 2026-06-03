//! The `(scheme base)` library — primitive procedures.
//!
//! This file is the long catalog of every primitive (`+`, `car`,
//! `string-length`, `error-object?`, …) that the REPL needs.
//!
//! ## What you'll learn here
//!
//! - **Scheme**: which procedures the base library provides. R7RS
//!   §6 walks them; this file is the working implementation.
//! - **The numeric tower in code**: the [`Num`] enum holds the four
//!   exact rungs (`Int`, `Big`, `Rat`, `Float`) plus inexact
//!   `Complex`. `num_add` / `num_sub` / `num_mul` / `num_div` show
//!   the R7RS contagion rules (any inexact operand → inexact
//!   result; otherwise stay in the lowest-needed exact rung). See
//!   `docs/0002-numeric-tower.md` for the rationale.
//! - **Rust pattern (a fourth time)**: how a primitive's
//!   implementation lives behind a `fn` pointer with the signature
//!   [`PrimitiveFn`](crate::value::PrimitiveFn). This is what makes
//!   `Procedure::Primitive` cheap: no heap allocation, no dyn
//!   dispatch.
//!
//! ## Reading note
//!
//! The file is long because the catalog is long. The shape is
//! uniform: [`install_base`] calls a series of `install_*`
//! sub-functions, each of which is a flat table of
//! `define(env, "name", arity, |args| { … })` calls. Skim or
//! search for the procedure you care about; you don't read this
//! file front-to-back the way you do `value.rs` or `eval.rs`.
//!
//! The order of installation:
//!
//!   `install_arithmetic`   — `+`, `-`, `*`, `/`, `<`, `<=`, …
//!   `install_comparison`   — `eq?`, `eqv?`, `equal?`, `>`, `<`, …
//!   `install_predicates`   — `null?`, `pair?`, `number?`, …
//!   `install_equality`     — the equality procedures
//!   `install_list_ops`     — `car`, `cdr`, `cons`, `list`, …
//!   `install_misc`         — `error`, `exit`, the assorted rest
//!   `install_chars`        — character predicates and conversions
//!   `install_strings`      — string ops
//!   `install_symbols`      — `symbol->string`, `string->symbol`
//!   `install_vectors`      — vector ops
//!   `install_bytevectors`  — bytevector ops
//!   `install_inexact`      — `sqrt`, `sin`, `log`, …
//!   `install_io`           — port I/O (in [`crate::io`])
//!
//! After the install phase, [`BOOTSTRAP`] near the bottom of the
//! file is loaded — that's the Scheme source that defines
//! higher-order procedures (`map`, `for-each`, `member`, …) on top
//! of the primitives.
//!
//! ## Read alongside
//!
//! - [`crate::value`] — the `Value` enum these primitives operate
//!   on and the `Procedure::Primitive` variant they install into.
//! - [`crate::eval::step_apply`] — the dispatcher that calls these
//!   primitives at runtime.
//! - R7RS §6.

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
    install_inexact(env);
    crate::io::install_io(env);
    // Internal hook used by the `dynamic-wind` special form: a
    // singleton procedure that the eval loop recognises and turns
    // into a wind-aware setup. See `step_apply` in eval.rs.
    env.define(
        Symbol::intern("%dynamic-wind-apply"),
        Value::Procedure(Rc::new(Procedure::DynamicWindStart)),
    );
    eval_source(BOOTSTRAP, env.clone())?;
    eval_source(crate::io::CURRENT_PORTS_BOOTSTRAP, env.clone())?;
    Ok(())
}

thread_local! {
    /// Cache of primitive procedure objects by name. `install_base` runs
    /// more than once per thread — once for the program env, again for
    /// the hermetic library-loader root — and a primitive is a pure
    /// singleton, so both installs must bind the *same* object.
    /// Otherwise a builtin like `eq?` passed from the program into a
    /// loaded library would not be `eq?`/`equal?` to that library's own
    /// `eq?`, breaking code that compares predicates (e.g. SRFI 125's
    /// make-hash-table inferring a hash from the equality predicate).
    static PRIMITIVE_CACHE: std::cell::RefCell<std::collections::HashMap<&'static str, Value>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn define(env: &EnvRef, name: &'static str, arity: Arity, body: PrimitiveFn) {
    let v = PRIMITIVE_CACHE.with(|c| {
        c.borrow_mut()
            .entry(name)
            .or_insert_with(|| {
                Value::Procedure(Rc::new(Procedure::Primitive { name, arity, body }))
            })
            .clone()
    });
    env.define(Symbol::intern(name), v);
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
    /// Inexact complex with Cartesian (re, im).
    Complex(f64, f64),
}

impl Num {
    fn from_value(v: &Value) -> Result<Self, RuntimeError> {
        match v {
            Value::Int(n) => Ok(Self::Int(*n)),
            Value::BigInt(b) => Ok(Self::Big((**b).clone())),
            Value::Rational(r) => Ok(Self::Rat((**r).clone())),
            Value::Float(f) => Ok(Self::Float(*f)),
            Value::Complex(c) => Ok(Self::Complex(c.re_f64(), c.im_f64())),
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
            Self::Complex(re, im) => {
                if im == 0.0 {
                    Value::Float(re)
                } else {
                    Value::complex_inexact(re, im)
                }
            }
        }
    }

    fn to_f64(&self) -> f64 {
        match self {
            #[allow(clippy::cast_precision_loss)]
            Self::Int(n) => *n as f64,
            Self::Big(b) => b.to_f64().unwrap_or(f64::NAN),
            Self::Rat(r) => r.to_f64().unwrap_or(f64::NAN),
            Self::Float(f) => *f,
            Self::Complex(re, _) => *re,
        }
    }

    fn is_inexact(&self) -> bool {
        matches!(self, Self::Float(_) | Self::Complex(_, _))
    }

    fn is_complex(&self) -> bool {
        matches!(self, Self::Complex(_, _))
    }

    /// (re, im) view, with the imaginary part 0 for non-complex
    /// numbers.
    fn to_complex(&self) -> (f64, f64) {
        match self {
            Self::Complex(re, im) => (*re, *im),
            _ => (self.to_f64(), 0.0),
        }
    }

    /// Promote an exact number to a common exact level.
    fn to_rational(&self) -> BigRational {
        match self {
            Self::Int(n) => BigRational::from_i64(*n).expect("i64 to BigRational"),
            Self::Big(b) => BigRational::from_integer(b.clone()),
            Self::Rat(r) => r.clone(),
            Self::Float(_) | Self::Complex(_, _) => unreachable!("to_rational on inexact"),
        }
    }
}

/// Detect inexact (Float) numeric arguments for R7RS contagion rules.
fn is_value_inexact(v: &Value) -> bool {
    matches!(v, Value::Float(_))
}

/// R7RS-style Unicode case folding. Rust's `to_lowercase` handles
/// most one-to-one cases; this helper layers on the well-known
/// one-to-many foldings that the chibi corpus exercises:
///
/// - U+00DF (`ß`) → "ss"
/// - U+017F (`ſ`, Latin long-s) → "s"
/// - U+03C2 (`ς`, Greek final sigma) → "σ"
///
/// We post-process the `to_lowercase` output rather than running our
/// own per-codepoint table because Rust already implements the
/// bulk of the simple mappings.
fn case_fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.to_lowercase().chars() {
        match c {
            'ß' => out.push_str("ss"),
            'ſ' => out.push('s'),
            'ς' => out.push('σ'),
            other => out.push(other),
        }
    }
    out
}

/// Simplest rational in [lo, hi] (inclusive). Uses the standard
/// Stern–Brocot / continued-fraction descent: split into integer
/// and fractional parts, recurse on `1/(hi - floor) ... 1/(lo -
/// floor)` for the fractional side. Handles negative intervals
/// by reflection. R7RS `rationalize` requires the result with the
/// smallest denominator (and, among those, smallest numerator).
fn simplest_rational_between(lo: &BigRational, hi: &BigRational) -> BigRational {
    use std::cmp::Ordering;
    match lo.cmp(hi) {
        Ordering::Equal => return lo.clone(),
        Ordering::Greater => return simplest_rational_between(hi, lo),
        Ordering::Less => {}
    }
    // If 0 lies in [lo, hi], 0 is the simplest.
    let zero = BigRational::zero();
    if lo <= &zero && hi >= &zero {
        return zero;
    }
    // Reflect a fully-negative interval into the positives.
    if hi <= &zero {
        return -simplest_rational_between(&-hi, &-lo);
    }
    // Now 0 < lo < hi. If `ceil(lo) <= hi`, the simplest integer in
    // range is `ceil(lo)`.
    let lo_ceil = lo.ceil();
    if &lo_ceil <= hi {
        return lo_ceil;
    }
    // Strip the common integer part and recurse on the reciprocals.
    let lo_floor = lo.floor();
    let lo_frac = lo - &lo_floor;
    let hi_frac = hi - &lo_floor;
    let inv_lo = BigRational::one() / hi_frac;
    let inv_hi = BigRational::one() / lo_frac;
    lo_floor + BigRational::one() / simplest_rational_between(&inv_lo, &inv_hi)
}

/// Complex square root via Cartesian form. Follows the principal
/// branch — real part is non-negative; the imaginary part takes the
/// sign of the input imag (so √(-1) = +i, √(0-0i) = 0).
fn complex_sqrt(re: f64, im: f64) -> (f64, f64) {
    if im == 0.0 {
        if re >= 0.0 {
            return (re.sqrt(), 0.0);
        }
        return (0.0, (-re).sqrt());
    }
    let r = re.hypot(im);
    let s = f64::midpoint(r, re).sqrt();
    let t = f64::midpoint(r, -re).sqrt();
    if im >= 0.0 { (s, t) } else { (s, -t) }
}

/// Format a `BigInt` in a non-decimal radix for `number->string`.
fn format_int_radix(b: &BigInt, radix: u32) -> String {
    match radix {
        2 => format!("{b:b}"),
        8 => format!("{b:o}"),
        16 => format!("{b:x}"),
        _ => format!("{b}"),
    }
}

/// R7RS §6.6: digit-value returns the numeric value of any character
/// in Unicode general category Nd (decimal digit). Each Nd block is
/// ten consecutive codepoints starting at "digit zero" for some
/// script. The table below covers the Unicode 15.x Nd ranges. We
/// don't pull in a Unicode-tables crate just for this.
fn unicode_digit_value(c: char) -> Option<u32> {
    const DIGIT_ZEROS: &[u32] = &[
        0x0030,  // ASCII
        0x0660,  // Arabic-Indic
        0x06F0,  // Extended Arabic-Indic
        0x07C0,  // NKo
        0x0966,  // Devanagari
        0x09E6,  // Bengali
        0x0A66,  // Gurmukhi
        0x0AE6,  // Gujarati
        0x0B66,  // Oriya
        0x0BE6,  // Tamil
        0x0C66,  // Telugu
        0x0CE6,  // Kannada
        0x0D66,  // Malayalam
        0x0DE6,  // Sinhala Lith
        0x0E50,  // Thai
        0x0ED0,  // Lao
        0x0F20,  // Tibetan
        0x1040,  // Myanmar
        0x1090,  // Myanmar Shan
        0x17E0,  // Khmer
        0x1810,  // Mongolian
        0x1946,  // Limbu
        0x19D0,  // New Tai Lue
        0x1A80,  // Tai Tham Hora
        0x1A90,  // Tai Tham Tham
        0x1B50,  // Balinese
        0x1BB0,  // Sundanese
        0x1C40,  // Lepcha
        0x1C50,  // Ol Chiki
        0xA620,  // Vai
        0xA8D0,  // Saurashtra
        0xA900,  // Kayah Li
        0xA9D0,  // Javanese
        0xA9F0,  // Myanmar Tai Laing
        0xAA50,  // Cham
        0xABF0,  // Meetei Mayek
        0xFF10,  // Fullwidth
        0x104A0, // Osmanya
        0x10D30, // Hanifi Rohingya
        0x11066, // Brahmi
        0x110F0, // Sora Sompeng
        0x11136, // Chakma
        0x111D0, // Sharada
        0x112F0, // Khudawadi
        0x11450, // Newa
        0x114D0, // Tirhuta
        0x11650, // Modi
        0x116C0, // Takri
        0x11730, // Ahom
        0x118E0, // Warang Citi
        0x11950, // Dives Akuru
        0x11C50, // Bhaiksuki
        0x11D50, // Masaram Gondi
        0x11DA0, // Gunjala Gondi
        0x11F50, // Kawi
        0x16A60, // Mro
        0x16AC0, // Tangsa
        0x16B50, // Pahawh Hmong
        0x1D7CE, // Mathematical Bold
        0x1D7D8, // Mathematical Double-Struck
        0x1D7E2, // Mathematical Sans-Serif
        0x1D7EC, // Mathematical Sans-Serif Bold
        0x1D7F6, // Mathematical Monospace
        0x1E140, // Nyiakeng Puachue Hmong
        0x1E2F0, // Wancho
        0x1E4F0, // Nag Mundari
        0x1E950, // Adlam
        0x1FBF0, // Segmented (Mathematical) Digits
    ];
    let cp = c as u32;
    for &z in DIGIT_ZEROS {
        if cp >= z && cp < z + 10 {
            return Some(cp - z);
        }
    }
    None
}

/// Convert an exact integer/rational `Value` to its inexact (Float)
/// counterpart when contagion requires it; leave Floats unchanged.
fn maybe_inexact(v: Value, inexact: bool) -> Value {
    if !inexact {
        return v;
    }
    match &v {
        #[allow(clippy::cast_precision_loss)]
        Value::Int(n) => Value::Float(*n as f64),
        Value::BigInt(b) => Value::Float(b.to_f64().unwrap_or(f64::NAN)),
        Value::Rational(r) => Value::Float(r.to_f64().unwrap_or(f64::NAN)),
        _ => v,
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
    if a.is_complex() || b.is_complex() {
        let (ax, ay) = a.to_complex();
        let (bx, by) = b.to_complex();
        return Num::Complex(ax + bx, ay + by);
    }
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
    if a.is_complex() || b.is_complex() {
        let (ax, ay) = a.to_complex();
        let (bx, by) = b.to_complex();
        return Num::Complex(ax - bx, ay - by);
    }
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
    if a.is_complex() || b.is_complex() {
        let (ax, ay) = a.to_complex();
        let (bx, by) = b.to_complex();
        return Num::Complex(ax * bx - ay * by, ax * by + ay * bx);
    }
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
    if a.is_complex() || b.is_complex() {
        let (ax, ay) = a.to_complex();
        let (bx, by) = b.to_complex();
        let denom = bx * bx + by * by;
        if denom == 0.0 {
            return Err(RuntimeError::DivisionByZero);
        }
        return Ok(Num::Complex(
            (ax * bx + ay * by) / denom,
            (ay * bx - ax * by) / denom,
        ));
    }
    if a.is_inexact() || b.is_inexact() {
        // IEEE 754 / R7RS: inexact (flonum) division by zero yields a
        // signed infinity or NaN, not an error. Only *exact* division by
        // zero (the rational branch below) is an error.
        return Ok(Num::Float(a.to_f64() / b.to_f64()));
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
        Num::Complex(re, im) => Num::Complex(-re, -im),
    }
}

fn num_is_zero(n: &Num) -> bool {
    match n {
        Num::Int(x) => *x == 0,
        Num::Big(b) => b.is_zero(),
        Num::Rat(r) => r.is_zero(),
        Num::Float(f) => *f == 0.0,
        Num::Complex(re, im) => *re == 0.0 && *im == 0.0,
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

/// R7RS-style numeric comparison. Returns `None` if either operand is
/// a NaN (R7RS §6.2.6: NaN comparisons must yield `#f`); otherwise
/// returns the `Ordering` of the mathematical values.
fn num_cmp(a: &Num, b: &Num) -> Option<std::cmp::Ordering> {
    // R7RS: ordering on complex is undefined unless both are real.
    // Treat any non-zero imaginary part as incomparable, mirroring
    // NaN's None result.
    if a.is_complex() || b.is_complex() {
        let (_, ay) = a.to_complex();
        let (_, by) = b.to_complex();
        if ay != 0.0 || by != 0.0 {
            return None;
        }
        return a.to_f64().partial_cmp(&b.to_f64());
    }
    // When both are inexact OR both are exact, the straightforward
    // paths give exact answers.
    if a.is_inexact() && b.is_inexact() {
        return a.to_f64().partial_cmp(&b.to_f64());
    }
    if !a.is_inexact() && !b.is_inexact() {
        return Some(a.to_rational().cmp(&b.to_rational()));
    }
    // Mixed: if the inexact operand is non-finite (±inf / NaN),
    // fall back to f64 comparison — NaN yields None, infinities
    // sort to their natural extremes. Otherwise convert both sides
    // into a `BigRational` representing the f64's exact value so
    // the comparison is precise rather than rounded.
    let inexact_non_finite = matches!(a, Num::Float(f) if !f.is_finite())
        || matches!(b, Num::Float(f) if !f.is_finite());
    if inexact_non_finite {
        return a.to_f64().partial_cmp(&b.to_f64());
    }
    let to_rational = |n: &Num| match n {
        Num::Float(f) => BigRational::from_f64(*f).expect("finite float to BigRational"),
        _ => n.to_rational(),
    };
    Some(to_rational(a).cmp(&to_rational(b)))
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
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        // R7RS quotient: truncation toward zero.
        Ok(maybe_inexact(bigint_to_value(a / b), inexact))
    });
    define(env, "remainder", Arity::Exact(2), |args| {
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        // R7RS remainder: same sign as dividend.
        Ok(maybe_inexact(bigint_to_value(a % b), inexact))
    });
    define(env, "truncate-quotient", Arity::Exact(2), |args| {
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        Ok(maybe_inexact(bigint_to_value(a / b), inexact))
    });
    define(env, "truncate-remainder", Arity::Exact(2), |args| {
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        Ok(maybe_inexact(bigint_to_value(a % b), inexact))
    });
    define(env, "truncate/", Arity::Exact(2), |args| {
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        Ok(Value::Values(Rc::new(vec![
            maybe_inexact(bigint_to_value(&a / &b), inexact),
            maybe_inexact(bigint_to_value(a % b), inexact),
        ])))
    });
    define(env, "floor-quotient", Arity::Exact(2), |args| {
        use num_integer::Integer;
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        Ok(maybe_inexact(bigint_to_value(a.div_floor(&b)), inexact))
    });
    define(env, "floor-remainder", Arity::Exact(2), |args| {
        use num_integer::Integer;
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        Ok(maybe_inexact(bigint_to_value(a.mod_floor(&b)), inexact))
    });
    define(env, "floor/", Arity::Exact(2), |args| {
        use num_integer::Integer;
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
        let a = value_to_bigint(&args[0])?;
        let b = value_to_bigint(&args[1])?;
        if b.is_zero() {
            return Err(RuntimeError::DivisionByZero);
        }
        Ok(Value::Values(Rc::new(vec![
            maybe_inexact(bigint_to_value(a.div_floor(&b)), inexact),
            maybe_inexact(bigint_to_value(a.mod_floor(&b)), inexact),
        ])))
    });
    define(env, "modulo", Arity::Exact(2), |args| {
        let inexact = is_value_inexact(&args[0]) || is_value_inexact(&args[1]);
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
        Ok(maybe_inexact(bigint_to_value(m), inexact))
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
            // R7RS abs requires a real argument; calling on complex
            // is an error per §6.2.6. Surface that as a type error.
            Num::Complex(_, _) => {
                return Err(RuntimeError::Type {
                    expected: "real".into(),
                    got: "complex".into(),
                });
            }
        }
        .into_value())
    });
    define(env, "min", Arity::AtLeast(1), |args| {
        let mut best = Num::from_value(&args[0])?;
        let mut any_inexact = matches!(best, Num::Float(_));
        for a in &args[1..] {
            let n = Num::from_value(a)?;
            if matches!(n, Num::Float(_)) {
                any_inexact = true;
            }
            if num_cmp(&n, &best) == Some(std::cmp::Ordering::Less) {
                best = n;
            }
        }
        // R7RS: when any argument is inexact, the result is inexact.
        if any_inexact && !matches!(best, Num::Float(_)) {
            best = Num::Float(best.to_f64());
        }
        Ok(best.into_value())
    });
    define(env, "max", Arity::AtLeast(1), |args| {
        let mut best = Num::from_value(&args[0])?;
        let mut any_inexact = matches!(best, Num::Float(_));
        for a in &args[1..] {
            let n = Num::from_value(a)?;
            if matches!(n, Num::Float(_)) {
                any_inexact = true;
            }
            if num_cmp(&n, &best) == Some(std::cmp::Ordering::Greater) {
                best = n;
            }
        }
        if any_inexact && !matches!(best, Num::Float(_)) {
            best = Num::Float(best.to_f64());
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
        // R7RS: comparisons involving NaN are #f.
        match num_cmp(&prev, &cur) {
            Some(ord) if pass(ord) => {}
            _ => return Ok(Value::Bool(false)),
        }
        prev = cur;
    }
    Ok(Value::Bool(true))
}

fn install_comparison(env: &EnvRef) {
    use std::cmp::Ordering::{Equal, Greater, Less};
    define(env, "=", Arity::AtLeast(2), |args| {
        // R7RS `=` accepts complex values too — two complex numbers
        // are equal when both real and imaginary parts match. For
        // real-only chains the ordinary num_cmp path is enough.
        let mut prev = Num::from_value(&args[0])?;
        for a in &args[1..] {
            let cur = Num::from_value(a)?;
            let same = if prev.is_complex() || cur.is_complex() {
                let (px, py) = prev.to_complex();
                let (cx, cy) = cur.to_complex();
                #[allow(clippy::float_cmp)]
                let real_eq = px == cx;
                #[allow(clippy::float_cmp)]
                let imag_eq = py == cy;
                real_eq && imag_eq
            } else {
                matches!(num_cmp(&prev, &cur), Some(o) if o == Equal)
            };
            if !same {
                return Ok(Value::Bool(false));
            }
            prev = cur;
        }
        Ok(Value::Bool(true))
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
        Ok(Value::Bool(match &a[0] {
            Value::Int(_) | Value::BigInt(_) | Value::Rational(_) | Value::Float(_) => true,
            // A complex with an exact zero imaginary part counts as
            // real (R7RS §6.2.6). An inexact zero (0.0) does not —
            // the parser collapses exact-zero forms so any surviving
            // Complex here has either a non-zero imag or an
            // inexact-zero imag.
            Value::Complex(c) => {
                matches!(&c.im, Value::Int(0))
                    || matches!(&c.im, Value::BigInt(b) if b.is_zero())
                    || matches!(&c.im, Value::Rational(r) if r.numer().is_zero())
            }
            _ => false,
        }))
    });
    define(env, "rational?", Arity::Exact(1), |a| {
        Ok(Value::Bool(
            matches!(a[0], Value::Int(_) | Value::BigInt(_) | Value::Rational(_))
                || matches!(a[0], Value::Float(f) if f.is_finite()),
        ))
    });
    define(env, "complex?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_number()))
    });
    define(env, "exact-integer?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(
            a[0],
            Value::Int(_) | Value::BigInt(_)
        )))
    });
    define(env, "exact-rational?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(
            a[0],
            Value::Int(_) | Value::BigInt(_) | Value::Rational(_)
        )))
    });
    define(env, "exact?", Arity::Exact(1), |a| match &a[0] {
        Value::Int(_) | Value::BigInt(_) | Value::Rational(_) => Ok(Value::Bool(true)),
        Value::Float(_) | Value::Complex(_) => Ok(Value::Bool(false)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "inexact?", Arity::Exact(1), |a| match &a[0] {
        Value::Float(_) | Value::Complex(_) => Ok(Value::Bool(true)),
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
            num_cmp(&n, &Num::Int(0)) == Some(std::cmp::Ordering::Greater),
        ))
    });
    define(env, "negative?", Arity::Exact(1), |a| {
        let n = Num::from_value(&a[0])?;
        Ok(Value::Bool(
            num_cmp(&n, &Num::Int(0)) == Some(std::cmp::Ordering::Less),
        ))
    });
    define(env, "string?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_string()))
    });
    define(env, "vector?", Arity::Exact(1), |a| {
        Ok(Value::Bool(a[0].is_vector()))
    });
    define(env, "bytevector?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Bytevector(_))))
    });
    define(env, "char?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Char(_))))
    });
    define(env, "odd?", Arity::Exact(1), |a| {
        use num_integer::Integer;
        let n = value_to_bigint(&a[0])?;
        Ok(Value::Bool(n.is_odd()))
    });
    define(env, "even?", Arity::Exact(1), |a| {
        use num_integer::Integer;
        let n = value_to_bigint(&a[0])?;
        Ok(Value::Bool(n.is_even()))
    });
    define(env, "boolean=?", Arity::AtLeast(2), |args| {
        let Value::Bool(first) = args[0] else {
            return Err(RuntimeError::Type {
                expected: "boolean".into(),
                got: args[0].type_name().into(),
            });
        };
        for a in &args[1..] {
            match a {
                Value::Bool(b) if *b == first => {}
                Value::Bool(_) => return Ok(Value::Bool(false)),
                other => {
                    return Err(RuntimeError::Type {
                        expected: "boolean".into(),
                        got: other.type_name().into(),
                    });
                }
            }
        }
        Ok(Value::Bool(true))
    });
    define(env, "square", Arity::Exact(1), |a| {
        let n = Num::from_value(&a[0])?;
        Ok(num_mul(n.clone(), n).into_value())
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
    define(env, "list-set!", Arity::Exact(3), |a| {
        let idx = value_to_usize(&a[1], "list-set!")?;
        let mut cur = a[0].clone();
        for _ in 0..idx {
            match cur {
                Value::Pair(p) => cur = p.borrow().cdr.clone(),
                _ => return Err(RuntimeError::Other("list-set!: index out of range".into())),
            }
        }
        match cur {
            Value::Pair(p) => {
                p.borrow_mut().car = a[2].clone();
                Ok(Value::Unspecified)
            }
            _ => Err(RuntimeError::Other("list-set!: index out of range".into())),
        }
    });
    define(env, "list", Arity::AtLeast(0), |a| {
        Ok(Value::list_from(a.iter().cloned()))
    });
    define(env, "make-list", Arity::Range { min: 1, max: 2 }, |a| {
        let n = value_to_usize(&a[0], "make-list")?;
        let fill = if a.len() == 2 {
            a[1].clone()
        } else {
            Value::Unspecified
        };
        let items: Vec<Value> = std::iter::repeat_n(fill, n).collect();
        Ok(Value::list_from(items))
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
            Value::Complex(_) => Err(RuntimeError::Other(
                "exact: nscheme v1 has no exact-complex tower".into(),
            )),
            other => Err(RuntimeError::Type {
                expected: "number".into(),
                got: other.type_name().into(),
            }),
        }
    });
    define(env, "inexact", Arity::Exact(1), |a| {
        if let Value::Complex(_) = &a[0] {
            return Ok(a[0].clone());
        }
        let n = Num::from_value(&a[0])?;
        Ok(Value::Float(n.to_f64()))
    });
    define(env, "numerator", Arity::Exact(1), |a| match &a[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::BigInt(b) => Ok(Value::BigInt(b.clone())),
        Value::Rational(r) => Ok(bigint_to_value(r.numer().clone())),
        Value::Float(f) => {
            // R7RS: numerator/denominator of an inexact rational
            // returns an inexact integer.
            let r = BigRational::from_f64(*f)
                .ok_or_else(|| RuntimeError::Other("numerator: not a finite number".into()))?;
            Ok(Value::Float(r.numer().to_f64().unwrap_or(f64::NAN)))
        }
        other => Err(RuntimeError::Type {
            expected: "rational".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "expt", Arity::Exact(2), |args| {
        // Integer exponent: stay in the exact tower.
        if matches!(
            &args[0],
            Value::Int(_) | Value::BigInt(_) | Value::Rational(_)
        ) && matches!(&args[1], Value::Int(_) | Value::BigInt(_))
        {
            let base = Num::from_value(&args[0])?;
            let exp_big = value_to_bigint(&args[1])?;
            if exp_big.is_zero() {
                return Ok(Value::Int(1));
            }
            if exp_big.sign() == num_bigint::Sign::Minus {
                // base^(-n) = 1/(base^n)
                let pos = -exp_big;
                let exp_u32 = pos.to_u32().ok_or_else(|| {
                    RuntimeError::Other("expt: exponent magnitude too large".into())
                })?;
                let pow_rat = match base {
                    Num::Int(n) => BigRational::from_integer(BigInt::from(n).pow(exp_u32)),
                    Num::Big(b) => BigRational::from_integer(b.pow(exp_u32)),
                    Num::Rat(r) => r.pow(i32::try_from(exp_u32).map_err(|_| {
                        RuntimeError::Other("expt: exponent magnitude too large".into())
                    })?),
                    Num::Float(_) | Num::Complex(_, _) => unreachable!(),
                };
                let one = BigRational::from_integer(BigInt::from(1));
                let inv = one / pow_rat;
                return Ok(rational_to_value(inv));
            }
            let exp_u32 = exp_big
                .to_u32()
                .ok_or_else(|| RuntimeError::Other("expt: exponent magnitude too large".into()))?;
            let result = match base {
                Num::Int(n) => bigint_to_value(BigInt::from(n).pow(exp_u32)),
                Num::Big(b) => bigint_to_value(b.pow(exp_u32)),
                Num::Rat(r) => rational_to_value(r.pow(i32::try_from(exp_u32).map_err(|_| {
                    RuntimeError::Other("expt: exponent magnitude too large".into())
                })?)),
                Num::Float(_) | Num::Complex(_, _) => unreachable!(),
            };
            return Ok(result);
        }
        // Inexact (real) base with a NON-NEGATIVE exact-integer exponent:
        // use powi (repeated squaring), which rounds per-multiplication
        // like x*x*x, rather than powf (exp/log) which loses up to a ULP;
        // this makes (expt x 3) agree with (* x x x), as SRFI 144 expects.
        // Negative exponents stay on powf: powi computes them as
        // 1/base^|n|, which underflows to 0 for extreme cases like
        // (expt 2.0 -1074) where the true value is the subnormal 2^-1074.
        let base_num = Num::from_value(&args[0])?;
        if !base_num.is_complex()
            && let Value::Int(n) = &args[1]
            && *n >= 0
            && let Ok(ni) = i32::try_from(*n)
        {
            return Ok(Value::Float(base_num.to_f64().powi(ni)));
        }
        // Otherwise fall through to f64.
        let b = base_num.to_f64();
        let e = Num::from_value(&args[1])?.to_f64();
        Ok(Value::Float(b.powf(e)))
    });
    define(env, "gcd", Arity::AtLeast(0), |args| {
        use num_integer::Integer;
        if args.is_empty() {
            return Ok(Value::Int(0));
        }
        let inexact = args.iter().any(is_value_inexact);
        let mut acc = value_to_bigint(&args[0])?;
        for a in &args[1..] {
            acc = acc.gcd(&value_to_bigint(a)?);
        }
        Ok(maybe_inexact(bigint_to_value(acc.abs()), inexact))
    });
    define(env, "lcm", Arity::AtLeast(0), |args| {
        use num_integer::Integer;
        if args.is_empty() {
            return Ok(Value::Int(1));
        }
        let inexact = args.iter().any(is_value_inexact);
        let mut acc = value_to_bigint(&args[0])?;
        for a in &args[1..] {
            acc = acc.lcm(&value_to_bigint(a)?);
        }
        Ok(maybe_inexact(bigint_to_value(acc.abs()), inexact))
    });
    define(env, "exact-integer-sqrt", Arity::Exact(1), |a| {
        // Returns two values per R7RS — we return a packet.
        let n = value_to_bigint(&a[0])?;
        if n.sign() == num_bigint::Sign::Minus {
            return Err(RuntimeError::Other(
                "exact-integer-sqrt: argument must be nonnegative".into(),
            ));
        }
        let s = n.sqrt();
        let r = &n - &s * &s;
        Ok(Value::Values(Rc::new(vec![
            bigint_to_value(s),
            bigint_to_value(r),
        ])))
    });
    define(env, "list-copy", Arity::Exact(1), |a| {
        let mut items: Vec<Value> = Vec::new();
        let mut cur = a[0].clone();
        let mut tail = Value::Null;
        loop {
            match cur {
                Value::Null => break,
                Value::Pair(p) => {
                    let pair = p.borrow();
                    items.push(pair.car.clone());
                    cur = pair.cdr.clone();
                }
                other => {
                    tail = other;
                    break;
                }
            }
        }
        // Build a fresh chain (improper if tail != Null).
        let mut acc = tail;
        for item in items.into_iter().rev() {
            acc = Value::cons(item, acc);
        }
        Ok(acc)
    });
    define(env, "list-tail", Arity::Exact(2), |a| {
        let mut cur = a[0].clone();
        let k = value_to_usize(&a[1], "list-tail")?;
        for _ in 0..k {
            match cur {
                Value::Pair(p) => cur = p.borrow().cdr.clone(),
                _ => {
                    return Err(RuntimeError::Other("list-tail: index out of range".into()));
                }
            }
        }
        Ok(cur)
    });
    define(env, "digit-value", Arity::Exact(1), |a| match &a[0] {
        Value::Char(c) => {
            Ok(unicode_digit_value(*c).map_or(Value::Bool(false), |d| Value::Int(i64::from(d))))
        }
        other => Err(type_err("char", other)),
    });
    define(env, "denominator", Arity::Exact(1), |a| match &a[0] {
        Value::Int(_) | Value::BigInt(_) => Ok(Value::Int(1)),
        Value::Rational(r) => Ok(bigint_to_value(r.denom().clone())),
        Value::Float(f) => {
            let r = BigRational::from_f64(*f)
                .ok_or_else(|| RuntimeError::Other("denominator: not a finite number".into()))?;
            Ok(Value::Float(r.denom().to_f64().unwrap_or(f64::NAN)))
        }
        other => Err(RuntimeError::Type {
            expected: "rational".into(),
            got: other.type_name().into(),
        }),
    });
    // R7RS rationalize: best rational approximation within tolerance.
    // For v1 we punt: return the input as-is.
    define(env, "rationalize", Arity::Exact(2), |a| {
        // R7RS §6.2.6: `(rationalize x y)` returns the simplest
        // rational number differing from `x` by no more than `y`
        // (in absolute value). Result inherits the inexactness of
        // the arguments.
        let x_num = Num::from_value(&a[0])?;
        let y_num = Num::from_value(&a[1])?;
        let inexact_result = x_num.is_inexact() || y_num.is_inexact();
        let to_rat = |n: Num| match n {
            Num::Int(k) => BigRational::from_i64(k).unwrap_or_else(BigRational::zero),
            Num::Big(b) => BigRational::from_integer(b),
            Num::Rat(r) => r,
            Num::Float(f) if f.is_finite() => {
                BigRational::from_f64(f).unwrap_or_else(BigRational::zero)
            }
            _ => BigRational::zero(),
        };
        let xr = to_rat(x_num);
        let yr = to_rat(y_num).abs();
        let lo = &xr - &yr;
        let hi = &xr + &yr;
        let simplest = simplest_rational_between(&lo, &hi);
        let value = rational_to_value(simplest);
        if inexact_result {
            Ok(maybe_inexact(value, true))
        } else {
            Ok(value)
        }
    });
    define(env, "magnitude", Arity::Exact(1), |a| match &a[0] {
        Value::Complex(c) => Ok(Value::Float(c.re_f64().hypot(c.im_f64()))),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        Value::Int(n) => Ok(Value::Int(n.checked_abs().unwrap_or(i64::MAX))),
        Value::BigInt(b) => Ok(bigint_to_value(b.as_ref().clone().abs())),
        Value::Rational(r) => Ok(rational_to_value(r.as_ref().clone().abs())),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "angle", Arity::Exact(1), |a| {
        if let Value::Complex(c) = &a[0] {
            return Ok(Value::Float(c.im_f64().atan2(c.re_f64())));
        }
        let n = Num::from_value(&a[0])?;
        let f = n.to_f64();
        Ok(Value::Float(if f.is_sign_negative() {
            std::f64::consts::PI
        } else {
            0.0
        }))
    });
    define(env, "real-part", Arity::Exact(1), |a| match &a[0] {
        Value::Complex(c) => Ok(c.re.clone()),
        other if other.is_number() => Ok(other.clone()),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "imag-part", Arity::Exact(1), |a| match &a[0] {
        Value::Complex(c) => Ok(c.im.clone()),
        other if other.is_number() => Ok(Value::Int(0)),
        other => Err(RuntimeError::Type {
            expected: "number".into(),
            got: other.type_name().into(),
        }),
    });
    define(env, "make-rectangular", Arity::Exact(2), |a| {
        if !a[0].is_number() {
            return Err(type_err("number", &a[0]));
        }
        let im_num = Num::from_value(&a[1])?;
        // (make-rectangular x 0) → x, preserving exactness.
        if num_is_zero(&im_num) && matches!(im_num, Num::Int(_) | Num::Big(_) | Num::Rat(_)) {
            return Ok(a[0].clone());
        }
        // Preserve component exactness when neither part is complex.
        if !matches!(a[0], Value::Complex(_)) && !matches!(a[1], Value::Complex(_)) {
            return Ok(Value::Complex(Rc::new(crate::value::ComplexValue {
                re: a[0].clone(),
                im: a[1].clone(),
            })));
        }
        let re = Num::from_value(&a[0])?.to_f64();
        Ok(Value::complex_inexact(re, im_num.to_f64()))
    });
    define(env, "make-polar", Arity::Exact(2), |a| {
        let mag = Num::from_value(&a[0])?.to_f64();
        let ang = Num::from_value(&a[1])?.to_f64();
        let re = mag * ang.cos();
        let im = mag * ang.sin();
        Ok(Value::complex_inexact(re, im))
    });
    define(env, "features", Arity::Exact(0), |_| {
        Ok(Value::list_from(
            crate::library::features()
                .into_iter()
                .map(|s| Value::Symbol(Symbol::intern(s))),
        ))
    });
    // (scheme time): current-second, current-jiffy, jiffies-per-second
    define(env, "current-second", Arity::Exact(0), |_| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| RuntimeError::Other(format!("current-second: {e}")))?;
        Ok(Value::Float(now.as_secs_f64()))
    });
    define(env, "current-jiffy", Arity::Exact(0), |_| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| RuntimeError::Other(format!("current-jiffy: {e}")))?;
        // Saturate on overflow rather than panic.
        Ok(Value::Int(
            i64::try_from(now.as_nanos()).unwrap_or(i64::MAX),
        ))
    });
    define(env, "jiffies-per-second", Arity::Exact(0), |_| {
        Ok(Value::Int(1_000_000_000))
    });
    // (scheme process-context)
    define(env, "command-line", Arity::Exact(0), |_| {
        let args: Vec<Value> = std::env::args().map(Value::string).collect();
        Ok(Value::list_from(args))
    });
    define(env, "exit", Arity::Range { min: 0, max: 1 }, |a| {
        let code: i32 = match a.first() {
            None | Some(Value::Bool(true)) => 0,
            Some(Value::Int(n)) => i32::try_from(*n).unwrap_or(1),
            _ => 1,
        };
        std::process::exit(code);
    });
    define(
        env,
        "emergency-exit",
        Arity::Range { min: 0, max: 1 },
        |a| {
            let code: i32 = match a.first() {
                None | Some(Value::Bool(true)) => 0,
                Some(Value::Int(n)) => i32::try_from(*n).unwrap_or(1),
                _ => 1,
            };
            std::process::exit(code);
        },
    );
    define(
        env,
        "get-environment-variable",
        Arity::Exact(1),
        |a| match &a[0] {
            Value::String(name) => {
                Ok(std::env::var(&*name.borrow()).map_or(Value::Bool(false), Value::string))
            }
            other => Err(RuntimeError::Type {
                expected: "string".into(),
                got: other.type_name().into(),
            }),
        },
    );
    define(env, "get-environment-variables", Arity::Exact(0), |_| {
        let pairs: Vec<Value> = std::env::vars()
            .map(|(k, v)| Value::cons(Value::string(k), Value::string(v)))
            .collect();
        Ok(Value::list_from(pairs))
    });
    // (scheme eval): R7RS `environment` takes library-name list args
    // and returns an env binding only those libraries' exports.
    // For v1 we ignore the args and return a sentinel that `eval`
    // accepts.
    define(env, "environment", Arity::AtLeast(0), |_args| {
        Ok(Value::Symbol(Symbol::intern("$global-environment")))
    });
    define(env, "interaction-environment", Arity::Exact(0), |_| {
        Ok(Value::Symbol(Symbol::intern("$global-environment")))
    });
    define(env, "null-environment", Arity::Exact(1), |_| {
        Ok(Value::Symbol(Symbol::intern("$global-environment")))
    });
    define(env, "scheme-report-environment", Arity::Exact(1), |_| {
        Ok(Value::Symbol(Symbol::intern("$global-environment")))
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
    // R7RS allows a second-arg converter, but invoking a user
    // procedure from inside a primitive requires re-entering the
    // evaluator. We accept the second arg, silently ignore the
    // converter, and let the value flow through unconverted. Tracked
    // as a v1 limitation.
    define(
        env,
        "make-parameter",
        Arity::Range { min: 1, max: 2 },
        |a| {
            let cell = Rc::new(crate::value::ParameterCell {
                value: std::cell::RefCell::new(a[0].clone()),
            });
            Ok(Value::Procedure(Rc::new(Procedure::Parameter { cell })))
        },
    );
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
// (scheme inexact) — floating-point math
// ---------------------------------------------------------------------

fn install_inexact(env: &EnvRef) {
    // Single-arg f64 transcendentals: exp, log, sin, cos, tan, asin,
    // acos, sqrt. log is special-cased for the 2-arg form below.
    macro_rules! f64_unary {
        ($name:literal, $rust:ident) => {
            define(env, $name, Arity::Exact(1), |a| {
                Ok(Value::Float(Num::from_value(&a[0])?.to_f64().$rust()))
            });
        };
    }
    f64_unary!("exp", exp);
    f64_unary!("sin", sin);
    f64_unary!("cos", cos);
    f64_unary!("tan", tan);
    f64_unary!("asin", asin);
    f64_unary!("acos", acos);
    // sqrt is special: R7RS §6.2.6 requires it to be defined on the
    // entire numeric tower, so sqrt of a negative real returns a
    // complex value, and sqrt of a complex value follows the
    // standard branch (positive real part, then non-negative imag
    // when real part is zero).
    define(env, "sqrt", Arity::Exact(1), |a| {
        match &a[0] {
            Value::Complex(c) => {
                let (re, im) = complex_sqrt(c.re_f64(), c.im_f64());
                Ok(Value::complex_inexact(re, im))
            }
            v if v.is_number() => {
                // Exact integer perfect square stays exact.
                if let Value::Int(n) = v
                    && *n >= 0
                {
                    #[allow(clippy::cast_precision_loss)]
                    let f = (*n as f64).sqrt();
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let i = f as i64;
                    if i * i == *n {
                        return Ok(Value::Int(i));
                    }
                }
                let f = Num::from_value(v)?.to_f64();
                if f.is_sign_negative() {
                    let (re, im) = complex_sqrt(f, 0.0);
                    return Ok(Value::complex_inexact(re, im));
                }
                Ok(Value::Float(f.sqrt()))
            }
            other => Err(type_err("number", other)),
        }
    });
    // log: (log z) or (log z base). With one arg, natural log.
    define(env, "log", Arity::Range { min: 1, max: 2 }, |a| {
        let x = Num::from_value(&a[0])?.to_f64();
        if a.len() == 1 {
            Ok(Value::Float(x.ln()))
        } else {
            let base = Num::from_value(&a[1])?.to_f64();
            Ok(Value::Float(x.log(base)))
        }
    });
    // atan: (atan y) or (atan y x). Two-arg is atan2.
    define(env, "atan", Arity::Range { min: 1, max: 2 }, |a| {
        let y = Num::from_value(&a[0])?.to_f64();
        if a.len() == 1 {
            Ok(Value::Float(y.atan()))
        } else {
            let x = Num::from_value(&a[1])?.to_f64();
            Ok(Value::Float(y.atan2(x)))
        }
    });
    define(env, "finite?", Arity::Exact(1), |a| match &a[0] {
        Value::Float(f) => Ok(Value::Bool(f.is_finite())),
        Value::Int(_) | Value::BigInt(_) | Value::Rational(_) => Ok(Value::Bool(true)),
        Value::Complex(c) => Ok(Value::Bool(
            c.re_f64().is_finite() && c.im_f64().is_finite(),
        )),
        other => Err(type_err("number", other)),
    });
    define(env, "infinite?", Arity::Exact(1), |a| match &a[0] {
        Value::Float(f) => Ok(Value::Bool(f.is_infinite())),
        Value::Int(_) | Value::BigInt(_) | Value::Rational(_) => Ok(Value::Bool(false)),
        Value::Complex(c) => Ok(Value::Bool(
            c.re_f64().is_infinite() || c.im_f64().is_infinite(),
        )),
        other => Err(type_err("number", other)),
    });
    define(env, "nan?", Arity::Exact(1), |a| match &a[0] {
        Value::Float(f) => Ok(Value::Bool(f.is_nan())),
        Value::Int(_) | Value::BigInt(_) | Value::Rational(_) => Ok(Value::Bool(false)),
        Value::Complex(c) => Ok(Value::Bool(c.re_f64().is_nan() || c.im_f64().is_nan())),
        other => Err(type_err("number", other)),
    });
    // floor / ceiling / truncate / round — return the same exactness
    // class as the input.
    define(env, "floor", Arity::Exact(1), |a| match &a[0] {
        Value::Float(f) => Ok(Value::Float(f.floor())),
        Value::Int(_) | Value::BigInt(_) => Ok(a[0].clone()),
        Value::Rational(r) => Ok(bigint_to_value(r.floor().to_integer())),
        other => Err(type_err("number", other)),
    });
    define(env, "ceiling", Arity::Exact(1), |a| match &a[0] {
        Value::Float(f) => Ok(Value::Float(f.ceil())),
        Value::Int(_) | Value::BigInt(_) => Ok(a[0].clone()),
        Value::Rational(r) => Ok(bigint_to_value(r.ceil().to_integer())),
        other => Err(type_err("number", other)),
    });
    define(env, "truncate", Arity::Exact(1), |a| match &a[0] {
        Value::Float(f) => Ok(Value::Float(f.trunc())),
        Value::Int(_) | Value::BigInt(_) => Ok(a[0].clone()),
        Value::Rational(r) => Ok(bigint_to_value(r.trunc().to_integer())),
        other => Err(type_err("number", other)),
    });
    define(env, "round", Arity::Exact(1), |a| match &a[0] {
        // R7RS round: round-half-to-even (banker's rounding).
        Value::Float(f) => Ok(Value::Float(round_half_to_even(*f))),
        Value::Int(_) | Value::BigInt(_) => Ok(a[0].clone()),
        Value::Rational(r) => Ok(bigint_to_value(r.round().to_integer())),
        other => Err(type_err("number", other)),
    });
}

#[allow(clippy::float_cmp)]
fn round_half_to_even(f: f64) -> f64 {
    let r = f.round();
    if (f - f.trunc()).abs() == 0.5 {
        // Round half to even.
        let t = f.trunc();
        #[allow(clippy::cast_possible_truncation)]
        let ti = t as i64;
        if ti % 2 == 0 {
            t
        } else if f > 0.0 {
            t + 1.0
        } else {
            t - 1.0
        }
    } else {
        r
    }
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
    define(env, "char-foldcase", Arity::Exact(1), |a| match &a[0] {
        // R7RS char-foldcase ≈ lowercase for the ASCII/Latin1 subset.
        Value::Char(c) => Ok(Value::Char(c.to_lowercase().next().unwrap_or(*c))),
        other => Err(type_err("char", other)),
    });
    // Case-insensitive char comparisons.
    define(env, "char-ci=?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x.eq_ignore_ascii_case(&y))
    });
    define(env, "char-ci<?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x.to_lowercase().next() < y.to_lowercase().next())
    });
    define(env, "char-ci>?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x.to_lowercase().next() > y.to_lowercase().next())
    });
    define(env, "char-ci<=?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x.to_lowercase().next() <= y.to_lowercase().next())
    });
    define(env, "char-ci>=?", Arity::AtLeast(2), |a| {
        char_chain(a, |x, y| x.to_lowercase().next() >= y.to_lowercase().next())
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
    define(env, "string->list", Arity::Range { min: 1, max: 3 }, |a| {
        let Value::String(s) = &a[0] else {
            return Err(type_err("string", &a[0]));
        };
        let chars: Vec<char> = s.borrow().chars().collect();
        let start = if a.len() > 1 {
            value_to_usize(&a[1], "string->list")?
        } else {
            0
        };
        let end = if a.len() > 2 {
            value_to_usize(&a[2], "string->list")?
        } else {
            chars.len()
        };
        if end > chars.len() || start > end {
            return Err(RuntimeError::Other(
                "string->list: indices out of range".into(),
            ));
        }
        Ok(Value::list_from(
            chars[start..end].iter().copied().map(Value::Char),
        ))
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
        Value::String(s) => {
            let chars: Vec<char> = s.borrow().chars().collect();
            let start = if a.len() > 1 {
                value_to_usize(&a[1], "string-copy")?
            } else {
                0
            };
            let end = if a.len() > 2 {
                value_to_usize(&a[2], "string-copy")?
            } else {
                chars.len()
            };
            if start > end || end > chars.len() {
                return Err(RuntimeError::Other(
                    "string-copy: range out of bounds".into(),
                ));
            }
            Ok(Value::string(chars[start..end].iter().collect::<String>()))
        }
        other => Err(type_err("string", other)),
    });
    define(env, "string-upcase", Arity::Exact(1), |a| match &a[0] {
        Value::String(s) => Ok(Value::string(s.borrow().to_uppercase())),
        other => Err(type_err("string", other)),
    });
    define(env, "string-downcase", Arity::Exact(1), |a| match &a[0] {
        Value::String(s) => Ok(Value::string(s.borrow().to_lowercase())),
        other => Err(type_err("string", other)),
    });
    // R7RS string-foldcase: Unicode case folding. We approximate
    // via `to_lowercase` plus a small fix-up table that turns
    // common one-to-many foldings (German `ß` → `ss`, the Latin
    // long-s, Greek final-sigma, …) into the canonical lower-case
    // forms the corpus expects.
    define(env, "string-foldcase", Arity::Exact(1), |a| match &a[0] {
        Value::String(s) => Ok(Value::string(case_fold(&s.borrow()))),
        other => Err(type_err("string", other)),
    });
    define(env, "string->vector", Arity::AtLeast(1), |a| match &a[0] {
        Value::String(s) => {
            let all: Vec<char> = s.borrow().chars().collect();
            let start = if a.len() > 1 {
                value_to_usize(&a[1], "string->vector")?
            } else {
                0
            };
            let end = if a.len() > 2 {
                value_to_usize(&a[2], "string->vector")?
            } else {
                all.len()
            };
            if start > end || end > all.len() {
                return Err(RuntimeError::Other(
                    "string->vector: range out of bounds".into(),
                ));
            }
            Ok(Value::vector(
                all[start..end].iter().copied().map(Value::Char).collect(),
            ))
        }
        other => Err(type_err("string", other)),
    });
    define(env, "vector->string", Arity::AtLeast(1), |a| match &a[0] {
        Value::Vector(v) => {
            let vec = v.borrow();
            let start = if a.len() > 1 {
                value_to_usize(&a[1], "vector->string")?
            } else {
                0
            };
            let end = if a.len() > 2 {
                value_to_usize(&a[2], "vector->string")?
            } else {
                vec.len()
            };
            if start > end || end > vec.len() {
                return Err(RuntimeError::Other(
                    "vector->string: range out of bounds".into(),
                ));
            }
            let mut s = String::new();
            for item in &vec[start..end] {
                let Value::Char(c) = item else {
                    return Err(type_err("vector of chars", item));
                };
                s.push(*c);
            }
            Ok(Value::string(s))
        }
        other => Err(type_err("vector", other)),
    });
    define(env, "string-copy!", Arity::AtLeast(3), |a| match &a[0] {
        Value::String(dest) => {
            let at = value_to_usize(&a[1], "string-copy!")?;
            let Value::String(src) = &a[2] else {
                return Err(type_err("string", &a[2]));
            };
            let src_chars: Vec<char> = src.borrow().chars().collect();
            let start = if a.len() > 3 {
                value_to_usize(&a[3], "string-copy!")?
            } else {
                0
            };
            let end = if a.len() > 4 {
                value_to_usize(&a[4], "string-copy!")?
            } else {
                src_chars.len()
            };
            // R7RS spec: copy characters at indices start..end into
            // dest starting at `at`. We reconstruct dest as a fresh
            // String to handle UTF-8 byte/char alignment correctly.
            let dest_chars: Vec<char> = dest.borrow().chars().collect();
            let mut out: Vec<char> = dest_chars.clone();
            let count = end.saturating_sub(start);
            if at + count > out.len() {
                return Err(RuntimeError::Other(
                    "string-copy!: destination range out of bounds".into(),
                ));
            }
            out[at..at + count].copy_from_slice(&src_chars[start..end]);
            *dest.borrow_mut() = out.into_iter().collect();
            Ok(Value::Unspecified)
        }
        other => Err(type_err("string", other)),
    });
    // Case-insensitive comparisons — fold both sides via the same
    // Unicode case-fold approximation `string-foldcase` uses.
    define(env, "string-ci=?", Arity::AtLeast(2), |a| {
        string_ci_chain(a, |x, y| case_fold(x) == case_fold(y))
    });
    define(env, "string-ci<?", Arity::AtLeast(2), |a| {
        string_ci_chain(a, |x, y| case_fold(x) < case_fold(y))
    });
    define(env, "string-ci>?", Arity::AtLeast(2), |a| {
        string_ci_chain(a, |x, y| case_fold(x) > case_fold(y))
    });
    define(env, "string-ci<=?", Arity::AtLeast(2), |a| {
        string_ci_chain(a, |x, y| case_fold(x) <= case_fold(y))
    });
    define(env, "string-ci>=?", Arity::AtLeast(2), |a| {
        string_ci_chain(a, |x, y| case_fold(x) >= case_fold(y))
    });
    // Comparison ops.
    define(env, "string-set!", Arity::Exact(3), |a| {
        let Value::String(s) = &a[0] else {
            return Err(type_err("string", &a[0]));
        };
        let idx = value_to_usize(&a[1], "string-set!")?;
        let Value::Char(c) = a[2] else {
            return Err(type_err("char", &a[2]));
        };
        let mut chars: Vec<char> = s.borrow().chars().collect();
        if idx >= chars.len() {
            return Err(RuntimeError::Other(
                "string-set!: index out of range".into(),
            ));
        }
        chars[idx] = c;
        *s.borrow_mut() = chars.into_iter().collect();
        Ok(Value::Unspecified)
    });
    define(env, "string-fill!", Arity::Range { min: 2, max: 4 }, |a| {
        let Value::String(s) = &a[0] else {
            return Err(type_err("string", &a[0]));
        };
        let Value::Char(fill) = a[1] else {
            return Err(type_err("char", &a[1]));
        };
        let chars: Vec<char> = s.borrow().chars().collect();
        let start = if a.len() > 2 {
            value_to_usize(&a[2], "string-fill!")?
        } else {
            0
        };
        let end = if a.len() > 3 {
            value_to_usize(&a[3], "string-fill!")?
        } else {
            chars.len()
        };
        if end > chars.len() || start > end {
            return Err(RuntimeError::Other(
                "string-fill!: indices out of range".into(),
            ));
        }
        let mut out: Vec<char> = chars;
        for c in &mut out[start..end] {
            *c = fill;
        }
        *s.borrow_mut() = out.into_iter().collect();
        Ok(Value::Unspecified)
    });
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
            let radix = if a.len() == 2 {
                value_to_usize(&a[1], "string->number")?
            } else {
                10
            };
            if radix == 10 {
                // Fast path via the parser, which handles every
                // numeric form (exact/inexact, rationals, ±inf, etc).
                return Ok(match crate::parse::parse_one(&s) {
                    Ok(v) if v.is_number() => v,
                    _ => Value::Bool(false),
                });
            }
            // Non-decimal radix: only integer parsing for v1. Accept
            // an optional leading `+`/`-` sign.
            let (radix_u32, _) = match radix {
                2 => (2u32, "2"),
                8 => (8u32, "8"),
                16 => (16u32, "16"),
                _ => return Ok(Value::Bool(false)),
            };
            let trimmed = s.trim();
            let (sign, digits) = match trimmed.as_bytes().first() {
                Some(b'-') => (-1i64, &trimmed[1..]),
                Some(b'+') => (1i64, &trimmed[1..]),
                _ => (1i64, trimmed),
            };
            if digits.is_empty() {
                return Ok(Value::Bool(false));
            }
            // First try i64, then fall back to BigInt for big values.
            if let Ok(n) = i64::from_str_radix(digits, radix_u32) {
                return Ok(Value::Int(sign * n));
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(match BigInt::parse_bytes(digits.as_bytes(), radix_u32) {
                Some(b) => bigint_to_value(if sign < 0 { -b } else { b }),
                None => Value::Bool(false),
            })
        },
    );
    define(
        env,
        "number->string",
        Arity::Range { min: 1, max: 2 },
        |a| {
            let radix = if a.len() == 2 {
                value_to_usize(&a[1], "number->string")?
            } else {
                10
            };
            if radix == 10 {
                return match &a[0] {
                    Value::Int(_) | Value::BigInt(_) | Value::Rational(_) | Value::Float(_) => {
                        Ok(Value::string(format!("{}", a[0])))
                    }
                    other => Err(type_err("number", other)),
                };
            }
            let radix_u32 = match radix {
                2 => 2u32,
                8 => 8,
                16 => 16,
                _ => {
                    return Err(RuntimeError::Other(
                        "number->string: radix must be 2, 8, 10 or 16".into(),
                    ));
                }
            };
            match &a[0] {
                Value::Int(n) => Ok(Value::string(format_int_radix(
                    &BigInt::from(*n),
                    radix_u32,
                ))),
                Value::BigInt(b) => Ok(Value::string(format_int_radix(b, radix_u32))),
                other => Err(RuntimeError::Other(format!(
                    "number->string: non-integer not supported for radix {radix}: {other}"
                ))),
            }
        },
    );
}

fn string_ci_chain(args: &[Value], pass: fn(&str, &str) -> bool) -> Result<Value, RuntimeError> {
    string_chain(args, pass)
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

pub(crate) fn value_to_usize(v: &Value, ctx: &'static str) -> Result<usize, RuntimeError> {
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
    define(env, "vector->list", Arity::Range { min: 1, max: 3 }, |a| {
        let Value::Vector(v) = &a[0] else {
            return Err(type_err("vector", &a[0]));
        };
        let vec_ref = v.borrow();
        let start = if a.len() > 1 {
            value_to_usize(&a[1], "vector->list")?
        } else {
            0
        };
        let end = if a.len() > 2 {
            value_to_usize(&a[2], "vector->list")?
        } else {
            vec_ref.len()
        };
        if end > vec_ref.len() || start > end {
            return Err(RuntimeError::Other(
                "vector->list: indices out of range".into(),
            ));
        }
        Ok(Value::list_from(vec_ref[start..end].iter().cloned()))
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
    define(env, "vector-fill!", Arity::Range { min: 2, max: 4 }, |a| {
        let Value::Vector(v) = &a[0] else {
            return Err(type_err("vector", &a[0]));
        };
        let mut vec_ref = v.borrow_mut();
        let start = if a.len() > 2 {
            value_to_usize(&a[2], "vector-fill!")?
        } else {
            0
        };
        let end = if a.len() > 3 {
            value_to_usize(&a[3], "vector-fill!")?
        } else {
            vec_ref.len()
        };
        if end > vec_ref.len() || start > end {
            return Err(RuntimeError::Other(
                "vector-fill!: indices out of range".into(),
            ));
        }
        for slot in &mut vec_ref[start..end] {
            *slot = a[1].clone();
        }
        Ok(Value::Unspecified)
    });
    define(env, "vector-copy", Arity::AtLeast(1), |a| match &a[0] {
        Value::Vector(v) => {
            let vec = v.borrow();
            let start = if a.len() > 1 {
                value_to_usize(&a[1], "vector-copy")?
            } else {
                0
            };
            let end = if a.len() > 2 {
                value_to_usize(&a[2], "vector-copy")?
            } else {
                vec.len()
            };
            if start > end || end > vec.len() {
                return Err(RuntimeError::Other(
                    "vector-copy: range out of bounds".into(),
                ));
            }
            Ok(Value::vector(vec[start..end].to_vec()))
        }
        other => Err(type_err("vector", other)),
    });
    define(env, "vector-copy!", Arity::AtLeast(3), |a| {
        let Value::Vector(dest) = &a[0] else {
            return Err(type_err("vector", &a[0]));
        };
        let at = value_to_usize(&a[1], "vector-copy!")?;
        let Value::Vector(src) = &a[2] else {
            return Err(type_err("vector", &a[2]));
        };
        let src_borrowed = src.borrow();
        let start = if a.len() > 3 {
            value_to_usize(&a[3], "vector-copy!")?
        } else {
            0
        };
        let end = if a.len() > 4 {
            value_to_usize(&a[4], "vector-copy!")?
        } else {
            src_borrowed.len()
        };
        let chunk: Vec<Value> = src_borrowed[start..end].to_vec();
        drop(src_borrowed);
        let mut d = dest.borrow_mut();
        if at + chunk.len() > d.len() {
            return Err(RuntimeError::Other(
                "vector-copy!: destination range out of bounds".into(),
            ));
        }
        for (i, v) in chunk.into_iter().enumerate() {
            d[at + i] = v;
        }
        Ok(Value::Unspecified)
    });
    define(env, "vector-append", Arity::AtLeast(0), |args| {
        let mut out: Vec<Value> = Vec::new();
        for a in args {
            match a {
                Value::Vector(v) => out.extend(v.borrow().iter().cloned()),
                other => return Err(type_err("vector", other)),
            }
        }
        Ok(Value::vector(out))
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
            let bytes = b.borrow();
            let start = if a.len() > 1 {
                value_to_usize(&a[1], "utf8->string")?
            } else {
                0
            };
            let end = if a.len() > 2 {
                value_to_usize(&a[2], "utf8->string")?
            } else {
                bytes.len()
            };
            if start > end || end > bytes.len() {
                return Err(RuntimeError::Other(
                    "utf8->string: range out of bounds".into(),
                ));
            }
            let s = String::from_utf8(bytes[start..end].to_vec())
                .map_err(|e| RuntimeError::Other(format!("utf8->string: {e}")))?;
            Ok(Value::string(s))
        }
        other => Err(type_err("bytevector", other)),
    });
    define(env, "string->utf8", Arity::AtLeast(1), |a| match &a[0] {
        Value::String(s) => {
            let chars: Vec<char> = s.borrow().chars().collect();
            let start = if a.len() > 1 {
                value_to_usize(&a[1], "string->utf8")?
            } else {
                0
            };
            let end = if a.len() > 2 {
                value_to_usize(&a[2], "string->utf8")?
            } else {
                chars.len()
            };
            if start > end || end > chars.len() {
                return Err(RuntimeError::Other(
                    "string->utf8: range out of bounds".into(),
                ));
            }
            let sub: String = chars[start..end].iter().collect();
            Ok(Value::bytevector(sub.into_bytes()))
        }
        other => Err(type_err("string", other)),
    });
    define(env, "bytevector-copy", Arity::AtLeast(1), |a| match &a[0] {
        Value::Bytevector(b) => {
            let src = b.borrow();
            let start = if a.len() > 1 {
                value_to_usize(&a[1], "bytevector-copy")?
            } else {
                0
            };
            let end = if a.len() > 2 {
                value_to_usize(&a[2], "bytevector-copy")?
            } else {
                src.len()
            };
            Ok(Value::bytevector(src[start..end].to_vec()))
        }
        other => Err(type_err("bytevector", other)),
    });
    define(env, "bytevector-copy!", Arity::AtLeast(3), |a| {
        let Value::Bytevector(dest) = &a[0] else {
            return Err(type_err("bytevector", &a[0]));
        };
        let at = value_to_usize(&a[1], "bytevector-copy!")?;
        let Value::Bytevector(src) = &a[2] else {
            return Err(type_err("bytevector", &a[2]));
        };
        let src_borrowed = src.borrow();
        let start = if a.len() > 3 {
            value_to_usize(&a[3], "bytevector-copy!")?
        } else {
            0
        };
        let end = if a.len() > 4 {
            value_to_usize(&a[4], "bytevector-copy!")?
        } else {
            src_borrowed.len()
        };
        let chunk: Vec<u8> = src_borrowed[start..end].to_vec();
        drop(src_borrowed);
        let mut d = dest.borrow_mut();
        if at + chunk.len() > d.len() {
            return Err(RuntimeError::Other(
                "bytevector-copy!: destination range out of bounds".into(),
            ));
        }
        d[at..at + chunk.len()].copy_from_slice(&chunk);
        Ok(Value::Unspecified)
    });
    define(env, "bytevector-append", Arity::AtLeast(0), |args| {
        let mut out: Vec<u8> = Vec::new();
        for a in args {
            match a {
                Value::Bytevector(b) => out.extend_from_slice(&b.borrow()),
                other => return Err(type_err("bytevector", other)),
            }
        }
        Ok(Value::bytevector(out))
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
;; Variadic map: (map f lst1 lst2 ...) applies f to corresponding
;; elements of each list, stopping at the shortest. The single-list
;; case is the common idiom and stays simple.
(define (map f . lists)
  (cond
   ((null? lists) (error 'map-needs-at-least-one-list))
   ((null? (car lists)) '())
   ((null? (cdr lists))
    ;; single list — straightforward recursion
    (let loop ((xs (car lists)))
      (if (null? xs)
          '()
          (cons (f (car xs)) (loop (cdr xs))))))
   (else
    ;; multiple lists — walk all in lockstep, stop at any null
    (let loop ((rests lists))
      (if (let any-null? ((rs rests))
            (cond ((null? rs) #f)
                  ((null? (car rs)) #t)
                  (else (any-null? (cdr rs)))))
          '()
          (cons (apply f (map-cars rests))
                (loop (map-cdrs rests))))))))

;; Helpers used only by the multi-list map (single-list internally).
(define (map-cars lists)
  (if (null? lists) '()
      (cons (car (car lists)) (map-cars (cdr lists)))))
(define (map-cdrs lists)
  (if (null? lists) '()
      (cons (cdr (car lists)) (map-cdrs (cdr lists)))))

(define (for-each f . lists)
  (cond
   ((null? lists) (error 'for-each-needs-at-least-one-list))
   ((null? (car lists)) (if #f #f))
   ((null? (cdr lists))
    (let loop ((xs (car lists)))
      (if (null? xs)
          (if #f #f)
          (begin (f (car xs)) (loop (cdr xs))))))
   (else
    (let loop ((rests lists))
      (if (let any-null? ((rs rests))
            (cond ((null? rs) #f)
                  ((null? (car rs)) #t)
                  (else (any-null? (cdr rs)))))
          (if #f #f)
          (begin
            (apply f (map-cars rests))
            (loop (map-cdrs rests))))))))

;; vector-map and friends: implementable on top of vector-ref /
;; vector-length / vector-set! / list operations.
(define (vector-map f v . rest)
  (let* ((len (apply min (vector-length v) (map vector-length rest)))
         (out (make-vector len)))
    (let loop ((i 0))
      (if (= i len)
          out
          (begin
            (vector-set! out i
              (apply f (vector-ref v i)
                     (map (lambda (x) (vector-ref x i)) rest)))
            (loop (+ i 1)))))))

(define (vector-for-each f v . rest)
  (let ((len (apply min (vector-length v) (map vector-length rest))))
    (let loop ((i 0))
      (if (= i len)
          (if #f #f)
          (begin
            (apply f (vector-ref v i)
                   (map (lambda (x) (vector-ref x i)) rest))
            (loop (+ i 1)))))))

(define (string-map f s . rest)
  (let* ((len (apply min (string-length s) (map string-length rest)))
         (chars
          (let loop ((i 0) (acc '()))
            (if (= i len)
                (reverse acc)
                (loop (+ i 1)
                      (cons (apply f (string-ref s i)
                                   (map (lambda (x) (string-ref x i)) rest))
                            acc))))))
    (list->string chars)))

(define (string-for-each f s . rest)
  (let ((len (apply min (string-length s) (map string-length rest))))
    (let loop ((i 0))
      (if (= i len)
          (if #f #f)
          (begin
            (apply f (string-ref s i)
                   (map (lambda (x) (string-ref x i)) rest))
            (loop (+ i 1)))))))

;; The c..r compositions (R7RS 6.4). The two-level forms are in
;; (scheme base); the three- and four-level forms are (scheme cxr).
;; nscheme installs them all in the global env, so both libraries are
;; satisfied. Each is built from the next-shorter forms, so the whole
;; family is correct by construction.
(define (caar p) (car (car p)))
(define (cadr p) (car (cdr p)))
(define (cdar p) (cdr (car p)))
(define (cddr p) (cdr (cdr p)))
;; three-level: (cL1L2L3r p) = (cL1L2r (cL3r p))
(define (caaar p) (caar (car p)))
(define (caadr p) (caar (cdr p)))
(define (cadar p) (cadr (car p)))
(define (caddr p) (cadr (cdr p)))
(define (cdaar p) (cdar (car p)))
(define (cdadr p) (cdar (cdr p)))
(define (cddar p) (cddr (car p)))
(define (cdddr p) (cddr (cdr p)))
;; four-level: (cL1L2L3L4r p) = (cL1L2r (cL3L4r p))
(define (caaaar p) (caar (caar p)))
(define (caaadr p) (caar (cadr p)))
(define (caadar p) (caar (cdar p)))
(define (caaddr p) (caar (cddr p)))
(define (cadaar p) (cadr (caar p)))
(define (cadadr p) (cadr (cadr p)))
(define (caddar p) (cadr (cdar p)))
(define (cadddr p) (cadr (cddr p)))
(define (cdaaar p) (cdar (caar p)))
(define (cdaadr p) (cdar (cadr p)))
(define (cdadar p) (cdar (cdar p)))
(define (cdaddr p) (cdar (cddr p)))
(define (cddaar p) (cddr (caar p)))
(define (cddadr p) (cddr (cadr p)))
(define (cdddar p) (cddr (cdar p)))
(define (cddddr p) (cddr (cddr p)))

;; dynamic-wind is implemented as a Rust special form (see
;; step_dynamic_wind in eval.rs) so its `before` / `after` thunks
;; can fire on call/cc jumps that cross the wind boundary.

;; (error msg arg ...) — build an error-object and raise it.
(define (error msg . irritants)
  (raise (apply make-error-object msg irritants)))

;; R7RS member/assoc support an optional 3rd-arg equality predicate.
;; The primitive versions (defined in Rust) handle the 2-arg case
;; with equal?. Override here in Scheme for the variadic shape.
(define (member obj list . maybe-compare)
  (let ((cmp (if (null? maybe-compare) equal? (car maybe-compare))))
    (let loop ((xs list))
      (cond ((null? xs) #f)
            ((cmp obj (car xs)) xs)
            (else (loop (cdr xs)))))))

(define (assoc obj alist . maybe-compare)
  (let ((cmp (if (null? maybe-compare) equal? (car maybe-compare))))
    (let loop ((xs alist))
      (cond ((null? xs) #f)
            ((and (pair? (car xs)) (cmp obj (caar xs))) (car xs))
            (else (loop (cdr xs)))))))

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
        // R7RS §6.11 conditions: errors from primitives raise so
        // (with-exception-handler …) / (guard …) can catch them.
        // An uncaught raise surfaces as EvalError::Raised carrying
        // an error-object.
        let err = run("(/ 1 0)").unwrap_err();
        match err {
            EvalError::Raised(Value::ErrorObject(e)) => {
                assert!(e.message.contains("division by zero"));
            }
            other => panic!("expected EvalError::Raised, got {other:?}"),
        }
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
