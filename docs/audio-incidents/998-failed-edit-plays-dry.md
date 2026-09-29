# 998 — a live edit that fails to build leaves the chain playing dry

## Symptom (reported)

Found by an agent while covering #987, not by the owner's ear. A live edit
that could not build logged `rebuild failed ... restoring previous state`, but
the chain was left with no blocks and played the dry input until the next edit
that built.

## Symptom (measured)

Offline, on the #987 edit-path harness: a chain `[volume, select]` edited to a
select whose `selected_block_id` is not among its options. Node serials in
`input_states[0].blocks`: `before=[1, 2] after=[]`.

## Rig

Any chain holding a `Select`; no hardware involved.

## Reproduce

`crates/engine/src/issue_987_in_place_edit_paths_tests.rs` →
`a_live_edit_that_fails_to_build_keeps_the_nodes_it_had`:

```
cargo test -p engine --lib a_live_edit_that_fails_to_build_keeps_the_nodes_it_had
```

## Hypotheses

- CONFIRMED — the swap moves the segment's live nodes out
  (`take_reusable_nodes`) and hands them to `build_runtime_block_nodes_with`,
  which drops them (and the nodes it already reused) on the `?` from
  `build_select_runtime_node`; the error branch restores only what is left,
  which is nothing. Red: `left: [] right: [1, 2]`.
- CONFIRMED (by reading, not tested) — nodes of earlier segments already
  moved into `new_input_states` are dropped by the same error return.

## Shipped

- `runtime_select_precheck::check_selects_build`, called by
  `update_chain_runtime_state_impl` before any live node is taken: a `Select`
  whose selected option is not among its options refuses the edit and the
  pipeline keeps playing the nodes it had.

## Open

- A `Select` can still fail inside the swap when its options build to mixed
  output layouts, or an option itself fails to build; that path still loses
  the nodes it took. Covering it needs the builder to give the nodes back on
  error.

## Related

- [987](987-click-on-live-edit.md) — the in-place edit paths.
