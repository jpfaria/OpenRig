#!/usr/bin/env python3
"""Score a branch diff the way codecov/patch does (#913).

Reads a unified diff on stdin and an lcov report as argv[1], and reports what
fraction of the INSTRUMENTED lines the branch adds are executed by the test
suite. Files listed under `ignore:` in codecov.yml are dropped first, so the
number here matches the check that runs on the PR.

Lines the diff adds that carry no instrumentation (comments, blank lines, most
declarations) are not counted either way — same as codecov.
"""
import os
import re
import sys
from fnmatch import fnmatch

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_TARGET = 65.0


def read_ignores():
    """The `ignore:` globs from codecov.yml. Hand-parsed: the file is a flat
    list of quoted globs and pulling in PyYAML for it would be a build dep."""
    path = os.path.join(ROOT, "codecov.yml")
    if not os.path.exists(path):
        return [], DEFAULT_TARGET
    globs, target, in_ignore = [], DEFAULT_TARGET, False
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
                in_ignore = False
        m = re.match(r"target:\s*([0-9.]+)%?", stripped)
        if m:
            target = float(m.group(1))
    return globs, target


def ignored(path, globs):
    return any(fnmatch(path, g) for g in globs)


def parse_lcov(path):
    """file path -> {line number: hit count} for every instrumented line."""
    hits, current = {}, None
    with open(path) as fh:
        for line in fh:
            if line.startswith("SF:"):
                current = os.path.relpath(line[3:].strip(), ROOT)
                hits.setdefault(current, {})
            elif line.startswith("DA:") and current is not None:
                num, count = line[3:].strip().split(",")[:2]
                # A line can appear more than once (generics, macros); it is
                # covered when ANY instantiation ran.
                prev = hits[current].get(int(num), 0)
                hits[current][int(num)] = max(prev, int(count))
            elif line.startswith("end_of_record"):
                current = None
    return hits


def parse_diff(stream):
    """file path -> set of line numbers the branch adds or changes."""
    added, path = {}, None
    new_line = 0
    hunk = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")
    for line in stream:
        if line.startswith("+++ "):
            target = line[4:].strip()
            path = None if target == "/dev/null" else target[2:]
            continue
        m = hunk.match(line)
        if m:
            new_line = int(m.group(1))
            continue
        if path is None:
            continue
        if line.startswith("+"):
            added.setdefault(path, set()).add(new_line)
            new_line += 1
        elif not line.startswith("-") and not line.startswith("\\"):
            new_line += 1
    return added


def main():
    if len(sys.argv) < 2:
        print("usage: patch_coverage.py <lcov.info>  (diff on stdin)", file=sys.stderr)
        return 2
    globs, target = read_ignores()
    target = float(os.environ.get("PATCH_TARGET", target))
    hits = parse_lcov(sys.argv[1])
    added = parse_diff(sys.stdin)

    rows, total, covered = [], 0, 0
    for path in sorted(added):
        if ignored(path, globs) or path not in hits:
            continue
        instrumented = [n for n in sorted(added[path]) if n in hits[path]]
        if not instrumented:
            continue
        miss = [n for n in instrumented if hits[path][n] == 0]
        total += len(instrumented)
        covered += len(instrumented) - len(miss)
        if miss:
            rows.append((len(miss), len(instrumented), path, miss))

    if total == 0:
        print("patch coverage: no instrumented lines added — nothing to score")
        return 0

    pct = 100.0 * covered / total
    rows.sort(reverse=True)
    if rows:
        print(f"{'uncovered':>9}  {'added':>5}  file")
        for miss, instrumented, path, lines in rows:
            preview = ",".join(str(n) for n in lines[:8])
            if len(lines) > 8:
                preview += ",…"
            print(f"{miss:>9}  {instrumented:>5}  {path}  ({preview})")
        print()
    print(f"patch coverage: {pct:.2f}% ({covered}/{total} added lines) vs target {target:.2f}%")
    return 0 if pct >= target else 1


if __name__ == "__main__":
    sys.exit(main())
