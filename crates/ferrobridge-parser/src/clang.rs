//! A thin, safe wrapper over the libclang C API (ADR-0001).
//!
//! All `unsafe` calls in the crate are gathered in this module. libclang resources (`CXIndex`,
//! `CXTranslationUnit`, `CXString`, `CXDiagnostic`) are released once, with `Drop` or where they
//! are created. `Cursor` and `Type` cannot outlive the `TranslationUnit` they belong to
//! (a lifetime parameter).
//!
//! Because the types contain raw pointers, they are not `Send`/`Sync`. This is intended: in
//! `runtime` mode, `clang-sys` loads libclang per thread.
//!
//! bindgen's `clang.rs` file was used as a model for the patterns.

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ulong};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use clang_sys as cx;

/// Copies a `CXString` returned by libclang into a Rust `String` and releases it.
fn take_string(s: cx::CXString) -> String {
    // SAFETY: `s` is a `CXString` returned by libclang and owned by the caller.
    // `clang_getCString` returns either null or a NUL-terminated C string that is valid until
    // `s` is released; the text is copied before it is released.
    let text = unsafe {
        let ptr = cx::clang_getCString(s);
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };
    // SAFETY: `s` is released only here, once.
    unsafe { cx::clang_disposeString(s) };
    text
}

/// The version text of the loaded libclang. Empty if it cannot be read.
pub(crate) fn version() -> String {
    // SAFETY: takes no arguments; ownership of the returned `CXString` passes to `take_string`.
    take_string(unsafe { cx::clang_getClangVersion() })
}

/// The file, line and column of a source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Location {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

fn file_location(location: cx::CXSourceLocation) -> Location {
    let mut file: cx::CXFile = ptr::null_mut();
    let (mut line, mut column, mut offset) = (0, 0, 0);
    // SAFETY: `location` came from libclang; the output pointers are valid local variables.
    unsafe {
        cx::clang_getFileLocation(location, &mut file, &mut line, &mut column, &mut offset);
    }
    let file = if file.is_null() {
        String::new()
    } else {
        // SAFETY: `file` is not null and is valid while the translation unit lives.
        take_string(unsafe { cx::clang_getFileName(file) })
    };
    Location { file, line, column }
}

/// A libclang index. Translation units are created from it.
pub(crate) struct Index {
    raw: cx::CXIndex,
}

impl Index {
    /// Creates a new index; `None` if libclang fails.
    pub(crate) fn new() -> Option<Self> {
        // SAFETY: the first argument (0) does not exclude PCH declarations, the second (0) does not
        // print diagnostics to stderr. If libclang is not loaded, `clang-sys` panics; no undefined behavior.
        let raw = unsafe { cx::clang_createIndex(0, 0) };
        (!raw.is_null()).then_some(Self { raw })
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `clang_createIndex` and is released only here.
        // `TranslationUnit`s tied to this index are dropped earlier, thanks to the lifetime parameter.
        unsafe { cx::clang_disposeIndex(self.raw) };
    }
}

/// A libclang diagnostic (warning, error...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawDiagnostic {
    pub(crate) severity: cx::CXDiagnosticSeverity,
    pub(crate) location: Location,
    pub(crate) message: String,
    /// The notes attached to this diagnostic (e.g. "to match this '('"); notes of notes are not taken.
    pub(crate) notes: Vec<RawDiagnostic>,
}

/// A parsed C++ file.
pub(crate) struct TranslationUnit<'index> {
    raw: cx::CXTranslationUnit,
    _index: PhantomData<&'index Index>,
}

