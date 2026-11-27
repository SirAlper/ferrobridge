//! `tests/fixtures/slices`, with the safe API: pointer + length pairs annotated `{ slice = "len" }`
//! are a single `&[T]` / `&mut [T]` in Rust.

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use ferrobridge::Error;
use ferrobridge_e2e::slices::{self as buf, Buffer};

#[test]
fn crc8_matches_the_reference_check_value() -> ferrobridge::Result<()> {
    // CRC-8 (polynomial 0x07, initial value 0, not reflected): "123456789" → 0xF4.
    assert_eq!(buf::crc8(b"123456789")?, 0xF4);
    assert_eq!(buf::crc8(&[])?, 0);
    Ok(())
}

#[test]
fn a_slice_longer_than_the_cpp_length_type_is_an_error_not_a_truncation() {
    // `uint8_t len`: 255 elements fit, 256 do not.
    assert!(buf::crc8(&[0xAB; 255]).is_ok());
    let error = buf::crc8(&[0xAB; 256]).unwrap_err();
    let Error::Cpp { item, message } = &error else {
        panic!("Error::Cpp bekleniyordu: {error:?}");
    };
    assert_eq!(*item, "buf::crc8");
    assert!(message.contains("too long"), "{message}");
    // The check returns `Err` in a `noexcept` function too (the program does not end).
    assert_eq!(buf::count_nonzero(&[0, 1, 2, 0]).unwrap(), 2);
    assert!(buf::count_nonzero(&vec![0; 65_536]).is_err());
}

#[test]
fn length_may_come_first_and_have_a_platform_dependent_type() -> ferrobridge::Result<()> {
    // C++: `double sum(std::size_t n, const double* values)`.
    assert_eq!(buf::sum(&[1.5, 2.5, -1.0])?, 3.0);
    assert_eq!(buf::sum(&[])?, 0.0);
    Ok(())
}

#[test]
fn mutable_slices_are_changed_in_place() -> ferrobridge::Result<()> {
    let mut data = [1.0_f32, -2.0, 0.5];
    buf::scale(&mut data, 2.0)?;
    assert_eq!(data, [2.0, -4.0, 1.0]);
    Ok(())
}

#[test]
fn two_slices_carry_their_own_lengths() -> ferrobridge::Result<()> {
    assert_eq!(buf::dot(&[1, 2, 3], &[4, 5, 6])?, 32);
    // C++ uses the length of the shorter one: the lengths really are passed separately.
    assert_eq!(buf::dot(&[1, 2, 3], &[10])?, 10);
    Ok(())
}

#[test]
fn methods_take_slices_too() -> ferrobridge::Result<()> {
    let mut buffer = Buffer::new()?;
    buffer.append(&[1, 2, 3])?;
    buffer.append(b"abcd")?;
    assert_eq!(buffer.total()?, 7);
    Ok(())
}
