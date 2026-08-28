#!/usr/bin/env python3
"""Patch coverage, the way codecov computes it.

Scores an lcov report against the lines the branch ADDS relative to a base ref,
so the number matches what `codecov/patch` reports on the PR — before the push,
not after CI says no.

usage: patch-coverage.py <lcov.info> <base-ref> [--files]
"""
import collections
import os
import re
import subprocess
import sys
from fnmatch import fnmatch


def repo_root():
    return subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True).stdout.strip()


def added_lines(base):
    """{path: {line numbers this branch adds or changes vs base}}"""
    diff = subprocess.run(
        ["git", "diff", "--unified=0", f"{base}...HEAD"],
        capture_output=True, text=True, check=True).stdout
    out = collections.defaultdict(set)
    path = None
    for line in diff.splitlines():
        if line.startswith("+++ b/"):
            path = line[6:]
        elif line.startswith("@@") and path:
            m = re.search(r"\+(\d+)(?:,(\d+))?", line)
            if m:
                start, count = int(m.group(1)), int(m.group(2) or 1)
                out[path].update(range(start, start + count))
    return out


def ignored_globs(root):
    """The `ignore:` globs from codecov.yml, so the number here matches the
    one codecov reports rather than counting files the PR check skips.

    Hand-parsed: the key is a flat list of quoted globs and pulling in PyYAML
    for it would be a build dependency.
    """
    path = os.path.join(root, "codecov.yml")
    if not os.path.exists(path):
        return []
    globs, in_ignore = [], False
    for line in open(path):
        stripped = line.strip()
        if stripped.startswith("ignore:"):
            in_ignore = True
            continue
        if in_ignore:
            if stripped.startswith("- "):
                globs.append(stripped[2:].strip().strip("\"'"))
                continue
            if stripped and not stripped.startswith("#"):
                break
    return globs


def lcov_hits(lcov, root):
    """{path: {line: hit count}} with repo-relative paths"""
    out = collections.defaultdict(dict)
    cur = None
    for line in open(lcov):
        if line.startswith("SF:"):
            cur = line[3:].strip()
            if cur.startswith(root + "/"):
                cur = cur[len(root) + 1:]
        elif line.startswith("DA:") and cur:
            n, h = line[3:].strip().split(",")[:2]
            out[cur][int(n)] = int(h)
    return out


def main():
    if len(sys.argv) < 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    lcov, base = sys.argv[1], sys.argv[2]
    show_files = "--files" in sys.argv

    root = repo_root()
    hits = lcov_hits(lcov, root)
    skip = ignored_globs(root)
    rows, total_hit, total = [], 0, 0
    for path, lines in added_lines(base).items():
        if any(fnmatch(path, glob) for glob in skip):
            continue  # codecov ignores it, so neither does this number
        covered = hits.get(path)
        if not covered:
            continue  # not instrumented: not Rust, or no coverage data
        h = sum(1 for n in lines if covered.get(n, -1) > 0)
        m = sum(1 for n in lines if covered.get(n, -1) == 0)
        if h + m:
            rows.append((m, h, path))
            total_hit += h
            total += h + m

    pct = 100 * total_hit / total if total else 100.0
    print(f"patch: {total_hit}/{total} lines hit = {pct:.2f}%")
    if show_files:
        for m, h, p in sorted(rows, reverse=True)[:30]:
            if m:
                print(f"{m:5d} missing {h:5d} hit  {p}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
