//! Compiles the generated bridges, the shims and the fixtures' C++ implementations.
//!
//! Uses the committed-file mode of `ferrobridge-build` (the same path as the README example).

const FIXTURES: [&str; 11] = [
    "classes",
    "enums",
    "strings",
    "ownership",
    "vectors",
    "slices",
    "inline_and_constants",
    "multi_header",
    "value_types",
    "operators",
    "no_exceptions",
];

fn main() {
    let mut build = ferrobridge_build::Build::new();
    for name in FIXTURES {
        build
            .committed(format!("src/generated/{name}"))
            .file(format!("../../tests/fixtures/{name}/impl.cc"));
    }
    build
        // Paths such as `classes/shim.h` and `classes/input.h` are resolved from these two roots.
        .include_dir("src/generated")
        .include_dir("../../tests/fixtures");
    if let Err(error) = build.compile("ferrobridge_e2e") {
        panic!("{error}");
    }
}
