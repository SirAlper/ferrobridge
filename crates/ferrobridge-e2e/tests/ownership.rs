//! `tests/fixtures/ownership`: pointers bound with `ferrobridge.toml` annotations, with the
//! safe API. No hand-written bridge, `unsafe` or cxx type.
//!
//! That ownership really passes to Rust is proven with live object counters in C++.

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use std::sync::{Mutex, MutexGuard};

use ferrobridge::Error;
use ferrobridge_e2e::ownership::{self as own, Widget};

/// The counters are global in C++ and the tests run in parallel. Every test creating a `Widget`/`Shape`
/// takes this lock; otherwise an object created by one test would break another test's count.
static COUNTERS: Mutex<()> = Mutex::new(());

fn counters() -> MutexGuard<'static, ()> {
    COUNTERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn owned_return_is_deleted_when_the_wrapper_drops() -> ferrobridge::Result<()> {
    let _guard = counters();
    let before = own::live_widgets();
    let widget = own::make_widget(7)?.expect("not null");
    assert_eq!(widget.id(), 7);
    assert_eq!(own::live_widgets(), before + 1);
    drop(widget);
    assert_eq!(
        own::live_widgets(),
        before,
        "the object must be deleted when the wrapper is dropped"
    );
    Ok(())
}

#[test]
fn owned_null_pointer_becomes_none() -> ferrobridge::Result<()> {
    assert!(own::make_null_widget()?.is_none());
    Ok(())
}

#[test]
fn owned_interface_pointer_runs_the_derived_destructor() -> ferrobridge::Result<()> {
    let _guard = counters();
    let before = own::live_shapes();
    let square = own::make_square(3.0)?.expect("not null");
    assert!(close(square.area()?, 9.0));
    assert_eq!(own::live_shapes(), before + 1);
    drop(square);
    // Thanks to the virtual destructor, the subclass's (Square's) destructor ran.
    assert_eq!(own::live_shapes(), before);
    Ok(())
}

#[test]
fn owned_string_pointer() -> ferrobridge::Result<()> {
    assert_eq!(own::make_label("etiket")?, Some("[etiket]".to_owned()));
    Ok(())
}

#[test]
fn borrowed_returns_are_never_deleted_and_null_is_an_error() -> ferrobridge::Result<()> {
    let _guard = counters();
    let before = own::live_widgets();
    let mut parent = Widget::new_i32(1)?;

    // No child: C++ returns null, Rust gets `Err` instead of a reference.
    let Err(error) = parent.child_view() else {
        panic!("a null pointer must not have been turned into a reference");
    };
    assert!(matches!(
        error,
        Error::Cpp {
            item: "own::Widget::child_view",
            ..
        }
    ));
    assert!(parent.child().is_err());

    parent.adopt(2)?;
    assert_eq!(parent.child_view()?.id(), 2);
    parent.child()?.adopt(3)?;
    assert_eq!(own::live_widgets(), before + 3);

    // A borrowed return with an extra parameter: its lifetime is tied to `parent`.
    assert_eq!(parent.child_named("2")?.id(), 2);
    assert!(parent.child_named("9").is_err());

    // The borrowed references were dropped, no object was deleted; deleting the parent deletes them all.
    assert_eq!(own::live_widgets(), before + 3);
    drop(parent);
    assert_eq!(own::live_widgets(), before);
    Ok(())
}

#[test]
fn borrowed_primitive_returns_and_out_params() -> ferrobridge::Result<()> {
    let _guard = counters();
    let mut widget = Widget::new_i32(5)?;
    *widget.weight()? = 2.5;
    let mut out = 0.0;
    widget.read_weight(&mut out)?;
    assert!(close(out, 2.5));

    *widget.weight_slot()? *= 2.0;
    widget.read_weight(&mut out)?;
    assert!(close(out, 5.0));

    let mut id = 0;
    widget.read_id(&mut id)?;
    assert_eq!(id, 5);

    let mut value = 1.5;
    own::scale(&mut value, 4.0)?;
    assert!(close(value, 6.0));
    Ok(())
}

#[test]
fn borrowed_object_params() -> ferrobridge::Result<()> {
    let _guard = counters();
    let mut a = Widget::new_i32(1)?;
    let mut b = Widget::new_i32(2)?;
    a.swap_ids(b.pin_mut())?;
    assert_eq!((a.id(), b.id()), (2, 1));
    a.copy_id_from(&b)?;
    assert_eq!(a.id(), 1);

    *a.weight()? = 4.5;
    let copy = Widget::new_Widget_i32(&a, 9)?;
    let mut out = 0.0;
    copy.read_weight(&mut out)?;
    assert_eq!(copy.id(), 9);
    assert!(close(out, 4.5));
    Ok(())
}

#[test]
fn unannotated_and_untyped_items_are_not_bound() {
    let bridge = include_str!("../src/generated/ownership/bridge.rs");
    for name in ["fn unannotated", "fn raw_handle", "fn make_leaky"] {
        assert!(!bridge.contains(name), "{name} should not have been bound");
    }
}
