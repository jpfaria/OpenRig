#!/usr/bin/env python3
"""Keep the vendored NeuralAmpModelerCore archive on the newest upstream release (#974).

The build never clones NeuralAmpModelerCore: `crates/nam/build.rs` unpacks
`deps/NeuralAmpModelerCore.tar.gz` (Git LFS), with its submodules (AudioDSPTools,
both Eigens) inside. This script is the only thing that talks to upstream:

    python3 scripts/nam_vendor.py check    # is there a newer release? (local builds)
    python3 scripts/nam_vendor.py update   # vendor it and commit (CI, develop)
    python3 scripts/nam_vendor.py push     # publish that commit (CI, after the tests)

`update` asks upstream for its newest `vX.Y.Z` tag. When that is newer than the
one in `deps/NeuralAmpModelerCore.lock`, it clones the tag with every submodule,
packs it reproducibly, rewrites the lock and commits both files on the current
branch (only those two paths — anything else staged stays staged). If the commit
fails, both files are put back. `check` only names the newer release. When
upstream (or any submodule host) cannot be reached or does not answer in time,
or the push is refused, every command prints a warning and exits 0: the vendored
copy keeps building.

Env: NAM_VENDOR_UPSTREAM overrides the upstream URL (tests use a local repo);
NAM_VENDOR_TIMEOUT caps each network step, in seconds (default 300).
"""
import argparse
import gzip
import hashlib
import os
import re
import shutil
import signal
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


def warn(message):
    """A warning on stderr, and an annotation the job shows in GitHub Actions."""
    print(f"warning: {message}", file=sys.stderr)
    if os.environ.get("GITHUB_ACTIONS") == "true":
        print(f"::warning::{message}")


def git(*args, cwd=None, check=True):
    return subprocess.run(
        ["git", *args], cwd=cwd, check=check, capture_output=True, text=True
    )


def _kill_tree(proc):
    """Kill git and every helper it started (remote-https, submodule clones)."""
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/T", "/F", "/PID", str(proc.pid)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    else:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def net_git(*args):
    """A git command that talks to a remote: killed, with every helper it
    spawned, once NAM_VENDOR_TIMEOUT runs out — or when this script is
    interrupted, so nothing is left holding the connection."""
    timeout = float(os.environ.get("NAM_VENDOR_TIMEOUT", "300"))
    proc = subprocess.Popen(
        ["git", *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=os.name != "nt",
        creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0),
    )
    try:
        out, err = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        _kill_tree(proc)
        try:
            proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            pass
        return subprocess.CompletedProcess(proc.args, 124, "", f"no answer in {timeout:g}s")
    except BaseException:
        _kill_tree(proc)
        raise
    return subprocess.CompletedProcess(proc.args, proc.returncode, out, err)


def version(tag):
    match = RELEASE.match(tag or "")
    return tuple(int(part) for part in match.groups()) if match else None


def newest_release(url):
    listed = net_git("ls-remote", "--tags", "--refs", url)
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
    # No line-ending or mode rewriting from the local git config: the archive
    # must hold upstream's bytes on every machine.
    steps = [
        ["-c", "core.autocrlf=false", "-c", "core.eol=lf",
         "clone", "--quiet", "--depth", "1", "--branch", tag, url, str(into)],
        ["-c", "core.autocrlf=false", "-c", "core.eol=lf", "-C", str(into),
         "submodule", "update", "--quiet", "--init", "--recursive", "--depth", "1"],
    ]
    for step in steps:
        done = net_git(*step)
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


def check(repo, url):
    """Name a newer upstream release, if any; never writes anything."""
    current = locked_tag(repo / LOCK)
    try:
        newest = newest_release(url)
    except Unreachable as err:
        warn(f"could not check {NAME} upstream ({err}).")
        return 0
    if version(current) is None or version(current) < version(newest):
        print(
            f"{NAME} {newest} is available (vendored: {current or 'none'}); "
            "it reaches develop through CI."
        )
    return 0


