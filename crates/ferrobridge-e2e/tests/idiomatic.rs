//! Are the safe API tests really idiomatic? Test files other than the raw layer (`raw_layer.rs`)
//! must not contain cxx types, raw methods or `unsafe`.

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
                    "{name}:{}: `{word}` appears in a safe API test: {line}",
                    number + 1
                );
            }
        }
        checked += 1;
    }
    assert_eq!(
        checked, 12,
        "the expected safe API test files were not found"
    );
}
