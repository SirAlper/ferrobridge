//! `tests/fixtures/ownership`: `ferrobridge.toml` açıklamalarıyla bağlanan pointer'lar.
//! Elle yazılmış hiçbir bridge ya da `unsafe` yok.
//!
//! Sahipliğin gerçekten Rust'a geçtiği C++'taki canlı nesne sayaçlarıyla kanıtlanır.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
#![allow(clippy::unwrap_used)]

use std::sync::{Mutex, MutexGuard};

use cxx::let_cxx_string;
use ferrobridge_e2e::ownership::ffi;

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
fn owned_return_is_deleted_when_the_unique_ptr_drops() {
    let _guard = counters();
    let before = ffi::live_widgets();
    let widget = ffi::make_widget(7).unwrap();
    assert_eq!(widget.id(), 7);
    assert_eq!(ffi::live_widgets(), before + 1);
    drop(widget);
    assert_eq!(
        ffi::live_widgets(),
        before,
        "UniquePtr düşünce nesne silinmeli"
    );
}

#[test]
fn owned_null_pointer_becomes_an_empty_unique_ptr() {
    assert!(ffi::make_null_widget().unwrap().is_null());
}

#[test]
fn owned_interface_pointer_runs_the_derived_destructor() {
    let _guard = counters();
    let before = ffi::live_shapes();
    let square = ffi::make_square(3.0).unwrap();
    assert!(close(square.area().unwrap(), 9.0));
    assert_eq!(ffi::live_shapes(), before + 1);
    drop(square);
    // Sanal destructor sayesinde alt sınıfın (Square) destructor'ı çalıştı.
    assert_eq!(ffi::live_shapes(), before);
}

#[test]
fn owned_string_pointer() {
    let_cxx_string!(text = "etiket");
    let label = ffi::make_label(&text).unwrap();
    assert_eq!(label.to_str().unwrap(), "[etiket]");
}

#[test]
fn borrowed_returns_are_never_deleted_and_null_is_an_error() {
    let _guard = counters();
    let before = ffi::live_widgets();
    let mut parent = ffi::make_widget(1).unwrap();

    // Çocuk yok: C++ null döndürür, Rust'a referans değil `Err` gelir.
    let Err(error) = ffi::Widget_child_view(&parent) else {
        panic!("null pointer referansa çevrilmemeliydi");
    };
    assert!(
        error.what().contains("own::Widget::child_view"),
        "{}",
        error.what()
    );
    assert!(ffi::Widget_child(parent.pin_mut()).is_err());

    parent.pin_mut().adopt(2).unwrap();
    assert_eq!(ffi::Widget_child_view(&parent).unwrap().id(), 2);
    let child = ffi::Widget_child(parent.pin_mut()).unwrap();
    child.adopt(3).unwrap();
    assert_eq!(ffi::live_widgets(), before + 3);

    // Ek referans parametresiyle (`'a` ömrü alıcıya bağlı).
    let_cxx_string!(name = "2");
    assert_eq!(ffi::Widget_child_named(&parent, &name).unwrap().id(), 2);
    let_cxx_string!(other = "9");
    assert!(ffi::Widget_child_named(&parent, &other).is_err());

    // Ödünç referanslar düştü, hiçbir nesne silinmedi; ebeveyni silmek hepsini siler.
    assert_eq!(ffi::live_widgets(), before + 3);
    drop(parent);
    assert_eq!(ffi::live_widgets(), before);
}

#[test]
fn borrowed_primitive_returns_and_out_params() {
    let _guard = counters();
    let mut widget = ffi::make_widget(5).unwrap();
    *widget.pin_mut().weight().unwrap() = 2.5;
    let mut out = 0.0;
    widget.read_weight(&mut out).unwrap();
    assert!(close(out, 2.5));

    *ffi::Widget_weight_slot(widget.pin_mut()).unwrap() *= 2.0;
    widget.read_weight(&mut out).unwrap();
    assert!(close(out, 5.0));

    let mut id = 0;
    ffi::Widget_read_id(&widget, &mut id).unwrap();
    assert_eq!(id, 5);

    let mut value = 1.5;
    ffi::scale(&mut value, 4.0).unwrap();
    assert!(close(value, 6.0));
}

#[test]
fn borrowed_object_params() {
    let _guard = counters();
    let mut a = ffi::make_widget(1).unwrap();
    let mut b = ffi::make_widget(2).unwrap();
    ffi::Widget_swap_ids(a.pin_mut(), b.pin_mut()).unwrap();
    assert_eq!((a.id(), b.id()), (2, 1));
    ffi::Widget_copy_id_from(a.pin_mut(), &b).unwrap();
    assert_eq!(a.id(), 1);

    *a.pin_mut().weight().unwrap() = 4.5;
    let copy = ffi::Widget_new_Widget_i32(&a, 9).unwrap();
    let mut out = 0.0;
    copy.read_weight(&mut out).unwrap();
    assert_eq!(copy.id(), 9);
    assert!(close(out, 4.5));
}

#[test]
fn unannotated_and_unsafe_items_are_not_bound() {
    let bridge = include_str!("../src/generated/ownership/bridge.rs");
    for name in ["fn unannotated", "fn raw_handle", "fn make_leaky"] {
        assert!(!bridge.contains(name), "{name} bağlanmamalıydı");
    }
}
