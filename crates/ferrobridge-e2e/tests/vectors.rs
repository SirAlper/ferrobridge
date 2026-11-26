//! `tests/fixtures/vectors`: elle yazılmış hiçbir bridge ya da `unsafe` olmadan.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
#![allow(clippy::unwrap_used)]

use cxx::{CxxVector, let_cxx_string};
use ferrobridge_e2e::vectors::ffi;

#[test]
fn vectors_built_in_rust_are_read_and_changed_in_cpp() {
    let mut numbers = CxxVector::<i32>::new();
    for n in [1, 2, 3] {
        numbers.pin_mut().push(n);
    }
    assert_eq!(ffi::sum(&numbers).unwrap(), 6);

    let mut values = CxxVector::<f64>::new();
    values.pin_mut().push(4.0);
    values.pin_mut().push(8.0);
    ffi::append_halves(values.pin_mut()).unwrap();
    assert_eq!(
        values.iter().copied().collect::<Vec<_>>(),
        [4.0, 8.0, 2.0, 4.0]
    );
}

#[test]
fn string_vectors_round_trip() {
    let_cxx_string!(text = "bir iki  üç");
    let words = ffi::split_words(&text).unwrap();
    let words_rs: Vec<&str> = words.iter().map(|w| w.to_str().unwrap()).collect();
    assert_eq!(words_rs, ["bir", "iki", "üç"]);
    assert_eq!(ffi::count_words(&words).unwrap(), 3);
}

#[test]
fn class_vectors_returned_by_value() {
    let samples = ffi::make_samples(3).unwrap();
    assert_eq!(samples.len(), 3);
    assert_eq!(samples.get(2).unwrap().value().unwrap(), 20);
    assert_eq!(ffi::first_value(&samples).unwrap(), 0);
    // `std::vector::at` boş vector'de exception fırlatır: `Err`.
    let empty = ffi::make_samples(0).unwrap();
    assert!(ffi::first_value(&empty).is_err());
}
