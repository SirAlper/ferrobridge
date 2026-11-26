//! `tests/fixtures/enums`: değerler C++'takilerle derleme zamanında doğrulandı; burada
//! çalışma zamanında gidip gelmeleri test ediliyor.

use ferrobridge_e2e::enums::ffi::{self, Color, Level, Mode, Offset};

#[test]
fn scoped_enums_round_trip() {
    assert_eq!(ffi::next(Mode::Manual).unwrap(), Mode::Auto);
    assert_eq!(ffi::next(Mode::Auto).unwrap(), Mode::Hybrid);
    assert_eq!(ffi::next(Mode::Hybrid).unwrap(), Mode::Manual);
}

#[test]
fn unscoped_enum_parameter_and_noexcept_function() {
    assert_eq!(ffi::level_for(Color::Red), Level::High);
    assert_eq!(ffi::level_for(Color::Green), Level::Low);
}

#[test]
fn discriminants_match_cpp() {
    assert_eq!(Mode::Hybrid.repr, 10);
    assert_eq!(Level::High.repr, 200);
    assert_eq!(Color::Blue.repr, 6);
    assert_eq!(Offset::Minus.repr, -3);
}