impl<'index> TranslationUnit<'index> {
    /// Parses `file` with the given compiler arguments.
    ///
    /// `unsaved` holds (name, contents) pairs of files that are not on disk or whose contents are
    /// given from memory; `file` can be one of them. On failure, returns libclang's error
    /// code.
    ///
    /// Function bodies are not skipped. libclang does not count a function whose body was skipped as a "definition";
    /// the bodies are needed to recognize `static inline` functions defined in the header.
    /// The bodies in the header are compiled anyway when the shim is compiled; their errors show up here.
    /// (Measured: no difference in time for `AP_Math.h` and the 28 libraries it pulls in.)
    pub(crate) fn parse(
        index: &'index Index,
        file: &CStr,
        args: &[CString],
        unsaved: &[(CString, CString)],
    ) -> Result<Self, cx::CXErrorCode> {
        let arg_ptrs: Vec<*const c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
        let Ok(arg_count) = c_int::try_from(arg_ptrs.len()) else {
            return Err(cx::CXError_InvalidArguments);
        };
        let mut unsaved_files = Vec::with_capacity(unsaved.len());
        for (name, contents) in unsaved {
            let Ok(length) = c_ulong::try_from(contents.as_bytes().len()) else {
                return Err(cx::CXError_InvalidArguments);
            };
            unsaved_files.push(cx::CXUnsavedFile {
                Filename: name.as_ptr(),
                Contents: contents.as_ptr(),
                Length: length,
            });
        }
        let Ok(unsaved_count) = c_uint::try_from(unsaved_files.len()) else {
            return Err(cx::CXError_InvalidArguments);
        };
        let mut raw: cx::CXTranslationUnit = ptr::null_mut();
        // SAFETY: `index.raw` is a valid index. All strings pointed to by `file`, `arg_ptrs` and
        // `unsaved_files` (the `CString`s in `unsaved`) are NUL-terminated C strings that live
        // during this call; every `Length` is the content's byte count without the NUL. libclang
        // copies the contents of unsaved files during the call. `raw` is a valid output
        // pointer.
        let code = unsafe {
            cx::clang_parseTranslationUnit2(
                index.raw,
                file.as_ptr(),
                arg_ptrs.as_ptr(),
                arg_count,
                unsaved_files.as_mut_ptr(),
                unsaved_count,
                cx::CXTranslationUnit_None,
                &mut raw,
            )
        };
        if code != cx::CXError_Success {
            if !raw.is_null() {
                // SAFETY: if libclang returned an error but still gave a translation unit, we own it;
                // it is released here, once, so that it does not leak, and is never used again.
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

    /// A file that was included in this translation unit; `None` if `path` was never opened.
    ///
    /// `path` must be the same as the path the file was included with.
    pub(crate) fn file(&self, path: &CStr) -> Option<File<'_>> {
        // SAFETY: `self.raw` is valid; `path` is a NUL-terminated C string. Returns null if the
        // file does not exist.
        let raw = unsafe { cx::clang_getFile(self.raw, path.as_ptr()) };
        (!raw.is_null()).then_some(File {
            raw,
            _tu: PhantomData,
        })
    }

    /// The root cursor of the translation unit.
    pub(crate) fn cursor(&self) -> Cursor<'_> {
        // SAFETY: `self.raw` is valid; the returned cursor is tied to the lifetime of `self`.
        Cursor::new(unsafe { cx::clang_getTranslationUnitCursor(self.raw) })
    }

    /// All diagnostics produced during parsing.
    pub(crate) fn diagnostics(&self) -> Vec<RawDiagnostic> {
        // SAFETY: `self.raw` is a valid translation unit.
        let count = unsafe { cx::clang_getNumDiagnostics(self.raw) };
        (0..count)
            .map(|i| {
                // SAFETY: `i < count`; the returned diagnostic is released once below.
                let diagnostic = unsafe { cx::clang_getDiagnostic(self.raw, i) };
                let result = read_diagnostic(diagnostic, true);
                // SAFETY: `diagnostic` is released only here; the values taken from it
                // were copied in `read_diagnostic`.
                unsafe { cx::clang_disposeDiagnostic(diagnostic) };
                result
            })
            .collect()
    }
}

/// Copies the values of a diagnostic; if `with_notes`, also the notes attached to it (one level).
fn read_diagnostic(diagnostic: cx::CXDiagnostic, with_notes: bool) -> RawDiagnostic {
    // SAFETY: `diagnostic` is a valid diagnostic held by the caller.
    let (severity, location, message) = unsafe {
        (
            cx::clang_getDiagnosticSeverity(diagnostic),
            cx::clang_getDiagnosticLocation(diagnostic),
            cx::clang_getDiagnosticSpelling(diagnostic),
        )
    };
    let mut notes = Vec::new();
    if with_notes {
        // SAFETY: `diagnostic` is valid. The returned set belongs to the diagnostic and must not be released
        // (libclang documentation); it is valid while the diagnostic lives.
        let set = unsafe { cx::clang_getChildDiagnostics(diagnostic) };
        if !set.is_null() {
            // SAFETY: `set` is not null and is valid.
            let count = unsafe { cx::clang_getNumDiagnosticsInSet(set) };
            for i in 0..count {
                // SAFETY: `i < count`. Per the documentation, a diagnostic taken from the set must be released;
                // this is done once below (it has no effect in libclang for diagnostics belonging to a set).
                let note = unsafe { cx::clang_getDiagnosticInSet(set, i) };
                if note.is_null() {
                    continue;
                }
                notes.push(read_diagnostic(note, false));
                // SAFETY: `note` is released only here; its values were copied.
                unsafe { cx::clang_disposeDiagnostic(note) };
            }
        }
    }
    RawDiagnostic {
        severity,
        location: file_location(location),
        message: take_string(message),
        notes,
    }
}

impl Drop for TranslationUnit<'_> {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `clang_parseTranslationUnit2` and is released only here.
        // `Cursor`s/`Type`s tied to it are dropped earlier, thanks to the lifetime parameter.
        unsafe { cx::clang_disposeTranslationUnit(self.raw) };
    }
}

