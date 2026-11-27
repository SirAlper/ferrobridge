//! README'deki "Bridge üretmek" örneğinin gerçek fixture'larla derlenen kopyası. README
//! değişince bu dosya da güncellenmeli.

use ferrobridge_e2e::classes::Vehicle;
use ferrobridge_e2e::enums::{self, Mode};
use ferrobridge_e2e::strings;

#[test]
fn readme_example() -> ferrobridge::Result<()> {
    let mut car = Vehicle::new_i32(4)?; // C++ constructor; sahip olan sarmalayıcı
    car.drive(12.5)?; // &mut self; C++ exception → Err
    let km = car.odometer(); // noexcept → doğrudan değer
    let text: String = strings::greet("Ada")?; // &str girer, String çıkar
    let next = match enums::next(Mode::Auto)? {
        // gerçek Rust enum'u, kapsam denetimli
        Mode::Manual => "elle",
        Mode::Auto => "otomatik",
        Mode::Hybrid => "karma",
    };
    assert!((km - 12.5).abs() < 1e-9);
    assert_eq!(text, "Merhaba, Ada!");
    assert_eq!(next, "karma");
    Ok(())
}
