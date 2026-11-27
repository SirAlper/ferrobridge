//! `SCurve` (sahip olunan C++ nesnesi), `Rotation` enum'u, `location.h` ve `definitions.h`
//! sabitleri.

#![forbid(unsafe_code)]

use ferrobridge_demo_ap_math::{definitions, location, rotations::Rotation, scurve::SCurve};

#[test]
fn scurve_is_an_owned_cpp_object() -> ferrobridge::Result<()> {
    let mut curve = SCurve::new()?;
    curve.init()?;
    // `init` yolu sıfırlar: kat edilecek yol yok, eğri bitmiş sayılır.
    assert!(curve.finished()?);
    Ok(())
}

#[test]
fn scurve_path_parameters_come_back_through_out_params() -> ferrobridge::Result<()> {
    let (mut jm, mut tj, mut t2, mut t4, mut t6) = (0.0, 0.0, 0.0, 0.0, 0.0);
    // 100 m'lik yol, 5 m/s, 2 m/s², 1 m/s³, snap 10.
    SCurve::calculate_path(10.0, 1.0, 0.0, 2.0, 5.0, 100.0, &mut jm, &mut tj, &mut t2, &mut t4, &mut t6)?;
    assert!(jm > 0.0 && jm <= 1.0, "Jm_out = {jm}");
    assert!([tj, t2, t4, t6].iter().all(|t| *t >= 0.0), "{tj} {t2} {t4} {t6}");
    assert!(t4 > 0.0, "sabit hız evresi olmalı: {t4}");
    Ok(())
}

#[test]
fn wgs84_constants_are_the_exact_cpp_values() {
    assert_eq!(definitions::WGS84_A, 6_378_137.0);
    assert_eq!(definitions::WGS84_F, 1.0 / definitions::WGS84_IF);
    // `WGS84_B = WGS84_A * (1 - WGS84_F)`: aynı IEEE işlemleri, aynı bitler.
    assert_eq!(
        definitions::WGS84_B.to_bits(),
        (definitions::WGS84_A * (1.0 - definitions::WGS84_F)).to_bits()
    );
}

#[test]
fn rotation_is_a_real_rust_enum() {
    let rotation = Rotation::ROTATION_YAW_90;
    assert_ne!(rotation, Rotation::ROTATION_NONE);
    assert_eq!(format!("{rotation:?}"), "ROTATION_YAW_90");
}

#[test]
fn latitude_and_longitude_checks() -> ferrobridge::Result<()> {
    assert!(location::check_lat_f32(45.0)?);
    assert!(!location::check_lat_f32(91.0)?);
    assert!(location::check_latlng_i32_i32(450_000_000, -1_200_000_000)?);
    Ok(())
}
