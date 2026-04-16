#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import unicodedata
from collections import Counter, defaultdict
from pathlib import Path


SUPPORTED_EXTENSIONS = {".nam"}

SOURCE_ALIASES = {
    "am3_mvave_captures": "mvave",
    "capturas_jmp_1": "jmp_1",
    "capturasmvavw2_0": "mvave",
    "ir": "legacy_ir",
    "ircelestion": "celestion",
    "nam_captures": "mvave",
    "nampremiumcaptures": "premium",
    "suhrirs": "suhr",
    "tankgpremiumcaptures": "premium",
    "tone3000": "tone3000",
}

def normalize_component(value: str) -> str:
    value = unicodedata.normalize("NFKD", value)
    value = value.encode("ascii", "ignore").decode("ascii")
    value = value.replace("&", " and ")
    value = re.sub(r"[^A-Za-z0-9]+", "_", value)
    value = re.sub(r"_+", "_", value).strip("_").lower()
    return value or "unnamed"


def source_id(top_level: str) -> str:
    normalized = normalize_component(top_level)
    return SOURCE_ALIASES.get(normalized, normalized)


def unique_destination(path: Path, rel_key: str) -> Path:
    if not path.exists():
        return path

    suffix = hashlib.sha1(rel_key.encode("utf-8")).hexdigest()[:8]
    return path.with_name(f"{path.stem}_{suffix}{path.suffix}")


def filename_lower(path: Path) -> str:
    return path.name.lower()


def is_head_capture(path: Path) -> bool:
    name = filename_lower(path)
    return "(no cab)" in name or "(head)" in name or name.endswith(" head.nam") or name.endswith(" head.am3data")


def is_full_rig(path: Path) -> bool:
    return "full rig" in filename_lower(path)


def relative_parts_after(parts: list[str], marker: str) -> list[str]:
    normalized = [normalize_component(part) for part in parts]
    try:
        index = normalized.index(normalize_component(marker))
    except ValueError:
        return parts[1:-1]
    return parts[index + 1 : -1]


def classify_nam_capture(relative_path: Path) -> tuple[Path, str]:
    parts = list(relative_path.parts)
    top = parts[0]
    src = source_id(top)
    remainder = parts[1:-1]
    normalized_remainder = [normalize_component(part) for part in remainder]

    if "pedals" in normalized_remainder:
        payload = relative_parts_after(parts[:-1], "pedals")
        return Path("nam") / "pedals" / src / Path(*map(normalize_component, payload)), "pedals"

    if "full_rig" in normalized_remainder or is_full_rig(relative_path):
        return (
            Path("nam") / "full_rigs" / src / Path(*map(normalize_component, remainder)),
            "full_rigs",
        )

    if is_head_capture(relative_path) or "head" in normalized_remainder:
        return (
            Path("nam") / "amps" / "heads" / src / Path(*map(normalize_component, remainder)),
            "amps/heads",
        )

    if src == "premium":
        return (
            Path("nam") / "amps" / "complete" / src / Path(*map(normalize_component, remainder)),
            "amps/combo",
        )

    return (
        Path("nam") / "amps" / "complete" / src / Path(*map(normalize_component, remainder)),
        "amps/combo",
    )


def destination_for(source_root: Path, source_file: Path) -> tuple[Path, str]:
    relative_path = source_file.relative_to(source_root)
    ext = source_file.suffix.lower()
    sanitized_name = normalize_component(source_file.stem) + ext

    if ext == ".nam":
        base_dir, category = classify_nam_capture(relative_path)
    else:
        raise ValueError(f"unsupported extension: {ext}")

    return base_dir / sanitized_name, category


def import_captures(source_root: Path, destination_root: Path) -> dict:
    stats = Counter()
    by_category = Counter()
    by_extension = Counter()
    imported_paths: dict[str, list[str]] = defaultdict(list)

    for source_file in source_root.rglob("*"):
        if not source_file.is_file():
            continue

        ext = source_file.suffix.lower()
        if ext not in SUPPORTED_EXTENSIONS:
            continue

        destination_rel, category = destination_for(source_root, source_file)
        destination_file = unique_destination(destination_root / destination_rel, str(source_file.relative_to(source_root)))
        destination_file.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source_file, destination_file)

        stats["files"] += 1
        stats["bytes"] += source_file.stat().st_size
        by_category[category] += 1
        by_extension[ext] += 1
        imported_paths[category].append(str(destination_file.relative_to(destination_root)))

    summary = {
        "source_root": str(source_root),
        "destination_root": str(destination_root),
        "files": stats["files"],
        "bytes": stats["bytes"],
        "by_extension": dict(sorted(by_extension.items())),
        "by_category": dict(sorted(by_category.items())),
    }

    (destination_root / "import_summary.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    return summary


def main() -> None:
    parser = argparse.ArgumentParser(description="Import and organize capture assets.")
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()

    source_root = args.source.expanduser().resolve()
    destination_root = args.destination.expanduser().resolve()
    destination_root.mkdir(parents=True, exist_ok=True)

    summary = import_captures(source_root, destination_root)
    print(json.dumps(summary, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
