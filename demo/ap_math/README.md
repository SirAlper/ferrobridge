# AP_Math demosu

ArduPilot'un [`AP_Math`](https://github.com/ArduPilot/ardupilot/tree/master/libraries/AP_Math) kütüphanesi Rust'tan kullanılıyor. Elle bridge ya da `unsafe` yazılmıyor; bridge'i `build.rs`, derleme sırasında `ferrobridge-build` ile üretiyor.

```rust
use ferrobridge_demo_ap_math::{self as ap, Quaternion, Rotation, Vector3f};

let a = Vector3f { x: 1.0, y: 2.0, z: 2.0 };       // gerçek Rust struct'ı
let b = a + Vector3f { x: 0.0, y: 1.0, z: 0.0 } * 2.0;  // C++ operatörleri, `std::ops` ile
let len = a.length();                               // `Result` yok: ArduPilot exception kullanmıyor

let mut q = Quaternion::new_noargs();
q.from_euler_f32_f32_f32(0.0, 0.0, std::f32::consts::FRAC_PI_2);
let nose = q * Vector3f { x: 1.0, y: 0.0, z: 0.0 };   // quaternion × vektör

let mut accel = Vector3f { x: 0.0, y: 0.0, z: -9.80665 };
accel.rotate(Rotation::ROTATION_ROLL_180);          // başka header'daki enum

let crc = !ap::crc_crc32(!0, b"123456789")?;        // `const uint8_t*` + uzunluk → `&[u8]`
```

## 5 dakikada çalıştır

```bash
git clone --depth 1 https://github.com/ArduPilot/ardupilot.git ~/ardupilot
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"      # macOS; Linux'ta libclang'ın dizini
ARDUPILOT_DIR=~/ardupilot cargo run --manifest-path demo/ap_math/Cargo.toml --example tour
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
```

`examples/tour.rs` sırayla şunları gösteriyor:
- vektör cebiri
- Euler açılarından quaternion, matrisle karşılaştırma
- `Rotation` ile sensör montajı
- `control.h`'tan sarsıntı sınırlı ivme
- CRC
- WGS84 sabitleri

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

**Doğrulandığı ortam:**
- ArduPilot `437a3301cc`, macOS (arm64), Homebrew LLVM 23.
- Tam checkout'la ve yukarıdaki 28 kütüphanelik seyrek klonla.
- CI (`.github/workflows/ci.yml`) aynı seyrek klonla Linux ve macOS'ta derleyip testleri ve turu çalıştırır.

## Elle cxx ile karşılaştırma

`Vector3f`'yi cxx ile elle bağlamak için şunların hepsini yazmak gerekir:
- bir `#[cxx::bridge]` modülü
- C++ tarafında `using Vector3f = Vector3<float>;` takma adı
- alanları birebir aynı sırada ve tipte bir `#[repr(C)]` Rust struct'ı ve onun için gerekçeli bir `unsafe impl cxx::ExternType`
- düzenin gerçekten aynı olduğunu doğrulayan `static_assert`'ler
- cxx operatör adlarını bağlayamadığı için her operatör için bir C++ sarmalayıcı fonksiyon
- her operatör için `impl std::ops::Add/Mul/...`
- aynı adlı overload'lar için ayrı Rust adları

Bunlar `Vector3f`'nin 51 üyesi için yazılır. `Vector2f`, `Matrix3f`, `Quaternion` ve `double` sürümleri için tekrarlanır. Bir alan ya da imza değiştiğinde de elle güncellenir.

ferrobridge ile aynı iş `ferrobridge.toml`'daki dört satır:

```toml
[templates]
Vector3f = "Vector3<float>"
Matrix3f = "Matrix3<float>"
Quaternion = "QuaternionT<float>"
```

**Demodaki toplam:**
- `ferrobridge.toml`'daki **59 açıklama satırından** **8 000 satırı aşkın üretilmiş kod** çıkıyor (`bridge.rs` + `shim.h` + `shim.cc`).
- Bunların içinde 8 değer tipi `unsafe impl` (hepsi gerekçeli) ve 48 düzen `static_assert`'i var.
- Hepsi derleme sırasında yeniden üretiliyor.

## Neler bağlanıyor

