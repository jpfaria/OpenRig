# Rust API Guidelines

Source: https://rust-lang.github.io/api-guidelines/about.html

A set of recommendations from the Rust library team on how to design and present APIs for Rust.

---

## Checklist (Quick Reference)

### Naming (C-CASE, C-CONV, C-GETTER, C-ITER, C-ITER-TY, C-FEATURE, C-WORD-ORDER)
- **C-CASE**: Use `UpperCamelCase` for types, `snake_case` for values/functions/modules. Acronyms: `Uuid` not `UUID`. No `-rs`/`-rust` crate suffixes.
- **C-CONV**: `as_` = free borrowed-to-borrowed; `to_` = expensive; `into_` = consumes ownership. Never implement `Into`/`TryInto` directly (blanket impls exist from `From`/`TryFrom`).
- **C-GETTER**: No `get_` prefix for getters. Use `first()` not `get_first()`. Exception: `Cell::get`.
- **C-ITER**: Collection iterator methods: `iter()`, `iter_mut()`, `into_iter()`.
- **C-ITER-TY**: Iterator type names match methods: `IntoIter` for `into_iter()`.
- **C-FEATURE**: Feature names without placeholder words: `std` not `use-std`.
- **C-WORD-ORDER**: Consistent word order. Error types use verb-object-error: `ParseIntError` not `IntParseError`.

### Interoperability
- **C-COMMON-TRAITS**: Eagerly implement: `Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display, Default` when applicable.
- **C-CONV-TRAITS**: Conversions via `From`, `TryFrom`, `AsRef`, `AsMut`. Never implement `Into`/`TryInto` directly.
- **C-COLLECT**: Collections implement `FromIterator` and `Extend`.
- **C-SERDE**: Data structures implement `Serialize`/`Deserialize` behind `"serde"` feature.
- **C-SEND-SYNC**: Types are `Send` and `Sync` where possible. Test for this with `fn assert_send_sync<T: Send + Sync>() {}`.
- **C-GOOD-ERR**: Error types implement `std::error::Error + Send + Sync`. Never use `()` as error type. Display messages: lowercase, no trailing punctuation.
- **C-NUM-FMT**: Binary number types implement `UpperHex`, `LowerHex`, `Octal`, `Binary`.
- **C-RW-VALUE**: Generic reader/writer functions take `R: Read` and `W: Write` by value.

### Macros
- **C-EVOCATIVE**: Input syntax evokes the output.
- **C-MACRO-ATTR**: Macros compose with attributes.
- **C-ANYWHERE**: Item macros work anywhere items are allowed.
- **C-MACRO-VIS**: Item macros support visibility specifiers.
- **C-MACRO-TY**: Type fragments are flexible.

### Documentation
- **C-CRATE-DOC**: Crate-level docs are thorough and include examples.
- **C-EXAMPLE**: All public items (module, trait, struct, enum, function, method, macro, type) have a rustdoc example showing *why* to use them.
- **C-QUESTION-MARK**: Examples use `?` not `try!` or `unwrap`.
- **C-FAILURE**: Docs include "Errors" section (error conditions), "Panics" section (panic conditions), "Safety" section for unsafe functions (caller responsibilities + invariants).
- **C-LINK**: Prose contains hyperlinks to relevant things.
- **C-METADATA**: `Cargo.toml` includes: `authors`, `description`, `license`, `repository`, `keywords`, `categories`.
- **C-RELNOTES**: Release notes document all significant changes; breaking changes clearly identified. Use annotated tags.
- **C-HIDDEN**: Use `#[doc(hidden)]` to hide unhelpful impl details. Use `pub(crate)` for internal-only items.

### Predictability
- **C-SMART-PTR**: Smart pointers don't add inherent methods (use associated functions like `Box::into_raw(b)`).
- **C-CONV-SPECIFIC**: Prefer `to_`/`as_`/`into_` over `from_` (more ergonomic). Conversion lives on the more specific type.
- **C-METHOD**: Operations clearly tied to a type are methods not standalone functions. Benefits: no import needed, auto-borrowing, rustdoc discoverability.
- **C-NO-OUT**: Return multiple values via tuples/structs, not mutable out-parameters.
- **C-OVERLOAD**: Implement operator traits only when operations meaningfully resemble mathematical counterparts.
- **C-DEREF**: Only smart pointers implement `Deref`/`DerefMut`.
- **C-CTOR**: Constructors are static inherent methods. Primary: `new()`. Secondary: `_with_foo` suffix. Conversion: `from_` prefix.

