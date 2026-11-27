//! `tests/fixtures/vectors`, with the safe API: `&[T]` in, `Vec<T>` out.

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use ferrobridge::Error;
use ferrobridge_e2e::vectors::{self as geo, Sample};

#[test]
fn slices_go_in() -> ferrobridge::Result<()> {
    assert_eq!(geo::sum(&[1, 2, 3])?, 6);
    assert_eq!(geo::sum(&[])?, 0);
    Ok(())
}

#[test]
fn string_vectors_come_out_as_vec_string() -> ferrobridge::Result<()> {
    assert_eq!(geo::split_words("one two  three")?, ["one", "two", "three"]);
    Ok(())
}

#[test]
fn class_vectors_stay_cpp_vectors() -> ferrobridge::Result<()> {
    // The elements are opaque C++ objects: they cannot be taken into a `Vec` by value; they stay a C++ vector.
    let samples = geo::make_samples(3)?;
    assert_eq!(samples.len(), 3);
    assert_eq!(samples.get(2).unwrap().value()?, 20);
    assert_eq!(geo::first_value(&samples)?, 0);
    // `std::vector::at` throws on an empty vector.
    let empty = geo::make_samples(0)?;
    assert!(matches!(
        geo::first_value(&empty),
        Err(Error::Cpp {
            item: "geo::first_value",
            ..
        })
    ));
    assert_eq!(Sample::new(5)?.value()?, 5);
    Ok(())
}
