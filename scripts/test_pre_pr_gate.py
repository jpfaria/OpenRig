#!/usr/bin/env python3
"""Tests for the pre-PR gate: scripts/pre-pr-gate.sh and its Claude hook
.claude/hooks/pre-pr-gate-guard.sh.

Run: pytest scripts/test_pre_pr_gate.py

Opening a PR, or pushing to a branch whose PR is open, has to run the same
suite CI runs, here, first. The gate script runs it and stamps the commit it
passed on; the hook denies `gh pr create` and such pushes unless HEAD carries
that stamp. Every test builds a throwaway git repo and a fake `gh` in a temp
dir; nothing touches the network, cargo or the real repository.
"""
import json
import os
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = ROOT / "scripts" / "pre-pr-gate.sh"
GUARD = ROOT / ".claude" / "hooks" / "pre-pr-gate-guard.sh"

GIT_ENV = {
    "GIT_AUTHOR_NAME": "t",
    "GIT_AUTHOR_EMAIL": "t@t",
    "GIT_COMMITTER_NAME": "t",
    "GIT_COMMITTER_EMAIL": "t@t",
}


def git(cwd: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(cwd), *args],
        check=True,
        capture_output=True,
        text=True,
        env={**os.environ, **GIT_ENV},
    ).stdout.strip()


def repo(tmp_path: Path, branch: str = "feature/issue-1") -> Path:
    r = tmp_path / "repo"
    r.mkdir()
    git(r, "init", "-q", "-b", branch)
    (r / "a.txt").write_text("a\n")
    git(r, "add", "a.txt")
    git(r, "commit", "-q", "-m", "a")
    return r


def fake_gh(tmp_path: Path, open_pr_branches: list[str]) -> Path:
    """A `gh` that reports an open PR only for the listed head branches."""
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    gh = bin_dir / "gh"
    branches = " ".join(open_pr_branches)
    gh.write_text(
        "#!/usr/bin/env bash\n"
        "head=''\n"
        'while [ $# -gt 0 ]; do [ "$1" = --head ] && head="$2"; shift; done\n'
        f"for b in {branches}; do\n"
        '  [ "$b" = "$head" ] && { echo 1; exit 0; }\n'
        "done\n"
        "echo 0\n"
    )
    gh.chmod(0o755)
    return bin_dir


def guard(cwd: Path, command: str, bin_dir: Path) -> str | None:
    """Run the hook on a Bash call; the deny reason, or None when allowed."""
    payload = json.dumps(
        {"tool_name": "Bash", "tool_input": {"command": command}, "cwd": str(cwd)}
    )
    out = subprocess.run(
        ["bash", str(GUARD)],
        input=payload,
        capture_output=True,
        text=True,
        check=True,
        env={**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}"},
    ).stdout.strip()
    if not out:
        return None
    decision = json.loads(out)["hookSpecificOutput"]
    return decision["permissionDecisionReason"] if decision["permissionDecision"] == "deny" else None


def gate(cwd: Path, suite: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["bash", str(GATE)],
        cwd=cwd,
        capture_output=True,
        text=True,
        env={**os.environ, "PRE_PR_GATE_SUITE": suite, "PRE_PR_GATE_LOCK": str(cwd.parent / "gate.lock")},
    )


def test_opening_a_pr_without_the_gate_is_denied(tmp_path):
    r = repo(tmp_path)
    reason = guard(r, "gh pr create --base release/v1 --fill", fake_gh(tmp_path, []))
    assert reason and "pre-pr-gate.sh" in reason


def test_opening_a_pr_after_the_gate_passed_on_head_is_allowed(tmp_path):
    r = repo(tmp_path)
    assert gate(r, "true").returncode == 0
    assert guard(r, "gh pr create --fill", fake_gh(tmp_path, [])) is None


def test_a_new_commit_after_the_gate_needs_the_gate_again(tmp_path):
    r = repo(tmp_path)
    assert gate(r, "true").returncode == 0
    (r / "b.txt").write_text("b\n")
    git(r, "add", "b.txt")
    git(r, "commit", "-q", "-m", "b")
    assert guard(r, "gh pr create --fill", fake_gh(tmp_path, [])) is not None


def test_pushing_to_a_branch_with_an_open_pr_needs_the_gate(tmp_path):
    r = repo(tmp_path)
    reason = guard(r, "git push origin feature/issue-1", fake_gh(tmp_path, ["feature/issue-1"]))
    assert reason and "pre-pr-gate.sh" in reason


def test_pushing_to_a_branch_with_no_pr_is_allowed(tmp_path):
    r = repo(tmp_path)
    assert guard(r, "git push origin feature/issue-1", fake_gh(tmp_path, [])) is None


def test_the_push_target_is_read_from_a_cd_prefix(tmp_path):
    r = repo(tmp_path)
    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()
    reason = guard(
        elsewhere,
        f"cd {r} && git add a.txt && git commit -m x && git push",
        fake_gh(tmp_path, ["feature/issue-1"]),
    )
    assert reason is not None


def test_the_push_target_is_read_from_git_dash_c(tmp_path):
    r = repo(tmp_path)
    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()
    reason = guard(elsewhere, f"git -C {r} push", fake_gh(tmp_path, ["feature/issue-1"]))
    assert reason is not None


def test_a_commit_and_push_in_one_command_is_denied_even_after_the_gate(tmp_path):
    r = repo(tmp_path)
    assert gate(r, "true").returncode == 0
    reason = guard(r, "git commit -am x && git push", fake_gh(tmp_path, ["feature/issue-1"]))
    assert reason is not None


