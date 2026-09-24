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
import time
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


def test_push_publishes_the_vendor_commit_on_the_branch(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    origin = tmp_path / "origin.git"
    git(tmp_path, "init", "-q", "--bare", str(origin))
    git(repo, "remote", "add", "origin", str(origin))
    git(repo, "push", "-q", "origin", "bug/issue-974")
    assert run(repo, "update", upstream_url=str(up)).returncode == 0

    result = run(repo, "push", upstream_url=str(up))

    assert result.returncode == 0, result.stderr
    assert git(origin, "rev-parse", "bug/issue-974") == git(repo, "rev-parse", "HEAD")
    assert "v0.5.4" in git(origin, "log", "-1", "--format=%s", "bug/issue-974")


def test_a_rejected_push_keeps_the_local_commit_and_never_fails(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    git(repo, "remote", "add", "origin", str(tmp_path / "no-such-remote.git"))
    assert run(repo, "update", upstream_url=str(up)).returncode == 0

    result = run(repo, "push", upstream_url=str(up))

    assert result.returncode == 0, "a push that fails must not break the build"
    assert "warning" in (result.stdout + result.stderr).lower()
    assert "v0.5.4" in git(repo, "log", "-1", "--format=%s")


def test_check_names_a_newer_release_and_changes_nothing(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    assert run(repo, "update", upstream_url=str(up)).returncode == 0
    git(up, "commit", "-q", "--allow-empty", "-m", "next")
    git(up, "tag", "v0.5.5")
    head = git(repo, "rev-parse", "HEAD")

    result = run(repo, "check", upstream_url=str(up))

    assert result.returncode == 0, result.stderr
    assert "v0.5.5 is available" in result.stdout
    assert git(repo, "rev-parse", "HEAD") == head, "check never commits"
    assert git(repo, "status", "--porcelain") == "", "check never writes"
    assert archive_body(repo) == "old"


def test_check_says_nothing_when_the_vendor_is_current(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    assert run(repo, "update", upstream_url=str(up)).returncode == 0

    result = run(repo, "check", upstream_url=str(up))

    assert result.returncode == 0, result.stderr
    assert "available" not in result.stdout + result.stderr


def test_check_of_an_unreachable_upstream_only_warns(tmp_path):
    repo = openrig(tmp_path)

    result = run(repo, "check", upstream_url=str(tmp_path / "gone"))

    assert result.returncode == 0
    assert "warning" in (result.stdout + result.stderr).lower()


def test_a_failed_commit_puts_the_archive_and_lock_back(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    assert run(repo, "update", upstream_url=str(up)).returncode == 0
    (up / "NAM" / "dsp.cpp").write_text("new")
    git(up, "commit", "-q", "-am", "v0.5.5")
    git(up, "tag", "v0.5.5")
    head = git(repo, "rev-parse", "HEAD")
    # Another git process (the IDE's refresh, a pull) holds the index.
    (repo / ".git" / "index.lock").write_text("")

    result = run(repo, "update", upstream_url=str(up))

    (repo / ".git" / "index.lock").unlink()
    assert result.returncode == 0, "a failed commit must not break the build"
    assert "warning" in (result.stdout + result.stderr).lower()
    assert git(repo, "rev-parse", "HEAD") == head
    assert git(repo, "status", "--porcelain") == "", "archive and lock are back"
    assert archive_body(repo) == "old"


def test_an_upstream_that_hangs_times_out_and_keeps_the_vendor(tmp_path):
    repo = openrig(tmp_path)
    env_hang = {
        # `ext::` runs a command as the transport: this one never answers.
        "GIT_CONFIG_COUNT": "1",
        "GIT_CONFIG_KEY_0": "protocol.ext.allow",
        "GIT_CONFIG_VALUE_0": "always",
        "NAM_VENDOR_TIMEOUT": "2",
    }
    started = time.monotonic()

    result = subprocess.run(
        [sys.executable, str(SCRIPT), "--repo", str(repo), "update"],
        capture_output=True,
        text=True,
        env={**os.environ, **env_hang, "NAM_VENDOR_UPSTREAM": "ext::sleep 30"},
        timeout=25,
    )

    assert result.returncode == 0, result.stderr
    assert "warning" in (result.stdout + result.stderr).lower()
    assert time.monotonic() - started < 20, "the check gave up on time"


def test_a_failed_first_vendoring_leaves_no_archive_or_lock(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    hook = repo / ".git" / "hooks" / "pre-commit"
    hook.write_text("#!/bin/sh\nexit 1\n")
    hook.chmod(0o755)

    result = run(repo, "update", upstream_url=str(up))

    assert result.returncode == 0
    assert "warning" in (result.stdout + result.stderr).lower()
    assert not (repo / "deps" / "NeuralAmpModelerCore.tar.gz").exists()
    assert not (repo / "deps" / "NeuralAmpModelerCore.lock").exists()
    assert git(repo, "status", "--porcelain") == "", "nothing left staged"


def test_push_from_a_detached_head_only_warns(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    origin = tmp_path / "origin.git"
    git(tmp_path, "init", "-q", "--bare", str(origin))
    git(repo, "remote", "add", "origin", str(origin))
    assert run(repo, "update", upstream_url=str(up)).returncode == 0
    git(repo, "checkout", "-q", "--detach")

    result = run(repo, "push", upstream_url=str(up))

    assert result.returncode == 0
    assert "detached" in result.stderr
    assert git(origin, "for-each-ref") == "", "nothing was pushed"


def test_a_refused_push_is_an_annotation_in_github_actions(tmp_path):
    up = upstream(tmp_path, {"v0.5.4": "old"})
    repo = openrig(tmp_path)
    git(repo, "remote", "add", "origin", str(tmp_path / "no-such-remote.git"))
    assert run(repo, "update", upstream_url=str(up)).returncode == 0

    result = subprocess.run(
        [sys.executable, str(SCRIPT), "--repo", str(repo), "push"],
        capture_output=True,
        text=True,
        env={**os.environ, "GITHUB_ACTIONS": "true"},
    )

    assert result.returncode == 0
    assert "::warning::" in result.stdout + result.stderr, "the job must show it"