/// The value computed by [`Cursor::evaluate`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Evaluated {
    /// An integer read as signed.
    Signed(i64),
    /// An integer read as unsigned (if libclang said it is unsigned).
    Unsigned(u64),
    /// A floating-point number (`float` values converted to `double` without loss).
    Float(f64),
}

/// A source file included in the translation unit.
#[derive(Clone, Copy)]
pub(crate) struct File<'tu> {
    raw: cx::CXFile,
    _tu: PhantomData<&'tu ()>,
}

/// A node in the AST.
#[derive(Clone, Copy)]
pub(crate) struct Cursor<'tu> {
    raw: cx::CXCursor,
    _tu: PhantomData<&'tu ()>,
}

/// The callback for `clang_visitChildren`: adds every child to the `Vec` in `data`.
extern "C" fn collect_child(
    child: cx::CXCursor,
    _parent: cx::CXCursor,
    data: cx::CXClientData,
) -> cx::CXChildVisitResult {
    let pushed = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `data` was created in `Cursor::children` from a `&mut Vec<CXCursor>` and
        // is valid until `clang_visitChildren` returns. libclang calls the callback on the same
        // thread and in order; there is no other reference at the same time.
        let children = unsafe { &mut *data.cast::<Vec<cx::CXCursor>>() };
        children.push(child);
    }));
    // A panic does not cross the FFI boundary: it is caught and the walk is stopped.
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

    /// The libclang name of the cursor kind (e.g. `"VarDecl"`).
    pub(crate) fn kind_spelling(&self) -> String {
        // SAFETY: valid for any cursor kind; ownership passes to `take_string`.
        take_string(unsafe { cx::clang_getCursorKindSpelling(self.raw.kind) })
    }

    pub(crate) fn spelling(&self) -> String {
        // SAFETY: `self.raw` is valid while the translation unit lives.
        take_string(unsafe { cx::clang_getCursorSpelling(self.raw) })
    }

    pub(crate) fn location(&self) -> Location {
        // SAFETY: `self.raw` is valid.
        file_location(unsafe { cx::clang_getCursorLocation(self.raw) })
    }

    /// Is the declaration in the file `file`? For declarations produced by a macro, the place
    /// where the macro was expanded is used.
    pub(crate) fn is_in_file(&self, file: File<'_>) -> bool {
        let mut found: cx::CXFile = ptr::null_mut();
        // SAFETY: `self.raw` is valid; line, column and offset are not requested (null is accepted),
        // `found` is a valid output pointer.
        unsafe {
            cx::clang_getExpansionLocation(
                cx::clang_getCursorLocation(self.raw),
                &mut found,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
        }
        // SAFETY: both files are from the same translation unit; a null file is not equal to any
        // file.
        !found.is_null() && unsafe { cx::clang_File_isEqual(found, file.raw) != 0 }
    }

    /// Evaluates the variable's initializer (or the expression) at compile time. `None` if there is
    /// no initializer, it cannot be evaluated, or the result is not a number.
    pub(crate) fn evaluate(&self) -> Option<Evaluated> {
        // SAFETY: `self.raw` is valid; returns null on failure. The returned result is released once
        // below.
        let result = unsafe { cx::clang_Cursor_Evaluate(self.raw) };
        if result.is_null() {
            return None;
        }
        // SAFETY: `result` is not null and not released yet. The read functions are called
        // according to the kind of the result.
        let value = unsafe {
            match cx::clang_EvalResult_getKind(result) {
                cx::CXEval_Int if cx::clang_EvalResult_isUnsignedInt(result) != 0 => Some(
                    Evaluated::Unsigned(cx::clang_EvalResult_getAsUnsigned(result)),
                ),
                cx::CXEval_Int => Some(Evaluated::Signed(cx::clang_EvalResult_getAsLongLong(
                    result,
                ))),
                cx::CXEval_Float => {
                    Some(Evaluated::Float(cx::clang_EvalResult_getAsDouble(result)))
                }
                _ => None,
            }
        };
        // SAFETY: `result` is released only here; its value was copied above.
        unsafe { cx::clang_EvalResult_dispose(result) };
        value
    }

    /// The declaration a reference (e.g. `DeclRefExpr`) points to; a null cursor otherwise.
    pub(crate) fn referenced(&self) -> Cursor<'tu> {
        // SAFETY: `self.raw` is valid; returns a null cursor if it is not a reference.
        Cursor::new(unsafe { cx::clang_getCursorReferenced(self.raw) })
    }

    /// Do the two cursors point to the same node?
    pub(crate) fn same_as(&self, other: &Cursor<'_>) -> bool {
        // SAFETY: both cursors are valid (a null cursor can be compared too).
        unsafe { cx::clang_equalCursors(self.raw, other.raw) != 0 }
    }

    /// The direct child nodes, in source order.
    pub(crate) fn children(&self) -> Vec<Cursor<'tu>> {
        let mut children: Vec<cx::CXCursor> = Vec::new();
        // SAFETY: `self.raw` is valid. `collect_child` writes only to `children`, and
        // `children` lives until this call ends.
        unsafe {
            cx::clang_visitChildren(self.raw, collect_child, (&raw mut children).cast());
        }
        children.into_iter().map(Cursor::new).collect()
    }

    /// The function's parameters; empty if it is not a function.
    pub(crate) fn arguments(&self) -> Vec<Cursor<'tu>> {
        // SAFETY: `self.raw` is valid; returns -1 for a cursor that is not a function.
        let count = unsafe { cx::clang_Cursor_getNumArguments(self.raw) };
        let count = u32::try_from(count).unwrap_or(0);
        (0..count)
            // SAFETY: `i < count`, i.e. a valid parameter index.
            .map(|i| Cursor::new(unsafe { cx::clang_Cursor_getArgument(self.raw, i) }))
            .collect()
    }

    pub(crate) fn ty(&self) -> Type<'tu> {
        // SAFETY: `self.raw` is valid.
        Type::new(unsafe { cx::clang_getCursorType(self.raw) })
    }

    pub(crate) fn result_type(&self) -> Type<'tu> {
        // SAFETY: `self.raw` is valid; returns an invalid type if it is not a function.
        Type::new(unsafe { cx::clang_getCursorResultType(self.raw) })
    }

    pub(crate) fn exception_spec(&self) -> c_int {
        // SAFETY: `self.raw` is valid; returns -1 if it is not a function.
        unsafe { cx::clang_getCursorExceptionSpecificationType(self.raw) }
    }

    pub(crate) fn linkage(&self) -> cx::CXLinkageKind {
        // SAFETY: `self.raw` is valid.
        unsafe { cx::clang_getCursorLinkage(self.raw) }
    }

    /// The underlying type of a typedef declaration.
    pub(crate) fn typedef_underlying(&self) -> Type<'tu> {
        // SAFETY: `self.raw` is valid; returns an invalid type if it is not a typedef.
        Type::new(unsafe { cx::clang_getTypedefDeclUnderlyingType(self.raw) })
    }

    /// The semantic parent containing the declaration (namespace, class or translation unit).
    pub(crate) fn semantic_parent(&self) -> Cursor<'tu> {
        // SAFETY: `self.raw` is valid; returns a null cursor if there is no parent.
        Cursor::new(unsafe { cx::clang_getCursorSemanticParent(self.raw) })
    }

    /// Is this a definition (not just a forward declaration)?
    pub(crate) fn is_definition(&self) -> bool {
        // SAFETY: `self.raw` is valid.
        unsafe { cx::clang_isCursorDefinition(self.raw) != 0 }
    }

    /// The template of a template instantiation (or specialization); a null cursor otherwise.
    pub(crate) fn specialized_template(&self) -> Cursor<'tu> {
        // SAFETY: `self.raw` is valid; returns a null cursor if it does not come from a template.
        Cursor::new(unsafe { cx::clang_getSpecializedCursorTemplate(self.raw) })
    }

    /// The definition of the declaration (if it is in the translation unit); a null cursor otherwise.
    pub(crate) fn definition(&self) -> Cursor<'tu> {
        // SAFETY: `self.raw` is valid; returns a null cursor if there is no definition.
        Cursor::new(unsafe { cx::clang_getCursorDefinition(self.raw) })
    }

    /// Is it a null cursor?
    pub(crate) fn is_null(&self) -> bool {
        // SAFETY: valid for any cursor.
        unsafe { cx::clang_Cursor_isNull(self.raw) != 0 }
    }

    /// Is it a bit field (`int x : 3`)?
    pub(crate) fn is_bit_field(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a field.
        unsafe { cx::clang_Cursor_isBitField(self.raw) != 0 }
    }

    /// Is it a copy or move constructor?
    pub(crate) fn is_copy_or_move_constructor(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a constructor.
        unsafe {
            cx::clang_CXXConstructor_isCopyConstructor(self.raw) != 0
                || cx::clang_CXXConstructor_isMoveConstructor(self.raw) != 0
        }
    }

    /// Is it a copy or move assignment operator?
    pub(crate) fn is_copy_or_move_assignment(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a method.
        unsafe {
            cx::clang_CXXMethod_isCopyAssignmentOperator(self.raw) != 0
                || cx::clang_CXXMethod_isMoveAssignmentOperator(self.raw) != 0
        }
    }

    /// Is it declared with `= default`?
    pub(crate) fn is_defaulted(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a method.
        unsafe { cx::clang_CXXMethod_isDefaulted(self.raw) != 0 }
    }

    /// Is it an unnamed namespace, struct or enum?
    pub(crate) fn is_anonymous(&self) -> bool {
        // SAFETY: `self.raw` is valid.
        unsafe { cx::clang_Cursor_isAnonymous(self.raw) != 0 }
    }

    /// Is it an `inline namespace` (e.g. libc++'s `std::__1`)?
    pub(crate) fn is_inline_namespace(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a namespace.
        unsafe { cx::clang_Cursor_isInlineNamespace(self.raw) != 0 }
    }

    /// The access specifier of a class member (`CX_CXXPublic` ...).
    pub(crate) fn access(&self) -> cx::CX_CXXAccessSpecifier {
        // SAFETY: `self.raw` is valid; returns `CX_CXXInvalidAccessSpecifier` if it is not a member.
        unsafe { cx::clang_getCXXAccessSpecifier(self.raw) }
    }

    /// Is it a function deleted with `= delete`?
    pub(crate) fn is_deleted(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a function.
        unsafe { cx::clang_CXXMethod_isDeleted(self.raw) != 0 }
    }

    pub(crate) fn is_const_method(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a method.
        unsafe { cx::clang_CXXMethod_isConst(self.raw) != 0 }
    }

    pub(crate) fn is_static_method(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a method.
        unsafe { cx::clang_CXXMethod_isStatic(self.raw) != 0 }
    }

    pub(crate) fn is_virtual_method(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a method.
        unsafe { cx::clang_CXXMethod_isVirtual(self.raw) != 0 }
    }

    /// Is it a class that cannot be created directly because it has a pure virtual method?
    pub(crate) fn is_abstract_record(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a class.
        unsafe { cx::clang_CXXRecord_isAbstract(self.raw) != 0 }
    }

    /// Is it an `enum class`?
    pub(crate) fn is_scoped_enum(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not an enum.
        unsafe { cx::clang_EnumDecl_isScoped(self.raw) != 0 }
    }

    /// The underlying integer type of the enum.
    pub(crate) fn enum_integer_type(&self) -> Type<'tu> {
        // SAFETY: `self.raw` is valid; returns an invalid type if it is not an enum.
        Type::new(unsafe { cx::clang_getEnumDeclIntegerType(self.raw) })
    }

    /// The signed numeric value of the enum constant.
    pub(crate) fn enum_value(&self) -> i64 {
        // SAFETY: `self.raw` is valid; returns `LLONG_MIN` if it is not an enum constant.
        unsafe { cx::clang_getEnumConstantDeclValue(self.raw) }
    }

    /// The unsigned numeric value of the enum constant.
    pub(crate) fn enum_unsigned_value(&self) -> u64 {
        // SAFETY: `self.raw` is valid; returns `ULLONG_MAX` if it is not an enum constant.
        unsafe { cx::clang_getEnumConstantDeclUnsignedValue(self.raw) }
    }
}

