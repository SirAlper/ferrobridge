//! Üretilmiş bridge'leri, shim'leri ve fixture'ların C++ gerçeklemelerini derler.
//!
//! `ferrobridge-build`'in commit'li dosya modunu kullanır (README'deki örnekle aynı yol).

const FIXTURES: [&str; 7] = [
    "classes",
    "enums",
    "strings",
    "ownership",
    "vectors",
    "slices",
    "inline_and_constants",
];

fn main() {
    let mut build = ferrobridge_build::Build::new();
    for name in FIXTURES {
        build
            .committed(format!("src/generated/{name}"))
            .file(format!("../../tests/fixtures/{name}/impl.cc"));
    }
    build
        // `classes/shim.h` ve `classes/input.h` gibi yollar bu iki kökten çözülür.
        .include_dir("src/generated")
        .include_dir("../../tests/fixtures");
    if let Err(error) = build.compile("ferrobridge_e2e") {
        panic!("{error}");
    }
}