def publish(repo):
    """Push the vendor commit to the branch it was made on; never fails."""
    branch = git("-C", str(repo), "symbolic-ref", "--quiet", "--short", "HEAD", check=False)
    if branch.returncode != 0:
        warn("detached HEAD; the vendor commit was not pushed.")
        return
    name = branch.stdout.strip()
    pushed = net_git("-C", str(repo), "push", "--quiet", "origin", f"HEAD:refs/heads/{name}")
    if pushed.returncode != 0:
        warn(
            f"could not push the vendor commit to {name} "
            f"({pushed.stderr.strip() or 'push failed'}); it stays local."
        )
    else:
        print(f"pushed to {name}.")


def update(repo, url):
    lock_path = repo / LOCK
    current = locked_tag(lock_path)
    with tempfile.TemporaryDirectory() as scratch:
        scratch = Path(scratch)
        try:
            newest = newest_release(url)
            if version(current) is not None and version(current) >= version(newest):
                print(f"{NAME} {current} is the newest release; nothing to update.")
                return 0
            tree = scratch / NAME
            lock_lines = fetch(url, newest, tree)
            packed = scratch / "archive.tar.gz"
            pack(tree, packed)
        except Unreachable as err:
            warn(
                f"could not check {NAME} upstream ({err}); "
                f"building with the vendored {current or 'archive'}."
            )
            return 0
        digest = hashlib.sha256(packed.read_bytes()).hexdigest()
        lock_text = "\n".join([*lock_lines, f"archive sha256: {digest}"]) + "\n"
        committed = _commit_vendor(repo, scratch, packed, lock_text, newest)
    if committed:
        print(f"{NAME} {current or '(none)'} -> {newest}: vendored and committed.")
    return 0


def _commit_vendor(repo, scratch, packed, lock_text, tag):
    """Swap the new archive and lock in and commit only them. On any failure
    (the index is locked, a hook refuses, an interrupt) both files are put back
    as they were and nothing stays staged."""
    paths = [repo / ARCHIVE, repo / LOCK]
    head = git("-C", str(repo), "rev-parse", "--verify", "--quiet", "HEAD", check=False).stdout
    saved = []
    for path in paths:
        backup = scratch / f"saved-{path.name}"
        if path.exists():
            backup.write_bytes(path.read_bytes())
            saved.append((path, backup))
        else:
            saved.append((path, None))
    try:
        (repo / ARCHIVE).parent.mkdir(parents=True, exist_ok=True)
        shutil.move(packed, repo / ARCHIVE)
        (repo / LOCK).write_text(lock_text)
        git("-C", str(repo), "add", "--", ARCHIVE, LOCK)
        git("-C", str(repo), "commit", "--quiet", "-m",
            f"chore(nam): vendor {NAME} {tag}", "--", ARCHIVE, LOCK)
        return True
    except BaseException as err:
        moved = git("-C", str(repo), "rev-parse", "--verify", "--quiet", "HEAD", check=False).stdout
        if moved != head:
            # Interrupted after the commit landed: the files ARE the commit.
            raise
        git("-C", str(repo), "reset", "--quiet", "--", ARCHIVE, LOCK, check=False)
        for path, backup in saved:
            if backup is None:
                path.unlink(missing_ok=True)
            else:
                path.write_bytes(backup.read_bytes())
        if not isinstance(err, subprocess.CalledProcessError):
            raise
        reason = (err.stderr or "").strip() or str(err)
        warn(f"could not commit the {NAME} {tag} update ({reason}); archive and lock put back.")
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("command", choices=["check", "update", "push"])
    args = parser.parse_args()
    repo = args.repo.resolve()
    url = os.environ.get("NAM_VENDOR_UPSTREAM", UPSTREAM)
    if args.command == "check":
        return check(repo, url)
    if args.command == "push":
        publish(repo)
        return 0
    return update(repo, url)


if __name__ == "__main__":
    sys.exit(main())
