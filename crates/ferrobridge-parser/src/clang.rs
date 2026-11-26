//! libclang C API'si üstünde ince, güvenli sarmalayıcı (ADR-0001).
//!
//! Crate'teki bütün `unsafe` çağrılar bu modülde toplanır. libclang kaynakları (`CXIndex`,
//! `CXTranslationUnit`, `CXString`, `CXDiagnostic`) `Drop` ile ya da oluşturuldukları yerde bir
//! kez serbest bırakılır. `Cursor` ve `Type`, ait oldukları `TranslationUnit`'ten uzun
//! yaşayamaz (ömür parametresi).
//!
//! Tipler ham pointer içerdiği için `Send`/`Sync` değildir. Bu istenen bir durum: `clang-sys`
//! `runtime` modunda libclang iş parçacığına özel yüklenir.
//!
//! Desen olarak bindgen'in `clang.rs` dosyası örnek alındı.

use std::ffi::{CStr, CString, c_char, c_int};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use clang_sys as cx;

/// libclang'ın döndürdüğü bir `CXString`'i Rust `String`'ine kopyalar ve serbest bırakır.
fn take_string(s: cx::CXString) -> String {
    // SAFETY: `s` libclang'ın döndürdüğü, sahibi çağıran olan bir `CXString`.
    // `clang_getCString` ya null ya da `s` serbest bırakılana kadar geçerli, NUL ile biten bir
    // C string'i döndürür; metin serbest bırakmadan önce kopyalanıyor.
    let text = unsafe {
        let ptr = cx::clang_getCString(s);
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };
    // SAFETY: `s` yalnızca burada, bir kez serbest bırakılıyor.
    unsafe { cx::clang_disposeString(s) };
    text
}

/// Yüklü libclang'ın sürüm metni. Okunamazsa boş döner.
pub(crate) fn version() -> String {
    // SAFETY: argüman almaz; dönen `CXString`'in sahipliği `take_string`'e geçer.
    take_string(unsafe { cx::clang_getClangVersion() })
}

/// Bir kaynak konumunun dosya, satır ve sütun bilgisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Location {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

fn file_location(location: cx::CXSourceLocation) -> Location {
    let mut file: cx::CXFile = ptr::null_mut();
    let (mut line, mut column, mut offset) = (0, 0, 0);
    // SAFETY: `location` libclang'dan geldi; çıktı işaretçileri geçerli yerel değişkenler.
    unsafe {
        cx::clang_getFileLocation(location, &mut file, &mut line, &mut column, &mut offset);
    }
    let file = if file.is_null() {
        String::new()
    } else {
        // SAFETY: `file` null değil ve translation unit yaşadığı sürece geçerli.
        take_string(unsafe { cx::clang_getFileName(file) })
    };
    Location { file, line, column }
}

/// Bir libclang indeksi. Translation unit'ler bundan oluşturulur.
pub(crate) struct Index {
    raw: cx::CXIndex,
}

impl Index {
    /// Yeni bir indeks oluşturur; libclang başaramazsa `None`.
    pub(crate) fn new() -> Option<Self> {
        // SAFETY: ilk argüman (0) PCH bildirimlerini dışlamaz, ikinci (0) tanıları stderr'e
        // basmaz. libclang yüklü değilse `clang-sys` panikler; tanımsız davranış oluşmaz.
        let raw = unsafe { cx::clang_createIndex(0, 0) };
        (!raw.is_null()).then_some(Self { raw })
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        // SAFETY: `raw` `clang_createIndex`'ten geldi ve yalnızca burada serbest bırakılıyor.
        // Bu indekse bağlı `TranslationUnit`'ler ömür parametresi sayesinde daha önce düşer.
        unsafe { cx::clang_disposeIndex(self.raw) };
    }
}

/// libclang'ın bir tanısı (uyarı, hata...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawDiagnostic {
    pub(crate) severity: cx::CXDiagnosticSeverity,
    pub(crate) location: Location,
    pub(crate) message: String,
}

/// Ayrıştırılmış bir C++ dosyası.
pub(crate) struct TranslationUnit<'index> {
    raw: cx::CXTranslationUnit,
    _index: PhantomData<&'index Index>,
}

