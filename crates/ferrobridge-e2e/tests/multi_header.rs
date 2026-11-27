//! `tests/fixtures/multi_header`: types from `geometry.h` in the functions of `input.h` in the
//! same bridge.

use ferrobridge_e2e::multi_header::{self as geo, Axis, Point};

#[test]
fn types_from_one_header_are_used_by_functions_of_another() -> ferrobridge::Result<()> {
    let a = Point::new(0.0, 0.0)?;
    let b = Point::new(3.0, 4.0)?;
    assert_eq!(geo::distance(&a, &b)?, 5.0);
    assert_eq!(geo::dominant(&b)?, Axis::Y);
    Ok(())
}
