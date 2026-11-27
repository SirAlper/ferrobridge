//! `tests/fixtures/enums`, with the safe API: real Rust `enum`s.

use ferrobridge::Error;
use ferrobridge_e2e::enums::{self as io, Color, Level, Mode};

#[test]
fn scoped_enums_round_trip() -> ferrobridge::Result<()> {
    assert_eq!(io::next(Mode::Manual)?, Mode::Auto);
    assert_eq!(io::next(Mode::Auto)?, Mode::Hybrid);
    assert_eq!(io::next(Mode::Hybrid)?, Mode::Manual);
    Ok(())
}

#[test]
fn matches_are_exhaustive() -> ferrobridge::Result<()> {
    // No `_` arm: because it is a real Rust enum, the compiler checks exhaustiveness.
    let label = match io::next(Mode::Manual)? {
        Mode::Manual => "manual",
        Mode::Auto => "automatic",
        Mode::Hybrid => "hybrid",
    };
    assert_eq!(label, "automatic");
    Ok(())
}

#[test]
fn unscoped_enum_parameter_and_noexcept_function() -> ferrobridge::Result<()> {
    assert_eq!(io::level_for(Color::Red)?, Level::High);
    assert_eq!(io::level_for(Color::Green)?, Level::Low);
    Ok(())
}

#[test]
fn undefined_enum_value_from_cpp_is_an_error() {
    assert_eq!(
        io::broken_mode(),
        Err(Error::UnknownEnumValue {
            ty: "io::Mode",
            value: 42
        })
    );
}