impl<'index> TranslationUnit<'index> {
    /// `file`'ı verilen derleyici argümanlarıyla ayrıştırır.
    ///
    /// Fonksiyon gövdeleri atlanır; header'ın yalnızca bildirimleri gerekiyor.
    /// Başarısızlıkta libclang'ın hata kodunu döndürür.
    pub(crate) fn parse(
        index: &'index Index,
        file: &CStr,
        args: &[CString],
    ) -> Result<Self, cx::CXErrorCode> {
        let arg_ptrs: Vec<*const c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
        let Ok(arg_count) = c_int::try_from(arg_ptrs.len()) else {
            return Err(cx::CXError_InvalidArguments);
        };
        let mut raw: cx::CXTranslationUnit = ptr::null_mut();
        // SAFETY: `index.raw` geçerli bir indeks. `file` ve `arg_ptrs`'in gösterdiği bütün
        // string'ler bu çağrı boyunca yaşayan, NUL ile biten C string'leri. Kaydedilmemiş dosya
        // yok (null, 0). `raw` geçerli bir çıktı işaretçisi.
        let code = unsafe {
            cx::clang_parseTranslationUnit2(
                index.raw,
                file.as_ptr(),
                arg_ptrs.as_ptr(),
                arg_count,
                ptr::null_mut(),
                0,
                cx::CXTranslationUnit_SkipFunctionBodies,
                &mut raw,
            )
        };
        if code != cx::CXError_Success {
            if !raw.is_null() {
                // SAFETY: libclang hata döndürse de bir translation unit verdiyse sahibi biziz;
                // sızmasın diye burada, bir kez serbest bırakılıyor ve bir daha kullanılmıyor.
                unsafe { cx::clang_disposeTranslationUnit(raw) };
            }
            return Err(code);
        }
        if raw.is_null() {
            return Err(cx::CXError_Failure);
        }
        Ok(Self {
            raw,
            _index: PhantomData,
        })
    }

    /// Translation unit'in kök cursor'ı.
    pub(crate) fn cursor(&self) -> Cursor<'_> {
        // SAFETY: `self.raw` geçerli; dönen cursor `self`'in ömrüne bağlanıyor.
        Cursor::new(unsafe { cx::clang_getTranslationUnitCursor(self.raw) })
    }

    /// Ayrıştırma sırasında üretilen bütün tanılar.
    pub(crate) fn diagnostics(&self) -> Vec<RawDiagnostic> {
        // SAFETY: `self.raw` geçerli bir translation unit.
        let count = unsafe { cx::clang_getNumDiagnostics(self.raw) };
        (0..count)
            .map(|i| {
                // SAFETY: `i < count`; dönen tanı aşağıda bir kez serbest bırakılıyor.
                let diagnostic = unsafe { cx::clang_getDiagnostic(self.raw, i) };
                // SAFETY: `diagnostic` geçerli ve henüz serbest bırakılmadı.
                let (severity, location, message) = unsafe {
                    (
                        cx::clang_getDiagnosticSeverity(diagnostic),
                        cx::clang_getDiagnosticLocation(diagnostic),
                        cx::clang_getDiagnosticSpelling(diagnostic),
                    )
                };
                let result = RawDiagnostic {
                    severity,
                    location: file_location(location),
                    message: take_string(message),
                };
                // SAFETY: `diagnostic` yalnızca burada serbest bırakılıyor; ondan alınan
                // değerler yukarıda kopyalandı.
                unsafe { cx::clang_disposeDiagnostic(diagnostic) };
                result
            })
            .collect()
    }
}

impl Drop for TranslationUnit<'_> {
    fn drop(&mut self) {
        // SAFETY: `raw` `clang_parseTranslationUnit2`'den geldi ve yalnızca burada serbest
        // bırakılıyor. Ona bağlı `Cursor`/`Type`'lar ömür parametresi sayesinde daha önce düşer.
        unsafe { cx::clang_disposeTranslationUnit(self.raw) };
    }
}

