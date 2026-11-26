//! `tests/fixtures/classes`: elle yazılmış hiçbir bridge ya da `unsafe` olmadan.

use ferrobridge_e2e::classes::ffi;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// C++'taki `length`, `float` alanlarla `std::sqrt(float)` hesaplıyor; beklenen değer de
/// `f32` hassasiyetinde.
fn sqrt5_as_float() -> f64 {
    f64::from(5.0_f32.sqrt())
}

#[test]
fn constructs_an_object_and_calls_its_methods() {
    let mut vehicle = ffi::Vehicle_new_i32(4).unwrap();
    assert_eq!(vehicle.wheels().unwrap(), 4);
    // `noexcept` metot: `Result` değil, doğrudan değer.
    assert!(close(vehicle.odometer(), 0.0));
    vehicle.pin_mut().drive(12.5).unwrap();
    assert!(close(vehicle.odometer(), 12.5));
}

#[test]
fn constructor_overloads_and_virtual_and_static_methods() {
    let vehicle = ffi::Vehicle_new_i32_f64(6, 100.0).unwrap();
    assert!(close(vehicle.odometer(), 100.0));
    assert_eq!(vehicle.honk().unwrap(), 60);
    assert_eq!(ffi::Vehicle_max_wheels().unwrap(), 18);
}

#[test]
fn cpp_exceptions_become_errors() {
    let mut vehicle = ffi::Vehicle_new_i32(4).unwrap();
    let error = vehicle.pin_mut().break_down().unwrap_err();
    assert_eq!(error.what(), "motor arızası");
    // `std::exception` türevi olmayan exception da program sonlanmadan `Err` olur.
    let error = vehicle.pin_mut().throw_unknown().unwrap_err();
    assert!(error.what().contains("bilinmeyen"), "{}", error.what());
    // Nesne hâlâ kullanılabilir.
    assert_eq!(vehicle.wheels().unwrap(), 4);
}

#[test]
fn classes_returned_by_value_and_by_reference() {
    let mut vehicle = ffi::Vehicle_new_i32(4).unwrap();
    vehicle.pin_mut().drive(3.0).unwrap();
    // Değerle dönen `Point`: shim `unique_ptr`'a sarar.
    let position = ffi::Vehicle_position(&vehicle).unwrap();
    assert!(close(ffi::geo_length(&position).unwrap(), 3.0));
    // Referansla dönen `const Point&`: ömrü `vehicle`'a bağlı.
    let home_length = ffi::geo_length(vehicle.home().unwrap()).unwrap();
    assert!(close(home_length, sqrt5_as_float()));
}

#[test]
fn method_overloads_with_class_and_primitive_params() {
    let mut vehicle = ffi::Vehicle_new_i32(4).unwrap();
    vehicle.pin_mut().move_to_f32_f32(3.0, 4.0).unwrap();
    let position = ffi::Vehicle_position(&vehicle).unwrap();
    assert!(close(ffi::geo_length(&position).unwrap(), 5.0));

    let home = ffi::Vehicle_new_i32(2).unwrap();
    vehicle
        .pin_mut()
        .move_to_Point(home.home().unwrap())
        .unwrap();
    let position = ffi::Vehicle_position(&vehicle).unwrap();
    assert!(close(ffi::geo_length(&position).unwrap(), sqrt5_as_float()));
}

#[test]
fn free_function_overloads_and_namespaces() {
    assert_eq!(ffi::clamp_i32_i32_i32(15, 0, 10).unwrap(), 10);
    assert!(close(ffi::clamp_f64_f64_f64(-1.5, 0.0, 1.0).unwrap(), 0.0));
    assert!(close(ffi::physics_length(7.0).unwrap(), 7.0));
}

#[test]
fn abstract_classes_through_unique_ptr_and_virtual_dispatch() {
    let shape = ffi::make_unit_square().unwrap();
    assert!(close(shape.area().unwrap(), 1.0));
    let square = ffi::Square_new(2.0).unwrap();
    assert!(close(square.area().unwrap(), 4.0));
}
