//! ArduPilot'un `AP_Math` kütüphanesini ve ondan üretilen bridge'leri derler.
//!
//! ArduPilot kodu bu depoya girmez (GPL-3.0; ADR-0003). `ARDUPILOT_DIR` bir checkout'u
//! göstermeli; bridge'ler derleme sırasında `OUT_DIR`'e üretilir ve commit'lenmez (ADR-0008).

use std::path::{Path, PathBuf};

/// (header, bridge adı)
const HEADERS: [(&str, &str); 8] = [
    ("AP_Math.h", "ap_math"),
    ("control.h", "control"),
    ("crc.h", "crc"),
    ("location.h", "location"),
    ("SCurve.h", "scurve"),
    ("SplineCurve.h", "spline_curve"),
    ("rotations.h", "rotations"),
    ("definitions.h", "definitions"),
];

/// waf'ın SITL derlemesinde tanımladıkları (ölçüm ve M5 Task 0).
const DEFINES: [(&str, &str); 6] = [
    ("CONFIG_HAL_BOARD", "HAL_BOARD_SITL"),
    ("CONFIG_HAL_BOARD_SUBTYPE", "HAL_BOARD_SUBTYPE_NONE"),
    ("HAL_PROGRAM_SIZE_LIMIT_KB", "4096"),
    ("__AP_LINE__", "__LINE__"),
    ("__STDC_FORMAT_MACROS", "1"),
    ("APM_BUILD_DIRECTORY", "APM_BUILD_UNKNOWN"),
];

fn main() {
    println!("cargo:rerun-if-env-changed=ARDUPILOT_DIR");
    let libraries = ardupilot_libraries();

    let mut build = ferrobridge_build::Build::new();
    build
        .include_dir(&libraries)
        // macOS'ta `feenableexcept` buradan gelir.
        .include_dir(libraries.join("AP_Common/missing"))
        // `hwdef.h`
        .include_dir("stubs")
        .file("stubs/ap_stubs.cc")
        .files(cpp_sources(&libraries.join("AP_Math")))
        // ArduPilot'un kendi uyarıları (ör. kullanılmayan parametreler) demo çıktısını
        // boğmasın; libclang'a geçmez.
        .flag("-w");
    for (name, value) in DEFINES {
        build.define(name, Some(value));
    }
    for (header, name) in HEADERS {
        let mut bridge = ferrobridge_build::Generate::new(libraries.join("AP_Math").join(header), name)
            .include_as(&format!("AP_Math/{header}"))
            // Bazı header'lar kendi başına derlenmiyor (`SCurve.h` `Vector3f`'i, `rotations.h`
            // `uint8_t`'yi include etmiyor); ArduPilot'ta hep `AP_Math.h`'dan sonra gelirler.
            .prelude("AP_Math/AP_Math.h");
        let config = Path::new("annotations").join(format!("{name}.toml"));
        if config.is_file() {
            bridge = bridge.config(config);
        }
        build.generate(bridge);
    }
    if let Err(error) = build.compile("ap_math") {
        panic!("{error}");
    }
}

fn ardupilot_libraries() -> PathBuf {
    let Some(dir) = std::env::var_os("ARDUPILOT_DIR") else {
        panic!(
            "`ARDUPILOT_DIR` ayarlı değil. Bir ArduPilot checkout'unu göstermeli, ör.:\n  \
             ARDUPILOT_DIR=~/ardupilot cargo test\n\
             Seyrek klonlama komutları: demo/ap_math/README.md"
        );
    };
    let libraries = PathBuf::from(dir).join("libraries");
    if !libraries.join("AP_Math/AP_Math.h").is_file() {
        panic!(
            "`{}` bir ArduPilot checkout'u değil (`libraries/AP_Math/AP_Math.h` yok).",
            libraries.display()
        );
    }
    libraries
}

/// `AP_Math`'in bütün `.cpp` dosyaları, ad sırasıyla (derleme tekrarlanabilir olsun).
fn cpp_sources(dir: &Path) -> Vec<PathBuf> {
    let mut sources: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
        .collect();
    sources.sort();
    sources
}
