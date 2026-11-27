//! A tour of ArduPilot `AP_Math`: all of it real ArduPilot C++ code, from Rust without `unsafe`.
//!
//! ```text
//! ARDUPILOT_DIR=~/ardupilot cargo run --example tour
//! ```

#![forbid(unsafe_code)]

use std::f32::consts::FRAC_PI_2;

use ferrobridge_demo_ap_math::{self as ap, Matrix3f, Quaternion, Rotation, Vector3f};

fn main() -> ferrobridge::Result<()> {
    println!("1) Vectors: fields, methods and operators directly in Rust");
    let a = Vector3f { x: 1.0, y: 2.0, z: 2.0 };
    let b = Vector3f { x: 0.0, y: 1.0, z: 0.0 };
    println!("   a = {a:.3?}, |a| = {}", a.length());
    println!("   a + 2b = {:.3?}", a + b * 2.0);
    println!("   a · b = {}, a × b = {:.3?}", a * b, a % b);

    println!("\n2) Orientation: quaternion from Euler angles, rotating a vector");
    let mut attitude = Quaternion::new_noargs();
    attitude.from_euler_f32_f32_f32(0.0, 0.0, FRAC_PI_2); // yaw 90°
    let nose = Vector3f { x: 1.0, y: 0.0, z: 0.0 };
    let rotated = attitude * nose;
    println!("   nose (body) {nose:.3?} → (earth) {rotated:.3?}");

    let mut dcm = Matrix3f::new_noargs();
    dcm.from_euler(0.0, 0.0, FRAC_PI_2);
    println!("   the same rotation with a matrix: {:.3?}", dcm * nose);

    let (mut roll, mut pitch, mut yaw) = (0.0, 0.0, 0.0);
    attitude.to_euler_mut_f32_mut_f32_mut_f32(&mut roll, &mut pitch, &mut yaw);
    println!(
        "   back: roll {:.1}°, pitch {:.1}°, yaw {:.1}°",
        ap::degrees(roll),
        ap::degrees(pitch),
        ap::degrees(yaw)
    );

    println!("\n3) Sensor mounting: the `Rotation` enum");
    let mut accel = Vector3f { x: 0.0, y: 0.0, z: -9.80665 };
    accel.rotate(Rotation::ROTATION_ROLL_180);
    println!("   accelerometer with ROTATION_ROLL_180: {accel:.3?}");

    println!("\n4) Controller: jerk-limited acceleration shaping (`control.h`)");
    let mut accel_cmd = 0.0_f32;
    for step in 1..=4 {
        ap::shape_accel(3.0, &mut accel_cmd, 5.0, 0.1);
        println!("   step {step}: acceleration command {accel_cmd:.2} m/s²");
    }
    println!("   45° lean = {:.3} m/s² horizontal acceleration", ap::angle_deg_to_accel_mss(45.0));

    println!("\n5) CRC: `&[u8]` in, `Err` if the length overflows (no silent truncation)");
    println!("   CRC-32(\"123456789\") = {:#010x}", !ap::crc_crc32(!0, b"123456789")?);
    match ap::crc_crc8(&[0u8; 300]) {
        Ok(crc) => println!("   unexpected: {crc}"),
        Err(error) => println!("   crc_crc8(300 bytes) → Err: {error}"),
    }

    println!("\n6) Constants: `pub const`, bit-for-bit identical to the C++ value");
    println!("   WGS84_A = {} m, WGS84_B = {} m", ap::WGS84_A, ap::WGS84_B);
    Ok(())
}
