//! `ferrobridge.toml`: kullanıcının sahiplik açıklamaları.
//!
//! C++ header'ı bir pointer'ın sahibini ve ömrünü söylemez. Bu dosyadaki her açıklama,
//! kullanıcının C++ kütüphanesi hakkında verdiği bir **sözdür** ("bu pointer saklanmıyor",
//! "dönen nesneyi silmek bana düşüyor"). Söz yanlışsa tanımsız davranış olabilir; bu yüzden
//! biçim katıdır: bilinmeyen anahtarlar ve değerler satır numarasıyla reddedilir.
//!
//! ```toml
//! [functions."geo::make_widget"]
//! returns = "owned"
//!
//! [functions."geo::Widget::attach".params]
//! target = "borrowed"
//!
//! [functions."crc_crc8".params]
//! p = { slice = "len" }   # p, len elemanlık bir dizi
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Spanned;

/// Bir dönüş değerinin sahipliği.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnAnnotation {
    /// Dönen pointer `new` ile ayrıldı; sahipliği (silme sorumluluğu) çağırana geçiyor.
    Owned,
    /// Dönen pointer nesnenin içini gösteriyor; nesne yaşadıkça geçerli, silinmez.
    Borrowed,
}

/// Bir parametrenin sahipliği.
///
/// TOML'da `"borrowed"` ya da `{ slice = "<uzunluk parametresi>" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamAnnotation {
    /// Pointer/referans yalnızca çağrı süresince kullanılıyor, saklanmıyor ve null değil.
    Borrowed,
    /// Pointer, aynı fonksiyondaki `len` parametresi kadar elemanlık bir diziyi gösteriyor ve
    /// yalnızca çağrı süresince kullanılıyor. Rust'ta pointer ve uzunluk tek bir slice olur.
    Slice {
        /// Eleman sayısını taşıyan parametrenin adı.
        len: String,
    },
}

const PARAM_EXPECTED: &str = "`\"borrowed\"` ya da `{ slice = \"<uzunluk parametresi>\" }`";

impl<'de> Deserialize<'de> for ParamAnnotation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ParamAnnotation;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(PARAM_EXPECTED)
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "borrowed" => Ok(ParamAnnotation::Borrowed),
                    other => Err(E::custom(format!(
                        "bilinmeyen parametre açıklaması `{other}`; beklenen: {PARAM_EXPECTED}"
                    ))),
                }
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                use serde::de::Error as _;
                let mut len: Option<String> = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key != "slice" {
                        return Err(A::Error::custom(format!(
                            "bilinmeyen anahtar `{key}`; beklenen: {PARAM_EXPECTED}"
                        )));
                    }
                    if len.is_some() {
                        return Err(A::Error::duplicate_field("slice"));
                    }
                    let value: String = map.next_value().map_err(|_| {
                        A::Error::custom(format!(
                            "`slice` uzunluk parametresinin adı olmalı (metin); beklenen: \
                             {PARAM_EXPECTED}"
                        ))
                    })?;
                    len = Some(value);
                }
                match len {
                    None => Err(A::Error::custom(format!(
                        "`slice` anahtarı eksik; beklenen: {PARAM_EXPECTED}"
                    ))),
                    Some(len) if len.trim().is_empty() => Err(A::Error::custom(
                        "`slice` uzunluk parametresinin adı boş olamaz",
                    )),
                    Some(len) => Ok(ParamAnnotation::Slice { len }),
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Dosyadaki satır numarasıyla birlikte bir değer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located<T> {
    /// Değer.
    pub value: T,
    /// Değerin yazıldığı satır (1'den başlar).
    pub line: usize,
}

/// Bir fonksiyon, metot ya da constructor için açıklamalar. Anahtar tam C++ adıdır
/// (`geo::Widget::attach`; constructor için `geo::Widget::Widget`) ve aynı adlı tüm
/// overload'lara uygulanır.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionAnnotation {
    /// Tablonun başlığının (anahtarın) yazıldığı satır.
    pub line: usize,
    /// Dönüş açıklaması.
    pub returns: Option<Located<ReturnAnnotation>>,
    /// Parametre adı → açıklama. İsimsiz parametreler `arg0`, `arg1`... diye anılır.
    pub params: BTreeMap<String, Located<ParamAnnotation>>,
}

