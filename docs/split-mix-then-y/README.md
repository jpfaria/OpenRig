# Mix split followed by a Y split (idea, 2026-09-30) — implemented

**Status:** implemented on `feature/issue-328` (owner-approved 2026-09-30).
The design lives in the #328 spec, §9 "Mix followed by Y":
`docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md`.

Owner's use case: sum two amps, then send the summed signal to two outputs
that differ only in the cab.

```
input → split (A: amp 1 ∥ B: amp 2) → Mix → split Y
                                              ├─ A: cab IR → FRFR
                                              └─ B: (no cab) → SYN-5050 (real speaker)
```

The rule now: a chain holds at most one Mix split and at most one Y split.
With both, the Mix comes first and the Y is the last processing block. Two
Mix, two Y, a Y before a Mix, a processing block after the Y and a split
inside a path are still refused (`crates/project/src/block/split_block_methods.rs`).
Every Y output runs its own copy of the Mix and of everything before the Y
(one pipeline per output), so their CPU cost counts once per Y output.
