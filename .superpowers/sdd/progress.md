# SDD progress — #717 DI dedicated stream

Branch: feature/issue-717
Plan: docs/superpowers/plans/2026-07-01-issue-717-di-dedicated-stream.md

Task 1: complete — mechanism = Candidate B (worker-clocked isolated DI runtime). `arm_di_stream`/`disarm_di_stream`/`di_stream_active`/`di_stream_loop_len` added to ProjectRuntimeController; new `di_streams: RefCell<HashMap<ChainId, DiStreamHandle>>` field + module `di_stream.rs`. Builds an isolated ChainRuntimeState (copy of chain blocks) fed by the loop, never the guitar runtime. RED seen (behavioral: di_stream_active==false) → GREEN. 108 lib tests + issue_717_di_dedicated_runtime green.
Task 2: complete — `Chain.di_output: Option<DiOutputRef>` (binding_id+endpoint), `#[serde(default, skip_serializing_if)]` so legacy `.openrig` deserialize to None. Serde round-trip test green; `di_output: None` propagated to all Chain literals (108 files). NOTE: found #758 left 2 pre-existing broken test targets (issue_614_load_di_loop, issue_614_compact_di_loop_wiring) calling the old DI loader API — out of #717 scope, flagged to owner.
Task 3: pending — Command::SetChainDiLoopOutput + parity.
Task 4: pending — route DI to chosen output at its rate (sizes elastic + resolves output).
Task 5: pending — adapter-gui arm/disarm wiring.
Task 6: pending — DI output select UI.
Task 7: pending — dedicated DI-stream graph + meters Query.
Task 8: pending — docs + HW battery.
