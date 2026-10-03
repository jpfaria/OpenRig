#!/usr/bin/env python3
"""Responsibility: lists the workspace packages a branch's changed files can affect.

Usage: git diff --name-only BASE...HEAD | pre_pr_gate_packages.py METADATA_JSON [--lib-only]

METADATA_JSON is the output of `cargo metadata --format-version 1`. Prints one
package name per line: every package that owns a changed file plus every
workspace package that depends on it, directly or not (dev-dependencies
included). Prints `ALL` when a changed file can affect any package (the root
manifest, the lockfile, cargo or nextest config, a file outside every crate)
and nothing when only docs, the site, CI or scripts changed. `--lib-only`
keeps only packages with a library target, the ones `cargo test --doc` takes.
"""
import json
import sys
from pathlib import PurePosixPath

NO_TESTS = ("docs/", "site/", ".github/", ".claude/", "scripts/", "tools/", "platform/")
NO_TESTS_FILES = (".gitignore", ".gitattributes", "LICENSE")


def main() -> None:
    meta = json.load(open(sys.argv[1]))
    lib_only = "--lib-only" in sys.argv[2:]
    root = PurePosixPath(meta["workspace_root"])
    members = set(meta["workspace_members"])
    by_id = {p["id"]: p for p in meta["packages"] if p["id"] in members}
    dirs = {
        str(PurePosixPath(p["manifest_path"]).parent.relative_to(root)): pid
        for pid, p in by_id.items()
    }

    owners: set[str] = set()
    for line in sys.stdin:
        path = line.strip()
        if not path or path.endswith(".md"):
            continue
        owner = max(
            (d for d in dirs if path.startswith(d + "/")), key=len, default=None
        )
        if owner is not None:
            owners.add(dirs[owner])
        elif path not in NO_TESTS_FILES and not path.startswith(NO_TESTS):
            print("ALL")
            return

    dependents: dict[str, set[str]] = {}
    for node in meta["resolve"]["nodes"]:
        if node["id"] not in members:
            continue
        for dep in node["deps"]:
            dependents.setdefault(dep["pkg"], set()).add(node["id"])

    affected, todo = set(owners), list(owners)
    while todo:
        for user in dependents.get(todo.pop(), ()):
            if user not in affected:
                affected.add(user)
                todo.append(user)

    for pid in sorted(affected, key=lambda i: by_id[i]["name"]):
        pkg = by_id[pid]
        if lib_only and not any("lib" in t["kind"] for t in pkg["targets"]):
            continue
        print(pkg["name"])


if __name__ == "__main__":
    main()
