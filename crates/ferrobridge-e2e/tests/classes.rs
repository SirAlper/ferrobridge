//! `tests/fixtures/classes`, güvenli API ile: elle yazılmış bridge, `unsafe`, `Pin` ya da
//! cxx tipi yok. Ham katman `raw_layer.rs`'te.

use ferrobridge::Error;
use ferrobridge_e2e::classes::{self as geo, Square, Vehicle};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// C++'taki `length`, `float` alanlarla `std::sqrt(float)` hesaplıyor; beklenen değer de
/// `f32` hassasiyetinde.
fn sqrt5_as_float() -> f64 {
    f64::from(5.0_f32.sqrt())
}

#[test]
fn constructs_an_object_and_calls_its_methods() -> ferrobridge::Result<()> {
    let mut vehicle = Vehicle::new_i32(4)?;
    assert_eq!(vehicle.wheels()?, 4);
    // `noexcept` metot: `Result` değil, doğrudan değer.
    assert!(close(vehicle.odometer(), 0.0));
    vehicle.drive(12.5)?;
    assert!(close(vehicle.odometer(), 12.5));
    Ok(())
}

#[test]
fn constructor_overloads_and_virtual_and_static_methods() -> ferrobridge::Result<()> {
    let vehicle = Vehicle::new_i32_f64(6, 100.0)?;
    assert!(close(vehicle.odometer(), 100.0));
    assert_eq!(vehicle.honk()?, 60);
    assert_eq!(Vehicle::max_wheels()?, 18);
    Ok(())
}

#[test]
fn cpp_exceptions_become_ferrobridge_errors() -> ferrobridge::Result<()> {
    let mut vehicle = Vehicle::new_i32(4)?;
    let error = vehicle.break_down().unwrap_err();
    assert_eq!(
        error,
        Error::Cpp {
            item: "geo::Vehicle::break_down",
            message: "motor arızası".to_owned()
        }
    );
    assert_eq!(error.to_string(), "geo::Vehicle::break_down: motor arızası");
    // `std::exception` türevi olmayan exception da program sonlanmadan `Err` olur.
    let error = vehicle.throw_unknown().unwrap_err();
    assert!(error.to_string().contains("bilinmeyen"), "{error}");
    // Nesne hâlâ kullanılabilir.
    assert_eq!(vehicle.wheels()?, 4);
    Ok(())
}

#[test]
fn classes_returned_by_value_and_by_reference() -> ferrobridge::Result<()> {
    let mut vehicle = Vehicle::new_i32(4)?;
    vehicle.drive(3.0)?;
    // Değerle dönen `Point`: sahip olan sarmalayıcı.
    let position = vehicle.position()?;
    assert!(close(geo::geo_length(&position)?, 3.0));
    // Referansla dönen `const Point&`: ömrü `vehicle`'a bağlı.
    assert!(close(geo::geo_length(vehicle.home()?)?, sqrt5_as_float()));
    Ok(())
}

#[test]
fn method_overloads_with_class_and_primitive_params() -> ferrobridge::Result<()> {
    let mut vehicle = Vehicle::new_i32(4)?;
    vehicle.move_to_f32_f32(3.0, 4.0)?;
    // Sarmalayıcı `&Point` → `&ffi::Point` deref coercion'ı, değer bir değişkene bağlıyken
    // çalışır (`&f()?` biçiminde `?`'in tipi bağlamdan çıkarılamaz).
    let position = vehicle.position()?;
    assert!(close(geo::geo_length(&position)?, 5.0));

    let other = Vehicle::new_i32(2)?;
    vehicle.move_to_Point(other.home()?)?;
    let position = vehicle.position()?;
    assert!(close(geo::geo_length(&position)?, sqrt5_as_float()));
    Ok(())
}

#[test]
fn free_function_overloads_and_namespaces() -> ferrobridge::Result<()> {
    assert_eq!(geo::clamp_i32_i32_i32(15, 0, 10)?, 10);
    assert!(close(geo::clamp_f64_f64_f64(-1.5, 0.0, 1.0)?, 0.0));
    assert!(close(geo::physics_length(7.0)?, 7.0));
    Ok(())
}

#[test]
fn abstract_classes_through_unique_ptr_and_virtual_dispatch() -> ferrobridge::Result<()> {
    let shape = geo::make_unit_square()?.expect("C++ null olmayan bir şekil döndürür");
    assert!(close(shape.area()?, 1.0));
    let square = Square::new(2.0)?;
    assert!(close(square.area()?, 4.0));
    Ok(())
}