/// A C++ type.
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
        // SAFETY: `self.raw` is valid while the translation unit lives.
        take_string(unsafe { cx::clang_getTypeSpelling(self.raw) })
    }

    pub(crate) fn canonical(&self) -> Type<'tu> {
        // SAFETY: `self.raw` is valid.
        Type::new(unsafe { cx::clang_getCanonicalType(self.raw) })
    }

    pub(crate) fn pointee(&self) -> Type<'tu> {
        // SAFETY: `self.raw` is valid; returns an invalid type if it is not a pointer/reference.
        Type::new(unsafe { cx::clang_getPointeeType(self.raw) })
    }

    pub(crate) fn is_const(&self) -> bool {
        // SAFETY: `self.raw` is valid.
        unsafe { cx::clang_isConstQualifiedType(self.raw) != 0 }
    }

    /// The size in bytes; `None` if libclang cannot compute it.
    pub(crate) fn size_of(&self) -> Option<u64> {
        // SAFETY: `self.raw` is valid; returns a negative value on error.
        u64::try_from(unsafe { cx::clang_Type_getSizeOf(self.raw) }).ok()
    }

    /// The alignment in bytes; `None` if libclang cannot compute it.
    pub(crate) fn align_of(&self) -> Option<u64> {
        // SAFETY: `self.raw` is valid; returns a negative value on error.
        u64::try_from(unsafe { cx::clang_Type_getAlignOf(self.raw) }).ok()
    }

    /// The offset (in bits) of the field named `field`; `None` if it does not exist or cannot be computed.
    pub(crate) fn offset_of(&self, field: &CStr) -> Option<u64> {
        // SAFETY: `self.raw` is valid; `field` is a NUL-terminated C string. Returns a negative
        // value on error.
        u64::try_from(unsafe { cx::clang_Type_getOffsetOf(self.raw, field.as_ptr()) }).ok()
    }

    /// Are the two types the same?
    pub(crate) fn same_as(&self, other: &Type<'_>) -> bool {
        // SAFETY: both types are valid.
        unsafe { cx::clang_equalTypes(self.raw, other.raw) != 0 }
    }

    /// The type wrapped by qualified names such as `std::int32_t`.
    pub(crate) fn named(&self) -> Type<'tu> {
        // SAFETY: `self.raw` is valid; returns an invalid type if it is not elaborated.
        Type::new(unsafe { cx::clang_Type_getNamedType(self.raw) })
    }

    pub(crate) fn declaration(&self) -> Cursor<'tu> {
        // SAFETY: `self.raw` is valid; returns a null cursor if there is no declaration.
        Cursor::new(unsafe { cx::clang_getTypeDeclaration(self.raw) })
    }

    pub(crate) fn is_variadic(&self) -> bool {
        // SAFETY: `self.raw` is valid; returns 0 if it is not a function type.
        unsafe { cx::clang_isFunctionTypeVariadic(self.raw) != 0 }
    }

    /// The type arguments of a template specialization (e.g. `[T, D]` for `std::unique_ptr<T, D>`).
    /// Empty if it is not a template.
    pub(crate) fn template_args(&self) -> Vec<Type<'tu>> {
        // SAFETY: `self.raw` is valid; returns -1 if it is not a template.
        let count = unsafe { cx::clang_Type_getNumTemplateArguments(self.raw) };
        let count = u32::try_from(count).unwrap_or(0);
        (0..count)
            // SAFETY: `i < count`, i.e. a valid argument index.
            .map(|i| Type::new(unsafe { cx::clang_Type_getTemplateArgumentAsType(self.raw, i) }))
            .collect()
    }
}
