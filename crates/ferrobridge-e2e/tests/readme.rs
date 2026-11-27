//! A copy of the README's "Generating a bridge" example, compiled with real fixtures. Update
//! this file when the README changes.

use ferrobridge_e2e::classes::Vehicle;
use ferrobridge_e2e::enums::{self, Mode};
use ferrobridge_e2e::strings;

#[test]
fn readme_example() -> ferrobridge::Result<()> {
    let mut car = Vehicle::new_i32(4)?; // C++ constructor; an owning wrapper
    car.drive(12.5)?; // &mut self; C++ exception → Err
    let km = car.odometer(); // noexcept → a plain value
    let text: String = strings::greet("Ada")?; // &str in, String out
    let next = match enums::next(Mode::Auto)? {
        // a real Rust enum, checked for exhaustiveness
        Mode::Manual => "manual",
        Mode::Auto => "automatic",
        Mode::Hybrid => "hybrid",
    };
    assert!((km - 12.5).abs() < 1e-9);
    assert_eq!(text, "Hello, Ada!");
    assert_eq!(next, "hybrid");
    Ok(())
}
