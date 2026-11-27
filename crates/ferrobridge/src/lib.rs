//! ferrobridge'in ürettiği güvenli Rust API'sinin çalışma zamanı.
//!
//! Üretilen `bridge.rs` iki katman içerir: cxx'in ham `ffi` modülü ve onun üstündeki güvenli
//! katman. Güvenli katmandaki bütün hatalar tek bir tip olan [`Error`] ile döner. Böylece
//! farklı header'lardan üretilmiş API'lerin hataları `?` ile birleşir.
//!
//! ```
//! fn hata_ozeti(error: &ferrobridge::Error) -> String {
//!     format!("{error}") // ör. "own::Widget::adopt: motor arızası"
//! }
//! ```

#![forbid(unsafe_code)]

use std::fmt;

/// Güvenli katmandaki bir çağrının hatası. Her durum, hatanın geldiği C++ öğesini söyler.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// C++ kodu bir exception fırlattı.
    Cpp {
        /// Exception'ın geldiği C++ öğesi (ör. `own::Widget::adopt`).
        item: &'static str,
        /// Exception'ın mesajı (`std::exception::what()`).
        message: String,
    },
    /// C++'tan gelen `std::string` geçerli UTF-8 değil; Rust `String`/`&str`'e çevrilemedi.
    Utf8 {
        /// String'i döndüren C++ öğesi.
        item: &'static str,
    },
    /// Null olamayacak bir sahip pointer null geldi (ferrobridge'de ya da C++'ta bir hata).
    NullPointer {
        /// Pointer'ı döndüren C++ öğesi.
        item: &'static str,
    },
    /// C++'tan, enum'da tanımlı olmayan bir değer geldi.
    UnknownEnumValue {
        /// Enum'un tam C++ adı (ör. `io::Mode`).
        ty: &'static str,
        /// Gelen değer.
        value: i128,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cpp { item, message } => write!(f, "{item}: {message}"),
            Error::Utf8 { item } => write!(f, "{item}: C++ string'i geçerli UTF-8 değil"),
            Error::NullPointer { item } => write!(f, "{item}: beklenmeyen null pointer"),
            Error::UnknownEnumValue { ty, value } => {
                write!(f, "{ty}: C++'tan tanımsız enum değeri geldi ({value})")
            }
        }
    }
}

impl std::error::Error for Error {}

/// `build.rs`'te `ferrobridge-build`'in `generate` özelliğiyle üretilen bridge'i bu noktaya
/// ekler. Ad, `ferrobridge_build::Generate::new`'e verilen addır.
///
/// ```ignore
/// pub mod crc {
///     ferrobridge::include_bridge!("crc");
/// }
/// ```
///
/// Üretilen dosya `$OUT_DIR/ferrobridge/<ad>/bridge.rs`'tedir; içindeki bağlanmayan öğeler ve
/// açıklamaya dayanan varsayımlar dosyanın başında listelenir.
///
/// Dosyadaki cxx katmanı (`ffi`) `unsafe extern` blokları içerir. Crate `unsafe_code`'u
/// yasaklıyorsa, bridge'i içeren modüle `#[allow(unsafe_code)]` verilmeli (`forbid` gevşetilemez).
#[macro_export]
macro_rules! include_bridge {
    ($name:literal) => {
        include!(concat!(
            env!("OUT_DIR"),
            "/ferrobridge/",
            $name,
            "/bridge.rs"
        ));
    };
}

/// Güvenli katmanın sonuç tipi.
pub type Result<T> = std::result::Result<T, Error>;

/// Üretilen kodun kullandığı yardımcılar. Kararlı bir API değildir; doğrudan kullanmayın.
#[doc(hidden)]
pub mod __private {
    use cxx::kind::Trivial;
    use cxx::vector::VectorElement;
    use cxx::{CxxString, CxxVector, ExternType, UniquePtr};

    use super::{Error, Result};

    /// `cxx::Exception`'ı, geldiği C++ öğesiyle birlikte [`Error::Cpp`]'ye çevirir.
    pub fn cpp(item: &'static str) -> impl FnOnce(cxx::Exception) -> Error {
        move |exception| Error::Cpp {
            item,
            message: exception.what().to_owned(),
        }
    }

