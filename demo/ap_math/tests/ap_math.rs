//! `AP_Math/AP_Math.h`: `static inline` açı dönüşümleri, overload'lar, sınırlama, `&mut [T]`.

#![forbid(unsafe_code)]

use std::f32::consts::PI;

use ferrobridge_demo_ap_math::ap_math as m;

#[test]
fn static_inline_angle_conversions() -> ferrobridge::Result<()> {
    assert_eq!(m::radians_f32(180.0)?, PI);
    assert_eq!(m::radians_f64(90.0)?, std::f64::consts::FRAC_PI_2);
    assert_eq!(m::radians_i32(180)?, PI);
    assert_eq!(m::degrees(PI)?, 180.0);
    assert_eq!(m::cd_to_rad(18_000.0)?, PI);
    assert_eq!(m::sq(3.0)?, 9.0);
    Ok(())
}

#[test]
fn wrapping_and_constraining() -> ferrobridge::Result<()> {
    assert_eq!(m::wrap_360_f32(-90.0)?, 270.0);
    assert_eq!(m::wrap_360_cd_i32(-9_000)?, 27_000);
    assert_eq!(m::constrain_int16(500, -100, 100)?, 100);
    assert_eq!(m::constrain_double(-1.5, 0.0, 1.0)?, 0.0);
    assert_eq!(m::norm_f32_f32(3.0, 4.0)?, 5.0);
    assert_eq!(m::hz_to_usec(400)?, 2_500);
    assert_eq!(m::get_twos_complement(0xFF, 8)?, -1);
    Ok(())
}

#[test]
fn fill_nanf_fills_a_mutable_slice() -> ferrobridge::Result<()> {
    let mut values = [1.0_f32; 4];
    m::fill_nanf_mut_slice_f32(&mut values)?;
    assert!(values.iter().all(|v| v.is_nan()), "{values:?}");
    let mut doubles = [1.0_f64; 2];
    m::fill_nanf_mut_slice_f64(&mut doubles)?;
    assert!(doubles.iter().all(|v| v.is_nan()), "{doubles:?}");
    m::fill_nanf_mut_slice_f32(&mut [])?;
    Ok(())
}
