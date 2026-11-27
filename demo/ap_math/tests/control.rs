//! `AP_Math/control.h`: `float&` giriş-çıkış parametreleri `&mut f32` olarak.

#![forbid(unsafe_code)]

use ferrobridge_demo_ap_math::control as c;

#[test]
fn shape_accel_moves_toward_the_target_within_the_jerk_limit() -> ferrobridge::Result<()> {
    let mut accel = 0.0_f32;
    // Hedef 10 m/s², sarsıntı sınırı 5 m/s³, adım 0.1 s: en fazla 0.5 m/s² değişebilir.
    c::shape_accel(10.0, &mut accel, 5.0, 0.1)?;
    assert!((accel - 0.5).abs() < 1e-6, "{accel}");
    Ok(())
}

#[test]
fn centred_sticks_give_level_attitude() -> ferrobridge::Result<()> {
    let (mut roll, mut pitch) = (1.0_f32, 1.0_f32);
    c::rc_input_to_roll_pitch_rad(0.0, 0.0, 0.5, 0.5, &mut roll, &mut pitch)?;
    assert_eq!((roll, pitch), (0.0, 0.0));
    Ok(())
}

#[test]
fn lean_angle_and_acceleration_convert_both_ways() -> ferrobridge::Result<()> {
    let accel = c::angle_deg_to_accel_mss(45.0)?;
    assert!((accel - 9.806_65).abs() < 1e-3, "{accel}");
    let angle = c::accel_mss_to_angle_deg(accel)?;
    assert!((angle - 45.0).abs() < 1e-3, "{angle}");
    Ok(())
}
