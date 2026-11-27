# ADR-0001: Use libclang through `clang-sys` to read C++ headers

**Date**: 2026-11-26
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

ferrobridge has to read functions, classes, methods, enums, type sizes and exception information from C++ headers correctly. Macros, `#include`s and compile flags (`compile_commands.json`) must be taken into account. The parser must work with several LLVM versions, and installation must be easy for the user: a single `brew install llvm` (or the distribution's package manager) should be enough. As of 2026-11-26 the development machine has Homebrew LLVM 23.1.6 installed.

## Decision

We use libclang's C API through the **`clang-sys`** crate (1.9.1, updated in July 2026). On top of it we write our own thin safe wrapper inside `ferrobridge-parser`. This is the proven approach that bindgen uses as well. The parser's output is only turned into IR; the IR knows nothing about libclang.

## Alternatives considered

### Alternative 1: the `clang` crate (a higher-level wrapper by the same author, 2.1.0)
- **Pros**: A ready-made safe API, a quick start.
- **Cons**: It picks up new libclang functions late. Dropping down to the lower layer for a missing function gets harder.
- **Why not chosen**: We want to control which libclang function we use and how. Can be reconsidered later.

### Alternative 2: Clang LibTooling (a C++ helper program)
- **Pros**: Full access to the AST: templates, triviality, attributes.
- **Cons**: The C++ API is unstable across LLVM versions. The user has to build against the LLVM development libraries. The project gains a second language and build chain.
- **Why not chosen**: Too costly for the MVP. Kept open as an escape hatch (see Risks).

### Alternative 3: `clang -Xclang -ast-dump=json`
- **Pros**: The full AST without writing code.
- **Cons**: The JSON format is not stable, the output is huge, and every header needs a separate process.
- **Why not chosen**: Not suitable as the main parser. Can be used as a debugging and comparison tool.

### Alternative 4: using bindgen as a library
- **Pros**: Mature libclang code.
- **Cons**: Its internal IR is not a public, stable API. That is why autocxx had to fork bindgen (`autocxx-bindgen`) and carries the maintenance burden.
- **Why not chosen**: To avoid repeating the maintenance burden autocxx has.

### Alternative 5: tree-sitter-cpp
- **Pros**: Fast, needs no LLVM.
- **Cons**: No type resolution, macro expansion or `#include` tracking.
- **Why not chosen**: Safe code cannot be generated without semantic information.

## Consequences

### Positive
- Simple installation: `brew install llvm` and setting `LIBCLANG_PATH` are enough.
- libclang's C API is kept stable across versions.
- bindgen's libclang code (`clang.rs`) is available as a learning resource.

### Negative
- libclang does not expose the whole C++ AST. Known gaps: the children of class templates (methods, constructors) are not visible, and some semantic queries are missing. This is why the C2Rust team moved to LibTooling.
- We maintain our own wrapper.

### Risks
- **libclang may fall short for an MVP feature** (e.g. telling whether a type can be moved safely). Mitigation: tested early with a spike in Milestones 2–4. If it falls short, only a small LibTooling helper that provides that information is added, with a new ADR.
- **Differences between LLVM versions.** Mitigation: the CI matrix covers current versions (e.g. LLVM 21–23). The exact minimum version is decided in the PRD's open question.

## References
- [Status/Future of libclang (LLVM Discourse)](https://discourse.llvm.org/t/status-future-of-libclang/41709)
- [libclang API is not sufficient for tools like bindgen (LLVM bug 54236)](https://lists.llvm.org/pipermail/llvm-bugs/2022-March/098440.html)
- [libclang or libtooling for transpiler (cfe-dev)](https://lists.llvm.org/pipermail/cfe-dev/2019-April/062179.html)
- [Clang: Choosing the Right Interface](https://rocm.docs.amd.com/projects/llvm-project/en/docs-6.4.1/LLVM/clang/html/Tooling.html)
- [bindgen clang.rs](https://docs.rs/bindgen/0.72.1/src/bindgen/clang.rs.html)