/// AST'deki bir düğüm.
#[derive(Clone, Copy)]
pub(crate) struct Cursor<'tu> {
    raw: cx::CXCursor,
    _tu: PhantomData<&'tu ()>,
}

/// `clang_visitChildren` için geri çağrı: her çocuğu `data`'daki `Vec`'e ekler.
extern "C" fn collect_child(
    child: cx::CXCursor,
    _parent: cx::CXCursor,
    data: cx::CXClientData,
) -> cx::CXChildVisitResult {
    let pushed = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `data`, `Cursor::children` içinde `&mut Vec<CXCursor>`'dan oluşturuldu ve
        // `clang_visitChildren` dönene kadar geçerli. libclang geri çağrıyı aynı iş
        // parçacığında ve sırayla çağırır; aynı anda başka bir referans yok.
        let children = unsafe { &mut *data.cast::<Vec<cx::CXCursor>>() };
        children.push(child);
    }));
    // Panik FFI sınırını geçmez: yakalanır ve gezme durdurulur.
    if pushed.is_ok() {
        cx::CXChildVisit_Continue
    } else {
        cx::CXChildVisit_Break
    }
}

impl<'tu> Cursor<'tu> {
    fn new(raw: cx::CXCursor) -> Self {
        Self {
            raw,
            _tu: PhantomData,
        }
    }

    pub(crate) fn kind(&self) -> cx::CXCursorKind {
        self.raw.kind
    }

    /// Cursor türünün libclang'daki adı (ör. `"VarDecl"`).
    pub(crate) fn kind_spelling(&self) -> String {
        // SAFETY: herhangi bir cursor türü için geçerli; sahiplik `take_string`'e geçer.
        take_string(unsafe { cx::clang_getCursorKindSpelling(self.raw.kind) })
    }

    pub(crate) fn spelling(&self) -> String {
        // SAFETY: `self.raw` translation unit yaşadığı sürece geçerli.
        take_string(unsafe { cx::clang_getCursorSpelling(self.raw) })
    }

    pub(crate) fn location(&self) -> Location {
        // SAFETY: `self.raw` geçerli.
        file_location(unsafe { cx::clang_getCursorLocation(self.raw) })
    }

    /// Bildirim, ayrıştırılan ana dosyada mı (`#include` edilen bir dosyada değil)?
    pub(crate) fn is_in_main_file(&self) -> bool {
        // SAFETY: `self.raw` geçerli.
        unsafe { cx::clang_Location_isFromMainFile(cx::clang_getCursorLocation(self.raw)) != 0 }
    }

