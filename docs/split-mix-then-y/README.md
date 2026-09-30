# Mix split followed by a Y split (idea, 2026-09-30)

Owner's use case: sum two amps, then send the summed signal to two outputs
that differ only in the cab.

```
input → split (A: amp 1 ∥ B: amp 2) → Mix → split Y
                                              ├─ A: cab IR → FRFR
                                              └─ B: (no cab) → SYN-5050 (real speaker)
```

Today this is not possible. The #328 spec (§1.1,
`docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md`)
allows at most one split per chain, and nothing may follow a Y split, so a
Mix and a Y cannot live in the same chain. The rule is enforced in
`crates/project/src/block/split_block_methods.rs`.

What it would take: allow one Mix split and one Y split in series, with the
Y as the last processing block. The engine already builds one segment per
output from the Y's per-output path sets (spec §4.2), so the Y part stays
the same. The shared pre-chain in front of the Y would then hold the Mix
split. Not decided; waiting on the owner.
