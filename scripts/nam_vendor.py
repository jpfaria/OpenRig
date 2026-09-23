#!/usr/bin/env python3
"""Keep the vendored NeuralAmpModelerCore archive on the newest upstream release (#974).

The build never clones NeuralAmpModelerCore: `crates/nam/build.rs` unpacks
`deps/NeuralAmpModelerCore.tar.gz` (Git LFS), with its submodules (AudioDSPTools,
both Eigens) inside. This script is the only thing that talks to upstream:

    python3 scripts/nam_vendor.py update

It asks upstream for its newest `vX.Y.Z` tag. When that is newer than the one in
`deps/NeuralAmpModelerCore.lock`, it clones the tag with every submodule, packs
it reproducibly, rewrites the lock and commits both files on the current branch
(only those two paths — anything else staged stays staged). When upstream (or
any submodule host) cannot be reached it prints a warning and exits 0: the
vendored copy keeps building.

Env: NAM_VENDOR_UPSTREAM overrides the upstream URL (tests use a local repo).
"""
import argparse
import gzip
import hashlib
import os
import re
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

UPSTREAM = "https://github.com/sdatkinson/NeuralAmpModelerCore.git"
NAME = "NeuralAmpModelerCore"
ARCHIVE = f"deps/{NAME}.tar.gz"
LOCK = f"deps/{NAME}.lock"
RELEASE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")


class Unreachable(Exception):
    pass


def git(*args, cwd=None, check=True):
    return subprocess.run(
        ["git", *args], cwd=cwd, check=check, capture_output=True, text=True
    )


def version(tag):
    match = RELEASE.match(tag or "")
    return tuple(int(part) for part in match.groups()) if match else None


def newest_release(url):
    listed = git("ls-remote", "--tags", "--refs", url, check=False)
    if listed.returncode != 0:
        raise Unreachable(listed.stderr.strip() or f"git ls-remote {url} failed")
    tags = [line.split("refs/tags/", 1)[1] for line in listed.stdout.splitlines() if "refs/tags/" in line]
    releases = [tag for tag in tags if version(tag)]
    if not releases:
        raise Unreachable(f"{url} lists no vX.Y.Z tag")
    return max(releases, key=version)


def locked_tag(lock_path):
    if not lock_path.exists():
        return None
    for line in lock_path.read_text().splitlines():
        if line.startswith("tag: "):
            return line[len("tag: "):].strip()
    return None


def fetch(url, tag, into):
    """Clone `tag` with every submodule at its pinned commit; return the lock lines."""
    steps = [
        ["clone", "--quiet", "--depth", "1", "--branch", tag, url, str(into)],
        ["-C", str(into), "submodule", "update", "--quiet", "--init", "--recursive", "--depth", "1"],
    ]
    for step in steps:
        done = git(*step, check=False)
        if done.returncode != 0:
            raise Unreachable(done.stderr.strip() or f"git {' '.join(step)} failed")
    lines = [f"tag: {tag}", f"commit: {git('-C', str(into), 'rev-parse', 'HEAD').stdout.strip()}"]
    status = git("-C", str(into), "submodule", "status", "--recursive").stdout
    for entry in sorted(status.splitlines(), key=lambda line: line.split()[1]):
        sha, path = entry.strip().lstrip("+-U").split()[:2]
        lines.append(f"submodule {path}: {sha}")
    return lines


def _normalized(info):
    info.mtime = 0
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    if info.isdir() or info.mode & 0o111:
        info.mode = 0o755
    else:
        info.mode = 0o644
    return info


def pack(tree, archive):
    """Byte-for-byte reproducible tar.gz of `tree` under a top-level NAME/ directory."""
    entries = []
    for root, dirs, files in os.walk(tree):
        dirs[:] = sorted(d for d in dirs if d != ".git")
        for name in sorted(files):
            if name != ".git":
                entries.append(Path(root) / name)
        entries.extend(Path(root) / d for d in dirs)
    entries.sort(key=lambda path: path.relative_to(tree).as_posix())
    archive.parent.mkdir(parents=True, exist_ok=True)
    with open(archive, "wb") as raw, gzip.GzipFile(
        filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9
    ) as zipped, tarfile.open(fileobj=zipped, mode="w", format=tarfile.GNU_FORMAT) as tar:
        tar.add(str(tree), arcname=NAME, recursive=False, filter=_normalized)
        for path in entries:
            arcname = f"{NAME}/{path.relative_to(tree).as_posix()}"
            tar.add(str(path), arcname=arcname, recursive=False, filter=_normalized)


def update(repo, url):
    lock_path = repo / LOCK
    current = locked_tag(lock_path)
    try:
        newest = newest_release(url)
        if version(current) is not None and version(current) >= version(newest):
            print(f"{NAME} {current} is the newest release; nothing to update.")
            return 0
        with tempfile.TemporaryDirectory() as scratch:
            tree = Path(scratch) / NAME
            lock_lines = fetch(url, newest, tree)
            pack(tree, repo / ARCHIVE)
    except Unreachable as err:
        print(
            f"warning: could not check {NAME} upstream ({err}); "
            f"building with the vendored {current or 'archive'}.",
            file=sys.stderr,
        )
        return 0
    digest = hashlib.sha256((repo / ARCHIVE).read_bytes()).hexdigest()
    lock_path.write_text("\n".join([*lock_lines, f"archive sha256: {digest}"]) + "\n")
    git("-C", str(repo), "add", "--", ARCHIVE, LOCK)
    git("-C", str(repo), "commit", "--quiet", "-m",
        f"chore(nam): vendor {NAME} {newest}", "--", ARCHIVE, LOCK)
    print(f"{NAME} {current or '(none)'} -> {newest}: vendored and committed.")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("command", choices=["update"])
    args = parser.parse_args()
    url = os.environ.get("NAM_VENDOR_UPSTREAM", UPSTREAM)
    return update(args.repo.resolve(), url)


if __name__ == "__main__":
    sys.exit(main())
