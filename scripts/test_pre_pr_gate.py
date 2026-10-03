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
        env={**os.environ, "PRE_PR_GATE_SUITE": suite},
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
