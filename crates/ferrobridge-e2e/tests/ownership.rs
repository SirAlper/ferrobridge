//! `tests/fixtures/ownership`: `ferrobridge.toml` açıklamalarıyla bağlanan pointer'lar,
//! güvenli API ile. Elle yazılmış bridge, `unsafe` ya da cxx tipi yok.
//!
//! Sahipliğin gerçekten Rust'a geçtiği C++'taki canlı nesne sayaçlarıyla kanıtlanır.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
#![allow(clippy::unwrap_used)]

use std::sync::{Mutex, MutexGuard};

use ferrobridge::Error;
use ferrobridge_e2e::ownership::{self as own, Widget};

/// Sayaçlar C++'ta global ve testler paralel çalışıyor. `Widget`/`Shape` oluşturan her test bu
/// kilidi alır; yoksa bir testin oluşturduğu nesne başka bir testin sayımını bozar.
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
    let widget = own::make_widget(7)?.expect("null değil");
    assert_eq!(widget.id(), 7);
    assert_eq!(own::live_widgets(), before + 1);
    drop(widget);
    assert_eq!(
        own::live_widgets(),
        before,
        "sarmalayıcı düşünce nesne silinmeli"
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
    let square = own::make_square(3.0)?.expect("null değil");
    assert!(close(square.area()?, 9.0));
    assert_eq!(own::live_shapes(), before + 1);
    drop(square);
    // Sanal destructor sayesinde alt sınıfın (Square) destructor'ı çalıştı.
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

    // Çocuk yok: C++ null döndürür, Rust'a referans değil `Err` gelir.
    let Err(error) = parent.child_view() else {
        panic!("null pointer referansa çevrilmemeliydi");
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

    // Ek parametreli ödünç dönüş: ömrü `parent`'a bağlı.
    assert_eq!(parent.child_named("2")?.id(), 2);
    assert!(parent.child_named("9").is_err());

    // Ödünç referanslar düştü, hiçbir nesne silinmedi; ebeveyni silmek hepsini siler.
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
        assert!(!bridge.contains(name), "{name} bağlanmamalıydı");
    }
}
