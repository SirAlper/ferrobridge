//! `ferrobridge.toml`: the user's ownership annotations.
//!
//! A C++ header does not say who owns a pointer or how long it lives. Every annotation in this
//! file is a **promise** the user makes about the C++ library ("this pointer is not kept",
//! "deleting the returned object is up to me"). A wrong promise can mean undefined behavior; so
//! the format is strict: unknown keys and values are rejected with a line number.
//!
//! ```toml
//! [functions."geo::make_widget"]
//! returns = "owned"
//!
//! [functions."geo::Widget::attach".params]
//! target = "borrowed"
//!
//! [functions."crc_crc8".params]
//! p = { slice = "len" }   # p, an array of len elements
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Spanned;

/// The ownership of a return value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnAnnotation {
    /// The returned pointer was allocated with `new`; ownership (the responsibility to delete) passes to the caller.
    Owned,
    /// The returned pointer points inside the object; valid while the object lives, not deleted.
    Borrowed,
}

/// The ownership of a parameter.
///
/// In TOML, `"borrowed"` or `{ slice = "<length parameter>" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamAnnotation {
    /// The pointer/reference is used only during the call, not kept, and not null.
    Borrowed,
    /// The pointer points to an array with as many elements as the `len` parameter of the same
    /// function, and is used only during the call. In Rust the pointer and the length become a single slice.
    Slice {
        /// The name of the parameter carrying the number of elements.
        len: String,
    },
}

const PARAM_EXPECTED: &str = "`\"borrowed\"` or `{ slice = \"<length parameter>\" }`";

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
                        "unknown parameter annotation `{other}`; expected: {PARAM_EXPECTED}"
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
                            "unknown key `{key}`; expected: {PARAM_EXPECTED}"
                        )));
                    }
                    if len.is_some() {
                        return Err(A::Error::duplicate_field("slice"));
                    }
                    let value: String = map.next_value().map_err(|_| {
                        A::Error::custom(format!(
                            "`slice` must be the name of the length parameter (text); expected: \
                             {PARAM_EXPECTED}"
                        ))
                    })?;
                    len = Some(value);
                }
                match len {
                    None => Err(A::Error::custom(format!(
                        "the `slice` key is missing; expected: {PARAM_EXPECTED}"
                    ))),
                    Some(len) if len.trim().is_empty() => Err(A::Error::custom(
                        "the name of the `slice` length parameter cannot be empty",
                    )),
                    Some(len) => Ok(ParamAnnotation::Slice { len }),
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// A value together with its line number in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located<T> {
    /// The value.
    pub value: T,
    /// The line the value is written on (starting at 1).
    pub line: usize,
}

/// Annotations for a function, method or constructor. The key is the full C++ name
/// (`geo::Widget::attach`; `geo::Widget::Widget` for a constructor) and applies to all
/// overloads with that name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionAnnotation {
    /// The line where the table's header (the key) is written.
    pub line: usize,
    /// The return annotation.
    pub returns: Option<Located<ReturnAnnotation>>,
    /// Parameter name → annotation. Unnamed parameters are referred to as `arg0`, `arg1`...
    pub params: BTreeMap<String, Located<ParamAnnotation>>,
}

/// An entry of the `[templates]` section: `Vector3f = "Vector3<float>"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateEntry {
    /// The Rust name of the instantiation.
    pub rust_name: String,
    /// The C++ spelling (resolved in the global scope after the headers are included).
    pub spelling: String,
    /// The entry's line.
    pub line: usize,
}

/// All annotations read from `ferrobridge.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Annotations {
    /// The file name to show in error messages.
    pub source_name: String,
    /// Full C++ name → annotations.
    pub functions: BTreeMap<String, FunctionAnnotation>,
    /// Class templates to instantiate, in file order.
    pub templates: Vec<TemplateEntry>,
    /// `[library] exceptions = false`: the promise "this library does not throw exceptions" and its line.
    pub no_exceptions: Option<usize>,
}

