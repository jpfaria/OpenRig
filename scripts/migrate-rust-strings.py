#!/usr/bin/env python3
"""
Wrap user-visible literal strings in adapter-gui Rust code with rust_i18n::t!()
so they participate in the rust-i18n YAML-based pipeline.

Targets these call patterns:
  set_status_error(&w, &t, "literal")
  set_status_warning(&w, &t, "literal")
  set_status_info(&w, &t, "literal")
  obj.set_status_message("literal".into())
  obj.set_toast_message("literal".into())
  anyhow!("literal")
  bail!("literal")

Skips:
  - Strings already wrapped in t!(...)
  - Empty strings ""
  - Strings without letters (likely identifiers, hex, paths)
  - format!(...) calls (manual)

This script is idempotent — running it twice produces no further changes.

After running, locale YAML files (crates/adapter-gui/locales/<lang>.yml)
need new entries for any new strings. The rust-i18n CLI (`cargo i18n`)
can refresh them; for now we maintain by hand.

Usage:
  scripts/migrate-rust-strings.py [--dry-run]
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / "crates" / "adapter-gui" / "src"

# Pattern A: set_status_*(args..., "literal")
PATTERN_STATUS = re.compile(
    r'(?P<call>set_status_(?:error|warning|info|success)\s*\([^"]*?,\s*)"(?P<body>(?:[^"\\]|\\.)*)"\s*\)',
    re.DOTALL,
)

# Pattern B: .set_status_message("literal".into())
PATTERN_SETTER = re.compile(
    r'(?P<call>\.set_(?:status_message|toast_message|error_message|warning_message)\s*\(\s*)"(?P<body>(?:[^"\\]|\\.)*)"\s*\.into\(\)\s*\)',
)

# Pattern C: anyhow!("literal") / bail!("literal")
PATTERN_ANYHOW = re.compile(
    r'(?P<call>(?:anyhow::)?(?:anyhow|bail)!\s*\(\s*)"(?P<body>(?:[^"\\]|\\.)*)"\s*\)',
)

ALREADY_WRAPPED_TOKENS = ("t!(", "rust_i18n::t!(", "i18n::t!(", "tr!(")


def is_translatable(body: str) -> bool:
    if not body or body.isspace():
        return False
    if not re.search(r"[A-Za-zÀ-ÿ]", body):
        return False
    if re.fullmatch(r"[A-Za-z0-9_\-./]+", body):
        return False
    return True


def already_wrapped(line_before: str) -> bool:
    return any(tok in line_before[-30:] for tok in ALREADY_WRAPPED_TOKENS)


def migrate_file(path: Path, dry_run: bool) -> tuple[int, set[str]]:
    text = path.read_text(encoding="utf-8")
    replacements = 0
    new_keys: set[str] = set()

    def replace_status(m):
        nonlocal replacements
        body = m["body"]
        if not is_translatable(body) or already_wrapped(m["call"]):
            return m.group(0)
        replacements += 1
        new_keys.add(body)
        return f'{m["call"]}&rust_i18n::t!("{body}"))'

    def replace_setter(m):
        nonlocal replacements
        body = m["body"]
        if not is_translatable(body) or already_wrapped(m["call"]):
            return m.group(0)
        replacements += 1
        new_keys.add(body)
        return f'{m["call"]}rust_i18n::t!("{body}").to_string().into())'

    def replace_anyhow(m):
        nonlocal replacements
        body = m["body"]
        if not is_translatable(body) or already_wrapped(m["call"]):
            return m.group(0)
        replacements += 1
        new_keys.add(body)
        return f'{m["call"]}"{{}}", rust_i18n::t!("{body}"))'

    new_text = PATTERN_STATUS.sub(replace_status, text)
    new_text = PATTERN_SETTER.sub(replace_setter, new_text)
    new_text = PATTERN_ANYHOW.sub(replace_anyhow, new_text)

    if replacements > 0 and new_text != text and not dry_run:
        path.write_text(new_text, encoding="utf-8")
    return replacements, new_keys


def main():
    dry = "--dry-run" in sys.argv
    total = 0
    files_changed = 0
    all_keys: set[str] = set()
    for rs in SRC.rglob("*.rs"):
        n, keys = migrate_file(rs, dry)
        if n > 0:
            files_changed += 1
            total += n
            all_keys |= keys
            rel = rs.relative_to(REPO).as_posix()
            print(f"  {n:>3}  {rel}")
    label = "would migrate" if dry else "migrated"
    print(f"\n{label} {total} string(s) across {files_changed} file(s)")

    if all_keys:
        print(f"\n{len(all_keys)} unique key(s) — add these to "
              f"crates/adapter-gui/locales/*.yml if not already present:")
        for k in sorted(all_keys):
            print(f'  "{k}": "{k}"')


if __name__ == "__main__":
    main()
