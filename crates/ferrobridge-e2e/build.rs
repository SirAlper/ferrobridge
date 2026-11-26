//! Üretilmiş bridge'leri, shim'leri ve fixture'ların C++ gerçeklemelerini derler.

const FIXTURES: [&str; 5] = ["classes", "enums", "strings", "ownership", "vectors"];

fn main() {
    let bridges: Vec<String> = FIXTURES
        .iter()
        .map(|name| format!("src/generated/{name}/bridge.rs"))
        .collect();
    let mut build = cxx_build::bridges(&bridges);
    for name in FIXTURES {
        build
            .file(format!("src/generated/{name}/shim.cc"))
            .file(format!("../../tests/fixtures/{name}/impl.cc"));
        for path in [
            format!("src/generated/{name}"),
            format!("../../tests/fixtures/{name}"),
        ] {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    build
        // `classes/shim.h` ve `classes/input.h` gibi yollar bu iki kökten çözülür.
        .include("src/generated")
        .include("../../tests/fixtures")
        .std("c++17")
        .compile("ferrobridge_e2e");
}
