#!/usr/bin/env python3
"""Tests for the machine-wide build lock: scripts/cargo-locked.sh and its
Claude hook .claude/hooks/build-lock-guard.sh.

Run: pytest scripts/test_cargo_locked.py

Agent sessions build in their own clones but share one Mac; two builds at once
slow both several times over. Every agent build goes through
scripts/cargo-locked.sh, which takes the lock the pre-PR gate takes, and the
hook denies a bare `cargo build/test/check/run/...` in a Bash call. A fake
`cargo` on PATH records its calls; nothing compiles.
"""
import json
import os
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCKED = ROOT / "scripts" / "cargo-locked.sh"
GUARD = ROOT / ".claude" / "hooks" / "build-lock-guard.sh"


def fake_cargo(tmp_path: Path, lock: Path, exit_code: int = 0) -> tuple[Path, Path]:
    """A `cargo` that logs its args and whether the lock was held at the time."""
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    log = tmp_path / "cargo.log"
    cargo = bin_dir / "cargo"
    cargo.write_text(
        "#!/usr/bin/env bash\n"
        f'if [ -d "{lock}" ]; then held=locked; else held=free; fi\n'
        f'echo "$held $*" >> "{log}"\n'
        f"exit {exit_code}\n"
    )
    cargo.chmod(0o755)
    return bin_dir, log


def run_locked(tmp_path: Path, lock: Path, bin_dir: Path, *args: str, **env: str):
    return subprocess.run(
        ["bash", str(LOCKED), *args],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        timeout=20,
        env={
            **os.environ,
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "OPENRIG_BUILD_LOCK": str(lock),
            **env,
        },
    )


def test_it_runs_cargo_with_the_args_under_the_lock(tmp_path):
    lock = tmp_path / "build.lock"
    bin_dir, log = fake_cargo(tmp_path, lock)
    result = run_locked(tmp_path, lock, bin_dir, "test", "-p", "engine")
    assert result.returncode == 0
    assert log.read_text().splitlines() == ["locked test -p engine"]
    assert not lock.exists()


def test_it_returns_cargos_exit_code_and_releases_the_lock(tmp_path):
    lock = tmp_path / "build.lock"
    bin_dir, _ = fake_cargo(tmp_path, lock, exit_code=101)
    result = run_locked(tmp_path, lock, bin_dir, "check")
    assert result.returncode == 101
    assert not lock.exists()


def test_it_waits_while_another_build_holds_the_lock(tmp_path):
    lock = tmp_path / "build.lock"
    bin_dir, log = fake_cargo(tmp_path, lock)
    lock.mkdir()
    (lock / "pid").write_text(f"{os.getpid()}\n")
    proc = subprocess.Popen(
        ["bash", str(LOCKED), "build"],
        cwd=tmp_path,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env={**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}", "OPENRIG_BUILD_LOCK": str(lock)},
    )
    time.sleep(2)
    assert proc.poll() is None and not log.exists()
    (lock / "pid").unlink()
    lock.rmdir()
    assert proc.wait(timeout=10) == 0
    assert log.read_text().splitlines() == ["locked build"]


def test_a_lock_left_by_a_dead_build_is_taken_over(tmp_path):
    lock = tmp_path / "build.lock"
    bin_dir, log = fake_cargo(tmp_path, lock)
    lock.mkdir()
    dead = subprocess.Popen(["true"])
    dead.wait()
    (lock / "pid").write_text(f"{dead.pid}\n")
    assert run_locked(tmp_path, lock, bin_dir, "build").returncode == 0
    assert log.read_text().splitlines() == ["locked build"]


def test_a_build_inside_a_holder_does_not_wait_for_itself(tmp_path):
    lock = tmp_path / "build.lock"
    bin_dir, log = fake_cargo(tmp_path, lock)
    lock.mkdir()
    (lock / "pid").write_text(f"{os.getpid()}\n")
    result = run_locked(tmp_path, lock, bin_dir, "test", OPENRIG_BUILD_LOCK_HELD="1")
    assert result.returncode == 0
    assert log.read_text().splitlines() == ["locked test"]
    assert lock.exists()


def test_run_builds_under_the_lock_and_runs_the_app_without_it(tmp_path):
    lock = tmp_path / "build.lock"
    bin_dir, log = fake_cargo(tmp_path, lock)
    result = run_locked(tmp_path, lock, bin_dir, "run", "-p", "adapter-gui", "--", "--mcp")
    assert result.returncode == 0
    assert log.read_text().splitlines() == [
        "locked build -p adapter-gui",
        "free run -p adapter-gui -- --mcp",
    ]


# --- Hook: no bare cargo build in an agent's Bash call ----------------------

def guard(command: str) -> str | None:
    payload = json.dumps({"tool_name": "Bash", "tool_input": {"command": command}, "cwd": "/tmp"})
    out = subprocess.run(
        ["bash", str(GUARD)], input=payload, capture_output=True, text=True, check=True
    ).stdout.strip()
    if not out:
        return None
    decision = json.loads(out)["hookSpecificOutput"]
    return decision["permissionDecisionReason"] if decision["permissionDecision"] == "deny" else None


def test_a_bare_cargo_build_is_denied():
    for cmd in [
        "cargo test -p engine",
        "cd /x/.solvers/issue-1 && cargo build -p adapter-gui",
        'RUSTFLAGS="-D warnings" cargo check --tests',
        "timeout 600 cargo nextest run --workspace",
        "cargo run -p adapter-gui -- --mcp",
        "cargo +nightly clippy",
    ]:
        reason = guard(cmd)
        assert reason and "cargo-locked.sh" in reason, cmd


def test_builds_through_the_lock_and_cheap_cargo_commands_pass():
    for cmd in [
        "scripts/cargo-locked.sh test -p engine",
        "cd /x && OPENRIG_PLUGINS_ROOT=/p ./scripts/cargo-locked.sh run -p adapter-gui -- --mcp",
        "cargo fmt --all -- --check",
        "cargo metadata --format-version 1",
        "cargo --version",
        "./scripts/pre-pr-gate.sh",
        "git status",
        'git commit -m "fix: the cargo test run was flaky"',
        "gh issue comment 1 --body 'ran cargo build -p engine: clean'",
    ]:
        assert guard(cmd) is None, cmd
