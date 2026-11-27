//! `Vector3f`, `Matrix3f`, `Quaternion`: value types instantiated with `[templates]`. Fields,
//! methods and operators directly in Rust.

#![forbid(unsafe_code)]

use std::f32::consts::FRAC_PI_2;

use ferrobridge_demo_ap_math::{Matrix3f, Quaternion, Rotation, Vector3f};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

fn v(x: f32, y: f32, z: f32) -> Vector3f {
    Vector3f { x, y, z }
}

#[test]
fn vectors_have_fields_methods_and_operators() {
    let a = v(1.0, 2.0, 2.0);
    assert_eq!(a.length(), 3.0);
    let b = a + v(1.0, 0.0, 0.0) * 2.0;
    assert_eq!((b.x, b.y, b.z), (3.0, 2.0, 2.0));
    // `*` with two vectors is the dot product, `%` the cross product (as in ArduPilot).
    assert_eq!(a * v(1.0, 1.0, 1.0), 5.0);
    let z = v(1.0, 0.0, 0.0) % v(0.0, 1.0, 0.0);
    assert_eq!((z.x, z.y, z.z), (0.0, 0.0, 1.0));
    let mut n = v(3.0, 0.0, 4.0);
    n.normalize();
    assert!(close(n.x, 0.6) && close(n.z, 0.8));
    assert!(-a == v(-1.0, -2.0, -2.0));
}

#[test]
fn rotations_and_quaternions_agree_with_matrices() {
    // Yaw 90°: the x axis turns into the y axis.
    let mut q = Quaternion::new_noargs();
    q.from_euler_f32_f32_f32(0.0, 0.0, FRAC_PI_2);
    let rotated = q * v(1.0, 0.0, 0.0);
    assert!(close(rotated.x, 0.0) && close(rotated.y, 1.0), "{rotated:?}");

    let mut m = Matrix3f::new_noargs();
    m.from_euler(0.0, 0.0, FRAC_PI_2);
    let by_matrix = m * v(1.0, 0.0, 0.0);
    assert!(close(by_matrix.x, rotated.x) && close(by_matrix.y, rotated.y));

    // The Euler angles come back through output parameters.
    let (mut roll, mut pitch, mut yaw) = (0.0_f32, 0.0_f32, 0.0_f32);
    q.to_euler_mut_f32_mut_f32_mut_f32(&mut roll, &mut pitch, &mut yaw);
    assert!(close(yaw, FRAC_PI_2) && close(roll, 0.0) && close(pitch, 0.0));
}

#[test]
fn the_rotation_enum_drives_vector_rotation() {
    let mut a = v(1.0, 0.0, 0.0);
    a.rotate(Rotation::ROTATION_YAW_90);
    assert!(close(a.x, 0.0) && close(a.y, 1.0), "{a:?}");
}
