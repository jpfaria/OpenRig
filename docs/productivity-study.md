# AI-productivity study (CEO report)

OpenRig doubles as a measured experiment about building software with an AI
agent. The report lives in the owner's Obsidian vault as
`OpenRig - Estudo de Produtividade com IA.md`, with the markdown source and PDF
under `assets/openrig-relatorio/`.

**Why the project exists (this leads the report):** OpenRig is not a product, it
is a deliberate worst case for an AI agent — unknown domain (realtime audio),
unknown language (Rust), new UI framework (Slint), shallow UX — holding one
variable constant: the owner's software-engineering experience. The goal is to
measure in numbers (1) how effective the AI is, (2) where it is weak (coupling,
debt, refactor), and (3) what the experienced developer is irreplaceable for.

**Window measured:** 2026-03-18 → 2026-06-29, 103 days, solo plus Claude Code.

Measured (GitHub API + tree count): 4,086 commits · 376/480 issues resolved ·
235/263 PRs merged · ~216.5k source LOC (Rust 177k + Slint 30k) · 3,646 tests
(~46% of the Rust) · 40 crates · 23 releases · 9 own skills (~8,475 lines).

Modeled estimate — **never presented as measured**: ~33 person-years of non-AI
equivalent effort, a team of 8–13 over 2.5–3 years.

**Thesis:** the AI produced volume; engineering discipline produced quality.
Without human review the system would have gone fully coupled — the refactor and
bug issues are the QA loop working. The AI amplifies good and bad judgement
alike; the senior engineer at the wheel is the differentiator.

**Chapters (13):** product · cadence · code · architecture · features ·
effort/labels · infra (skills, hooks, gate) · commit signals · process evolution
(guardrails, workspaces, skill curation) · effort estimate · cost estimate ·
thesis · methodology.

**Updating it:** re-pull the numbers from `gh` rather than trusting the snapshot
above, and keep the measured facts strictly separated from the modeled estimate.
