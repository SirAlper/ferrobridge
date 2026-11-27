//! `tests/fixtures/strings`, güvenli API ile: `&str` girer, `String` çıkar.

use ferrobridge::Error;
use ferrobridge_e2e::strings::{self as text, Label};

#[test]
fn str_in_string_out() -> ferrobridge::Result<()> {
    assert_eq!(text::greet("Dünya")?, "Merhaba, Dünya!");
    Ok(())
}

#[test]
fn class_holding_a_string() -> ferrobridge::Result<()> {
    let mut label = Label::new("selam")?;
    assert_eq!(label.text()?, "selam");
    assert_eq!(label.shout()?, "SELAM");
    label.set_text("güle güle")?;
    assert_eq!(label.text()?, "güle güle");
    Ok(())
}

#[test]
fn invalid_utf8_from_cpp_is_an_error_not_a_panic() {
    assert_eq!(
        text::raw_bytes(),
        Err(Error::Utf8 {
            item: "text::raw_bytes"
        })
    );
}
