# Rust Analyzer Style Guide

Source: https://rust-analyzer.github.io/book/contributing/style.html

---

## General

### Scale of Changes

Three kinds of changes:
1. Internals of a single component changed (no `pub` items changed) — merged if works, has tests, doesn't panic.
2. API of a component expanded (new `pub` function added) — heavy scrutiny, minimize changed lines.
3. New dependency between components introduced (`pub use` reexport or new `[dependencies]` entry) — should be rare.

### Crates.io Dependencies

Very conservative. No small "helper" crates. Exceptions: `itertools` and `either`. Put general reusable bits into `stdx` crate instead.

**Rationale:** keep compile times low, reduce breakage surface.

### Clippy

Use Clippy to improve code. Allow annoying lints in `[workspace.lints.clippy]` section of `Cargo.toml`.

---

## Code

### Minimal Tests

Test snippets should be minimal — remove everything that could be removed. Format compactly (e.g., `enum E { Foo, Bar }` on one line). Use unindented raw string literals for multiline fixtures.

### #[should_panic]

Do NOT use. Check for `None`, `Err`, etc. explicitly.
**Rationale:** rust-analyzer must handle any user input without panics. Panic messages in logs from `#[should_panic]` are confusing.

### #[ignore]

Do NOT `#[ignore]` tests. If test doesn't work, assert the wrong behavior and add a fixme.
**Rationale:** notice when behavior is fixed; ensure wrong behavior is at least acceptable (not a panic).

### Function Preconditions

Express preconditions in types, force the caller to provide them — don't check in callee.

```rust
// GOOD
fn frobnicate(walrus: Walrus) { ... }

// BAD
fn frobnicate(walrus: Option<Walrus>) {
    let walrus = match walrus { Some(it) => it, None => return };
    ...
}
```

Avoid splitting precondition check and use across functions.

### Control Flow

Do not hide control flow inside functions — push it to the caller:

```rust
// GOOD
if cond { f() }

// BAD
fn f() { if !cond { return; } ... }
```

### Assertions

Assert liberally. Prefer `stdx::never!` to `assert!`.

### Getters & Setters

If field has no invariant: make it public. If there's an invariant: document it, enforce in constructor, make field private, provide getter. Never provide setters.

Getters return borrowed data:
```rust
// GOOD
fn first_name(&self) -> &str { self.first_name.as_str() }
fn middle_name(&self) -> Option<&str> { self.middle_name.as_ref() }

// BAD
fn first_name(&self) -> String { self.first_name.clone() }
fn middle_name(&self) -> &Option<String> { &self.middle_name }
```

### Useless Types

Always prefer types on the left:
```
GOOD         BAD
&[T]         &Vec<T>
&str         &String
Option<&T>   &Option<T>
&Path        &PathBuf
```

### Constructors

Prefer `Default` to zero-argument `new`. Use `Vec::new()` rather than `vec![]`. Avoid "dummy" states to implement `Default`.

### Functions Over Objects

Avoid "doer" objects (created only to execute one action):
```rust
// GOOD
do_thing(arg1, arg2);

// BAD
ThingDoer::new(arg1, arg2).do();
```

OK to use an internal `Ctx` struct as an implementation detail.

### Functions with Many Parameters

Introduce a `Config` struct instead of boolean/optional params. Do NOT implement `Default` for `Config` — let caller decide. Do NOT store `Config` in state — pass explicitly.

### Prefer Separate Functions Over Parameters

If a function takes a `bool` or `Option` and is always called with literals, split into two functions.

### Premature Pessimization

**Avoid allocations:** Don't allocate a `Vec` where an iterator would do.
```rust
// GOOD
let (first, second) = match text.split_ascii_whitespace().collect_tuple() { ... }

// BAD
let words = text.split_ascii_whitespace().collect::<Vec<_>>();
```

**Push allocations to call site:** Let the caller allocate when possible.

**Collection types:** Prefer `rustc_hash::FxHashMap`/`FxHashSet` over `std::collections`.

**Avoid intermediate collections:** Use accumulator parameter in recursive functions (accumulator goes first).

**Avoid monomorphization:** Wrap generic closures in `dyn` for large function bodies. Avoid `AsRef` polymorphism.

```rust
// GOOD
fn frobnicate(mut f: impl FnMut()) { frobnicate_impl(&mut f) }
fn frobnicate_impl(f: &mut dyn FnMut()) { /* lots of code */ }
```

---

## Style

### Order of Imports

1. `std`
2. External crates (crates.io + other workspace crates)
3. Current crate (`use crate::{}`)
4. Parent and child modules (`use super::{}`)
5. Re-exports after imports (use sparingly)

Separate groups with blank lines. One `use` per crate.

### Import Style

Qualify items from `hir` and `ast`:
```rust
// GOOD
use syntax::ast;
fn frobnicate(func: hir::Function, strukt: ast::Struct) {}

// BAD
use hir::Function;
fn frobnicate(func: Function, ...) {}
```

For `std::fmt`/`std::ops` trait implementations, import the module:
```rust
// GOOD
use std::fmt;
impl fmt::Display for Foo { ... }
```

Avoid local `use MyEnum::*`. Prefer `use crate::foo::bar` over `use super::bar`.

### Order of Items

Public items first. `struct`s and `enum`s before functions and impls. Order types top-down.

### Context Parameters

Thread-through parameters go first:
```rust
// GOOD
fn go(graph: &Graph, visited: &mut FxHashSet<Vertex>, v: usize) -> usize { ... }

// BAD
fn go(v: usize, graph: &Graph, visited: &mut FxHashSet<Vertex>) -> usize { ... }
```

### Variable Naming

Use boring, long names. Default: lowercased type name (`global_state: GlobalState`). Avoid ad-hoc acronyms. Consistent short names: `db`, `ctx`, `acc`, `res`, `it`.

Mangled names for keyword conflicts: `krate`, `enum_`, `func`, `imp`, `mac`, `module`, `strukt`, `trait_`, `ty`.

### Error Handling

Use `anyhow::Result` (not bare `Result`). Use `anyhow::format_err!` (not `anyhow::anyhow`). Do not end error messages with `.`.

### Early Returns

Use early returns to reduce cognitive stack:
```rust
// GOOD
fn foo() -> Option<Bar> {
    if !condition() { return None; }
    Some(...)
}
```

Use `return Err(err)` to throw (not `Err(...)?`).

### Comparisons

Use `<`/`<=`, avoid `>`/`>=`:
```rust
// GOOD
assert!(lo <= x && x <= hi);

// BAD
assert!(x >= lo && x <= hi);
```

### If-let

Avoid `if let ... { } else { }`. Use `match` instead.

### Functional Combinators

Use `map`, `then` when natural. Avoid `bool::then` and `Option::filter`. If combinators create friction, use `for`/`if`/`match`.

### Turbofish

Prefer type ascription over turbofish. Avoid `_` in ascriptions:
```rust
// GOOD
let mutable: Vec<T> = ...collect();

// BAD
let mutable = ...collect::<Vec<_>>();
```

### Helper Variables

Introduce freely, especially for multiline conditions.

### Helper Functions

Avoid single-use helpers — use a block instead. Exception: when you need `return` or `?`.

### Local Helper Functions

Put nested helpers at the end of the enclosing function (requires `return`). Don't nest more than one level deep.

### Documentation

Style inline comments as proper sentences: start with capital, end with dot.
