//! `tests/fixtures/vectors`, güvenli API ile: `&[T]` girer, `Vec<T>` çıkar.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
#![allow(clippy::unwrap_used)]

use ferrobridge::Error;
use ferrobridge_e2e::vectors::{self as geo, Sample};

#[test]
fn slices_go_in() -> ferrobridge::Result<()> {
    assert_eq!(geo::sum(&[1, 2, 3])?, 6);
    assert_eq!(geo::sum(&[])?, 0);
    Ok(())
}

#[test]
fn string_vectors_come_out_as_vec_string() -> ferrobridge::Result<()> {
    assert_eq!(geo::split_words("bir iki  üç")?, ["bir", "iki", "üç"]);
    Ok(())
}

#[test]
fn class_vectors_stay_cpp_vectors() -> ferrobridge::Result<()> {
    // Elemanlar opak C++ nesneleri: değerle `Vec`'e alınamaz, C++ vector'ü olarak kalır.
    let samples = geo::make_samples(3)?;
    assert_eq!(samples.len(), 3);
    assert_eq!(samples.get(2).unwrap().value()?, 20);
    assert_eq!(geo::first_value(&samples)?, 0);
    // `std::vector::at` boş vector'de exception fırlatır.
    let empty = geo::make_samples(0)?;
    assert!(matches!(
        geo::first_value(&empty),
        Err(Error::Cpp {
            item: "geo::first_value",
            ..
        })
    ));
    assert_eq!(Sample::new(5)?.value()?, 5);
    Ok(())
}
