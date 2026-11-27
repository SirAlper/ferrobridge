# AP_Math demosu

ArduPilot'un [`AP_Math`](https://github.com/ArduPilot/ardupilot/tree/master/libraries/AP_Math) kütüphanesini Rust'tan, elle bridge ya da `unsafe` yazmadan kullanır. Bridge'leri `build.rs` derleme sırasında `ferrobridge-build` ile üretir.

```rust
use ferrobridge_demo_ap_math::{crc, scurve::SCurve};

let checksum = !crc::crc_crc32(!0, b"123456789")?;    // C++: crc_crc32(uint32_t, const uint8_t*, uint32_t)
let mut curve = SCurve::new()?;                         // sahip olunan C++ nesnesi
curve.init()?;
```

## Lisans

Bu dizin **GPL-3.0-or-later** lisanslıdır (`LICENSE`), çünkü GPL-3.0 lisanslı ArduPilot'a bağlanır. ferrobridge'in kendisi `MIT OR Apache-2.0` lisanslı kalır. Ayrıntılar `docs/adr/0008-gpl-demo-out-of-tree.md`'de.

- ArduPilot kodu bu depoya girmez. `ARDUPILOT_DIR` bir checkout'u göstermelidir.
- GPL header'larından üretilen `bridge.rs`/`shim.*` dosyaları commit'lenmez; `target/` altındaki `OUT_DIR`'e üretilir.
- `stubs/` ve `annotations/` bu demo için yazıldı.

## Çalıştırma

```bash
git clone --depth 1 https://github.com/ArduPilot/ardupilot.git ~/ardupilot
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"   # macOS
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
```

Tam checkout'a gerek yok. Derlemenin include ettiği 28 kütüphane yeterli (`clang -M` ile çıkarıldı):

```bash
git clone --depth 1 --filter=blob:none --sparse https://github.com/ArduPilot/ardupilot.git ~/ardupilot
git -C ~/ardupilot sparse-checkout set \
  libraries/AC_Fence libraries/AP_Arming libraries/AP_BoardConfig libraries/AP_CANManager \
  libraries/AP_Common libraries/AP_CustomRotations libraries/AP_Filesystem libraries/AP_HAL \
  libraries/AP_HAL_SITL libraries/AP_InertialSensor libraries/AP_InternalError libraries/AP_Logger \
  libraries/AP_MSP libraries/AP_Math libraries/AP_Mission libraries/AP_Mount libraries/AP_Networking \
  libraries/AP_Param libraries/AP_ROMFS libraries/AP_Rally libraries/AP_RangeFinder libraries/AP_Relay \
  libraries/AP_Scripting libraries/AP_SerialManager libraries/AP_Terrain libraries/AP_Vehicle \
  libraries/GCS_MAVLink libraries/StorageManager
```

Doğrulama: tam checkout'ta, ArduPilot `437a3301cc`, macOS (arm64), Homebrew LLVM 23. Seyrek klonla parse ölçümde denendi (`.claude/plans/m5-apmath-olcum.md`), derleme denenmedi.

## Neler var

| Modül | Header | Ne bağlanıyor |
|---|---|---|
| `ap_math` | `AP_Math.h` | 56 fonksiyon: açı dönüşümleri (`radians_f32`, `degrees`), sarma, sınırlama, birim dönüşümleri, `fill_nanf(&mut [f32])` |
| `control` | `control.h` | 17 fonksiyon: ivme/hız şekillendirme (`&mut f32` çıkış parametreleri), açı ↔ ivme |
| `crc` | `crc.h` | 26 fonksiyon: CRC-8/16/24/32/64, Fletcher, FNV-1a, hepsi `&[u8]` / `&[u32]` |
| `scurve` | `SCurve.h` | `SCurve` sınıfı ve 8 üyesi |
| `spline_curve` | `SplineCurve.h` | `SplineCurve` metotları (public constructor'ı olmadığı için Rust'ta oluşturulamıyor) |
| `location` | `location.h` | 6 enlem/boylam kontrolü |
| `rotations` | `rotations.h` | `Rotation` enum'u |
| `definitions` | `definitions.h` | `WGS84_*` sabitleri |

Template olmayan API'nin %90'ı, toplam API'nin %31'i bağlanıyor. Kalanların çoğu `Vector3f`, `Matrix3f`, `Quaternion` gibi template'ler; bunlar Milestone 6'nın işi. Her modülün bağlanmayan öğeleri ve gerekçeleri, üretilen `bridge.rs`'in başında listelenir (yolu derleme uyarısında yazar).

## Açıklamalar (`annotations/*.toml`)

Header'lar pointer'ların ve referansların sahibini söylemez. Her `.toml` dosyası, ArduPilot kaynağı okunarak verilmiş sözleri tutar. Örneğin "`crc_crc32`'nin `buf`'ı `size` elemanlık bir dizi ve saklanmıyor" sözü:

```toml
[functions.crc_crc32.params]
buf = { slice = "size" }
```

Bu söz Rust'ta `crc_crc32(crc: u32, buf: &[u8])` imzası olur. Slice `uint32_t`'ye sığmazsa çağrı `Err` döner; uzunluk sessizce kırpılmaz. Her header'ın ayrı bir dosyası var, çünkü açıklama dosyasındaki her ad o header'da bulunmalı (yazım hatalarını yakalamak için).

`stubs/ap_stubs.cc`, `AP_Math`'in kendi dışından istediği dört parçayı sağlar: `AP_InternalError`, `AP_CustomRotations`, `AP_HAL::panic` ve `AP::*` erişimcileri.
