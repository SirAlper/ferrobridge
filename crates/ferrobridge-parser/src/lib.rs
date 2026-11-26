//! C++ header'larını libclang ile okuyan katman.
//!
//! libclang'ın C API'sine `clang-sys` üzerinden erişilir (bkz.
//! `docs/adr/0001-libclang-via-clang-sys.md`). Kütüphane derleme anında değil, çalışma anında
//! yüklenir (`clang-sys` `runtime` özelliği). Böylece libclang bulunamazsa program açılışta
//! çökmez; anlaşılır bir hata döner.
//!
//! Header okuma ve IR'a dönüştürme Milestone 1'de eklenecek.

use std::ffi::CStr;

/// Parser katmanının hataları.
#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    /// libclang dinamik kütüphanesi bulunamadı veya yüklenemedi.
    #[error("libclang yüklenemedi: {0}")]
    LoadFailed(String),
    /// libclang yüklendi ama sürüm bilgisi okunamadı.
    #[error("libclang sürüm bilgisi okunamadı")]
    VersionUnavailable,
}

/// libclang bu iş parçacığında yüklü değilse yükler.
fn ensure_loaded() -> Result<(), ParserError> {
    if clang_sys::is_loaded() {
        return Ok(());
    }
    clang_sys::load().map_err(ParserError::LoadFailed)
}

/// Yüklenen libclang'ın sürüm metnini döndürür (ör. `"Homebrew clang version 23.1.6"`).
///
/// # Errors
///
/// libclang bulunamazsa [`ParserError::LoadFailed`], sürüm metni okunamazsa
/// [`ParserError::VersionUnavailable`] döner.
pub fn libclang_version() -> Result<String, ParserError> {
    ensure_loaded()?;

    // SAFETY: libclang yukarıda yüklendi. `clang_getClangVersion` argüman almaz ve sahibi
    // çağıran olan bir `CXString` döndürür; aşağıda bir kez serbest bırakılıyor.
    let cx_version = unsafe { clang_sys::clang_getClangVersion() };

    // SAFETY: `cx_version` geçerli bir `CXString`. Dönen işaretçi ya null'dır ya da
    // `cx_version` serbest bırakılana kadar geçerli, NUL ile biten bir C string'idir.
    let ptr = unsafe { clang_sys::clang_getCString(cx_version) };

    let result = if ptr.is_null() {
        Err(ParserError::VersionUnavailable)
    } else {
        // SAFETY: `ptr` null değil ve `cx_version` henüz serbest bırakılmadı; metin burada
        // sahip olunan bir `String`'e kopyalanıyor.
        Ok(unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned())
    };

    // SAFETY: `cx_version` bu fonksiyonda yalnızca bir kez serbest bırakılıyor ve `ptr`
    // bundan sonra kullanılmıyor.
    unsafe { clang_sys::clang_disposeString(cx_version) };

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_libclang_version() {
        let version = libclang_version().unwrap();
        assert!(
            version.contains("clang version"),
            "beklenmeyen sürüm metni: {version}"
        );
    }
}
