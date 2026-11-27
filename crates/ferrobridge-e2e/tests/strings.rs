//! `tests/fixtures/strings`, with the safe API: `&str` in, `String` out.

use ferrobridge::Error;
use ferrobridge_e2e::strings::{self as text, Label};

#[test]
fn str_in_string_out() -> ferrobridge::Result<()> {
    assert_eq!(text::greet("Zoë")?, "Hello, Zoë!");
    Ok(())
}

#[test]
fn class_holding_a_string() -> ferrobridge::Result<()> {
    let mut label = Label::new("hello")?;
    assert_eq!(label.text()?, "hello");
    assert_eq!(label.shout()?, "HELLO");
    label.set_text("goodbye ✓")?;
    assert_eq!(label.text()?, "goodbye ✓");
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
