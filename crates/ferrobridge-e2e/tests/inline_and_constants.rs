//! `tests/fixtures/inline_and_constants`, güvenli API ile: header'da tanımlı `static inline`
//! fonksiyonlar ve Rust `const` olarak üretilen sabitler.

use ferrobridge_e2e::inline_and_constants as k;

#[test]
fn static_inline_functions_are_callable() -> ferrobridge::Result<()> {
    assert_eq!(k::add_one(41)?, 42);
    assert_eq!(k::half(3.0)?, 1.5);
    assert_eq!(k::triple(2.5)?, 7.5);
    Ok(())
}

#[test]
fn constants_keep_their_cpp_types_and_values() {
    assert_eq!(k::WGS84_A, 6_378_137.0_f64);
    assert_eq!(k::WGS84_IF, 298.257_223_563_f64);
    assert_eq!(k::GRAVITY_MSS, 9.806_65_f32);
    assert_eq!(k::MAX_WHEELS, 8_i32);
    assert_eq!(k::OFFSET, -5_i8);
    assert_eq!(k::ALL_BITS, u64::MAX);
    const { assert!(k::ENABLED) };
    assert_eq!(k::LETTER as u8, b'A');
    assert_eq!(k::FEET_PER_METER, 3.280_84_f64);
}

#[test]
fn constants_are_bit_for_bit_what_cpp_sees_at_run_time() -> ferrobridge::Result<()> {
    // `WGS84_F = 1.0 / WGS84_IF`: bölme derleme anında (ferrobridge) ve çalışma anında (C++)
    // aynı sonucu vermeli. `TENTH`: `float`'a dar dönüşüm (`0.1` → `0.1f`).
    assert_eq!(k::WGS84_F.to_bits(), k::wgs84_f_at_runtime()?.to_bits());
    assert_eq!(k::TENTH.to_bits(), k::tenth_at_runtime()?.to_bits());
    Ok(())
}