### Flexibility
- **C-INTERMEDIATE**: Expose intermediate results (e.g., `Vec::binary_search` returns insertion point on miss).
- **C-CALLER-CONTROL**: Take ownership only when necessary. Prefer borrowing otherwise.
- **C-GENERIC**: Functions minimize assumptions via generics. Prefer `fn foo<I: IntoIterator<Item = i64>>(iter: I)` over `fn foo(v: &Vec<i64>)`.
- **C-OBJECT**: Design traits for either generic bounds or trait objects early. Use `where Self: Sized` to exclude generic methods from trait objects.

### Type Safety
- **C-NEWTYPE**: Use newtypes for static distinctions (e.g., `Miles(f64)` vs `Kilometers(f64)`).
- **C-CUSTOM-TYPE**: Use enums/structs instead of `bool` or `Option` params. `Widget::new(Small, Round)` > `Widget::new(true, false)`.
- **C-BITFLAG**: Use `bitflags` crate for flag sets, not enums.
- **C-BUILDER**: Use builder pattern for complex construction with many optional parameters.
  - *Non-consuming builder*: methods take `&mut self`, return `&mut Self`. Terminal method takes `&self`.
  - *Consuming builder*: all methods take and return owned `self`.

### Dependability
- **C-VALIDATE**: Validate arguments. Prefer static enforcement (types) > dynamic enforcement (runtime checks) > debug assertions.
- **C-DTOR-FAIL**: Destructors never fail. Provide separate `close() -> Result` method for fallible cleanup.
- **C-DTOR-BLOCK**: Destructors don't perform blocking operations. Provide non-blocking alternative.

### Debuggability
- **C-DEBUG**: All public types implement `Debug`.
- **C-DEBUG-NONEMPTY**: `Debug` representation is never empty.

### Future Proofing
- **C-SEALED**: Use sealed traits to prevent downstream implementations:
  ```rust
  mod private { pub trait Sealed {} }
  pub trait TheTrait: private::Sealed { ... }
  ```
- **C-STRUCT-PRIVATE**: Structs have private fields. Public fields are a strong commitment to representation and eliminate validation.
- **C-NEWTYPE-HIDE**: Newtypes encapsulate implementation details (wrap complex generic return types to hide internals).
- **C-STRUCT-BOUNDS**: Data structures don't duplicate derived trait bounds. Never add `Clone`/`PartialEq`/`Debug`/`Display`/`Default`/`Error` as bounds on structs — breaking change. Exceptions: associated types, `?Sized`, `Drop` impl requirements.

### Necessities
- **C-STABLE**: Public dependencies of a stable crate are stable.
- **C-PERMISSIVE**: Crate and its dependencies have a permissive license.

---

## Key Rules Expanded

### Conversion Conventions (C-CONV)

| Prefix | Cost | Ownership | Example |
|--------|------|-----------|---------|
| `as_` | Free | Borrowed → borrowed | `str::as_bytes()` |
| `to_` | Expensive | Works on borrowed | `str::to_string()` |
| `into_` | Variable | Consumes self | `String::into_bytes()` |
| `from_` | Variable | Creates from | `String::from("s")` |

### Error Types (C-GOOD-ERR)

```rust
// GOOD: meaningful error type
#[derive(Debug)]
pub enum MyError { InvalidInput(String), Timeout }
impl std::fmt::Display for MyError { ... }
impl std::error::Error for MyError {}

// BAD: () as error type, loses context
fn parse(s: &str) -> Result<Foo, ()> { ... }
```

Display messages: lowercase, no trailing `.` — e.g., `"connection timed out"` not `"Connection timed out."`.

### Builder Pattern (C-BUILDER)

```rust
// Non-consuming (preferred when terminal method doesn't need ownership)
pub struct RequestBuilder { url: String, timeout: Duration }
impl RequestBuilder {
    pub fn timeout(&mut self, t: Duration) -> &mut Self { self.timeout = t; self }
    pub fn send(&self) -> Result<Response, Error> { ... }
}

// Consuming (when ownership is needed)
pub struct Command { program: String, args: Vec<String> }
impl Command {
    pub fn arg(mut self, a: impl Into<String>) -> Self { self.args.push(a.into()); self }
    pub fn spawn(self) -> Result<Child, Error> { ... }
}
```

### Sealed Trait Pattern (C-SEALED)

```rust
mod private {
    pub trait Sealed {}
    impl Sealed for u8 {}
    impl Sealed for u16 {}
}

pub trait MyTrait: private::Sealed {
    fn method(&self);
}
```

Downstream crates cannot implement `private::Sealed`, so they cannot implement `MyTrait`. Enables adding methods to `MyTrait` without breaking changes.
