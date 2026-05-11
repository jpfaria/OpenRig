# OpenRig Code Quality Skill

Architecture and code quality checklist for the OpenRig project. Must be applied BEFORE writing any code.

## Installation

### Claude Code (CLI)

Copy or symlink to your Claude skills directory:

```bash
# Option 1: Symlink (recommended — stays in sync with repo)
ln -s "$(pwd)/.skills/openrig-code-quality" ~/.claude/skills/openrig-code-quality

# Option 2: Copy
cp -r skills/openrig-code-quality ~/.claude/skills/openrig-code-quality
```

Then invoke with `/openrig-code-quality` in any Claude Code session.

### Codex

Copy or symlink to your Codex skills directory:

```bash
# Option 1: Symlink
ln -s "$(pwd)/.skills/openrig-code-quality" ~/.agents/skills/openrig-code-quality

# Option 2: Copy
cp -r skills/openrig-code-quality ~/.agents/skills/openrig-code-quality
```

### Verification

After installation, the skill should appear when you list available skills. In Claude Code, type `/` and look for `openrig-code-quality`.

## Usage

Invoke before writing any code:

```
/openrig-code-quality
```

The checklist will be loaded into context. Follow ALL rules before writing code.

## Updating

If installed via symlink, pulling the repo automatically updates the skill. If copied, re-copy after changes:

```bash
cp -r skills/openrig-code-quality ~/.claude/skills/openrig-code-quality
```
