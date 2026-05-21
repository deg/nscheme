//! Smoke test that proves the crate builds and exposes its public surface.
//! Real language tests live next to the modules they exercise.

#[test]
fn crate_exposes_version() {
    assert!(!nscheme::VERSION.is_empty());
}