/// The annotation file could not be read or is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("could not read {}: {message}", path.display())]
    Read {
        /// The file path.
        path: PathBuf,
        /// The operating system's error message.
        message: String,
    },
    /// The file is not a valid annotation file.
    #[error("{source_name}:{line}: {message}")]
    Invalid {
        /// The file name.
        source_name: String,
        /// The line of the error (starting at 1).
        line: usize,
        /// What is wrong and how to fix it.
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    functions: BTreeMap<Spanned<String>, RawFunction>,
    #[serde(default)]
    templates: BTreeMap<Spanned<String>, Spanned<String>>,
    library: Option<RawLibrary>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLibrary {
    exceptions: Option<Spanned<bool>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFunction {
    returns: Option<Spanned<ReturnAnnotation>>,
    #[serde(default)]
    params: BTreeMap<Spanned<String>, Spanned<ParamAnnotation>>,
}

impl Annotations {
    /// Reads the annotation file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError::Read {
            path: path.to_owned(),
            message: e.to_string(),
        })?;
        Self::from_toml_str(&text, &path.display().to_string())
    }

    /// Reads from TOML text. `source_name` appears as the file name in error messages.
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
                    message: "the function name cannot be empty".to_owned(),
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
        let mut templates = Vec::new();
        for (name, spelling) in raw.templates {
            let line = line_of(text, name.span().start);
            let rust_name = name.into_inner();
            let mut chars = rust_name.chars();
            let identifier = chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !identifier {
                return Err(ConfigError::Invalid {
                    source_name: source_name.to_owned(),
                    line,
                    message: format!(
                        "the `[templates]` key `{rust_name}` must be a Rust type name (e.g. `Vector3f`)"
                    ),
                });
            }
            let spelling = spelling.into_inner();
            if spelling.trim().is_empty() {
                return Err(ConfigError::Invalid {
                    source_name: source_name.to_owned(),
                    line,
                    message: format!(
                        "`[templates]` `{rust_name}` cannot be empty; give a C++ spelling (e.g. \
                         `\"Vector3<float>\"`)"
                    ),
                });
            }
            templates.push(TemplateEntry {
                rust_name,
                spelling,
                line,
            });
        }
        templates.sort_by_key(|t| t.line);
        let no_exceptions = raw
            .library
            .and_then(|library| library.exceptions)
            .filter(|exceptions| !*exceptions.get_ref())
            .map(|exceptions| line_of(text, exceptions.span().start));
        Ok(Annotations {
            source_name: source_name.to_owned(),
            functions,
            templates,
            no_exceptions,
        })
    }

    /// Are there no annotations at all?
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty() && self.templates.is_empty() && self.no_exceptions.is_none()
    }

    /// The instantiations to give to the parser: (Rust name, C++ spelling).
    pub fn instantiations(&self) -> Vec<(String, String)> {
        self.templates
            .iter()
            .map(|t| (t.rust_name.clone(), t.spelling.clone()))
            .collect()
    }
}

fn located<T>(text: &str, spanned: Spanned<T>) -> Located<T> {
    let line = line_of(text, spanned.span().start);
    Located {
        value: spanned.into_inner(),
        line,
    }
}

/// The line number of a byte position (starting at 1).
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
            other => panic!("should have been invalid: {other:?}"),
        }
    }

    #[test]
    fn reads_return_and_param_annotations_with_lines() {
        let text = r#"# annotations
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
        assert!(parse("# only a comment\n").unwrap().is_empty());
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
            ("[functions.\"f\".params]\np = { slice = \" \" }\n", "empty"),
            ("[functions.\"f\".params]\np = { slice = 3 }\n", "slice"),
            ("[functions.\"f\".params]\np = \"slice\"\n", "slice = "),
        ] {
            let (line, message) = invalid_line(text);
            assert_eq!(line, 2, "{text}");
            assert!(message.contains(needle), "{text}: {message}");
        }
    }

    #[test]
    fn templates_are_read_in_file_order() {
        let text =
            "[templates]\nVector3f = \"Vector3<float>\"\nQuaternion = \"QuaternionT<float>\"\n";
        let annotations = parse(text).unwrap();
        assert_eq!(
            annotations.instantiations(),
            [
                ("Vector3f".to_owned(), "Vector3<float>".to_owned()),
                ("Quaternion".to_owned(), "QuaternionT<float>".to_owned()),
            ]
        );
        assert_eq!(annotations.templates[1].line, 3);
        assert!(!annotations.is_empty());
    }

    #[test]
    fn template_names_must_be_rust_type_names() {
        let (line, message) =
            invalid_line("[templates]\n\"Vector3<float>\" = \"Vector3<float>\"\n");
        assert_eq!(line, 2);
        assert!(message.contains("Rust type name"), "{message}");
        let (line, _) = invalid_line("[templates]\nV = \" \"\n");
        assert_eq!(line, 2);
    }

    #[test]
    fn library_without_exceptions_is_a_promise_with_a_line() {
        let annotations = parse("# library\n[library]\nexceptions = false\n").unwrap();
        assert_eq!(annotations.no_exceptions, Some(3));
        assert_eq!(
            parse("[library]\nexceptions = true\n")
                .unwrap()
                .no_exceptions,
            None
        );
        let (line, message) = invalid_line("[library]\nexception = false\n");
        assert_eq!(line, 2);
        assert!(message.contains("exception"), "{message}");
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
        let error = Annotations::from_file(Path::new("/missing/ferrobridge.toml")).unwrap_err();
        assert!(matches!(error, ConfigError::Read { .. }));
        assert!(error.to_string().contains("/missing/ferrobridge.toml"));
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