/// `ferrobridge.toml`'dan okunan açıklamaların tamamı.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Annotations {
    /// Hata mesajlarında gösterilecek dosya adı.
    pub source_name: String,
    /// Tam C++ adı → açıklamalar.
    pub functions: BTreeMap<String, FunctionAnnotation>,
}

/// Açıklama dosyası okunamadı ya da geçersiz.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// Dosya okunamadı.
    #[error("{} okunamadı: {message}", path.display())]
    Read {
        /// Dosya yolu.
        path: PathBuf,
        /// İşletim sisteminin hata mesajı.
        message: String,
    },
    /// Dosya geçerli bir açıklama dosyası değil.
    #[error("{source_name}:{line}: {message}")]
    Invalid {
        /// Dosya adı.
        source_name: String,
        /// Hatanın satırı (1'den başlar).
        line: usize,
        /// Ne yanlış ve nasıl düzeltilir.
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    functions: BTreeMap<Spanned<String>, RawFunction>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFunction {
    returns: Option<Spanned<ReturnAnnotation>>,
    #[serde(default)]
    params: BTreeMap<Spanned<String>, Spanned<ParamAnnotation>>,
}

impl Annotations {
    /// Açıklama dosyasını okur.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError::Read {
            path: path.to_owned(),
            message: e.to_string(),
        })?;
        Self::from_toml_str(&text, &path.display().to_string())
    }

    /// TOML metninden okur. `source_name` hata mesajlarında dosya adı olarak görünür.
    pub fn from_toml_str(text: &str, source_name: &str) -> Result<Self, ConfigError> {
        let raw: RawFile = toml::from_str(text).map_err(|e| ConfigError::Invalid {
            source_name: source_name.to_owned(),
            line: e.span().map_or(1, |span| line_of(text, span.start)),
            message: e.message().trim_end().to_owned(),
        })?;
        let mut functions = BTreeMap::new();
        for (name, function) in raw.functions {
            let line = line_of(text, name.span().start);
            let name = name.into_inner();
            if name.trim().is_empty() {
                return Err(ConfigError::Invalid {
                    source_name: source_name.to_owned(),
                    line,
                    message: "fonksiyon adı boş olamaz".to_owned(),
                });
            }
            let returns = function.returns.map(|r| located(text, r));
            let params = function
                .params
                .into_iter()
                .map(|(param, annotation)| (param.into_inner(), located(text, annotation)))
                .collect();
            functions.insert(
                name,
                FunctionAnnotation {
                    line,
                    returns,
                    params,
                },
            );
        }
        Ok(Annotations {
            source_name: source_name.to_owned(),
            functions,
        })
    }

    /// Hiç açıklama yok mu?
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

fn located<T>(text: &str, spanned: Spanned<T>) -> Located<T> {
    let line = line_of(text, spanned.span().start);
    Located {
        value: spanned.into_inner(),
        line,
    }
}

/// Bayt konumunun satır numarası (1'den başlar).
fn line_of(text: &str, offset: usize) -> usize {
    let end = offset.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

impl fmt::Display for ReturnAnnotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Owned => "owned",
            Self::Borrowed => "borrowed",
        })
    }
}

