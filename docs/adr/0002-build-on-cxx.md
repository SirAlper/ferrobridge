# ADR-0002: Build on `cxx` as the backend

**Date**: 2026-11-26 (the decision was made the same day, after the competitor analysis; this record was written afterwards)
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

The code ferrobridge generates has to carry calls between Rust and C++ safely. Writing this layer (FFI, ABI compatibility, bridges for `std::string`/`std::vector`/`std::unique_ptr`, catching exceptions) from scratch would take long and increase the risk of UB. According to the competitor analysis (November 2026) cxx is mature and used in Chromium and Android; Google stated that it will not deprecate cxx even as it moves to Crubit (July 2026).

## Decision

ferrobridge does not write its own FFI layer. The generated code is built on `#[cxx::bridge]` modules and cxx's std type bridges. ferrobridge's contribution sits **on top** of them: automatic generation from headers, ownership annotations and a safe, idiomatic wrapper API.

## Alternatives considered

### Alternative 1: writing our own FFI layer (`extern "C"` shims + raw pointers)
- **Pros**: Full control, no dependence on cxx's restrictions.
- **Cons**: A lot of work; std type bridges, exception handling and ABI details written from scratch. High risk of UB.
- **Why not chosen**: Our difference is not in the FFI layer but in the generation and safety layer.

### Alternative 2: using Crubit's runtime pieces
- **Pros**: Aims at richer type support.
- **Cons**: Tied to Bazel, no Cargo support yet, many features experimental.
- **Why not chosen**: Our target users use Cargo/CMake.

### Alternative 3: Zngur
- **Pros**: Strong generic and trait support, works with any build system.
- **Cons**: Mostly solves the opposite direction (C++ using Rust).
- **Why not chosen**: Our MVP direction is Rust → C++. Interoperating with Zngur is a later goal.

## Consequences

### Positive
- A reliable, widely used foundation; std type bridges and exception → `Result` come ready-made.
- Users who know cxx find the generated code familiar.

### Negative
- cxx's restrictions become ours: opaque types, `Pin<&mut T>`, the need for `UniquePtr`. The safe wrapper layer has to hide these from the user.
- Constructs cxx does not support need extra shims.

### Risks
- **cxx's development stops or changes direction.** Mitigation: since the generation layer works from the IR, another backend can be added; the quarterly competitor check also follows the state of cxx.