    /// Doğrudan çocuk düğümler, kaynak sırasıyla.
    pub(crate) fn children(&self) -> Vec<Cursor<'tu>> {
        let mut children: Vec<cx::CXCursor> = Vec::new();
        // SAFETY: `self.raw` geçerli. `collect_child` yalnızca `children`'a yazar ve
        // `children` bu çağrı bitene kadar yaşar.
        unsafe {
            cx::clang_visitChildren(self.raw, collect_child, (&raw mut children).cast());
        }
        children.into_iter().map(Cursor::new).collect()
    }

    /// Fonksiyon parametreleri; fonksiyon değilse boş.
    pub(crate) fn arguments(&self) -> Vec<Cursor<'tu>> {
        // SAFETY: `self.raw` geçerli; fonksiyon olmayan cursor için -1 döner.
        let count = unsafe { cx::clang_Cursor_getNumArguments(self.raw) };
        let count = u32::try_from(count).unwrap_or(0);
        (0..count)
            // SAFETY: `i < count`, yani geçerli bir parametre indeksi.
            .map(|i| Cursor::new(unsafe { cx::clang_Cursor_getArgument(self.raw, i) }))
            .collect()
    }

    pub(crate) fn ty(&self) -> Type<'tu> {
        // SAFETY: `self.raw` geçerli.
        Type::new(unsafe { cx::clang_getCursorType(self.raw) })
    }

    pub(crate) fn result_type(&self) -> Type<'tu> {
        // SAFETY: `self.raw` geçerli; fonksiyon değilse geçersiz tip döner.
        Type::new(unsafe { cx::clang_getCursorResultType(self.raw) })
    }

    pub(crate) fn exception_spec(&self) -> c_int {
        // SAFETY: `self.raw` geçerli; fonksiyon değilse -1 döner.
        unsafe { cx::clang_getCursorExceptionSpecificationType(self.raw) }
    }

    pub(crate) fn linkage(&self) -> cx::CXLinkageKind {
        // SAFETY: `self.raw` geçerli.
        unsafe { cx::clang_getCursorLinkage(self.raw) }
    }

    /// Typedef bildiriminin altta yatan tipi.
    pub(crate) fn typedef_underlying(&self) -> Type<'tu> {
        // SAFETY: `self.raw` geçerli; typedef değilse geçersiz tip döner.
        Type::new(unsafe { cx::clang_getTypedefDeclUnderlyingType(self.raw) })
    }

    /// Bildirimi içeren anlamsal üst düğüm (namespace, sınıf veya translation unit).
    pub(crate) fn semantic_parent(&self) -> Cursor<'tu> {
        // SAFETY: `self.raw` geçerli; üstü yoksa null cursor döner.
        Cursor::new(unsafe { cx::clang_getCursorSemanticParent(self.raw) })
    }

    /// Bu bir tanım mı (yalnızca ileri bildirim değil)?
    pub(crate) fn is_definition(&self) -> bool {
        // SAFETY: `self.raw` geçerli.
        unsafe { cx::clang_isCursorDefinition(self.raw) != 0 }
    }

    /// İsimsiz bir namespace, struct veya enum mu?
    pub(crate) fn is_anonymous(&self) -> bool {
        // SAFETY: `self.raw` geçerli.
        unsafe { cx::clang_Cursor_isAnonymous(self.raw) != 0 }
    }

    /// `inline namespace` mi (ör. libc++'ın `std::__1`'i)?
    pub(crate) fn is_inline_namespace(&self) -> bool {
        // SAFETY: `self.raw` geçerli; namespace değilse 0 döner.
        unsafe { cx::clang_Cursor_isInlineNamespace(self.raw) != 0 }
    }

    /// Sınıf üyesinin erişim belirteci (`CX_CXXPublic` ...).
    pub(crate) fn access(&self) -> cx::CX_CXXAccessSpecifier {
        // SAFETY: `self.raw` geçerli; üye değilse `CX_CXXInvalidAccessSpecifier` döner.
        unsafe { cx::clang_getCXXAccessSpecifier(self.raw) }
    }

    /// `= delete` ile silinmiş bir fonksiyon mu?
    pub(crate) fn is_deleted(&self) -> bool {
        // SAFETY: `self.raw` geçerli; fonksiyon değilse 0 döner.
        unsafe { cx::clang_CXXMethod_isDeleted(self.raw) != 0 }
    }

    pub(crate) fn is_const_method(&self) -> bool {
        // SAFETY: `self.raw` geçerli; metot değilse 0 döner.
        unsafe { cx::clang_CXXMethod_isConst(self.raw) != 0 }
    }

    pub(crate) fn is_static_method(&self) -> bool {
        // SAFETY: `self.raw` geçerli; metot değilse 0 döner.
        unsafe { cx::clang_CXXMethod_isStatic(self.raw) != 0 }
    }

    pub(crate) fn is_virtual_method(&self) -> bool {
        // SAFETY: `self.raw` geçerli; metot değilse 0 döner.
        unsafe { cx::clang_CXXMethod_isVirtual(self.raw) != 0 }
    }

    /// Saf sanal metodu olduğu için doğrudan oluşturulamayan bir sınıf mı?
    pub(crate) fn is_abstract_record(&self) -> bool {
        // SAFETY: `self.raw` geçerli; sınıf değilse 0 döner.
        unsafe { cx::clang_CXXRecord_isAbstract(self.raw) != 0 }
    }

    /// `enum class` mı?
    pub(crate) fn is_scoped_enum(&self) -> bool {
        // SAFETY: `self.raw` geçerli; enum değilse 0 döner.
        unsafe { cx::clang_EnumDecl_isScoped(self.raw) != 0 }
    }

    /// Enum'un altta yatan tamsayı tipi.
    pub(crate) fn enum_integer_type(&self) -> Type<'tu> {
        // SAFETY: `self.raw` geçerli; enum değilse geçersiz tip döner.
        Type::new(unsafe { cx::clang_getEnumDeclIntegerType(self.raw) })
    }

    /// Enum değerinin işaretli sayısal karşılığı.
    pub(crate) fn enum_value(&self) -> i64 {
        // SAFETY: `self.raw` geçerli; enum değeri değilse `LLONG_MIN` döner.
        unsafe { cx::clang_getEnumConstantDeclValue(self.raw) }
    }

    /// Enum değerinin işaretsiz sayısal karşılığı.
    pub(crate) fn enum_unsigned_value(&self) -> u64 {
        // SAFETY: `self.raw` geçerli; enum değeri değilse `ULLONG_MAX` döner.
        unsafe { cx::clang_getEnumConstantDeclUnsignedValue(self.raw) }
    }
}

