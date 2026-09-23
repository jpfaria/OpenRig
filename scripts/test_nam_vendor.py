#!/usr/bin/env python3
"""Tests for nam_vendor.py — the vendored NeuralAmpModelerCore archive (#974).

Run: pytest scripts/test_nam_vendor.py

Every test builds its own throwaway git repositories in a temp dir: a fake
"upstream" NeuralAmpModelerCore with release tags and a fake OpenRig checkout.
Nothing touches the network or the real repository.
"""
import os
import subprocess
import sys
import tarfile
from pathlib import Path

SCRIPT = Path(__file__).parent / "nam_vendor.py"


def git(cwd: Path, *args: str) -> str:
    env = {
        **os.environ,
        "GIT_AUTHOR_NAME": "t",
        "GIT_AUTHOR_EMAIL": "t@t",
        "GIT_COMMITTER_NAME": "t",
        "GIT_COMMITTER_EMAIL": "t@t",
    }
    return subprocess.run(
        ["git", "-C", str(cwd), *args], check=True, capture_output=True, text=True, env=env
    ).stdout.strip()


def upstream(tmp: Path, tags: dict) -> Path:
    """A fake upstream repo: one commit per tag, NAM/dsp.cpp holding the tag."""
    repo = tmp / "upstream"
    repo.mkdir()
    git(repo, "init", "-q", "-b", "main")
    for tag, body in tags.items():
        (repo / "NAM").mkdir(exist_ok=True)
        (repo / "NAM" / "dsp.cpp").write_text(body)
        git(repo, "add", "-A")
        git(repo, "commit", "-q", "-m", tag)
        git(repo, "tag", tag)
    return repo


def openrig(tmp: Path) -> Path:
    repo = tmp / "openrig"
    (repo / "deps").mkdir(parents=True)
    git(repo, "init", "-q", "-b", "bug/issue-974")
    (repo / "README").write_text("x")
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "-m", "init")
    return repo


def run(repo: Path, *args: str, upstream_url: str) -> subprocess.CompletedProcess:
    env = {
        **os.environ,
        "NAM_VENDOR_UPSTREAM": upstream_url,
        "GIT_AUTHOR_NAME": "t",
        "GIT_AUTHOR_EMAIL": "t@t",
        "GIT_COMMITTER_NAME": "t",
        "GIT_COMMITTER_EMAIL": "t@t",
    }
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--repo", str(repo), *args],
        capture_output=True,
        text=True,
        env=env,
    )


def archive_body(repo: Path) -> str:
    with tarfile.open(repo / "deps" / "NeuralAmpModelerCore.tar.gz") as tar:
        member = tar.extractfile("NeuralAmpModelerCore/NAM/dsp.cpp")
        return member.read().decode()


def test_update_vendors_the_newest_release_and_commits_it_on_the_branch(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old", "v0.5.5": "new"})
    repo = openrig(tmp_path)

    result = run(repo, "update", upstream_url=str(up))

    assert result.returncode == 0, result.stderr
    assert archive_body(repo) == "new"
    lock = (repo / "deps" / "NeuralAmpModelerCore.lock").read_text()
    assert lock.startswith("tag: v0.5.5\n"), lock
    assert git(repo, "rev-parse", "--abbrev-ref", "HEAD") == "bug/issue-974"
    assert "v0.5.5" in git(repo, "log", "-1", "--format=%s")
    assert git(repo, "status", "--porcelain") == "", "the vendor update is committed"


def test_update_does_nothing_when_the_vendor_is_current(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    assert run(repo, "update", upstream_url=str(up)).returncode == 0
    commits = git(repo, "rev-list", "--count", "HEAD")

    result = run(repo, "update", upstream_url=str(up))

    assert result.returncode == 0, result.stderr
    assert git(repo, "rev-list", "--count", "HEAD") == commits, "no new commit"


def test_an_unreachable_upstream_keeps_the_vendor_and_never_fails(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    assert run(repo, "update", upstream_url=str(up)).returncode == 0
    commits = git(repo, "rev-list", "--count", "HEAD")

    result = run(repo, "update", upstream_url=str(tmp_path / "gone"))

    assert result.returncode == 0, "an offline check must not break the build"
    assert "warning" in (result.stdout + result.stderr).lower()
    assert archive_body(repo) == "old"
    assert git(repo, "rev-list", "--count", "HEAD") == commits


def test_the_commit_holds_only_the_vendor_files(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    (repo / "README").write_text("the owner's unfinished edit")
    git(repo, "add", "README")

    assert run(repo, "update", upstream_url=str(up)).returncode == 0

    changed = git(repo, "show", "--name-only", "--format=", "HEAD").splitlines()
    assert sorted(changed) == [
        "deps/NeuralAmpModelerCore.lock",
        "deps/NeuralAmpModelerCore.tar.gz",
    ]
    assert "README" in git(repo, "diff", "--cached", "--name-only"), "left staged, untouched"


def test_packing_is_reproducible(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    first = openrig(tmp_path / "a")
    second = openrig(tmp_path / "b")
    assert run(first, "update", upstream_url=str(up)).returncode == 0
    assert run(second, "update", upstream_url=str(up)).returncode == 0

    archive = "deps/NeuralAmpModelerCore.tar.gz"
    assert (first / archive).read_bytes() == (second / archive).read_bytes()
