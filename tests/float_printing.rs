//! Float printing (bead nscheme-ecg).
//!
//! `write_float` now uses Ryu for the shortest round-tripping digits,
//! reformatted to Scheme conventions. Two guarantees matter:
//!   1. round-trip — `(string->number (number->string x))` recovers x
//!      exactly (R7RS §6.2.6), for *every* finite f64;
//!   2. canonical form — specific values print the way chibi/Racket do.

use nscheme::value::Value;

/// The Scheme `write` form of a float (Debug == write in this crate).
fn show(x: f64) -> String {
    format!("{:?}", Value::Float(x))
}

#[test]
fn known_values_print_canonically() {
    let cases: &[(f64, &str)] = &[
        (0.0, "0.0"),
        (1.0, "1.0"),
        (-1.0, "-1.0"),
        (0.1, "0.1"),
        (0.3, "0.3"),
        (0.1 + 0.2, "0.30000000000000004"), // the shortest digit, not 0.3
        (1.0 / 3.0, "0.3333333333333333"),
        (100.0, "100.0"),
        (123_456_789.0, "123456789.0"),
        (0.001, "0.001"),
        (1e30, "1.0e+30"),
        (1e-100, "1.0e-100"),
        (1.5e-8, "1.5e-8"),
        (6.02e23, "6.02e+23"),
        (2.0_f64.sqrt(), "1.4142135623730951"),
    ];
    for (x, expected) in cases {
        assert_eq!(&show(*x), expected, "printing {x}");
    }
}

#[test]
fn negative_zero_keeps_its_sign() {
    assert_eq!(show(-0.0), "-0.0");
    assert_eq!(show(0.0), "0.0");
}

#[test]
fn special_values() {
    assert_eq!(show(f64::INFINITY), "+inf.0");
    assert_eq!(show(f64::NEG_INFINITY), "-inf.0");
    assert_eq!(show(f64::NAN), "+nan.0");
}

/// Every finite `f64` must round-trip through its printed form. Sweep a
/// deterministic pseudo-random set of bit patterns plus the structural
/// edges (subnormals, `MAX`, `MIN_POSITIVE`, powers of ten).
#[test]
fn round_trips_for_a_wide_sample() {
    let mut checked = 0u64;
    let mut check = |x: f64| {
        if x.is_finite() {
            let s = show(x);
            let back: f64 = s.parse().unwrap_or_else(|_| panic!("unparseable: {s:?} for {x}"));
            assert_eq!(back.to_bits(), x.to_bits(), "round-trip failed: {x} -> {s:?}");
            checked += 1;
        }
    };

    // Structural edges.
    for x in [
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::from_bits(1),      // smallest subnormal
        f64::from_bits(0x000f_ffff_ffff_ffff), // largest subnormal
        5e-324,                                // == smallest subnormal, via literal
    ] {
        check(x);
    }
    for p in -30..=30 {
        check(10f64.powi(p));
        check(2f64.powi(p));
    }

    // Deterministic pseudo-random bit patterns (xorshift64; no rand dep).
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for _ in 0..200_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        check(f64::from_bits(state));
    }
    assert!(checked > 100_000, "sample too small: {checked}");
}
