---
name: No CI test workflow
description: User does not want GitHub Actions running tests on every commit — costs money
type: feedback
---

NEVER create GitHub Actions workflows that run tests automatically on push/PR. User doesn't want to pay for GitHub Actions CI.

**Why:** GitHub Actions costs money and the user doesn't want automated test runs on every commit.

**How to apply:** Tests are run manually only (`cargo test --workspace`). Coverage script is local only (`scripts/coverage.sh`). No CI workflows for testing.