/// Bir C++ tipi.
#[derive(Clone, Copy)]
pub(crate) struct Type<'tu> {
    raw: cx::CXType,
    _tu: PhantomData<&'tu ()>,
}

impl<'tu> Type<'tu> {
    fn new(raw: cx::CXType) -> Self {
        Self {
            raw,
            _tu: PhantomData,
        }
    }

    pub(crate) fn kind(&self) -> cx::CXTypeKind {
        self.raw.kind
    }

    pub(crate) fn spelling(&self) -> String {
        // SAFETY: `self.raw` translation unit yaşadığı sürece geçerli.
        take_string(unsafe { cx::clang_getTypeSpelling(self.raw) })
    }

    pub(crate) fn canonical(&self) -> Type<'tu> {
        // SAFETY: `self.raw` geçerli.
        Type::new(unsafe { cx::clang_getCanonicalType(self.raw) })
    }

    pub(crate) fn pointee(&self) -> Type<'tu> {
        // SAFETY: `self.raw` geçerli; pointer/referans değilse geçersiz tip döner.
        Type::new(unsafe { cx::clang_getPointeeType(self.raw) })
    }

    pub(crate) fn is_const(&self) -> bool {
        // SAFETY: `self.raw` geçerli.
        unsafe { cx::clang_isConstQualifiedType(self.raw) != 0 }
    }

    /// Bayt cinsinden boyut; libclang hesaplayamazsa `None`.
    pub(crate) fn size_of(&self) -> Option<u64> {
        // SAFETY: `self.raw` geçerli; hata durumunda negatif değer döner.
        u64::try_from(unsafe { cx::clang_Type_getSizeOf(self.raw) }).ok()
    }

    /// `std::int32_t` gibi nitelenmiş adların sardığı tip.
    pub(crate) fn named(&self) -> Type<'tu> {
        // SAFETY: `self.raw` geçerli; elaborated değilse geçersiz tip döner.
        Type::new(unsafe { cx::clang_Type_getNamedType(self.raw) })
    }

    pub(crate) fn declaration(&self) -> Cursor<'tu> {
        // SAFETY: `self.raw` geçerli; bildirimi yoksa null cursor döner.
        Cursor::new(unsafe { cx::clang_getTypeDeclaration(self.raw) })
    }

    pub(crate) fn is_variadic(&self) -> bool {
        // SAFETY: `self.raw` geçerli; fonksiyon tipi değilse 0 döner.
        unsafe { cx::clang_isFunctionTypeVariadic(self.raw) != 0 }
    }

    /// Template özelleştirmesinin tip argümanları (ör. `std::unique_ptr<T, D>` için `[T, D]`).
    /// Template değilse boş.
    pub(crate) fn template_args(&self) -> Vec<Type<'tu>> {
        // SAFETY: `self.raw` geçerli; template değilse -1 döner.
        let count = unsafe { cx::clang_Type_getNumTemplateArguments(self.raw) };
        let count = u32::try_from(count).unwrap_or(0);
        (0..count)
            // SAFETY: `i < count`, yani geçerli bir argüman indeksi.
            .map(|i| Type::new(unsafe { cx::clang_Type_getTemplateArgumentAsType(self.raw, i) }))
            .collect()
    }
}
