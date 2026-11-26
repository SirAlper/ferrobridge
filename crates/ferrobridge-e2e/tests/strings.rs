//! `tests/fixtures/strings`: `std::string` gönderme, alma ve yerinde değiştirme.

use cxx::let_cxx_string;
use ferrobridge_e2e::strings::ffi;

#[test]
fn string_in_string_out() {
    let_cxx_string!(name = "Dünya");
    let greeting = ffi::greet(&name).unwrap();
    assert_eq!(greeting.to_str().unwrap(), "Merhaba, Dünya!");
}

#[test]
fn string_modified_in_place() {
    let_cxx_string!(text = "Dikkat");
    ffi::append_exclamation(text.as_mut()).unwrap();
    assert_eq!(text.to_str().unwrap(), "Dikkat!");
}

#[test]
fn class_holding_a_string() {
    let_cxx_string!(initial = "selam");
    let mut label = ffi::Label_new(&initial).unwrap();
    assert_eq!(label.text().unwrap().to_str().unwrap(), "selam");
    assert_eq!(ffi::Label_shout(&label).unwrap().to_str().unwrap(), "SELAM");

    let_cxx_string!(updated = "güle güle");
    label.pin_mut().set_text(&updated).unwrap();
    assert_eq!(label.text().unwrap().to_str().unwrap(), "güle güle");
}