    /// Null olmaması gereken bir `UniquePtr`'ı doğrular.
    pub fn non_null<T>(item: &'static str, ptr: UniquePtr<T>) -> Result<UniquePtr<T>>
    where
        T: cxx::memory::UniquePtrTarget,
    {
        if ptr.is_null() {
            Err(Error::NullPointer { item })
        } else {
            Ok(ptr)
        }
    }

    /// C++ string'inin ödünç UTF-8 görünümü.
    pub fn str_ref<'a>(item: &'static str, value: &'a CxxString) -> Result<&'a str> {
        value.to_str().map_err(|_| Error::Utf8 { item })
    }

    /// Sahip olunan C++ string'ini Rust `String`'e kopyalar.
    pub fn string(item: &'static str, value: UniquePtr<CxxString>) -> Result<String> {
        let value = non_null(item, value)?;
        str_ref(item, &value).map(str::to_owned)
    }

    /// Sahip olunan sayı vector'ünü Rust `Vec`'e kopyalar.
    pub fn vec<T>(item: &'static str, value: UniquePtr<CxxVector<T>>) -> Result<Vec<T>>
    where
        T: VectorElement + Copy,
    {
        let value = non_null(item, value)?;
        Ok(value.iter().copied().collect())
    }

    /// Sahip olunan string vector'ünü `Vec<String>`'e kopyalar.
    pub fn strings(
        item: &'static str,
        value: UniquePtr<CxxVector<CxxString>>,
    ) -> Result<Vec<String>> {
        let value = non_null(item, value)?;
        value
            .iter()
            .map(|s| str_ref(item, s).map(str::to_owned))
            .collect()
    }

    /// Rust slice'ını yeni bir C++ vector'üne kopyalar.
    pub fn cxx_vector<T>(values: &[T]) -> UniquePtr<CxxVector<T>>
    where
        T: VectorElement + ExternType<Kind = Trivial> + Copy,
    {
        let mut vector = CxxVector::new();
        for value in values {
            vector.pin_mut().push(*value);
        }
        vector
    }

    /// Enum'da tanımlı olmayan bir değer için hata.
    pub fn unknown_enum(ty: &'static str, value: impl Into<i128>) -> Error {
        Error::UnknownEnumValue {
            ty,
            value: value.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use cxx::{CxxString, CxxVector, UniquePtr, let_cxx_string};

    use super::__private::*;
    use super::*;

    #[test]
    fn errors_name_the_cpp_item() {
        let cases = [
            (
                Error::Cpp {
                    item: "own::Widget::adopt",
                    message: "motor arızası".to_owned(),
                },
                "own::Widget::adopt: motor arızası",
            ),
            (
                Error::Utf8 {
                    item: "text::greet",
                },
                "text::greet: C++ string'i geçerli UTF-8 değil",
            ),
            (
                Error::NullPointer { item: "own::make" },
                "own::make: beklenmeyen null pointer",
            ),
            (
                unknown_enum("io::Mode", 7_i32),
                "io::Mode: C++'tan tanımsız enum değeri geldi (7)",
            ),
        ];
        for (error, shown) in cases {
            assert_eq!(error.to_string(), shown);
        }
    }

    #[test]
    fn enum_values_of_every_width_fit() {
        assert_eq!(
            unknown_enum("e", u64::MAX),
            Error::UnknownEnumValue {
                ty: "e",
                value: i128::from(u64::MAX)
            }
        );
        assert_eq!(
            unknown_enum("e", i16::MIN),
            Error::UnknownEnumValue {
                ty: "e",
                value: -32768
            }
        );
    }

    #[test]
    fn null_unique_ptrs_are_errors() {
        let null: UniquePtr<CxxString> = UniquePtr::null();
        assert_eq!(
            string("f", null).unwrap_err(),
            Error::NullPointer { item: "f" }
        );
        let null: UniquePtr<CxxVector<i32>> = UniquePtr::null();
        assert_eq!(
            vec("g", null).unwrap_err(),
            Error::NullPointer { item: "g" }
        );
    }

    #[test]
    fn strings_are_checked_for_utf8() {
        let_cxx_string!(good = "çalışıyor");
        assert_eq!(str_ref("f", &good).unwrap(), "çalışıyor");
        let_cxx_string!(bad = [0xff_u8, 0xfe]);
        assert_eq!(str_ref("f", &bad).unwrap_err(), Error::Utf8 { item: "f" });
    }

    #[test]
    fn number_vectors_round_trip() {
        let vector = cxx_vector(&[1.5_f64, -2.0, 4.25]);
        assert_eq!(vector.len(), 3);
        assert_eq!(vec("f", vector).unwrap(), [1.5, -2.0, 4.25]);
        assert!(cxx_vector::<i32>(&[]).is_empty());
    }
}
