//! `AP_Math/crc.h`: pointer + uzunluk çiftleri `&[u8]` olarak; değerler standart CRC kataloğunun
//! "123456789" kontrol değerleriyle karşılaştırılır.

#![forbid(unsafe_code)]

use ferrobridge::Error;
use ferrobridge_demo_ap_math as ap;

const CHECK: &[u8] = b"123456789";

#[test]
fn crc32_matches_the_reference_check_value() -> ferrobridge::Result<()> {
    // CRC-32/ISO-HDLC: başlangıç ve son XOR 0xFFFFFFFF.
    assert_eq!(!ap::crc_crc32(!0, CHECK)?, 0xCBF4_3926);
    assert_eq!(!ap::crc32_small(!0, CHECK)?, 0xCBF4_3926);
    Ok(())
}

#[test]
fn crc16_variants_match_their_reference_check_values() -> ferrobridge::Result<()> {
    assert_eq!(ap::crc_xmodem(CHECK)?, 0x31C3); // CRC-16/XMODEM
    assert_eq!(ap::calc_crc_modbus(CHECK)?, 0x4B37); // CRC-16/MODBUS
    assert_eq!(ap::crc16_ccitt(CHECK, 0xFFFF)?, 0x29B1); // CRC-16/IBM-3740
    Ok(())
}

#[test]
fn crc8_variants_match_their_reference_check_values() -> ferrobridge::Result<()> {
    assert_eq!(ap::crc8_maxim(CHECK)?, 0xA1); // CRC-8/MAXIM-DOW
    // Adına rağmen `crc8_dvb_update` polinom 0x07 kullanıyor (`crc.cpp`): CRC-8/SMBUS.
    assert_eq!(ap::crc8_dvb_update(0, CHECK)?, 0xF4);
    // DVB-S2 (polinom 0xD5) bayt bayt `crc8_dvb_s2` ile; slice almadığı için doğrudan değer.
    let dvb_s2 = CHECK.iter().fold(0, |crc, &byte| ap::crc8_dvb_s2(crc, byte));
    assert_eq!(dvb_s2, 0xBC); // CRC-8/DVB-S2
    Ok(())
}

#[test]
fn fnv1a_updates_the_hash_through_a_mutable_reference() -> ferrobridge::Result<()> {
    // FNV-1a 64: başlangıç değeri çağırandan gelir (`*hash`).
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    ap::hash_fnv_1a(b"a", &mut hash)?;
    assert_eq!(hash, 0xaf63_dc4c_8601_ec8c);
    Ok(())
}

#[test]
fn a_buffer_longer_than_the_length_type_is_rejected() {
    // `crc_crc8(const uint8_t* p, uint8_t len)`: 255 bayt sığar, 256 sığmaz; sessizce
    // kırpılsaydı yanlış bir CRC dönerdi. ferrobridge'in kendi kontrolü olduğu için
    // `exceptions = false` olsa da `Result`.
    assert!(ap::crc_crc8(&[7; 255]).is_ok());
    let error = ap::crc_crc8(&[7; 256]).unwrap_err();
    assert!(
        matches!(&error, Error::Cpp { item: "crc_crc8", message } if message.contains("sığmayacak")),
        "{error:?}"
    );
}
