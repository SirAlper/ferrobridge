//! The escape hatch: the raw cxx layer (`ffi`) under the safe layer can also be used directly.
//! For copy-free access or types the safe layer does not convert yet (a mutable string, a
//! string vector parameter).

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use cxx::{CxxVector, let_cxx_string};
use ferrobridge_e2e::{classes, enums, ownership, strings, vectors};

#[test]
fn classes_raw() {
    let mut vehicle = classes::ffi::Vehicle_new_i32(4).unwrap();
    assert_eq!(vehicle.raw_wheels().unwrap(), 4);
    vehicle.pin_mut().raw_drive(1.5).unwrap();
    // Safe methods work on the raw object too.
    assert_eq!(vehicle.wheels().unwrap(), 4);
}

#[test]
fn strings_raw_mutable_string() {
    let_cxx_string!(text = "Careful");
    strings::ffi::append_exclamation(text.as_mut()).unwrap();
    assert_eq!(text.to_str().unwrap(), "Careful!");
}

#[test]
fn enums_raw_discriminants_match_cpp() {
    assert_eq!(enums::ffi::Mode::from(enums::Mode::Hybrid).repr, 10);
    assert_eq!(enums::ffi::Level::High.repr, 200);
    assert_eq!(enums::ffi::Color::Blue.repr, 6);
    assert_eq!(enums::ffi::Offset::Minus.repr, -3);
}

#[test]
fn vectors_raw_in_place_and_string_vectors() {
    let mut values = CxxVector::<f64>::new();
    values.pin_mut().push(4.0);
    values.pin_mut().push(8.0);
    vectors::ffi::append_halves(values.pin_mut()).unwrap();
    assert_eq!(
        values.iter().copied().collect::<Vec<_>>(),
        [4.0, 8.0, 2.0, 4.0]
    );

    let_cxx_string!(text = "a b c");
    let words = vectors::ffi::split_words(&text).unwrap();
    assert_eq!(vectors::ffi::count_words(&words).unwrap(), 3);
}

#[test]
fn ownership_wrapper_round_trips_through_unique_ptr() {
    let widget = ownership::Widget::new_i32(3).unwrap();
    let raw = widget.into_inner();
    assert_eq!(raw.raw_id(), 3);
    let widget = ownership::Widget::from_unique_ptr(raw).unwrap();
    assert_eq!(widget.id(), 3);
    assert!(ownership::Widget::from_unique_ptr(cxx::UniquePtr::null()).is_none());
}
