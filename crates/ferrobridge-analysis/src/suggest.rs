//! `ferrobridge suggest`: açıklama gereken yerler için `ferrobridge.toml` taslağı.
//!
//! Taslaktaki bütün değerler **yorum satırıdır**. ferrobridge sahipliği tahmin etmez
//! (güvenlik kuralı 1); kullanıcı her satırı C++ kütüphanesini bilerek açar. Açılmayan
//! satırın öğesi için kod üretilmemeye devam eder.

use std::fmt::Write as _;

use crate::config::ReturnAnnotation;
use crate::plan::{BridgePlan, Suggestion, SuggestionTarget};

const HEADER: &str = "\
# ferrobridge.toml taslağı (`ferrobridge suggest` üretti).
#
# Her açıklama, C++ kütüphanesi hakkında verdiğin bir SÖZDÜR. Söz yanlışsa üretilen kod
# tanımsız davranışa yol açabilir. Emin olduğun satırın başındaki `# ` işaretini kaldır;
# emin olmadıklarını yorumda bırak, onlar için kod üretilmez.
#
#   borrowed (parametre): pointer/referans tek bir nesneyi gösteriyor (dizi değil), null
#                         değil, yalnızca çağrı süresince kullanılıyor ve saklanmıyor.
#   { slice = \"n\" } (parametre): pointer `n` parametresi kadar elemanlık bir diziyi gösteriyor
#                         ve yalnızca çağrı süresince kullanılıyor; Rust'ta ikisi tek bir
#                         `&[T]` olur. Aynı parametre için yalnızca bir satır aç.
#   owned    (dönüş):     pointer `new` ile ayrıldı; silmek çağırana düşüyor.
#   borrowed (dönüş):     pointer/referans nesnenin sahip olduğu bir şeyi gösteriyor ve
#                         nesne yaşadıkça geçerli (yalnızca metotlar).
#
# Aynı başlık dosyanda zaten varsa satırları o başlığın altına taşı.
";

/// Bir anahtar için birleştirilmiş öneriler (overload'lar aynı anahtarı paylaşır).
struct Group<'p> {
    key: &'p str,
    signatures: Vec<(&'p str, String)>,
    returns: Vec<(ReturnAnnotation, &'p str)>,
    /// (ad, C++ tipi, aday slice uzunlukları)
    params: Vec<(&'p str, &'p str, &'p [String])>,
}

/// Plandaki önerileri yorum satırlı bir TOML taslağına çevirir. Öneri yoksa bunu söyleyen
/// tek satırlık bir yorum döner.
pub fn render_suggestions(plan: &BridgePlan) -> String {
    if plan.suggestions.is_empty() {
        return "# Açıklama gereken yer yok.\n".to_owned();
    }
    let groups = group(&plan.suggestions);
    let mut out = String::from(HEADER);
    for group in groups {
        out.push('\n');
        for (signature, location) in &group.signatures {
            let _ = writeln!(out, "# {signature}  ({location})");
        }
        let key = toml_key(group.key);
        if !group.returns.is_empty() {
            let _ = writeln!(out, "[functions.{key}]");
            for (option, cpp_type) in &group.returns {
                let _ = writeln!(out, "# returns = \"{option}\"  # dönüş: {cpp_type}");
            }
        }
        if !group.params.is_empty() {
            let _ = writeln!(out, "[functions.{key}.params]");
            for (name, cpp_type, lengths) in &group.params {
                let key = toml_key(name);
                let _ = writeln!(out, "# {key} = \"borrowed\"  # {cpp_type}");
                for len in *lengths {
                    let _ = writeln!(
                        out,
                        "# {key} = {{ slice = \"{len}\" }}  # {name}, {len} elemanlık bir dizi"
                    );
                }
            }
        }
    }
    out
}

fn group(suggestions: &[Suggestion]) -> Vec<Group<'_>> {
    let mut groups: Vec<Group<'_>> = Vec::new();
    for suggestion in suggestions {
        let index = match groups.iter().position(|g| g.key == suggestion.key) {
            Some(index) => index,
            None => {
                groups.push(Group {
                    key: &suggestion.key,
                    signatures: Vec::new(),
                    returns: Vec::new(),
                    params: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        let location = format!("{}:{}", suggestion.location.file, suggestion.location.line);
        if !group
            .signatures
            .iter()
            .any(|(s, _)| *s == suggestion.signature)
        {
            group.signatures.push((&suggestion.signature, location));
        }
        match &suggestion.target {
            SuggestionTarget::Return { options } => {
                for option in options {
                    if !group.returns.iter().any(|(o, _)| o == option) {
                        group.returns.push((*option, &suggestion.cpp_type));
                    }
                }
            }
            SuggestionTarget::Param {
                name,
                slice_lengths,
            } => {
                if !group.params.iter().any(|(n, _, _)| n == name) {
                    group
                        .params
                        .push((name, &suggestion.cpp_type, slice_lengths));
                }
            }
        }
    }
    groups
}

/// TOML anahtarı: düz anahtar olabiliyorsa olduğu gibi, değilse tırnaklı.
fn toml_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        key.to_owned()
    } else {
        format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\""))
    }
}
