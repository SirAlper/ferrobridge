//! `AP_Math.h`, `control.h`, `location.h`, `SCurve.h`, `definitions.h`: `static inline`
//! functions, output parameters, owned C++ objects, constants.

#![forbid(unsafe_code)]

use std::f32::consts::PI;

use ferrobridge_demo_ap_math::{self as ap, SCurve};

#[test]
fn static_inline_angle_conversions_return_plain_values() {
    assert_eq!(ap::radians_f32(180.0), PI);
    assert_eq!(ap::degrees(PI), 180.0);
    assert_eq!(ap::cd_to_rad(18_000.0), PI);
    assert_eq!(ap::wrap_360_f32(-90.0), 270.0);
    assert_eq!(ap::constrain_int16(500, -100, 100), 100);
}

#[test]
fn fill_nanf_fills_a_mutable_slice() -> ferrobridge::Result<()> {
    let mut values = [1.0_f32; 4];
    ap::fill_nanf_mut_slice_f32(&mut values)?;
    assert!(values.iter().all(|v| v.is_nan()), "{values:?}");
    Ok(())
}

#[test]
fn shape_accel_moves_toward_the_target_within_the_jerk_limit() {
    let mut accel = 0.0_f32;
    // Target 10 m/s², jerk limit 5 m/s³, step 0.1 s: it can change by at most 0.5 m/s².
    ap::shape_accel(10.0, &mut accel, 5.0, 0.1);
    assert!((accel - 0.5).abs() < 1e-6, "{accel}");
}

#[test]
fn scurve_is_an_owned_cpp_object() -> ferrobridge::Result<()> {
    // The constructor of an opaque class returns the owning wrapper inside `Result`.
    let mut curve = SCurve::new()?;
    curve.init();
    assert!(curve.finished());
    Ok(())
}

#[test]
fn wgs84_constants_are_the_exact_cpp_values() {
    assert_eq!(ap::WGS84_A, 6_378_137.0);
    assert_eq!(
        ap::WGS84_B.to_bits(),
        (ap::WGS84_A * (1.0 - ap::WGS84_F)).to_bits()
    );
}

#[test]
fn latitude_checks() {
    assert!(ap::check_lat_f32(45.0));
    assert!(!ap::check_lat_f32(91.0));
}