impl fmt::Display for ParamAnnotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Borrowed => f.write_str("borrowed"),
            Self::Slice { len } => write!(f, "{{ slice = \"{len}\" }}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Annotations, ConfigError> {
        Annotations::from_toml_str(text, "ferrobridge.toml")
    }

    fn invalid_line(text: &str) -> (usize, String) {
        match parse(text) {
            Err(ConfigError::Invalid { line, message, .. }) => (line, message),
            other => panic!("geçersiz sayılmalıydı: {other:?}"),
        }
    }

    #[test]
    fn reads_return_and_param_annotations_with_lines() {
        let text = r#"# açıklamalar
[functions."geo::make_widget"]
returns = "owned"

[functions."geo::Widget::buffer"]
returns = "borrowed"

[functions."geo::Widget::attach".params]
target = "borrowed"
arg1 = "borrowed"
"#;
        let annotations = parse(text).unwrap();
        assert_eq!(annotations.functions.len(), 3);
        let make = &annotations.functions["geo::make_widget"];
        assert_eq!(make.line, 2);
        assert_eq!(
            make.returns,
            Some(Located {
                value: ReturnAnnotation::Owned,
                line: 3
            })
        );
        assert!(make.params.is_empty());
        assert_eq!(
            annotations.functions["geo::Widget::buffer"]
                .returns
                .as_ref()
                .unwrap()
                .value,
            ReturnAnnotation::Borrowed
        );
        let attach = &annotations.functions["geo::Widget::attach"];
        assert_eq!(attach.returns, None);
        assert_eq!(
            attach.params["target"],
            Located {
                value: ParamAnnotation::Borrowed,
                line: 9
            }
        );
        assert_eq!(attach.params["arg1"].line, 10);
    }

    #[test]
    fn empty_file_has_no_annotations() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("# yalnızca yorum\n").unwrap().is_empty());
    }

    #[test]
    fn misspelled_value_is_rejected_with_its_line() {
        let (line, message) = invalid_line("[functions.\"f\"]\n\nreturns = \"owend\"\n");
        assert_eq!(line, 3);
        assert!(message.contains("owend"), "{message}");
        assert!(message.contains("owned"), "{message}");
    }

    #[test]
    fn misspelled_keys_are_rejected() {
        let (line, message) = invalid_line("[functions.\"f\"]\nreturn = \"owned\"\n");
        assert_eq!(line, 2);
        assert!(message.contains("return"), "{message}");
        let (line, _) = invalid_line("[function.\"f\"]\nreturns = \"owned\"\n");
        assert_eq!(line, 1);
    }

    #[test]
    fn params_only_accept_borrowed() {
        let (line, message) = invalid_line("[functions.\"f\".params]\nout = \"owned\"\n");
        assert_eq!(line, 2);
        assert!(message.contains("borrowed"), "{message}");
    }

    #[test]
    fn slice_annotation_names_the_length_parameter() {
        let text = "[functions.\"crc8\".params]\np = { slice = \"len\" }\n";
        let annotations = parse(text).unwrap();
        assert_eq!(
            annotations.functions["crc8"].params["p"],
            Located {
                value: ParamAnnotation::Slice {
                    len: "len".to_owned()
                },
                line: 2
            }
        );
        assert_eq!(
            annotations.functions["crc8"].params["p"].value.to_string(),
            "{ slice = \"len\" }"
        );
    }

    #[test]
    fn malformed_slice_annotations_are_rejected() {
        for (text, needle) in [
            (
                "[functions.\"f\".params]\np = { slise = \"len\" }\n",
                "slise",
            ),
            ("[functions.\"f\".params]\np = { }\n", "slice"),
            ("[functions.\"f\".params]\np = { slice = \" \" }\n", "boş"),
            ("[functions.\"f\".params]\np = { slice = 3 }\n", "slice"),
            ("[functions.\"f\".params]\np = \"slice\"\n", "slice = "),
        ] {
            let (line, message) = invalid_line(text);
            assert_eq!(line, 2, "{text}");
            assert!(message.contains(needle), "{text}: {message}");
        }
    }

    #[test]
    fn empty_function_name_is_rejected() {
        let (line, _) = invalid_line("[functions.\"\"]\nreturns = \"owned\"\n");
        assert_eq!(line, 1);
    }

    #[test]
    fn syntax_errors_carry_a_line() {
        let (line, _) = invalid_line("[functions.\"f\"]\nreturns = \n");
        assert_eq!(line, 2);
    }

    #[test]
    fn missing_file_is_a_read_error() {
        let error = Annotations::from_file(Path::new("/olmayan/ferrobridge.toml")).unwrap_err();
        assert!(matches!(error, ConfigError::Read { .. }));
        assert!(error.to_string().contains("/olmayan/ferrobridge.toml"));
    }

    #[test]
    fn invalid_error_message_shows_file_and_line() {
        let error = parse("[functions.\"f\"]\nreturns = \"x\"\n").unwrap_err();
        assert!(
            error.to_string().starts_with("ferrobridge.toml:2: "),
            "{error}"
        );
    }
}