def test_commands_that_neither_push_nor_open_a_pr_pass(tmp_path):
    r = repo(tmp_path)
    assert guard(r, "gh pr view 12 && git status", fake_gh(tmp_path, ["feature/issue-1"])) is None


def test_a_failing_suite_stamps_nothing(tmp_path):
    r = repo(tmp_path)
    assert gate(r, "false").returncode != 0
    assert guard(r, "gh pr create --fill", fake_gh(tmp_path, [])) is not None


def test_the_gate_refuses_a_dirty_tree(tmp_path):
    r = repo(tmp_path)
    (r / "a.txt").write_text("changed\n")
    result = gate(r, "true")
    assert result.returncode != 0
    assert "commit" in result.stderr


# --- Lock: one gate at a time on the machine -------------------------------

def test_the_gate_waits_while_another_gate_holds_the_lock(tmp_path):
    r = repo(tmp_path)
    lock = tmp_path / "gate.lock"
    lock.mkdir()
    (lock / "pid").write_text(f"{os.getpid()}\n")
    marker = tmp_path / "ran"
    proc = subprocess.Popen(
        ["bash", str(GATE)],
        cwd=r,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env={**os.environ, "PRE_PR_GATE_SUITE": f"touch {marker}", "PRE_PR_GATE_LOCK": str(lock)},
    )
    time.sleep(2)
    assert proc.poll() is None and not marker.exists()
    (lock / "pid").unlink()
    lock.rmdir()
    assert proc.wait(timeout=10) == 0
    assert marker.exists()
    assert not lock.exists()


def test_a_lock_left_by_a_dead_gate_is_taken_over(tmp_path):
    r = repo(tmp_path)
    lock = tmp_path / "gate.lock"
    lock.mkdir()
    dead = subprocess.Popen(["true"])
    dead.wait()
    (lock / "pid").write_text(f"{dead.pid}\n")
    result = subprocess.run(
        ["bash", str(GATE)],
        cwd=r,
        capture_output=True,
        text=True,
        timeout=10,
        env={**os.environ, "PRE_PR_GATE_SUITE": "true", "PRE_PR_GATE_LOCK": str(lock)},
    )
    assert result.returncode == 0
    assert not lock.exists()


# --- Package selection: test only what the branch can affect ---------------

PACKAGES = ROOT / "scripts" / "pre_pr_gate_packages.py"


def metadata(tmp_path: Path) -> Path:
    """A workspace: core <- engine <- gui, engine <- tool (bin only), plus serde outside."""
    def pkg(name, kinds=("lib",)):
        return {
            "id": f"{name}-id",
            "name": name,
            "manifest_path": f"/w/crates/{name}/Cargo.toml",
            "targets": [{"kind": [k]} for k in kinds],
        }

    meta = {
        "workspace_root": "/w",
        "workspace_members": ["core-id", "engine-id", "gui-id", "tool-id"],
        "packages": [
            pkg("core"),
            pkg("engine"),
            pkg("gui"),
            pkg("tool", kinds=("bin",)),
            {"id": "serde-id", "name": "serde", "manifest_path": "/reg/serde/Cargo.toml", "targets": [{"kind": ["lib"]}]},
        ],
        "resolve": {
            "nodes": [
                {"id": "core-id", "deps": [{"pkg": "serde-id"}]},
                {"id": "engine-id", "deps": [{"pkg": "core-id"}]},
                {"id": "gui-id", "deps": [{"pkg": "engine-id"}]},
                {"id": "tool-id", "deps": [{"pkg": "engine-id"}]},
                {"id": "serde-id", "deps": []},
            ]
        },
    }
    path = tmp_path / "metadata.json"
    path.write_text(json.dumps(meta))
    return path


def packages(tmp_path: Path, changed: list[str], *flags: str) -> list[str]:
    out = subprocess.run(
        ["python3", str(PACKAGES), str(metadata(tmp_path)), *flags],
        input="\n".join(changed) + "\n",
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return sorted(out.split())


def test_a_change_in_a_crate_tests_it_and_everything_that_depends_on_it(tmp_path):
    assert packages(tmp_path, ["crates/engine/src/lib.rs"]) == ["engine", "gui", "tool"]


def test_a_change_in_a_leaf_crate_tests_only_that_crate(tmp_path):
    assert packages(tmp_path, ["crates/gui/tests/a.rs"]) == ["gui"]


def test_a_change_in_the_bottom_crate_reaches_the_whole_chain(tmp_path):
    assert packages(tmp_path, ["crates/core/Cargo.toml"]) == ["core", "engine", "gui", "tool"]


def test_doc_changes_test_nothing(tmp_path):
    assert packages(tmp_path, ["docs/testing.md", "README.md", "site/index.html", ".github/workflows/test.yml"]) == []


def test_a_root_build_file_tests_the_whole_workspace(tmp_path):
    assert packages(tmp_path, ["Cargo.lock"]) == ["ALL"]
    assert packages(tmp_path, [".config/nextest.toml"]) == ["ALL"]


def test_a_file_outside_every_crate_tests_the_whole_workspace(tmp_path):
    assert packages(tmp_path, ["assets/fixtures/a.wav"]) == ["ALL"]


def test_doc_tests_skip_crates_without_a_library(tmp_path):
    assert packages(tmp_path, ["crates/engine/src/lib.rs"], "--lib-only") == ["engine", "gui"]
