//! Güvenli API testleri gerçekten idiomatik mi? Ham katman (`raw_layer.rs`) dışındaki test
//! dosyalarında cxx tipleri, ham metotlar ya da `unsafe` geçmemeli.

use std::fs;
use std::path::Path;

const FORBIDDEN: [&str; 7] = [
    "unsafe",
    "let_cxx_string",
    "CxxString",
    "CxxVector",
    "cxx::",
    "ffi::",
    ".raw_",
];

#[test]
fn safe_api_tests_use_no_cxx_types_or_unsafe() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut checked = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".rs") || name == "raw_layer.rs" || name == "idiomatic.rs" {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        for (number, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for word in FORBIDDEN {
                assert!(
                    !code.contains(word),
                    "{name}:{}: güvenli API testinde `{word}` geçiyor: {line}",
                    number + 1
                );
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 8, "beklenen güvenli API test dosyaları bulunamadı");
}
