---
name: rust-best-practices
description: Use when writing, reviewing, or refactoring Rust code — covers idiomatic patterns, ownership, error handling, performance, testing, generics, type system, and API design best practices
---

# Rust Best Practices

Sources:
- [Apollo GraphQL Rust Best Practices](https://github.com/apollographql/rust-best-practices)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/about.html)
- [Rust Analyzer Style Guide](https://rust-analyzer.github.io/book/contributing/style.html)

## Capítulos completos (conteúdo integral)

Os arquivos abaixo contêm o conteúdo **completo e original** de cada capítulo:

| Arquivo | Conteúdo |
|---|---|
| [chapter_01.md](chapter_01.md) | Coding Styles & Idioms — borrowing, Copy, Option/Result, iterators, comments |
| [chapter_02.md](chapter_02.md) | Clippy & Linting — cargo clippy, lints importantes, workspace config |
| [chapter_03.md](chapter_03.md) | Performance — flamegraph, cloning, stack/heap, zero-cost abstractions |
| [chapter_04.md](chapter_04.md) | Error Handling — Result, unwrap, thiserror, anyhow, `?`, async errors |
| [chapter_05.md](chapter_05.md) | Automated Testing — naming, doc tests, unit vs integration, snapshot |
| [chapter_06.md](chapter_06.md) | Generics & Dispatch — static vs dynamic, trait objects, trade-offs |
| [chapter_07.md](chapter_07.md) | Type State Pattern — PhantomData, builder, state machine |
| [chapter_08.md](chapter_08.md) | Comments vs Documentation — `//`, `///`, `//!`, TODOs, rustdoc lints |
| [chapter_09.md](chapter_09.md) | Pointers — &T, Box, Rc, Arc, Mutex, RwLock, OnceLock, LazyLock |
| [api-guidelines.md](api-guidelines.md) | Rust API Guidelines — naming, interop, type safety, future proofing, checklist completo |
| [rust-analyzer-style.md](rust-analyzer-style.md) | Rust Analyzer Style Guide — imports, control flow, naming, perf, getters, constructors |

**Quando precisar de detalhes completos com todos os exemplos, leia o chapter correspondente.**

---

# OpenRig — Rust + Cargo Workflow

Regras específicas de Rust e Cargo que aplicam ao **este projeto**. Princípios de metodologia (zero coupling, single source of truth, separação business/presentation, file organization) vivem em `openrig-code-quality` e são linguagem-agnósticos — esta seção complementa com o que é de Rust/Cargo.

## MANDATORY — Run Static Validation on Every File You Touch

After creating or modifying **any** `.rs` or `.slint` file, run:

```bash
./scripts/validate.sh path/to/file1.rs path/to/file2.slint ...
```

Or let it auto-detect changed files from git diff:

```bash
./scripts/validate.sh
```

**What it checks:**
| Check | Rust | Slint |
|---|---|---|
| File size | ≤ 600 lines | ≤ 500 lines |
| Formatting | `cargo fmt --check` | — |
| Linting | `cargo clippy -D warnings` | — |
| Compilation | — | `cargo check -p adapter-gui` |

**Rules:**
- `FAIL` (red) = hard violation — fix before committing, no exceptions
- `WARN` (yellow) = known debt file — do NOT add more lines to it
- If a file shows `WARN` and you need to add logic: refactor it into smaller modules first

**Anti-pattern:**
```
❌ Modify crates/adapter-gui/src/lib.rs (9441 lines) and add 50 more lines
   // WRONG: known debt file. Create a new module, move logic there first.

✅ Extract the relevant logic to crates/adapter-gui/src/project.rs (new file)
   then add your feature there
```

**Zero tolerance for FAIL:** A task is not done until `validate.sh` exits 0.

## File Size — 600 lines per `.rs` (hard cap)

If a file exceeds 600 lines, it MUST be split before adding anything new. This is the Rust-specific operationalization of the language-agnostic *file organization* rule from `openrig-code-quality`.

## Zero Warnings (OBRIGATORIO)

- [ ] `cargo build` MUST produce zero warnings — no exceptions
- [ ] Before committing: `cargo build 2>&1 | grep "^warning"` must return empty
- [ ] Code that introduces warnings is not mergeable
- [ ] `#[allow(dead_code)]` or `#[allow(unused)]` are NOT acceptable fixes — fix the root cause

**Anti-Pattern:**
```rust
❌ cargo build  // with 3 warnings about unused variables
   // WRONG: warnings are not acceptable, fix them

❌ #[allow(dead_code)]
   pub fn unused_helper() { ... }
   // WRONG: suppress warning without fixing root cause

✅ Remove or use the dead code
✅ cargo build 2>&1 | grep "^warning"  // → empty output
```

## Cargo Clean em `.solvers/` — OBRIGATORIO antes de build

Workspaces em `.solvers/issue-N/` acumulam estado inconsistente no `target/` ao longo de merges, edições em vários crates, troca de branches, ou uso compartilhado com o Docker builder. Sintomas clássicos:

- `error[E0460]: possibly newer version of crate X which Y depends on`
- `error[E0463]: can't find crate for <crate>`
- `rustc panicked at rmeta/decoder.rs ... no encoded ident for item` (ICE)
- Build parece verde mas run-time dispara "fn X not found" em símbolo que existe

**Regra:** antes de rodar QUALQUER build que o usuário vá consumir (teste local, `build-linux-local.sh`, `build-deb-local.sh`, `build-orange-pi-image.sh`), executar `cargo clean` na raiz do workspace `.solvers/issue-N/` primeiro.

**Quando é obrigatório:**
- [ ] Após `git merge` de qualquer branch (develop, feature irmã, develop→feature)
- [ ] Após editar struct/enum fields em mais de 1 crate na mesma sessão
- [ ] Após `#[cfg(...)]` adicionado/removido em qualquer struct público
- [ ] Antes do PRIMEIRO `build-*local.sh` numa sessão nova (sempre)
- [ ] Antes de sugerir o comando pro usuário executar na máquina dele

**Anti-pattern:**
```
❌ cargo build --workspace   # após 3 merges sem cargo clean
   // WRONG: cache inconsistente. Pode passar localmente e quebrar no Docker

❌ Sugerir "./scripts/build-deb-local.sh" sem prefixo de cargo clean
   // WRONG: quase sempre falha com E0460/E0463 no Docker
```

**Correct pattern:**
```
✅ cd .solvers/issue-N && cargo clean && cargo build --workspace
✅ cd .solvers/issue-N && cargo clean && ./scripts/build-linux-local.sh --clean ...
✅ Sugerir pro usuário: "cargo clean && ./scripts/build-deb-local.sh --clean"
```

O flag `--clean` (ou `--nuke`) do build-linux-local.sh/build-deb-local.sh já cobre a limpeza DENTRO do Docker. Mas cargo clean LOCAL é ainda mais rápido e deve ser reflexo.

**Red flag:** se você rodou `cargo build` e deu verde sem ter feito `cargo clean` após merges ou cfg changes, NÃO confie no resultado. Refaça com clean.

## Platform Isolation — `cfg` Guards (técnica Rust)

Premissa geral de isolamento por plataforma (nunca quebrar áudio em outro SO) vive em `CLAUDE.md` → "Prioridades de Produto" e "Premissa de distribuicao". Esta seção cobre apenas a técnica Rust pra aplicar a premissa.

- [ ] Platform-specific code MUST be behind `#[cfg(target_os = "...")]` or feature flags
- [ ] Linux/JACK fixes must use `#[cfg(all(target_os = "linux", feature = "jack"))]`
- [ ] macOS/Windows behavior must not be affected by Linux-only changes

**Anti-Pattern:**
```rust
❌ // Changing a cross-platform audio constant to fix Linux behavior
   const BUFFER_SIZE: usize = 256;  // was 128, changed for JACK stability
   // WRONG: affects macOS and Windows — use cfg guard

✅ #[cfg(all(target_os = "linux", feature = "jack"))]
   const BUFFER_SIZE: usize = 256;
   #[cfg(not(all(target_os = "linux", feature = "jack")))]
   const BUFFER_SIZE: usize = 128;
```

## Build.rs Auto-detection (registry pattern)

OpenRig's `block-*` crates use a `build.rs` that scans `src/*.rs` for `pub const MODEL_DEFINITION` and codegens the registry. Practical consequences:

- Adding a new model = drop a `nam_<id>.rs` (or `lv2_<id>.rs`, `ir_<id>.rs`, `native_<id>.rs`) with `MODEL_DEFINITION` const. **No manual registry edit.**
- File-name prefix matters — `build.rs` may filter by stem (e.g. `starts_with("nam_")`). Renaming a model file silently breaks the build if the new name doesn't match the convention. `grep "starts_with\|stem ==" crates/block-*/build.rs` before renaming.

## Tests — Rust specifics

Princípios gerais de test coverage em `openrig-code-quality`. Aqui o operacional Rust:

- [ ] Testes dentro do módulo: `#[cfg(test)] mod tests { ... }`
- [ ] Sem framework externo — usar `assert!`, `assert_eq!`, `assert!(result.is_err())`
- [ ] **DSP nativo**: golden samples com tolerância `1e-4`, processar silêncio/sine, verificar non-NaN
- [ ] **NAM/LV2/IR builds**: marcar `#[ignore]` (dependem de assets externos; rodar com `cargo test -- --ignored`)
- [ ] **Registry tests** para `block-*` crates: iterar sobre TODOS os modelos via registry (`schema()`, `validate()`, `build()`)
- [ ] `cargo test --workspace` DEVE passar antes de qualquer commit
- [ ] Cobertura local: `scripts/coverage.sh` (requer `cargo-llvm-cov`)

## Safe Refactoring — Rust specifics

- [ ] **Never use `sed -i` on Slint files** (ver `slint-best-practices`). Em `.rs` sed funciona, mas Edit tool ainda é mais seguro.
- [ ] **After renaming files**: `cargo clean -p <crate> && cargo build` (NÃO incremental — pode deixar binários antigos)
- [ ] **After changing public struct fields**: update ALL modules that construct the struct (compilador pega, mas antes rebuild garante)

---

# Capítulos do Apollo Rust Best Practices Handbook

A seguir o conteúdo original dos chapters (não-OpenRig-specific).

---
# Chapter 1 - Coding Styles and Idioms

## 1.1 Borrowing Over Cloning

Rust’s ownership system encourages **borrow** (`&T`) instead of **cloning** (`T.clone()`). 
> ❗ Performance recommendation

### ✅ When to `Clone`:

* You need to change the object AND preserve the original object (immutable snapshots).
* When you have `Arc` or `Rc` pointers.
* When data is shared across threads, usually `Arc`.
* Avoid massive refactoring of non performance critical code.
* When caching results (dummy example below):
```rust
fn get_config(&self) -> Config {
  self.cached_config.clone()
}
```
* When the underlying API expects Owned Data.

### 🚨 `Clone` traps to avoid:

* Auto-cloning inside loops `.map(|x| x.clone)`, prefer to call `.cloned()` or `.copied()` at the end of the iterator.
* Cloning large data structures like `Vec<T>` or `HashMap<K, V>`.
* Clone because of bad API design instead of adjusting lifetimes.
* Prefer `&[T]` instead of `Vec<T>` or `&Vec<T>`.
* Prefer `&str` or `&String` instead of `String`.
* Prefer `&T` instead of `T`.
* Clone a reference argument, if you need ownership, make it explicit in the arguments for the caller. Example:
```rust
fn take_a_borrow(thing: &Thing) {
  let thing_cloned = thing.clone(); // the caller should have passed ownership instead
}
```

### ✅ Prefer borrowing:
```rust
fn process(name: &str) {
  println!(“Hello {name}”);
}

let user = String::from(“foo”);
process(&user);
```

### ❌ Avoid redundant cloning:
```rust
fn process_string(name: String) {
  println!(“Hello {name}”);
}

let user = String::from(“foo”);
process(user.clone()); // Unnecessary clone
```

## 1.2 When to pass by value? (Copy trait)

Not all types should be passed by reference (`&T`). If a type is **small** and it is **cheap to copy**, it is often better to **pass it by value**. Rust makes it explicit via the `Copy` trait.

### ✅ When to pass by value, `Copy`:
* The type **implements** `Copy` (`u32`, `bool`, `f32`, small structs).
* The cost of moving the value is negligible.

```rust
fn increment(x: u32) -> u32 {
    x + 1
}

let num = 1;
let new_num = increment(num); // `num` still usable after this point
```

### ❓ Which structs should be `Copy`?
* When to consider declaring `Copy` on your own types:
* All fields are `Copy` themselves.
* The struct is `small`, up to 2 (maybe 3) words of memory or 24 bytes (each word is 64 bits/8bytes).
* The struct **represents a “plain data object”**, without resourcing to ownership (no heap allocations. Example: `Vec` and `Strings`).

❗**Rust Arrays are stack allocated.** Which means they can be copied if their underlying type is `Copy`, but this will be allocated in the program stack which can easily become a stack overflow. More on [Chapter 3 - Stack vs Heap](https://github.com/apollographql/rust-best-practices/blob/main/book/chapter_03.md#33-stack-vs-heap-be-size-smart)

For reference, each primitive type size in bytes:

#### Integers:

| Type        	| Size     	|
|-------------	|----------	|
|    i8 u8    	|  1 byte  	|
| i16 u16     	| 2 bytes  	|
| i32 u32     	| 4 bytes  	|
| i64 u64     	| 8 bytes  	|
| isize usize 	| Arch     	|
| i128 u128   	| 16 bytes 	|

#### Floating Point:

| Type     	| Size     	|
|----------	|----------	|
| f32     	| 4 bytes  	|
| f64     	| 8 bytes  	|


#### Other:

| Type     	| Size     	|
|----------	|----------	|
| bool     	| 1 byte  	|
| char     	| 4 bytes  	|


### ✅ Good struct to derive `Copy`:
```rust
#[derive(Debug, Copy, Clone)]
struct Point {
  x: f32,
  y: f32,
  z: f32
}
```

### ❌ Bad struct to derive `Copy`:
```rust
#[derive(Debug, Clone)]
struct BadIdea {
  age: i32,
  name: String, // String is not `Copy`
}
```

### ❓Which Enums should be `Copy`?
* If your enum acts like tags and atoms.
* The enum payloads are all `Copy`.
* **❗Enums size are based on their largest element.**

### ✅ Good Enum to derive
```rust
#[derive(Debug, Copy, Clone)]
enum Direction {
  North,
  South,
  East,
  West,
}
```

## 1.3 Handling `Option<T>` and `Result<T, E>`
Rust 1.65 introduced a better way to safely unpack Option and Result types with the `let Some(x) = … else { … }` or `let Ok(x) = … else { … }` when you have a default `return` value, `continue` or `break` default else case. It allows early returns when the missing case is **expected and normal**, not exceptional.

### ✅ Cases to use each pattern matching for Option and Return
* Use `match` when you want to pattern match against the inner types `T` and `E`
```rust
match self {
  Ok(Direction::South) => { … },
  Ok(Direction::North) => { … },
  Ok(Direction::East) => { … },
  Ok(Direction::West) => { … },
  Err(E::One) => { … },
  Err(E::Two) => { … },
}

match self {
  Some(3|5) => { … }
  Some(x) if x > 10  => { … }
  Some(x) => { … }
  None => { … }
}
```

* Use `match` when your type is transformed into something more complex Like `Result<T, E>` becoming `Result<Option<U>, E>`.
```rust
match self {
  Ok(t) => Ok(Some(t)),
  Err(E::Empty) => Ok(None),
  Err(err) => Err(err),
}
```

* Use `let PATTERN = EXPRESSION else {  DIVERGING_CODE; }` when the divergent code doesn’t need to know about the failed pattern matches or doesn’t need extra computation:
```rust
let Some(&Direction::North) = self.direction.as_ref() else {
	return Err(DirectionNotAvailable(self.direction));
}
```

* Use `let PATTERN = EXPRESSION else {  DIVERGING_CODE; }` when you want to break or continue a pattern match
```rust
for x in self {
    let Some(x) = x else {
	continue;
    }
}
```

* Use `if let PATTERN = EXPRESSION else {  DIVERGING_CODE; }` when `DIVERGING_CODE` needs extra computation:
```rust
if let Some(x) = self.next() {
  // computation
} else {
  // computation when `None/Err` or not matched
}
```

❗**If you don’t care about the value of the `Err` case, please use `?` to propagate the `Err` to the caller.**

### ❌ Bad Option/Return pattern matching:

* Conversion between Result and Option (prefer `.ok()`,`.ok_or()`, and `ok_or_else()`)
```rust
match self {
  Ok(t) => Some(t),
  Err(_) => None
}
```

* `if let PATTERN = EXPRESSION else {  DIVERGING_CODE; }` when divergent code is a default or pre-computed value (prefer `let PATTERN = EXPRESSION else {  DIVERGING_CODE; }`):
```rust
if let Some(values) = self.next() {
  // computation
  (Some(..), values)
} else {
  (None, Vec::new())
}
```

* Using `unwrap` or `expect` outside tests:
```rust
let port = config.port.unwrap();
```

## 1.4 Prevent Early Allocation

When dealing with functions like `or`, `map_or`, `unwrap_or`, `ok_or`, consider that they have special cases for when memory allocation is required, like creating a new string, creating a collection or even calling functions that manage some state, so they can be replaced with their `_else` counter-part:

### ✅ Good cases

```rust
let x = None;
assert_eq!(x.ok_or(ParseError::ValueAbsent), Err(ParseError::ValueAbsent));

let x = None;
assert_eq!(x.ok_or_else(|| ParseError::ValueAbsent(format!("this is a value {x}"))), Err(ParseError::ValueAbsent));


let x: Result<_, &str> = Ok("foo");
assert_eq!(x.map_or(42, |v| v.len()), 3);


let x : Result<_, String> = Ok("foo");
assert_eq!(x.map_or_else(|e|format!("Error: {e}"), |v| v.len()), 3);

let x = "1,2,3,4";
assert_eq!(x.parse_to_option_vec.unwrap_or_else(Vec::new), Ok(vec![1, 2, 3, 4]));
```

### ❌ Bad cases

```rust
let x : Result<_, String> = Ok("foo");
assert_eq!(x.map_or(format!("Error with uninformed content"), |v| v.len()), 3);

let x = "1,2,3,4";
assert_eq!(x.parse_to_option_vec.unwrap_or(Vec::new()), Ok(vec![1, 2, 3, 4])); // could be replaced with `.unwrap_or_default`

let x = None;
assert_eq!(x.ok_or(ParseError::ValueAbsent(format!("this is a value {x}"))), Err(ParseError::ValueAbsent));
```

### Mapping Err

When dealing with Result::Err, sometimes is necessary to log and transform the Err into a more abstract or more detailed error, this can be done with `inspect_err` and `map_err`:

```rust
let x = Err(ParseError::InvalidContent(...));

x
.inspect_err(|err| tracing::error!("function_name: {err}"))
.map_err(|err| GeneralError::from(("function_name", err)))?;
```

## 1.5 Iterator, `.iter` vs `for`

First we need to understand a basic loop with each one of them. Let's consider the following problem, we need to sum all even numbers between 0 and 10 incremented by 1:

* `for`:
```rust
let mut sum = 0;
for x in 0..=10 {
    if x % 2 == 0 {
        sum += x + 1;
    }
}
```

* `iter`:
```rust
let sum: i32 = (0..=10)
    .filter(|x| x % 2 == 0)
    .map(|x| x + 1)
    .sum();
```

> Both versions do the same thing and are correct and idiomatic, but each shines in different contexts.

### When to prefer `for` loops
* When you need **early exits** (`break`, `continue`, `return`).
* **Simple iteration** with side-effects (e.g., logging, IO)
    * logging can be done correctly in `Iterators` using `inspect` and `inspect_err` functions.
* When readability matters more than simplicity or chaining.

#### Example:
```rust
for value in &mut value {
    if *value == 0 {
        break;
    }
    *value += fancy_equation();
}
```

### When to prefer `iterators` loops (`.iter()` and `.into_iter()`)
* When you are `transforming collections` or `Option/Results`.
* You can **compose multiple steps** elegantly.
* No need for early exits.
* You need support for indexed values with `.enumerate`.
```rust
let values: Vec<_> = vec.into_iter()
    .enumerate()
    .filter(|(_index, value)| value % 2 == 0)
    .map(|(index, value)| value % index)
    .collect()
```
* You need to use collections functions like `.windows` or `chunks`.
* You need to combine data from multiple sources and don't want to allocate multiple collections.
* Iterators can be combined with `for` loops:
```rust
for value in vec.iter().enumerate()
    .filter(|(index, value)| value % index == 0) {
    // ...
}
    
```

> #### ❗REMEMBER: Iterators are Lazy
>
> * `.iter`, `.map`, `.filter` don't do anything until you call its consumer, e.g. `.collect`, `.sum`, `.for_each`.
> * **Lazy Evaluation** means that iterator chains are fused into one loop at compile time.

### 🚨 Anti-patterns to AVOID

* Don't chain without formatting. Prefer each chained function on its own line with the correct indentation (`rustfmt` should take care of this).
* Don't chain if it makes the code unreadable.
* Avoid needlessly collect/allocate of a collection (e.g. vector) just to throw it away later by some larger operation or by another iteration.
* Prefer `iter` over `into_iter` unless you don't need the ownership of the collection.
* Prefer `iter` over `into_iter` for collections that inner type implements `Copy`, e.g. `Vec<T: Copy>`.
* For summing numbers prefer `.sum` over `.fold`. `.sum` is specialized for summing values, so the compiler knows it can make optimizations on that front, while fold has a blackbox closure that needs to be applied at every step. If you need to sum by an initial value, just added in the expression `let my_sum = [1, 2, 3].sum() + 3`.

## 1.6 Comments: Context, not Clutter

> "Context are for why, not what or how"

Well-written Rust code, with expressive types and good naming, often speaks for itself. Many high-quality codebases thrive on **few or no comments**. And that's a good thing.

Still, there are **moments where code alone isn't enough** - when there are performance quirks, external constraints, or non-obvious tradeoffs that require a nudge to the reader. In those cases, a concise comment can prevent hours of head-scratching or searching git history.

### ✅ Good comments 

* Safety concerns:
```rust
// SAFETY: We have checked that the pointer is valid and non-null. @Function xyz.
unsafe { std::ptr::copy_nonoverlapping(src, dst, len); }
```

* Performance quirks:
```rust
// This algorithm is a fast square root approximation
const THREE_HALVES: f32 = 1.5;
fn q_rsqrt(number: f32 ) -> f32 {
	let mut i: i32 = number.to_bits() as i32;
i = 0x5F375A86_i32.wrapping_sub(i >> 1);
let y = f32::from_bits(i as u32);
y * (THREE_HALVES - (number * 0.5 * y * y))
}
```

* Clear code beats comments. However, when the why isn't obvious, say it plainly - or link to where:
```rust
// PERF: Generating the root store per subgraph caused high TLS startup latency on MacOS
// This works as a caching alternative. See: [ADR-123](link/to/adr-123)
let subgraph_tls_root_store: RootCertStore = configuration
    .tls
    .subgraph
    .all
    .create_certificate_store()
    .transpose()?
    .unwrap_or_else(crate::services::http::HttpClientService::native_roots_store);
let connector_tls_root_store: RootCertStore = configuration
    .tls
    .connector
    .all
    .create_certificate_store()
    .transpose()?
    .unwrap_or_else(crate::services::http::HttpClientService::native_roots_store);
```

* ❗ More use cases to come in their appropriate sections.

### ❌ Bad comments

* Wall-of-text explanations: long comments and multiline comments
```rust
// Lorem Ipsum is simply dummy text of the printing and typesetting industry. 
// Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, 
// when an unknown printer took a galley
fn do_something_odd() {
  …
}
```
> Prefer `/// doc` comment if it's describing the function.

* Comments that could be better represented as functions or are plain obvious
```rust
fn computation() {
  // increment i by 1
  i += 1;
}
```

### ✅ Breaking up long functions over commenting them

If you find yourself writing a long comment explaining "what", "how" or "each step" in a function, it might be time to split it. So the suggestion is to refactor. This can be beneficial not only for readability, but testability:

#### ❌ Instead of:
```rust
fn process_request(request: T) {
    // We first need to validate request, because of corner case x, y, z
    // As the payload can only be decoded when they are valid
    // Then we can perform authorization on the payload
    // lastly with the authorized payload we can dispatch to handler
}
```

#### ✅ Prefer
```rust
fn process_request(request: T) -> Result<(), Error> {
    validate_request_headers(&request)?;
    let payload = decode_payload(&request);
    authorize(&payload)?;
    dispatch_to_handler(payload)
}

#[cfg(test)]
mod tests {
    #[test]
    fn validate_request_happy_path() { ... }

    #[test]
    fn validate_request_fails_on_x() { ... }

    #[test]
    fn validate_request_fails_on_y() { ... }

    #[test]
    fn decode_validated_request() { ... }

    #[test]
    fn authorize_payload_xyz() { ... }
}
```

Let **structure** and **naming** replace commentary, and enhance its documentation with **tests as living documentation**.

### 📝 TODOs are not comments - track them properly

Avoid leaving lingering `// TODO: Lorem Ipsum` comments in the code. Instead:
* Turn them into Jira or Github Issues.
* If needed, to avoid future confusion, reference the issue in the code and the code in the issue.

```rust
// See issue #123: support hyper 2.0
```

This helps keeping the code clean and making sure tasks are not forgotten.

### Comments as Living Documentation

There are a few gotchas when calling comments "living documentation":
* Code evolves.
* Context changes.
* Comments get stale.
* Many large comments make people avoid reading them.
* Team becomes fearful of delete irrelevant comments.

If you find a comment, **don't trust it blindly**. Read it in context. If it's wrong or outdated, fix or remove it. A misleading comment is worse than no comments at all. 

> Comments should bother you - they demand re-verification, just like stale tests.

When deeper justification is needed, prefer to:
* **Link to a Design Doc or an ADR**, business logic lives well in design docs while performance tradeoffs live well in ADRs.
* Move runtime example and usage docs into Rust Docs, `/// doc comment`, where they can be tested and kept up-to-date by tools like `cargo doc`.

> Doc-comments and Doc-testing, `///` and `//!` in [Chapter 8 - Comments vs Documentation](./chapter_08.md)

## 1.7 Use Declarations - "imports"

Different languages have different ways of sorting their imports, in the Rust ecosystem the [standard way](https://github.com/rust-lang/rustfmt/issues/4107) is:

- `std` (`core`, `alloc` would also fit here).
- External crates (what is in your Cargo.toml `[dependencies]`).
- Workspace crates (workspace member crates).
- This module `super::`.
- This module `crate::`.

```rust
// std
use std::sync::Arc;

// external crates
use chrono::Utc;
use juniper::{FieldError, FieldResult};
use uuid::Uuid;

// crate code lives in workspace
use broker::database::PooledConnection;

// super:: / crate::
use super::schema::{Context, Payload};
use super::update::convert_publish_payload;
use crate::models::Event;
```

Some enterprise solutions opt to include their core packages after `std`, so all external packages that start with enterprise name are located before the others:

```rust
// std
use std::sync::Arc;

// enterprise external crates
use enterprise_crate_name::some_module::SomeThing;

// external crates
use chrono::Utc;
use juniper::{FieldError, FieldResult};
use uuid::Uuid;

// crate code lives in workspace
use broker::database::PooledConnection;

// super:: / crate::
use super::schema::{Context, Payload};
use super::update::convert_publish_payload;
use crate::models::Event;
```

One way of not having to manually control this is using the following arguments in your `rustfmt.toml`:

```toml
reorder_imports = true
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
```

> As of Rust version 1.88, it is necessary to execute rustfmt in nightly to correctly reorder code `cargo +nightly fmt`.

---
# Chapter 2 - Clippy and Linting Discipline

Be sure to have `cargo clippy` installed with your rust compiler, run `cargo clippy -V` in your terminal for a rust project and you should get something like this `clippy 0.1.86 (05f9846f89 2025-03-31)`. If terminal fails to show a clippy version, please run the following code `rustup update && rustup component add clippy`.

Clippy documentation can be found [here](https://doc.rust-lang.org/clippy/usage.html).

## 2.1 Why care about linting?

Rust compiler is a powerful tool that catches many mistakes. However, some more in-depth analysis require extra tools, that is where `cargo clippy` clippy comes into to play. Clippy checks for:
* Performance pitfalls.
* Style issues.
* Redundant code.
* Potential bugs.
* Non-idiomatic Rust.

## 2.2 Always run `cargo clippy`

Add the following to your daily workflow:

```shell
$ cargo clippy --all-targets --all-feature --locked -- -D warnings
```

* `--all-targets`: checks library, tests, benches and examples.
* `--all-feature`: checks code for all features enabled, auto solves conflicting features.
* `--locked`: Requires `Cargo.lock` to be up-to-date, can be solved with `$ cargo update`.
* `-D warnings`: treats warnings as errors

Potential additions elements to add:

* `-- -W clippy::pedantic`: lints which are rather strict or have occasional false positives.
* `-- -W clippy::nursery`: Optionally can be added to check for new lints that are still under development.
* ❗ Add this to your Makefile, Justfile, xtask or CI Pipeline.

> Example at ApolloGraphQL
>
> In the `Router` project there is a `xtask` configured for linting that can be executed with `cargo xtask lint`. 

## 2.3 Important Clippy Lints to Respect

| Lint Name | Why | Link |
| --------- | ----| -----|
| `redundant_clone` | Detects unnecessary `clones`, has performance impact | [link (nursery + perf)](https://rust-lang.github.io/rust-clippy/master/#redundant_clone) |
| `needless_borrow` group | Removes redundant `&` borrowing | [link (style)](https://rust-lang.github.io/rust-clippy/master/#needless_borrow) |
| `map_unwrap_or` / `map_or` | Simplifies nested `Option/Result` handling | [`map_unwrap_or`](https://rust-lang.github.io/rust-clippy/master/#map_unwrap_or) [`unnecessary_map_or`](https://rust-lang.github.io/rust-clippy/master/#unnecessary_map_or) [`unnecessary_result_map_or_else`](https://rust-lang.github.io/rust-clippy/master/#unnecessary_result_map_or_else) |
| `manual_ok_or` | Suggest using `.ok_or_else` instead of `match` | [link (style)](https://rust-lang.github.io/rust-clippy/master/#manual_ok_or) |
| `large_enum_variant` | Warns if an enum has very large variant which is bad for memory. Suggests `Boxing` it | [link (perf)](https://rust-lang.github.io/rust-clippy/master/#large_enum_variant) |
| `unnecessary_wraps` | If your function always returns `Some` or `Ok`, you don't need `Option`/`Result` | [link (pedantic)](https://rust-lang.github.io/rust-clippy/master/#unnecessary_wraps) |
| `clone_on_copy` | Catches accidental `.clone()` on `Copy` types like `u32` and `bool` | [link (complexity)](https://rust-lang.github.io/rust-clippy/master/#clone_on_copy) |
| `needless_collect` | Prevents collecting and allocating an iterator, when allocation is not needed | [link (nursery)](https://rust-lang.github.io/rust-clippy/master/#needless_collect) |

## 2.4 Fix warnings, don't silence them!

**NEVER** just `#[allow(clippy::lint_something)]` unless:

* You **truly understand** why the warning happens and you have a reason why it is better that way.
* You **document** why it is being ignored.
* ❗ Don't use `allow`, but `expect`, it will give a warning in case the lint is not true anymore, `#[expect(clippy::lint_something)]`.

### Example:

```rust
// Faster matching is preferred over size efficiency
#[expect(clippy::large_enum_variant)]
enum Message {
    Code(u8),
    Content([u8; 1024]),
}
```

> The fix would be:
> 
> ```rust
> // Faster matching is preferred over size efficiency
> #[expect(clippy::large_enum_variant)]
> enum Message {
>     Code(u8),
>     Content(Box<[u8; 1024]>),
> }
> ```

### Handling false positives

Sometimes Clippy complains even when your code is correct, in those cases there are two solutions:
1. Try to refactor the code, so it improves the warning.
2. **Locally** override the lint with `#[expect(clippy::lint_name)]` and a comment with the reason.
3. Avoid global overrides, unless it is core crate issue, a good example of this is the Bevy Engine that has a set of lints that should be allowed by default.

## 2.5 Configure workspace/package lints

In your `Cargo.toml` file it is possible to determine which lints and their priorities over each other. In case of 2 or more conflicting lints, the higher priority one will be chosen. Example configuration for a package:

```toml
[lints.rust]
future-incompatible = "warn"
nonstandard_style = "deny"

[lints.clippy]
all = { level = "deny", priority = 10 }
redundant_clone = { level = "deny", priority = 9 }
manual_while_let_some = { level = "deny", priority = 4 }
pedantic = { level = "warn", priority = 3 }
```

And for a workspace:

```toml
[workspace.lints.rust]
future-incompatible = "warn"
nonstandard_style = "deny"

[workspace.lints.clippy]
all = { level = "deny", priority = 10 }
redundant_clone = { level = "deny", priority = 9 }
manual_while_let_some = { level = "deny", priority = 4 }
pedantic = { level = "warn", priority = 3 }
```
---
# Chapter 3 - Performance Mindset

The **golden rule** of performance work:

> Don't guess, measure.

Rust code is often already pretty fast - don't "optimize" without evidence. Optimize only after finding bottlenecks.

### A good first steps
* Use `--release` flag on you builds (might sound dummy, but it is quite common to hear people complaining that their Rust code is slower than their X language code, and 99% of the time is because they didn't use the `--release` flag).
* `$ cargo clippy -- -D clippy::perf` gives you important tips on best practices for performance.
* [`cargo bench`](https://doc.rust-lang.org/cargo/commands/cargo-bench.html) is a cargo tool to create micro-benchmarks and test different code solutions. Write a test scenario and bench you solution against the original code, if your improvement is larger than 5%, might be a good performance improvement.
* [`cargo flamegraph`](https://github.com/flamegraph-rs/flamegraph) a powerful profiler for Rust code. For MacOS, [samply](https://github.com/mstange/samply) might be a better DX option.

> #### Further reading on Benchmarking:
> - [How to build a Custom Benchmarking Harness in Rust](https://bencher.dev/learn/benchmarking/rust/custom-harness/)


## 3.1 Flamegraph

Flamegraph helps you visualize how much time CPU spent on each task.

```shell
# Installing flamegraph
cargo install flamegraph

# cargo support provided through the cargo-flamegraph binary!
# defaults to profiling cargo run --release
cargo flamegraph

# by default, `--release` profile is used,
# but you can override this:
cargo flamegraph --dev

# if you'd like to profile a specific binary:
cargo flamegraph --bin=stress2

# Profile unit tests.
# Note that a separating `--` is necessary if `--unit-test` is the last flag.
cargo flamegraph --unit-test -- test::in::package::with::single::crate
cargo flamegraph --unit-test crate_name -- test::in::package::with::multiple:crate

# Profile integration tests.
cargo flamegraph --test test_name

# Run criterion benchmark
# Note that the last --bench is required for `criterion 0.3` to run in benchmark mode, instead of test mode.
cargo flamegraph --bench some_benchmark --features some_features -- --bench

# Run workspace example
cargo flamegraph --example some_example --features some_features
```

> ❗ Always run your profiles with `--release` enabled, the `--dev` flag isn't realistic as it doesn't have optimizations enabled.

The result will look like the following:
<img src="/wiki/download/attachments/1532592375/flamegraph.png" width="1000px" height="377px" onerror="this.style.display='none'"/>

![Flamegraph profile](../images/flamegraph.png)

* The `y-axis` shows the **stack depth number**. When looking at a flamegraph, the main function of your program will be closer to the bottom, and the called functions will be stacked on top, with the functions that they call stacked on top of them.

The `width of each box` shows the **total time that that function** is on the CPU or is part of the call stack. If a function's box is wider than others, that means that it consumes more CPU per execution than other functions, or that it is called more than other functions.

> ❗ The **color of each box** isn't significant, and **is chosen at random**.

### 🚨 Remember
* Thick stacks: heavy CPU usage
* Thin stacks: low intensity (cheap)

## 3.2 Avoid Redundant Cloning

> Cloning is cheap... **until it isn't**

In sections [Borrowing over Cloning](./chapter_01.md#11-borrowing-over-cloning) and [Important Clippy lints to respect](./chapter_02.md#23-important-clippy-lints-to-respect) we mentioned the impacts of cloning and the relevant clippy lint [`redundant_clone`](https://rust-lang.github.io/rust-clippy/master/#redundant_clone), so in this section we will explore a bit "when to pass ownership".

* 🚨 If you really need to clone, leave it to the last moment.

### When to pass ownership?

* Only `.clone()` if you truly need a new owned copy. A few examples:
    * Crate API Design requires owned data.
    * Have overloaded `std::ops` but still need ownership to the old data:
    ```rust
    use std::ops::Add;

    #[derive(Debug, Copy, Clone, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    impl Add for Point {
        type Output = Self;

        fn add(self, other: Self) -> Self {
            Self {
                x: self.x + other.x,
                y: self.y + other.y,
            }
        }
    }

    assert_eq!(Point { x: 1, y: 0 } + Point { x: 2, y: 3 },
            Point { x: 3, y: 3 });
    ```
    * Need to do comparison snapshots or due to API you need multiple owned instances of the data.
    ```rust
    fn snapshot(a: &MyValue, b:&MyValue) -> MyValueDiff {
        a - b
    }

    impl Sub for MyValue {
        type Output = MyValueDiff;

        fn sub(self, other: Self) -> MyValue {
            ...
        }
    }

    fn main() {
        let mut a = MyValue::default();
        let b = a.clone();

        a.magical_update();
        println!("{:?}", snapshot(&a, &b));
    }
    ```
* You have reference counted pointers (`Arc, Rc`).
* You have small structs that are to big to `Copy` but as costly as `std::collections`. An example is HTTP client like `hyper_util::client::legacy::Client` that cloning allows you to share the connection pool.
* You have a chained struct modifier that needs owned mutation, some **builders** require owned mutation, but most custom builders can be done with `pub fn with_xyz(&mut self, value: Xyz) -> &mut Self`.
```rust
// Inline `HashMap` insertion extension

fn insert_owned(mut self, key: K, value: V) -> Self {
    self.insert(key, value);
    self
}

```
* Ownership can also be a good way to model business logic / state. For example:
```rust
let not_validated: String = ...;// some user source
let validated = Validate::try_from(not_validated)?;
// Technically that `try_from` maybe didn't need ownership, but taking it lets us model intent
```

### When **NOT** to pass ownership?

* Prefer API designs that take reference (`fn process(values: &[T])`), instead of ownership (`fn process(values: Vec<T>)`).
* If you only need read access to elements, prefer `.iter` or slices:
```rust
for item in &some_vec {
    ...
}
```
* You need to mutate data that is owned by another thread, use `&mut MyStruct`.

### Use `Cow` for `Maybe Owned` data

Sometimes you don't actually need owned data, but that is not clear from the API perspective, so using [`std::borrow::Cow`](https://doc.rust-lang.org/std/borrow/enum.Cow.html) is a way to efficiently address this case:

```rust
use std::borrow::Cow;

fn hello_greet(name: Cow<'_, str>) {
    println!("Hello {name}");
}

hello_greet(Cow::Borrowed("Julia"));
hello_greet(Cow::Owned("Naomi".to_string()));
```

## 3.3 Stack vs Heap: Be size-smart!

### ✅ Good Practices 

* Keep small types (`impl Copy`, `usize`, `bool`, etc) **on the stack**.
* Avoid passing huge types (`> 512 bytes`)  by value or transferring ownership. Prefer pass by reference (e.g. `&T` and `&mut T`).
* Heap allocate recursive data structures:
```rust
enum OctreeNode<T> {
    Node(T),
    Children(Box<[Node<T>; 8]>),
}
```
* Return small types by value, types that implement `Copy` or a cheaply Cloned are efficient to return by value (e.g. `struct Vector2 {x: f32, y: f32}`).

### ❗ Be Mindful

* Only use `#[inline]` when benchmark proves beneficial, Rust is already pretty good at inlining **without** hints.
* Avoid massive stack allocations, box them. Example `let buffer: Box<[u8; 65536]> = Box::new(..)` would first allocate `[u8; 65536]` on the stack then box it, a non-const solution to this would be `let buffer: Box<[u8]> = vec![0; 65536].into_boxed_slice()`.
* For large `const` arrays, considering using [crate smallvec](https://docs.rs/smallvec/latest/smallvec/) as it behaves like an array, but is smart enough to allocate large arrays on the heap.

## 3.4 Iterators and Zero-Cost Abstractions

Rust iterators are lazy, but eventually compiled away into very efficient tight loops that are only called when consumed. Chaining `.filter()`, `.map()`, `.rev()`, `.skip()`, `.take()`, `.collect()` usually doesn't cost extra and the compiler can reason well enough how to optimize them.
* Prefer `iterators` over manual `for` loops when working with collections, the compiler can optimize them better than manually doing it.
* Calling `.iter()` only creates a **reference** to the original collection, this allows you to hold multiple iterators of the same collection.

#### ❗ Avoid creating intermediate collections unless it is really needed:

* Consider that `process` accepts an `iterator`.
* ❌ BAD - useless intermediate collection:
```rust
let doubled: Vec<_> = items.iter().map(|x| x * 2).collect();
process(doubled);
```
* ✅ GOOD - pass the iterator (`fn process(arg: impl Iterator<Item = u32>)`):
```rust
let doubled_iter = items.iter().map(|x| x * 2);
process(doubled_iter);
```
---
# Chapter 4 - Errors Handling

Rust enforces a strict error handling approach, but *how* you handle them defines where your code feels ergonomic, consistent and safe - as opposing cryptic and painful. This chapter dives into best practices for modeling and managing fallible operations across libraries and binaries.

> Even if you decide to crash you application with `unwrap` or `expect`, Rust forces you to declare that intentionally.

## 4.1 Prefer `Result`, avoid panic 🫨

Rust has a powerful type that wraps fallible data, [`Result`](https://doc.rust-lang.org/std/result/), this allows us to handle Error cases according to our needs and manage the state of the application based on that.

* If your function can fail, prefer to return a `Result`:
```rust
fn divide(x: f64, y: f64) -> Result<f64, DivisionError> {
    if y == 0.0 {
        Err(DivisionError::DividedByZero)
    } else {
        Ok(x / y)
    }
}
```

* Use `panic!` only in unrecoverable conditions - typically tests, assertions, bugs or a need to crash the application for some explicit reason.
* There are 3 relevant macros that can replace `panic!` in appropriate conditions:
    * `todo!`, similar to panic, but alerts the compiler that you are aware that there is code missing.
    * `unreachable!`, you have reasoned about the code block and are sure that condition `xyz` is not possible and if ever becomes possible you want to be alerted.
    * `unimplemented!`, specially useful for alerting that a block is not yet implement with a reason.

## 4.2 Avoid `unwrap`/`expect` in Production

Although `expect` is preferred to `unwrap`, as it can have context, they should be avoided in production code as there are smarter alternatives to them. Considering that, they should be used in the following scenarios:
- In tests, assertions or test helper functions.
- When failure is impossible.
- When the smarter options can't handle the specific case.

### 🚨 Alternative ways of handling `unwrap`/`expect`:

* If your `Result` (or `Option`) can have a predefined early return value in case of `Result::Err`, that doesn't need to know the `Err` value, use `let Ok(..) = else { return ... }` pattern, as it helps with flatten functions:
```rust
let Ok(json) = serde_json::from_str(&input) else {
    return Err(MyError::InvalidJson);
}
```
* If your `Result` (or `Option`) needs error recovery in case of `Result::Err`, that doesn't need to know the `Err` value, use `if let Ok(..) else { ... }` pattern:
```rust
if let Ok(json) = serde_json::from_str(&input) else {
    ...
} else {
    Err(do_something_with_input(&input))
}
```
* Functions that can have to handle `Option::None` values are recommended to return `Result<T, E>`, where `E` is a crate or module level error, like the examples above.
* Lastly `unwrap_or`, `unwrap_or_else` or `unwrap_or_default`, these functions help you create alternative exits to unwrap that manage the uninitialized values.

## 4.3 `thiserror` for Crate level errors

Deriving Error manually is verbose and error prone, the rust ecosystem has a really good crate to help with this, `thiserror`. It allows you to create error types that easily implement `From` trait as well as easy error message (`Display`), improving developer experience while working seamlessly with `?` and integrating with `std::error::Error`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("Network Timeout")]
    Timeout,
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid request information. Header: {headers}, Metadata: {metadata}")]
    InvalidRequest {
        headers: Headers,
        metadata: Metadata
    }
}
```

### Error Hierarchies and Wrapping

For layered systems the best practice is to use nested `enum/struct` errors with `#[from]`:

```rust
use crate::database::DbError;
use crate::external_services::ExternalHttpError;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Database handler error: {0}")]
    Db(#[from] DbError),
    #[error("External services error: {0}")]
    ExternalServices(#[from] ExternalHttpError)
    
}
```

## 4.4 Reserve `anyhow` for Binaries

`anyhow` is an amazing crate, and quite useful for projects that are beginning and need accelerated speed. However, there is a turning point where it just painfully propagates through your code, considering this, `anyhow` is recommended only for **binaries**, where ergonomic error handling is needed and there is no need for precise error types:

```rust
use anyhow::{Context, Result, anyhow};

fn main() -> Result<Config> {
    let content = std::fs::read_to_string("config.json")
        .context("Failed to read config file")?;
    Config::from_str(&content)
        .map_err(|err| anyhow!("Config parsing error: {err}"))
}
```

### 🚨 `Anyhow` Gotchas

* Keeping the `context` and `anyhow` strings up-to-date in all code base is harder than keeping `thiserror` messages as you don't have a single point of entry.
* `anyhow::Result` erases context that a caller might need, so avoid using it in a library.
* test helper functions can use `anyhow` with little to no issues.

## 4.5 Use `?` to Bubble Errors

Prefer using `?` over verbose alternatives like `match` chains:
```rust
fn handle_request(req: &Request) -> Result<ValidatedRequest, RequestValidationError> {
    validate_headers(req)?;
    validate_body_format(req)?;
    validate_credentials(req)?;
    let body = Body::try_from(req)?;

    Ok(ValidatedRequest::try_from((req, body))?)
}
```

> In case error recovery is needed, use `or_else`, `map_err`, `if let Ok(..) else`. To **inspect or log your error**, use `inspect_err`.

## 4.6 Unit Test should exercise errors

While many errors don't implement PartialEq and Eq, making it hard to do direct assertions between them, it is possible to check the error messages with `format!` or `to_string()`, making the errors meaningful and test validated:

```rust
#[test]
fn error_does_not_implement_partial_eq() {
    let err = divide(10., 0.0).unwrap_err();
    assert_eq!(err.to_string(), "division by zero");
}

#[test]
fn error_implements_partial_eq() {
    let err = process(my_value).unwrap_err();

    assert_eq!(
        err,
        MyError {
            ..
        }
    )
}
```

## 4.7 Important Topics

### Custom Error Structs

Sometimes you don't need an enum to handle your errors, as there is only one type of error that your module can have. This can be solved with `struct Errors`:

```rust
#[derive(Debug, thiserror::Error, PartialEq)]
#[error("Request failed with code `{code}`: {message}")]
struct HttpError {
    code: u16,
    message: String
}
```

### Async Errors

When using async runtimes, like Tokio, make sure that your errors implement `Send + Sync + 'static` where needed, specially in tasks or across `.await` boundaries:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    ...
    Ok(())
}
```

> Avoid `Box<dyn std::error::Error>` in libraries unless it is really needed
---
# Chapter 5 - Automated Testing

> Tests are not just for correctness. They are the first place people look to understand how your code works.

* Tests in rust are declared with the attribute macro `#[test]`. Most code editors can compile and run the functions declared under the macro individually or blocks of them.
* Test can have special compilation flags with `#[cfg(test)]`. Also executable in code editors if it contained `#[test]`, it is a good way to mock complicated functions or override traits.

## 5.1 Tests as Living Documentation

In Rust, as in many other languages, tests often show how the functions are meant to be used. If a test is clear and targeted, it's often more helpful than reading the function body, when combined with other tests, they serve as living documentation.

### Use descriptive names

> In the unit test name we should see the following:
> * `unit_of_work`: which *function* we are calling. The **action** that will be executed. This is often be the name of the the test `mod` where the function is being tested.
```rust
#[cfg(test)] 
mod test { 
  mod function_name { 
    #[test] 
    fn returns_y_when_x() { ... } 
  } 
}
```
> * `expected_behavior`: the set of **assertions** that we need to verify that the test works.
> * `state_that_the_test_will_check`: the general **arrangement**, or setup, of the specific test case.

#### ❌ Don't use a generic name for a test
```rust
#[test]
fn test_add_happy_path() {
    assert_eq!(add(2, 2), 4);
}
```
#### ✅ Use a name which reads like a sentence, describing the desired behavior
> Alternatively, if you function has too many tests, you can blob them together in a `mod`, it makes it easier to read and navigate.

```rust
// OPTION 1
#[test]
fn process_should_return_blob_when_larger_than_b() {
    let a = setup_a_to_be_xyz();
    let b = Some(2);
    let expected = MyExpectedStruct { ... };

    let result = process(a, b).unwrap();

    assert_eq!(result, expected);
}

// OPTION 2
mod process {
  #[test]
  fn should_return_blob_when_larger_than_b() {
      let a = setup_a_to_be_xyz();
      let b = Some(2);
      let expected = MyExpectedStruct { ... };

      let result = process(a, b).unwrap();

      assert_eq!(result, expected);
  }
}
```

> When executing `cargo test` the test output for each option will look like:
> Option 1: `process_should_return_blob_when_larger_than_b`.
> Option 2: `process::should_return_blob_when_larger_than_b`.

### Use modules for organization

Most IDEs can run a single module of tests all together.
The test name in the output will also contain the name of the module.
Together, that means you can use the module name to group related tests together:

```rust
#[cfg(test)]
mod test {  // IDEs will provide a ▶️ button here

  mod process {
    #[test] // IDEs will provide a ▶️ button here
    fn returns_error_xyz_when_b_is_negative() {
        let a = setup_a_to_be_xyz();
        let b = Some(-5);
        let expected = MyError::Xyz;
    
        let result = process(a, b).unwrap_err();
    
        assert_eq!(result, expected);
    }

    #[test] // IDEs will provide a ▶️ button here
    fn returns_invalid_input_error_when_a_and_b_not_present() {
      let a = None;
      let b = None;
      let expected = MyError::InvalidInput;

      let result = process(a, b).unwrap_err();

      assert_eq!(result, expected);
    }
  }
}
```

### Only test one behavior per function

To keep tests clear, they should describe _one_ thing that the unit does.
This makes it easier to understand why a test is failing.

#### ❌ Don't test multiple things in the same test
```rust
fn test_thing_parser(...) {
  assert!(Thing::parse("abcd").is_ok());
  assert!(Thing::parse("ABCD").is_err());
}
```

#### ✅ Test one thing per test
```rust
#[cfg(test)]
mod test_thing_parser {
  #[test]
  fn lowercase_letters_are_valid() {
    assert!(
      Thing::parse("abcd").is_ok(),
      // Works like `eprintln`, `format` and `println` macros
      "Thing parse error: {:?}", 
      Thing::parse("abcd").unwrap_err()
    );
  }

  #[test]
  fn capital_letters_are_invalid() {
    assert!(Thing::parse("ABCD").is_err());
  }
}
```

> `Ok` scenarios should have an `eprintln` of the `Err` case.

### Use very few, ideally one, assertion per test

When there are multiple assertions per test, it's both harder to understand the intended behavior and 
often requires many iterations to fix a broken test, as you work through assertions one by one.

❌ Don't include many assertions in one test:

```rust
#[test]
fn test_valid_inputs() {
  assert!(the_function("a").is_ok());
  assert!(the_function("ab").is_ok());
  assert!(the_function("ba").is_ok());
  assert!(the_function("bab").is_ok());
}
```

If you are testing separate behaviors, make multiple tests each with descriptive names.
To avoid boilerplate, either use a shared setup function or [rstest](https://crates.io/crates/rstest) cases *with descriptive test names*:
```rust
#[rstest]
#[case::single("a")]
#[case::first_letter("ab")]
#[case::last_letter("ba")]
#[case::in_the_middle("bab")]
fn the_function_accepts_all_strings_with_a(#[case] input: &str) {
  assert!(the_function(input).is_ok());
}
```

> Considerations when using `rstest`
>
> * It’s harder for both IDEs and humans to run/locate specific tests.
> * Expectation vs condition naming is now visually inverted (expectation first).

## 5.2 Add Test Examples to your Docs

We will deep dive into docs at a later stage, so in this section we will just briefly go over how to add tests to you docs. Rustdoc can turn examples into executable tests using `///` with a few advantages:

* These tests run with `cargo test` **BUT NOT** `cargo nextest run`. If using `nextest`, make sure to run `cargo t --doc` separately.
* They serve both as documentation and correctness checks, and are kept up to date by changes, due to the fact that the compiler checks them.
* No extra testing boilerplate. You can easily hide test sections by prefixing the line with `#`.
* ❗ There is no issue if you have test duplication between doc-tests and other non-public facing tests.

```rust
/// Helper function that adds any two numeric values together.
/// This function reasons about which would be the correct type to parse based on the type 
/// and the size of the numeric value.
/// 
/// # Examples
/// 
/// ```rust
/// # use crate_name::generic_add;
/// use num::numeric;
/// 
/// # assert_eq!(
/// generic_add(5.2, 4) // => 9.2
/// # , 9.2)
/// 
/// # assert_eq!(
/// generic_add(2, 2.0) // => 4
/// # , 4)
/// ```
```

This documentation code would look like:
```rust
use num::numeric;

generic_add(5.2, 4) // => 9.2
generic_add(2, 2.0) // => 4
```

## 5.3 Unit Test vs Integration Tests vs Doc tests

As a general rule, without delving into *test pyramid naming*, rust has 3 sets of tests:

### Unit Test

Tests that go in the **same module** as the **tested unit** was declared, this allows the test runner to have visibility over private functions and parent `use` declarations. They can also consume `pub(crate)` functions from other modules if needed. Unit tests can be more focused on **implementation and edge-cases checks**.

* They should be as simple as possible, testing one state and one behavior of the unit. KISS.
* They should test for errors and edge cases.
* Different tests of the same unit can be combined under a single `#[cfg(test)] mod test_unit_of_work {...}`, allowing multiple submodules for different `units_of_work`.
* Try to keep external states/side effects to your API to minimum and focus those tests on the `mod.rs` files.
* Tests that are not yet fully implemented can be ignored with the `#[ignore = "optional message"]` attribute.
* Tests that intentionally panic should be annotated with the attribute `#[should_panic]`.

```rust
#[cfg(test)]
mod unit_of_work_tests {
    use super::*;

    #[test]
    fn unit_state_behavior() {
        let expected = ...;
        let result   = ...;
        assert_eq!(result, expected, "Failed because {}", result - expected);
    }
}
```

### Integration Tests

Tests that go under the `tests/` directory, they are entirely external to your library and use the same code as any other code would use, not have access to private and crate level functions, which means they can **only test** functions on your **public API**. 

> Their purpose is to test whether many parts of the code work together correctly, units of code that work correctly on their own could have problems when integrated.

* Test for happy paths and common use cases.
* Allow external states and side effects, [testcontainers](https://rust.testcontainers.org/) might help.
* if testing binaries, try to break **executable** and **functions** into `src/main.rs` and `src/lib.rs`, respectively.

```
├── Cargo.lock 
├── Cargo.toml 
├── src 
│   └── lib.rs 
└── tests 
    ├── mod.rs 
    ├── common 
    │   └── mod.rs 
    └── integration_test.rs
```

### Doc Testing

As mentioned in section [5.2](#52-add-test-examples-to-your-docs), doc tests should have happy paths, general public API usage and more powerful attributes that improve documentation, like custom CSS for the code blocks.

### Attributes:

* `ignore`: tells rust to ignore the code, usually not recommended, if you want just a code formatted text, use `text`.
* `should_panic`: tells the rust compiler that this example block will panic.
* `no_run`: compiles but doesn't execute the code, similar to `cargo check`. Very useful when dealing with side-effects for documentation.
* `compile_fail`: Test rustdoc that this block should cause a compilation fail, important when you want to demonstrate wrong use cases.

## 5.4 How to `assert!`

Rust comes with 2 macros to make assertions:
* `assert!` for asserting boolean values like `assert!(value.is_ok(), "'value' is not Ok: {value:?}")`
* `assert_eq!` for checking equality between two different values, `assert_eq!(result, expected, "'result' differs from 'expected': {}", result.diff(expected))`.

### 🚨 `assert!` reminders
* Rust asserts support formatted strings, like the previous examples, those strings will be printed in case of failure, so it is a good practice to add what the actual state was and how it differs from the expected.
* If you don't care about the exact pattern matching value, using `matches!` combined with `assert!` might be a good alternative.
```rust
assert!(matches!(error, MyError::BadInput(_), "Expected `BadInput`, found {error}"));
```
* Use `#[should_panic]` wisely. It should only be used when panic is the desired behavior, prefer result instead of panic.
* There are some other that can enhance your testing experience like:
    * [`rstest`](https://crates.io/crates/rstest): fixture based test framework with procedural macros.
    * [`pretty_assertions`](https://crates.io/crates/pretty_assertions): overrides `assert_eq` and `assert_ne`,  and creates colorful diffs between them.

## 5.5 Snapshot Testing with `cargo insta`

> When correctness is visual or structural, snapshots tell the story better than asserts.

1. Add to your dependencies:
```toml
insta = { version = "1.42.2", features = ["yaml"] }
```
> For most real world applications the recommendation is to use YAML snapshots of serializable values. This is because they look best under version control and the diff viewer and support redaction. To use this enable the yaml feature of insta.

2. For a better review experience, add the CLI `cargo install cargo-insta`.
<img src="/wiki/download/attachments/1532592393/insta.svg" onerror="this.style.display='none'"/>

![cargo insta review](../images/insta.svg)

3. Writing a simple test:
```rust
fn split_words(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

#[test]
fn test_split_words() {
    let words = split_words("hello from the other side");
    insta::assert_yaml_snapshot!(words);
}
```

4. Run `cargo insta test` to execute, and `cargo insta review` to review conflicts.

To learn more about `cargo insta` check out its [documentation](https://insta.rs/docs/quickstart/) as it is a very complete and well documented tool.

### What is snapshot testing?

Snapshot testing compares your output (text, Json, HTML, YAML, etc) against a saved "golden" version. On future runs, the test fails if the output changes, unless humanly approved. It is perfect for:
* Generate code.
* Serializing complex data.
* Rendered HTML.
* CLI output.

#### ❌ What not to test with snapshot
* Very stable, numeric-only or small structured data associated logic (prefer `assert_eq!`).
* Critical path logic (prefer precise unit tests).
* Flaky tests, randomly generated output, unless redacted.
* Snapshots of external resources, use mocks and stubs.

## 5.6 ✅ Snapshot Best Practices

* Named snapshots, it gives meaningful snapshot files names, e.g. `snapshots/this_is_a_named_snapshot.snap`
```rust
assert_snapshot!("this_is_a_named_snapshot", output);
```

* Keep snapshots small and clear. 
```rust
// ✅ Best case:
assert_snapshot!("app_config/http", whole_app_config.http);

// ❌ Worst case:
assert_snapshot!("app_config", whole_app_config); // Huge object
```

> #### 🚨 Avoid snapshotting huge objects 
> Huge objects become hard to review and reason about.

* Avoid snapshotting simple types (primitives, flat enums, small structs):
```rust
// ✅ Better:
assert_eq!(meaning_of_life, 42);

// ❌ OVERKILL:
assert_snapshot!("the_meaning_of_life", meaning_of_life); // meaning_of_life == 42
```

* Use [redactions](https://insta.rs/docs/redactions/) for unstable fields (randomly generated, timestamps, uuid, etc):
```rust
use insta::assert_json_snapshot;

#[test]
fn endpoint_get_user_data() {
    let data = http::client.get_user_data();
    assert_json_snapshot!(
        "endpoints/subroute/get_user_data",
        data,
        ".created_at" => "[timestamp]",
        ".id" => "[uuid]"
    );
}
```
* Commit your snapshots into git. They will be stored in `snapshots/` alongside your tests.
* Review changes carefully before accepting.
---
# Chapter 6 - Generics, Dynamic Dispatch and Static Dispatch

> Static where you can, dynamic where you must

Rust allows you to handle polymorphic code in two ways:
* **Generics / Static Dispatch**: compile-time, monomorphized per use.
* **Trait Objects / Dynamic Dispatch**: runtime vtable, single implementation.

Understanding the trade-offs lets you write faster, smaller and more flexible code.

## 6.1 [Generics](https://doc.rust-lang.org/book/ch10-00-generics.html)

Every programming language has tools for effectively handling the duplication of concepts. In Rust, one such tool is generics: abstract stand-ins for concrete types or other properties. We can express the behavior of generics or how they relate to other generics without knowing what will be in their place when compiling and running the code. 

We use generics to create definitions for items like function signatures or structs, which we can then use with many different concrete data types. Let’s first look at how to define functions, structs, enums, and methods using generics. Generics can also be used to implement Type State Pattern and constrain a struct functionality to certain expected types, more on type state on [chapter 7](chapter_07.md).

[Generics by Examples](https://doc.rust-lang.org/rust-by-example/generics.html).

### Generics Performance

You might be wondering whether there is a runtime cost when using generic type parameters. The good news is that using generic types won’t make your program run any slower than it would with concrete types. Rust accomplishes this by performing monomorphization of the code using generics at compile time. Monomorphization is the process of turning generic code into specific code by filling in the concrete types that are used when compiled. The compiler checks for all occurrences of the generic parameter and generates code for the concrete types the generic code is called with.

## 6.2 Static Dispatch: `impl Trait` or `<T: Trait>`

A static dispatch is basically a constrained version of a generics, a trait bounded generic, at compile-time it is able to check if your generic satisfies the declared traits.

### ✅  Best when:
* You want **zero runtime cost**, by paying the compile time cost.
* You need **tight loops or performance**.
* Your types are **known at compile time**.
* Your are working with **single-use implementations** (monomorphized).

### 🏎️ Example: High-performance function with generic
```rust
fn specialized_sum<U: Sum + RandomMapping, T: Iterator<Item = U>>(iter: T) -> U {
    iter.map(|x| x.random_mapping()).sum()
}

// or, equivalent, more modern
fn specialized_sum<U: Sum + RandomMapping>(iter: impl Iterator<Item = U>) -> U {
    iter.map(|x| x.random_mapping()).sum()
}
```

This is compiled into **specialized machine code** for each usage, fast and inlined.

## 6.3 Dynamic Dispatch: `dyn Trait`

Usually dynamic dispatch is used with some kind of pointer or a reference, like `Box<dyn Trait>`, `Arc<dyn Trait>` or `&dyn trait`.

### ✅  Best when:
* You absolutely need runtime polymorphism.
* You need to **store different implementations** in one collection.
* You want to **abstract internals behind a stable interface**.
* You are writing a **plugin-style architecture**.

> ❗ Closer to what you would get in an object oriented language and can have some heavy costs associated to it. Can avoid generic entirely and let you mix types that implement the same traits.

### 🚚 Example: Heterogeneous collection

```rust
trait Animal {
    fn greet(&self) -> String;
}

struct Dog;
impl Animal for Dog {
    fn greet(&self) -> String {
        "woof".to_string()
    }
}

struct Cat;
impl Animal for Cat {
    fn greet(&self) -> String {
        "meow".to_string()
    }
}

fn all_animals_greeting(animals: Vec<Box<dyn Animal>>) {
    for animal in animals {
        println!("{}", animal.greet())
    }
}
```

## 6.4 Trade-off summary

|                   	| Static Dispatch (impl Trait) 	|    Dynamic Dispatch (dyn Trait)   	|
|-------------------	|------------------------------	|---------------------------------- 	|
| Performance       	| ✅ Faster, inlined            	| ❌ Slower: vtable indirection         |
| Compile time      	| ❌ Slower: monomorphization   	| ✅ Faster: shared code                |
| Binary size       	| ❌ Larger: per-type codegen   	| ✅ Smaller                            |
| Flexibility       	| ❌ Rigid, one type at a time  	| ✅ Can mix types in collections       |
| Use in trait fn() 	| ❌ Traits must be object-safe 	| ✅ Works with trait objects           |
| Errors            	| ✅ Clearer                    	| ❌ Erased types can confuse errors    |

* Prefer generics/static dispatch when you control the call site and want performance.
* Use dynamic dispatch when you need abstraction, plugins or mixed types. 🚨 Runtime cost.
* If you are not sure, start with generics, trait bound them - then use `Box<dyn Trait>` when flexibility outweighs speed.

> Favor static dispatch until your trait needs to live behind a pointer.

## 6.5 Best Practices for Dynamic Dispatch

Dynamic dispatch `Ptr<dyn Trait>` is a powerful tool, but it also has significant performance trade-offs. You should only reach for it when **type erasure or runtime polymorphism** are essential. It is important to know when you need Trait Objects:

### ✅ Use Dynamic Dispatch When:

* You need heterogeneous types in a collection:
```rust
fn all_animals_greeting(animals: Vec<Box<dyn Animal>>) {
    for animal in animals {
        println!("{}", animal.greet())
    }
}
```

* You want runtime plugins or hot-swappable components.
* You want to abstract internals from the caller (library design).


### ❌ Avoid Dynamic Dispatch When:

* You control the concrete types.
* You are writing code in performance critical paths.
* You can express the same logic in other ways while keeping simplicity, e.g. generics.

## 6.6 🚨 Trait Objects Ergonomics

* Prefer `&dyn Trait` over `Box<dyn Trait>` when you don't need ownership.
* Use `Arc<dyn Trait + Send + Sync>` for shared access across threads.
* Don't use `dyn Trait` if the trait has methods that return `Self`.
* **Avoid boxing too early**. Don't box inside structs unless you are sure it'll be beneficial or is required (recursive).
```rust
// ✅ Use generics when possible
struct Renderer<B: RenderBackend> {
    backend: B
}

// ❌ Premature Boxing
struct Renderer {
    backend: Box<dyn RenderBackend> // Boxing too early
}
```
* If you must expose a `dyn trait` in a public API, `Box` at the boundary, not internally.
* **Object Safety**: You can only create `dyn Traits` from object-safe traits:
    * It has **no generic methods**.
    * It doesn't require `Self: Sized`.
    * All method signatures use `&self`, `&mut self` or `self`.
    ```rust
    // ✅ Object Safe
    trait Runnable {
        fn run(&self);
    }

    // ❌ Not Object Safe
    trait Factory {
        fn create<T>() -> T; // generic methods are not allowed
    }
    ```
---
# Chapter 7 - Type State Pattern

Models state at compile time, preventing bugs by making illegal states unrepresentable. It takes advantage of the Rust generics and type system to create sub-types that can only be reached if a certain condition is achieved, making some operations illegal at compile time. 

> Recently it became the standard design pattern of Rust programming. However, it is not exclusive to Rust, as it is achievable and has inspired other languages to do the same [swift](https://swiftology.io/articles/typestate/) and [typescript](https://catchts.com/type-state).

## 7.1 What is Type State Pattern?

**Type State Pattern** is a design pattern where you encode different **states** of the system as **types**, not as runtime flags or enums. This allows the compiler to enforce state transitions and prevent illegal actions at compile time. It also improves the developer experience, as developers only have access to certain functions based on the state of the type.

> Invalid states become compile errors instead of runtime bugs.

## 7.2 Why use it?

* Avoids runtime checks for state validity. If you reach certain states, you can make certain assumptions of the data you have.
* Models state transitions as type transitions. This is similar to a state machine, but in compile time.
* Prevents data misuse, e.g. using uninitialized objects.
* Improves API safety and correctness.
* The phantom data field is removed after compilation so no extra memory is allocated.

## 7.3 Simple Example: File State

[Github Example](https://github.com/apollographql/rust-best-practices/tree/main/examples/simple-type-state)
```rust
use std::{io, path::{Path, PathBuf}};

struct FileNotOpened;
struct FileOpened;

#[derive(Debug)]
struct File<State> {
    /// Path to the opened file
    path: PathBuf,
    /// Open `File` handler
    handle: Option<std::fs::File>,
    /// Type state manager
    _state: std::marker::PhantomData<State> 
}

impl File<FileNotOpened> {
    /// `open` is the only entry point for this struct.
    /// * When called with a valid path, it will return a `File<FileOpened>` with a valid `handler` and `path`
    /// * `open` serves as an alternative to `new` and `defaults` methods (usable when your struct needs valid data to exist).
    fn open(path: &Path) -> io::Result<File<FileOpened>> {
        // If file is invalid, it will return `std::io::Error`
        let file = std::fs::File::open(path)?;
        Ok(
            File {
                path: path.to_path_buf(),
                // Always valid
                handle: Some(file),
                _state: std::marker::PhantomData::<FileOpened>
            }
        )
    }
}

impl File<FileOpened> {
    /// Reads the content of the `File` as a `String`.
    /// `read` can only be called by state `File<FileOpened>`
    fn read(&mut self) -> io::Result<String> {
        use io::Read;

        let mut content = String::new();
        let Some(handle)=  self.handle.as_mut() else {
            unreachable!("Safe to unwrap as state can only be reached when file is open");
        };
        handle.read_to_string(&mut content)?;
        Ok(content)
    }

    /// Returns the valid path buffer.
    fn path(&self) -> &PathBuf {
        &self.path
    }
}
```

## 7.4 Real-World Examples

### Builder Pattern with Compile-Time Guarantees

> Forces the user to **set required fields** before calling `.build()`.

[Github Example](https://github.com/apollographql/rust-best-practices/tree/main/examples/type-state-builder)

A type-state pattern can have more than one associated states:

```rust
use std::marker::PhantomData;

struct MissingName;
struct NameSet;
struct MissingAge;
struct AgeSet;

#[derive(Debug)]
struct Person {
    name: String,
    age: u8,
    email: Option<String>,
}

struct Builder<HasName, HasAge> {
    name: Option<String>,
    age: u8,
    email: Option<String>,
    _name_marker: PhantomData<HasName>,
    _age_marker: PhantomData<HasAge>,
}

impl Builder<MissingName, MissingAge> {
    fn new() -> Self {
        Builder { name: None, age: 0, _name_marker: PhantomData, _age_marker: PhantomData, email: None }
    }

    fn name(self, name: String) -> Builder<NameSet, MissingAge> {
        Builder { name: Some(name), _name_marker: PhantomData::<NameSet>, age: self.age, _age_marker: PhantomData, email: None }
    }

    fn age(self, age: u8) -> Builder<MissingName, AgeSet> {
        Builder { age,  _age_marker: PhantomData::<AgeSet>, name: None, _name_marker: PhantomData, email: None }
    }
}

impl Builder<NameSet, MissingAge> {
    fn age(self, age: u8) -> Builder<NameSet, AgeSet> {
        Builder { age,  _age_marker: PhantomData::<AgeSet>, name: self.name, _name_marker: PhantomData::<NameSet>, email: None }
    }
}

impl Builder<MissingName, AgeSet> {
    fn email(self, email: String) -> Self {
        Self { name: self.name , age: self.age , email: Some(email) , _name_marker: self._name_marker , _age_marker: self._age_marker  }
    }

    fn name(self, name: String) -> Builder<NameSet, AgeSet> {
        Builder { name: Some(name), _name_marker: PhantomData::<NameSet>, age: self.age, _age_marker: PhantomData::<AgeSet>, email: self.email }
    }
}

impl Builder<NameSet, AgeSet> {
    fn build(self) -> Person {
        Person { 
            name: self.name.unwrap_or_else(|| unreachable!("Name is guarantee to be set")), 
            age: self.age,
            email: self.email,
        }
    }
}
```

Although a bit more verbose than a usual builder, this guarantees that all necessary fields are present (note that e-mail is optional field only present in the final builder).

#### Usage:
```rust
// ✅ Valid cases
let person: Person = Builder::new().name("name".to_string()).age(30).build();
let person: Person = Builder::new().age(30).name("name".to_string()).build();
let person: Person = Builder::new().age(30).name("name".to_string()).email("myself@email.com".to_string()).build();

// ❌ Invalid cases
let person: Person = Builder::new().name("name".to_string()).build(); // ❌ Compile error: Age required to `build`
let person: Person = Builder::new().age(30).build(); // ❌ Compile error:  Name required to `build`
let person: Person = Builder::new().age(30).email("myself@email.com".to_string()).build(); // ❌ Compile error:  Name required to `build`
let person: Person = Builder::new().build();// ❌ Compile error:  Name and Age required to `build`
```

### Network Protocol State Machine

Illegal transitions like sending a message before connecting **simply don't compile**:

```rust
// Mock example
struct Disconnected;
struct Connected;

struct Client<State> {
    stream: Option<std::net::TcpStream>,
    _state: std::marker::PhantomData<State>
}

impl Client<Disconnected> {
    fn connect(addr: &str) -> std::io::Result<Client<Connected>> {
        let stream = std::net::TcpStream::connect(addr)?;
        Ok(Client {
            stream: Some(stream),
            _state: std::marker::PhantomData::<Connected>
        })
    }
}

impl Client<Connected> {
    fn send(&mut self, msg: &str) {
        use std::io::Write;
        let Some(stream) = self.stream.as_mut() else {
            unreachable!("Stream is guarantee to be set");
        };
        stream.write_all(msg.as_bytes())
    }
}
```

## 7.5 Pros and Cons

### ✅ Use Type-State Pattern When:
* Your want **compile-time state safety**.
* You need to enforce **API constraints**.
* You are writing a library/crate that is heavy dependent on variants.
* Your want to replace runtime booleans or enums with **type-safe code paths**.
* You need compile time correctness.

### ❌ Avoid it when:
* Writing trivial states like enums.
* Don't need type-safety.
* When it leads to overcomplicated generics.
* When runtime flexibility is required.

### 🚨 Downsides and Cautions
* Can lead to more **verbose solutions**.
* Can lead to **complex type signatures**.
* May require **unsafe** to return **variant outputs** based on different states.
* May required a bunch of duplication (e.g. same struct field reused).
* PhantomData is not intuitive for beginners and can feel a bit hacky.

> Use this pattern when it **saves bugs, increases safety or simplifies logic**, not just for cleverness.
---
# Chapter 8 - Comments vs Documentation

> Clear code beats clear comments. However, when the why isn't obvious, comment it plainly - or link to where you can read more context.

## 8.1 Comments vs Documentation: Know the Difference

| Purpose      	| Use `// comment`                          	| Use `/// doc` or `//! crate doc`                                  |
|--------------	|-------------------------------------------	|----------------------------------------------------------------	|
| Describe Why 	| ✅ Yes - explains tricky reasoning            | ❌ Not for documentation                                          	|
| Describe API 	| ❌ Not useful                                 | ✅ Yes - public interfaces, usage, details, errors, panics         	|
| Maintainable 	| 🚨 Often becomes obsolete and hard to reason 	| ✅ Tied to code, appears in generated docs and can run test cases 	|
| Visibility   	| Local development only                    	| Exported to users and tools like `cargo doc`                   	|

## 8.2 When to use comments

Use `//` comments (double slashed) when something can't be expressed clearly in code, like:
* **Safety Guarantees**, some of which can be better expressed with code conditionals.
* Workarounds or **Optimizations**.
* Legacy or **platform-specific** behaviors. Some of them can be expressed with `#[cfg(..)]`.
* Links to **Design Docs** or **ADRs**.
* Assumptions or **gotchas** that aren't obvious.

> Name your comments! For example, a comment regarding a safety guarantee should start with `// SAFETY: ...`.

### ✅ Good comment:
```rust
// SAFETY: `ptr` is guaranteed to be non-null and aligned by caller
unsafe { std::ptr::copy_nonoverlapping(src, dst, len); }
```

### ✅ Design context comment:
```rust
// CONTEXT: Reuse root cert store across subgraphs to avoid duplicate OS calls:
// [ADR-12](link/to/adr-12): TLS Performance on MacOS
```

## 8.3 When comments get in the way

Avoid comments that:
* Restate obvious things (`// increment i by 1 for the next loop`).
* Can grow stale over time.
* `TODO`s without actions (links to some versioned issue).
* Could be replaced by better naming or smaller functions.

### ❌ Bad comment:
```rust
fn compute(counter: &mut usize) {
    // increment by 1
    *counter += 1;
}
```

### ❌ Too long or outdated
```rust
// Originally written in 2028 for some now-defunct platform
```

## 8.4 Don't Write Living Documentation (living comments)

Comments as a "living documentation" is a **dangerous myth**, as comments are **not free**:
* They **rot** - nobody compiles comments.
* They **mislead** - readers usually assume they are true with no critique, e.g. "the other developer knows this code better than I do".
* They **go stale** - unless maintained with the code, they become irrelevant.
* They are **noisy** - comments can clutter your code with multiple unnecessary lines.

If something deserves to live beyond a PR, put it in:
* An **ADR** (Architectural Design Record).
* A Design Document.
* Document it **in code** by using types, doc comments, examples, renaming code blocks into cleaner functions.
* Add tests to cover and explain the change.

> ### 🚨 If you find a comment, **read it in context**. Does it still make sense? If not, remove or update it, or ask for help. Comments should bother you.

## 8.5 Replace Comments with Code

Instead of long commented blocks, break logic into named helper functions:

#### ❌ Commented code block:
```rust
fn save_user(&self) -> Result<(), MyError> {
    // check if the user is authenticated
    if self.is_authenticated() {
        // serialize user data
        let data = serde_json::to_string(self)?;
        // write to file
        std::fs::write(self.path(), data)?;
    }
}

```
**✅ Extract for clarity**:

```rust
fn save_auth_user(&self) -> Result<PathBuf, MyError> {
    if self.is_authenticated() {
        let path = self.path();
        let serialized_user = serde_json::to_string(self)?;
        std::fs::write(path, serialized_user)?;
        Ok(path)
    } else {
        Err(MyError::UserNotAuthenticated)
    }
}
```

## 8.6 `TODO` should become issues

Don't leave `// TODO:` scattered around the codebase with no owner. Instead:
1. File Github Issue or Jira Ticket. (Prefer github issues on public repositories).
2. Reference the issue in the code:

```rust
// TODO(issue #42): Remove workaround after bugfix
```

This makes `TODO`s trackable, actionable and visible to everyone.

## 8.7 When to use doc comments

Use `///` doc comments  to document:
* All **public functions, structs, traits, enums**.
* Their purpose, their usage and their behaviors.
* Anything developers need to understand how to use it correctly.
* Add context that related to `Errors` and `Panics`.
* Plenty of examples.

### ✅ Good doc comment:

```rust
/// Loads [`User`] profile from disk
/// 
/// # Error
/// - Returns [`MyError`] if the file is missing [`MyError::FileNotFound`].
/// - Returns [`MyError`] if the content is an invalid Json, [`MyError::InvalidJson`].
fn load_user(path: &Path) -> Result<User, MyError> {...}
```

**Doc comments can also include examples, links and even tests:**

```rust
/// Returns the square of the integer part of any number.
/// Square is limited to `u128`.
/// 
/// # Examples
/// 
/// ```rust
/// assert_eq!(square(4.3), 16)
/// ```
fn square(x: impl ToInt) -> u128 { ... }
```

## 8.8 Documentation in Rust: How, When and Why

Rust provides **first-class documentation tooling** via rustdoc, which makes documenting your code a key part of writing idiomatic and maintainable rust. There are doc specific lints to help with documentation, like:

| Lint      	| Description                               	|
|--------------	|-------------------------------------------	|
| [missing_docs](https://doc.rust-lang.org/rustdoc/lints.html#missing_docs) 	| Warns that a public functions, struct, const, enum has missing documentation           	|
| [broken_intra_doc_links](https://doc.rust-lang.org/rustdoc/lints.html#broken_intra_doc_links) 	| Detects if an internal documentation link is broken. Specially useful when things are renamed.                                	|
| [empty_docs](https://rust-lang.github.io/rust-clippy/master/#empty_docs) 	| Disallow empty docs - preventing bypass of `missing_docs` 	|
| [missing_panics_doc](https://rust-lang.github.io/rust-clippy/master/#missing_panics_doc)   	| Warns that documentation should have a `# Panics` section if function can panic                    	|
| [missing_errors_doc](https://rust-lang.github.io/rust-clippy/master/#missing_errors_doc)   	| Warns that documentation should have a `# Errors` section if function returns a `Result` explaining `Err` conditions                    	|
| [missing_safety_doc](https://rust-lang.github.io/rust-clippy/master/#missing_safety_doc)   	| Warns that documentation should have a `# Safety` section if public facing functions have visible unsafe blocks                    	|


### Difference between `///` and `//!`

| Style     | Used for                     	| Scope                                        	|Example                                                  	|
|----------	|------------------------------	|-------------------------------------------	|----------------------------------------------------------------	|
| `///` 	| Line doc comment           	| Public items like struct, fn, enum, consts   	| Documenting, giving context and usage to `fn`, `struct`, `enum`, etc   	|
| `//!` 	| Module level doc comment     	| Modules or entire crates                  	| Explaining crate/module purpose with common use cases and quickstart   	|

### `///` Item level documentation

Use `///` for functions, structs, traits, enums, const, etc:

```rust
/// Adds two numbers together.
///
/// # Examples
///
/// ```
/// let result = my_crate::add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```
* ✅ Write clear and descriptive **What it does** and **how to use it**.
* ✅ Use `# Examples` section to better explain **how to use it**.
* ✅ Prefer writing examples that can be tested via `cargo test`, even if you have to hide their output with starting `#`:
```rust
/// ```
/// let result = my_crate::add(2, 3);
/// # assert_eq!(result, 5);
/// ```
```
* ✅ Use `# Panics`, `# Errors` and `# Safety` sections when relevant.
* Add relevant context to the type.

### `//!` Module/Crate level Documentation

Use `//!` when you want to document the **purpose of a module or a crate**. It is places at the top of a `lib.rs` or `mod.rs` file, for example `engine/mod.rs`:
```rust
//! This module implements a custom chess engine.
//! 
//! It handles board state, move generation and check detection.
//! 
//! # Example
//! ```
//! let board = chess::engine::Board::default();
//! assert!(board.is_valid());
//! ```
```

## 8.9 Checklist for Documentation coverage

📦 Crate-Level (lib.rs)
- [ ] `//!` doc at top explains **what the crate does**, and **what problems it solves**.
- [ ]  Includes crate-level `# Examples` or pointers to modules.
📁 Modules (mod.rs or inline)
- [ ]  `//!` doc explains **what this module is for**, its **exports**, and **invariants**.
- [ ]  Avoid repeating doc comments on re-exported items unless clarification is needed.
🧱 Structs, Enums, Traits
- `///` doc explains:
    - [ ]  The role this type plays.
    - [ ]  Invariants or expectations.
    - [ ]  Example construction or usage.
- [ ]  Consider using [`#[non_exhaustive]`](https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute) if external users may match on it.
🔧 Functions and Methods
- `///` doc covers:
    - [ ] What it does.
    - [ ] Parameters and their meaning.
    - [ ] Return value behavior.
    - [ ] Edge cases (`# Panics`, `# Errors`).
    - [ ] Usage example, `# Examples`.
📑 Traits
- [ ] Explain the **purpose** of the trait (marker? dynamic dispatch?).
- [ ] Doc for each method — include **when/why** to implement it.
- [ ] Document clearly default implemented methods and when to override.
📦 Public Constants
- [ ] Document what they configure and when you'd want to use them.

### 📌 Best Practices
* ✅ Use examples generously — they double as test cases.
* ✅ Prefer clarity over formality — it’s for humans, not machines.
* ✅ Prefer doc comments to explain usage, and leave implementation details to code comments if needed.
* ✅ Use `cargo doc --open` to check your output often.
* ✅ Add `#![deny(missing_docs)]` and other relevant doc lints in top-level modules if you want to enforce full doc coverage.
---
# Chapter 9 - Understanding Pointers

Many higher level languages hide memory management, typically **passing by value** (copy data) or **passing by reference** (reference to shared data) without worrying about allocation, heap, stack, ownership and lifetimes, it is all delegated to the garbage collector or VM. Here is a comparison on this topic between a few languages:

### 📌 Language Comparison 

| Language   	| Value Types                         	| Reference/Pointer Types                                   	| Async Model & Types                                                        	| Manual Memory                	|
|------------	|-------------------------------------	|-----------------------------------------------------------	|----------------------------------------------------------------------------	|------------------------------	|
| Python     	| None                                	| Everything is a reference                                 	| async def, await, Task, coroutines and asyncio.Future                      	| ❌ Not Allowed                  	|
| Javascript 	| Primitives                          	| Objects                                                   	| `async/await`, `Promise`, `setTimeout`. single threaded event loop         	| ❌ Not Allowed                  	|
| Java       	| Primitives                          	| Objects                                                   	| `Future<T>`, threads, Loom (green threads)                                   	| ❌ Almost none & not recommended 	|
| Go         	| Values are copied unless using `&T` 	| Pointers (`*T`, `&T`), escape analysis                    	| goroutines, `channels`, `sync.Mutex`, `context.Context`                          	| ⚠️ Limited                      	|
| C          	| Primitives and structs supported    	| Raw pointers `T*` and `*void`                             	| Threads, event loops (`libuv`, `libevent`)                                 	| ✅ Fully                        	|
| C++        	| Primitives and references           	| Raw `T*` and smart pointers `shared_ptr` and `unique_ptr` 	| threads, `std::future`, `std::async`, (since c++ 20 `co_await/coroutines`) 	| ✅ Mostly                       	|
| Rust       	| Primitives, Arrays, `impl Copy`     	| `&T`, `&mut T`, `Box<T>`, `Arc<T>`                                	| `async/await`, `tokio`, `Future`, `JoinHandle`, `Send + Sync`              	|    ✅🔒  Safe and Explicit                        	|

## 9.1 Thread Safety

Rust tracks pointers using `Send` and `Sync` traits:
- `Send` means data can move across threads.
- `Sync` means data can be referenced from multiple threads.

> A pointer is thread-safe only if the data behind it is.

| Pointer Type   	| Short Description                                                         	| Send + Sync?                          |  Main Use  	|
|----------------	|---------------------------------------------------------------------------	|--------------------------------------	|------------	|
| `&T`             	| Shared reference                                                          	| Yes                                 	| Shared access      |
| `&mut T`         	| Exclusive mutable reference                                               	| No, not Send                         	| Exclusive mutation |
| `Box<T>`         	| Heap-allocated owning pointer                                             	| Yes, if T: Send + Sync               	| Heap allocation    |
| `RC<T>`          	| Single-threaded ref counted pointer                                       	| No, neither                          	| Multiple owners (single-thread) |
| `Arc<T>`         	| Atomic ref counter pointer                                                	| Yes                                  	| Multiple owners (multi-thread) |
| `Cell<T>`        	| Interior mutability for copy types                                        	| No, not Sync                         	| Shared mutable, non-threaded |
| `RefCell<T>`     	| Interior mutability (dynamic borrow checker)                              	| No, not Sync                         	| Shared mutable, non-threaded |
| `Mutex<T>`       	| Thread-safe interior mutability with exclusive access                     	| Yes                                  	| Shared mutable, threaded |
| `RwLock<T>`      	| Thread-safe shared readonly access OR exclusive mutable access            	| Yes                                  	| Shared mutable, threaded |
| `OnceCell<T>`    	| Single-thread one-time initialization container (interior mutability ONCE)    | No, not Sync                         	| Simple lazy value initialization |
| `LazyCell<T>`    	| A lazy version of `OnceCell<T>` that calls function closure to initialize 	| No, not Sync                         	| Complex lazy value initialization 
| `OnceLock<T>`    	| Thread-safe version of `OnceCell<T>`                                      	| Yes                                  	| Multi-thread single init |
| `LazyLock<T>`    	| Thread-safe version of  `LazyCell<T>`                                     	| Yes                                  	| Multi-thread complex init	|
| `*cont T/*mut T` 	| Raw Pointers                                                              	| No, user must ensure safety manually 	| Raw memory / FFI |

## 9.2 When to use pointers:

### `&T` - Shared Borrow:

Probably the most common type in a Rust code base, it is **Safe, with no mutation** and allows **multiple readers**.

```rust
let data: String = String::from_str("this a string").unwrap();

print_len(&data);
print_capacity(&data);
print_bytes(&data);

fn print_len(s: &str) {
    println!("{}", s.len())
}

fn print_capacity(s: &String) {
    println!("{}", s.capacity())
}

fn print_bytes(s: &String) {
    println!("{:?}", s.as_bytes())
}
```
### `&mut T` - Exclusive Borrow:

Probably the most common *mutable* type in a Rust code base, it is **Safe, but only allows one mutable borrow at a time**.

```rust
let mut data: String = String::from_str("this a string").unwrap();
mark_update(&mut data);

fn mark_update(s: &mut String) {
    s.push_str("_update");
}
```

### [`Box<T>`](https://doc.rust-lang.org/std/boxed/struct.Box.html) - Heap Allocated

Single-owner heap-allocated data, great for recursive types and large structs.

```rust
pub enum MySubBoxedEnum<T> {
    Single(T),
    Double(Box<MySubBoxedEnum<T>>, Box<MySubBoxedEnum<T>>),
    Multi(Vec<T>), // Note that Vec is already a boxed value
}
```

### [`Rc<T>`](https://doc.rust-lang.org/std/rc/struct.Rc.html) - Reference Counter (single-thread)

You need multiple references to data in a single thread. Most common example is linked-list implementation.

### [`Arc<T>`](https://doc.rust-lang.org/std/sync/struct.Arc.html) - Atomic Reference Counter (multi-thread)

You need multiple references to data in multiple threads. Most common use cases is sharing readonly Vec across thread with `Arc<[T]>` and wrapping a `Mutex` so it can be easily shared across threads, `Arc<Mutex<T>>`.

### [`RefCell<T>`](https://doc.rust-lang.org/std/cell/struct.RefCell.html) - Runtime checked interior mutability

Used when you need shared access and the ability to mutate date, borrow rules are enforced at runtime. **It may panic!**.

```rust
use std::cell::RefCell;
let x = RefCell::new(42);
*x.borrow_mut() += 1;

assert_eq!(&*x.borrow(), 42, "Not meaning of life");
```

Panic example:
```rust
use std::cell::RefCell;
let x = RefCell::new(42);

let borrow = x.borrow();

let mutable = x.borrow_mut();
```

### [`Cell<T>`](https://doc.rust-lang.org/std/cell/struct.Cell.html) - Copy-only interior mutability

Somewhat the fast and safe version of `RefCell`, but it is limited to types that implement the `Copy` trait:

```rust
use std::cell::Cell;

struct SomeStruct {
    regular_field: u8,
    special_field: Cell<u8>,
}

let my_struct = SomeStruct {
    regular_field: 0,
    special_field: Cell::new(1),
};

let new_value = 100;

// ERROR: `my_struct` is immutable
// my_struct.regular_field = new_value;

// WORKS: although `my_struct` is immutable, `special_field` is a `Cell`,
// which can always be mutated with copy values
my_struct.special_field.set(new_value);
assert_eq!(my_struct.special_field.get(), new_value);
```

### [`Mutex<T>`](https://doc.rust-lang.org/std/sync/struct.Mutex.html) - Thread-safe mutability

An exclusive access pointer that allows a thread to read/write the data contained inside. It is usually wrapped in an `Arc` to allow shared access to the Mutex.

### [`RwLock<T>`](https://doc.rust-lang.org/std/sync/struct.RwLock.html) - Thread-safe mutability

Similar to a `Mutex`, but it allows multiple threads to read it OR a single thread to write. It is usually wrapped in an `Arc` to allow shared access to the RwLock.


### [`*const T/*mut T`](https://doc.rust-lang.org/std/primitive.pointer.html) - Raw pointers

Inherently **unsafe** and necessary for FFI. Rust makes their usage explicit to avoid accidental misuse and unwilling manual memory management.

```rust
let x = 5;
let ptr = &x as *const i32
unsafe {
    println!("PTR is {}", *ptr)
}
```

### [`OnceCell`](https://doc.rust-lang.org/std/cell/struct.OnceCell.html) - Single-thread single initialization container

Most useful when you need to share a configuration between multiple data structures.

```rust
use std::{cell::OnceCell, rc::Rc};

#[derive(Debug, Default)]
struct MyStruct {
    distance: usize,
    root: Option<Rc<OnceCell<MyStruct>>>,    
}

fn main() {
    let root = MyStruct::default();
    let root_cell = Rc::new(OnceCell::new());
    if let Err(previous) = root_cell.set(root) {
        eprintln!("Previous Root {previous:?}");
    }
    let child_1 = MyStruct{
        distance: 1,
        root: Some(root_cell.clone())
    };

    let child_2 = MyStruct{
        distance: 2,
        root: Some(root_cell.clone())
    };


    println!("CHild 1: {child_1:?}");
    println!("CHild 2: {child_2:?}");
}
```

### [`LazyCell`](https://doc.rust-lang.org/std/cell/struct.LazyCell.html) - Lazy initialization of `OnceCell`

Useful when the initialized data can be delayed to when it is actually being called.

### [`OnceLock`](https://doc.rust-lang.org/std/sync/struct.OnceLock.html) - thread-safe `OnceCell`

Useful when you need a `static` value.

```rust
use std::sync::OnceLock;

static CELL: OnceLock<usize> = OnceLock::new();

// `OnceLock` has not been written to yet.
assert!(CELL.get().is_none());

// Spawn a thread and write to `OnceLock`.
std::thread::spawn(|| {
    let value = CELL.get_or_init(|| 12345);
    assert_eq!(value, &12345);
})
.join()
.unwrap();

// `OnceLock` now contains the value.
assert_eq!(
    CELL.get(),
    Some(&12345),
);
```

### [`LazyLock`](https://doc.rust-lang.org/std/sync/struct.LazyLock.html) - thread-safe `LazyCell`

Similar to `OnceLock`, but the static value is a bit more complex to initialize.

```rust
use std::sync::LazyLock;

static CONFIG: LazyLock<HashMap<String, T>> = LazyLock::new(|| {
    let data = read_config();
    let mut config: HashMap<String, T> = data.into();
    config.insert("special_case", T::Default());
    config
});

let _ = &*CONFIG;
```

## References
- [Mara Bos - Rust Atomics and Locks](https://marabos.nl/atomics/)
- [Semicolon video on pointers](https://www.youtube.com/watch?v=Ag_6Q44PBNs)
