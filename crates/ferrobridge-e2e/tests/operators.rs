//! `tests/fixtures/operators`: değer tiplerinin operatörleri `std::ops` ile; diğerleri adlı
//! metot.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
#![allow(clippy::unwrap_used)]

use ferrobridge::Error;
use ferrobridge_e2e::operators::{Counter, V2};

#[test]
fn arithmetic_operators_are_rust_operators() {
    let a = V2 { x: 1.0, y: 2.0 };
    let b = V2 { x: 3.0, y: 5.0 };
    let sum = a + b;
    assert_eq!((sum.x, sum.y), (4.0, 7.0));
    let diff = b - a;
    assert_eq!((diff.x, diff.y), (2.0, 3.0));
    let neg = -a;
    assert_eq!((neg.x, neg.y), (-1.0, -2.0));
    let scaled = a * 2.0;
    assert_eq!((scaled.x, scaled.y), (2.0, 4.0));
    // Aynı operatörün başka sağ tarafı: iç çarpım.
    assert_eq!(a * b, 13.0);
    // `%` çapraz çarpım.
    assert_eq!(a % b, -1.0);
}

#[test]
fn compound_assignment_and_equality() {
    let mut a = V2 { x: 1.0, y: 1.0 };
    a += V2 { x: 1.0, y: 2.0 };
    assert_eq!((a.x, a.y), (2.0, 3.0));
    assert!(a == V2 { x: 2.0, y: 3.0 });
    assert!(a != V2 { x: 0.0, y: 0.0 });
    // Karşılaştırma adlı metot.
    assert!(V2 { x: 1.0, y: 9.0 }.op_lt(&V2 { x: 2.0, y: 0.0 }));
}

#[test]
fn operators_that_may_throw_are_named_methods_returning_result() {
    let a = V2 { x: 4.0, y: 2.0 };
    let half = a.op_div(2.0).unwrap();
    assert_eq!((half.x, half.y), (2.0, 1.0));
    assert!(matches!(a.op_div(0.0), Err(Error::Cpp { .. })));
}

#[test]
fn opaque_classes_get_named_operator_methods() -> ferrobridge::Result<()> {
    let mut counter = Counter::new()?;
    counter.op_add_assign(3)?;
    counter.op_add_assign(4)?;
    assert_eq!(counter.value()?, 7);
    Ok(())
}