| | |
|---|---|
| **Değer tipleri** | `Vector2f/3f`, `Matrix3f`, `Quaternion` ve `double` sürümleri. Rust struct'ı; alanlar okunup yazılıyor; metotlar `&self`/`&mut self`; `+ - * / %`, `+= -= *= /=`, `==`, tekli `-` operatörleri `std::ops` ile |
| **Opak sınıflar** | `SCurve`, `chirp`: sahip olan sarmalayıcı, düşünce C++ destructor'ı çalışıyor. `SplineCurve`'ün public constructor'ı yok; metotları başka yollardan gelen nesnelerde kullanılabilir |
| **Serbest fonksiyonlar** | Açı dönüşümleri (`radians_f32`, `degrees`, `wrap_360_f32`), sınırlama, birim dönüşümleri, `control.h`'ın kontrolcü yardımcıları (`&mut f32` çıkış parametreleri), CRC'ler (`&[u8]`, `&[u32]`) |
| **Enum** | `Rotation` (gerçek Rust enum'u) |
| **Sabitler** | `WGS84_*` (`pub const`; değeri C++'takiyle bit bit aynı) |

**Kapsam:**
- `AP_Math` API'sinin yaklaşık **%82**'si bağlanıyor: 337/412 öğe, artı değer tiplerinin 12 alanı.
- Yöntem M5 ölçümüyle aynı: template üyeleri template başına bir kez sayılıyor; yalnızca `float` örneklemeleriyle sonuç %78.
- M5'te %31, M5 öncesinde %20'ydi.
- Ayrıntı: `.claude/plans/m5-apmath-olcum.md` → "M6 sonrası".

## Bilinen sınırlar

- **Fonksiyon template'leri bağlanmıyor:** `wrap_180<T>`, `is_zero<T>`, `constrain_value<T>` (v0.2).
- **`operator[]` bağlanmıyor.** ArduPilot'ta `[]` sınır denetimi yapmıyor (`MATH_CHECK_INDEXES` kapalı). Güvenli Rust'tan `v[5]` tanımsız davranış olurdu. Alanları (`v.x`) kullanın.
- **Overload'lar tip soneki alıyor:** `from_euler_f32_f32_f32`, `to_euler_mut_f32_mut_f32_mut_f32` (ADR-0005). Kuralı açık ama uzun adlar üretiyor.
- **ferrobridge, C++ fonksiyonlarının ön koşullarını bilemez.** Bir C++ fonksiyonu ön koşulu bozulunca tanımsız davranışa giriyorsa, ona güvenli Rust'tan yapılan çağrı da girer. Bu, bütün bağlama araçlarının sınırı. ferrobridge'in yapabildikleri şunlar: pointer ve uzunlukları slice yapıp taşmayı yakalamak, null dönüşleri `Err` yapmak, denetimsiz `[]`'yi bağlamamak.
- **Opak sınıfların constructor'ları** sahip olan sarmalayıcıyı `Result` içinde döndürüyor (`SCurve::new()?`).
- **Sözlerin bozulması.** `[library] exceptions = false` ArduPilot için doğru: firmware `-fno-exceptions` ile derleniyor ve `AP_Math`'te `throw` yok. Söz bozulsaydı program `std::terminate` ile sonlanırdı, tanımsız davranış olmazdı.

## Açıklamalar (`ferrobridge.toml`)

Header'lar pointer'ların ve referansların sahibini söylemez. `ferrobridge.toml`'daki her satır, ArduPilot kaynağı okunarak verilmiş bir sözdür. Örneğin "`crc_crc32`'nin `buf`'ı `size` elemanlık bir dizi ve saklanmıyor":

```toml
[functions.crc_crc32.params]
buf = { slice = "size" }
```

Bu söz Rust'ta `crc_crc32(crc: u32, buf: &[u8])` olur. Slice `uint32_t`'ye sığmazsa çağrı `Err` döner. Bilerek açıklanmayanlar (`crc_crc4` ve `void*` alan bir fonksiyon) dosyada gerekçeleriyle yazılı. Bütün sözler, üretilen `bridge.rs`'in başında "Açıklamaya dayanan varsayımlar" olarak listelenir.

`stubs/ap_stubs.cc`, `AP_Math`'in kendi dışından istediği dört parçayı sağlar: `AP_InternalError`, `AP_CustomRotations`, `AP_HAL::panic` ve `AP::*` erişimcileri. Sınırları dosyanın başında yazılı.

## Lisans

Bu dizin **GPL-3.0-or-later** lisanslıdır (`LICENSE`), çünkü GPL-3.0 lisanslı ArduPilot'a bağlanır. ferrobridge'in kendisi `MIT OR Apache-2.0` kalır (ADR-0003, ADR-0008).
- ArduPilot kodu bu depoya girmez; `ARDUPILOT_DIR` bir checkout'u gösterir.
- GPL header'larından üretilen dosyalar commit'lenmez; `target/` altındaki `OUT_DIR`'e üretilir.
