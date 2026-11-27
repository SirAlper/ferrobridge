//! `tests/fixtures/value_types`: classes whose fields are public and that are trivially copyable are
//! structs in Rust; fields are read and written directly, passed by value.

use ferrobridge_e2e::value_types::{self as geo, Account, Frozen, Line, PairIF, Pt, Vec3f};

#[test]
fn fields_are_plain_rust_fields() -> ferrobridge::Result<()> {
    let mut p = Pt::new_f64_f64(3.0, 4.0)?;
    assert_eq!((p.x, p.y), (3.0, 4.0));
    p.y = 0.0;
    assert_eq!(p.norm2()?, 9.0);
    // It can also be constructed from its fields: all fields are public, there is no hidden invariant.
    let q = Pt { x: 1.0, y: 1.0 };
    assert_eq!(q.norm2()?, 2.0);
    Ok(())
}

#[test]
fn mutating_methods_take_mut_self() -> ferrobridge::Result<()> {
    let mut p = Pt { x: 1.0, y: 2.0 };
    p.scale(10.0)?;
    assert_eq!((p.x, p.y), (10.0, 20.0));
    Ok(())
}

#[test]
fn values_go_in_and_come_out_by_value() -> ferrobridge::Result<()> {
    let a = Pt { x: 0.0, y: 0.0 };
    let b = Pt { x: 4.0, y: 2.0 };
    let m = geo::midpoint(a, &b)?;
    assert_eq!((m.x, m.y), (2.0, 1.0));
    // A copy: `a` is still usable.
    let line: Line = geo::make_line(a, Pt { x: 3.0, y: 4.0 })?;
    assert_eq!(line.to.y, 4.0);
    assert_eq!(line.length()?, 5.0);
    Ok(())
}

#[test]
fn template_instances_are_value_types_too() -> ferrobridge::Result<()> {
    let x = Vec3f {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    let y = Vec3f::new_f32_f32_f32(0.0, 1.0, 0.0)?;
    let z = geo::cross(&x, &y)?;
    assert_eq!((z.x, z.y, z.z), (0.0, 0.0, 1.0));
    assert_eq!(z.scaled(2.0)?.z, 2.0);
    let mut v = Vec3f {
        x: 3.0,
        y: 0.0,
        z: 4.0,
    };
    v.normalize()?;
    assert_eq!((v.x, v.z), (0.6, 0.8));
    Ok(())
}

#[test]
fn classes_with_private_fields_stay_opaque() -> ferrobridge::Result<()> {
    let account = Account::new()?;
    assert_eq!(account.fee()?, 0.5);
    Ok(())
}

#[test]
fn instances_with_commas_in_their_spelling_work() -> ferrobridge::Result<()> {
    let pair: PairIF = geo::make_pair(7, 2.5)?;
    assert_eq!((pair.first, pair.second), (7, 2.5));
    Ok(())
}

#[test]
fn classes_with_const_fields_stay_opaque() -> ferrobridge::Result<()> {
    // If it were a value type, `frozen.limit = ...` could be written.
    let frozen = Frozen::new()?;
    assert_eq!(frozen.get()?, 1.5);
    Ok(())
}
