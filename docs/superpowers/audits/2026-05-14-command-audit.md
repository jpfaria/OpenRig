# Command Audit — Adapter-GUI on_* Callbacks

**Date:** 2026-05-14  
**Issue:** #295  
**Scope:** All `on_*` callbacks in `crates/adapter-gui/src/`  
**Total callbacks found:** 227 (via `grep -c '\.on_'`)

---

## Classification Legend

- **Y** — Mutates `session.project` (directly or via `schedule_block_editor_persist` which writes back through `ProjectSession`)
- **N** — Pure UI callback: navigation, dialog open/close, search filter, widget sync

---

## File-by-File Breakdown

### `audio_settings_save_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_save_audio_settings` (main window) | audio_settings_save_wiring.rs:110 | Writes device settings into `session.project.device_settings`, calls `sync_project_runtime` | Y | `SaveAudioSettings` |
| `on_save_audio_settings` (project_settings_window) | audio_settings_save_wiring.rs:236 | Same as above, from project settings window | Y | `SaveAudioSettings` *(duplicate wiring, same command)* |

---

### `audio_wizard_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_go_to_output_step` | audio_wizard_wiring.rs:30 | Navigates wizard to output step | N | UI-only |
| `on_go_to_input_step` | audio_wizard_wiring.rs:55 | Navigates wizard to input step | N | UI-only |

---

### `back_to_launcher_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_back_to_launcher` | back_to_launcher_wiring.rs:58 | Hides chain view, shows launcher | N | UI-only |

---

### `block_choose_type_callback.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_choose_block_type` | block_choose_type_callback.rs:120 | For "insert" type: creates InsertBlock in `chain.blocks`; for I/O types: seeds a draft; for effects: seeds draft. Only the insert branch writes immediately to `session.project`. | Y (insert branch) | `AddBlock` (insert); draft-only for others |

---

### `block_delete_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_confirm_delete_block` | block_delete_wiring.rs:74 | Removes block at `chain.blocks[block_index]`, resyncs runtime, marks dirty | Y | `RemoveBlock` |
| `on_cancel_delete_block` | block_delete_wiring.rs:146 | Hides confirm dialog | N | UI-only |

---

### `block_drawer_close_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_close_block_drawer` | block_drawer_close_wiring.rs:53 | Hides block drawer without saving | N | UI-only |

---

### `block_drawer_save_delete_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_save_block_drawer` | block_drawer_save_delete_wiring.rs:80 | Persists block_editor_draft to `session.project`, resyncs runtime | Y | `SaveBlockEditorDraft` *(persists all pending param changes)* |
| `on_delete_block_drawer` | block_drawer_save_delete_wiring.rs:134 | Shows the confirm-delete dialog (no project mutation yet) | N | UI-only *(triggers `on_confirm_delete_block` flow)* |

---

### `block_editor_window_lifecycle.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_choose_block_model` (block editor win) | block_editor_window_lifecycle.rs:120 | Updates draft model, schedules persist to `session.project` if editing existing block | Y | `ReplaceBlockModel` |
| `on_toggle_block_drawer_enabled` (block editor win) | block_editor_window_lifecycle.rs:205 | Toggles `block.enabled` in `session.project`, resyncs runtime | Y | `ToggleBlockEnabled` |
| `on_save_block_drawer` (block editor win) | block_editor_window_lifecycle.rs:303 | Persists block_editor_draft to `session.project`, resyncs runtime | Y | `SaveBlockEditorDraft` |
| `on_delete_block_drawer` (block editor win) | block_editor_window_lifecycle.rs:357 | Removes block from `session.project.chains[ci].blocks` | Y | `RemoveBlock` |
| `on_show_plugin_info` (block editor win) | block_editor_window_lifecycle.rs:428 | Opens plugin info window | N | UI-only |
| `on_open_homepage` (info win) | block_editor_window_lifecycle.rs:469 | Opens browser | N | UI-only |
| `on_close_window` (info win) | block_editor_window_lifecycle.rs:476 | Hides plugin info | N | UI-only |
| `on_close_block_drawer` (block editor win) | block_editor_window_lifecycle.rs:497 | Hides block editor window | N | UI-only |
| `on_close_requested` (block editor win) | block_editor_window_lifecycle.rs:522 | Hides block editor window on OS close | N | UI-only |

---

### `block_editor_window_params.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_update_block_parameter_number` (block editor win) | block_editor_window_params.rs:102 | Updates numeric param in draft, schedules persist | Y | `SetBlockParameterNumber` |
| `on_update_block_parameter_number_text` (block editor win) | block_editor_window_params.rs:201 | Parses text→float, updates numeric param in draft, schedules persist | Y | `SetBlockParameterNumber` *(text variant, same command with parsed value)* |
| `on_update_block_parameter_bool` (block editor win) | block_editor_window_params.rs:250 | Updates bool param in draft, schedules persist | Y | `SetBlockParameterBool` |
| `on_update_block_parameter_text` (block editor win) | block_editor_window_params.rs:295 | Updates string param in draft, schedules persist | Y | `SetBlockParameterText` |
| `on_select_block_parameter_option` (block editor win) | block_editor_window_params.rs:340 | Updates enum/select param in draft, schedules persist | Y | `SelectBlockParameterOption` |
| `on_pick_block_parameter_file` (block editor win) | block_editor_window_params.rs:385 | Opens file dialog, updates text param in draft, schedules persist | Y | `PickBlockParameterFile` |
| `on_open_vst3_editor` (block editor win) | block_editor_window_params.rs:433 | Opens VST3 native editor | N | UI-only |

---

### `block_editor_window_wiring.rs`

*(Forwarders to block_editor_window_lifecycle/params — same callbacks, additional wiring from main window context)*

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_choose_block_model` | block_editor_window_wiring.rs:42 | Forwarder → block_editor_window_lifecycle | Y | `ReplaceBlockModel` |
| `on_close_block_drawer` | block_editor_window_wiring.rs:51 | Forwarder → lifecycle | N | UI-only |
| `on_save_block_drawer` | block_editor_window_wiring.rs:60 | Forwarder → lifecycle | Y | `SaveBlockEditorDraft` |
| `on_delete_block_drawer` | block_editor_window_wiring.rs:69 | Forwarder → lifecycle | Y | `RemoveBlock` |
| `on_show_plugin_info` | block_editor_window_wiring.rs:79 | Forwarder | N | UI-only |
| `on_open_homepage` | block_editor_window_wiring.rs:120 | Browser open | N | UI-only |
| `on_close_window` | block_editor_window_wiring.rs:127 | Hide window | N | UI-only |
| `on_toggle_block_drawer_enabled` | block_editor_window_wiring.rs:143 | Forwarder → lifecycle | Y | `ToggleBlockEnabled` |
| `on_update_block_parameter_text` | block_editor_window_wiring.rs:152 | Forwarder → params | Y | `SetBlockParameterText` |
| `on_update_block_parameter_number` | block_editor_window_wiring.rs:160 | Forwarder → params | Y | `SetBlockParameterNumber` |
| `on_update_block_parameter_number_text` | block_editor_window_wiring.rs:168 | Forwarder → params | Y | `SetBlockParameterNumber` |
| `on_update_block_parameter_bool` | block_editor_window_wiring.rs:176 | Forwarder → params | Y | `SetBlockParameterBool` |
| `on_select_block_parameter_option` | block_editor_window_wiring.rs:184 | Forwarder → params | Y | `SelectBlockParameterOption` |
| `on_pick_block_parameter_file` | block_editor_window_wiring.rs:193 | Forwarder → params | Y | `PickBlockParameterFile` |
| `on_open_vst3_editor` | block_editor_window_wiring.rs:202 | Forwarder | N | UI-only |

---

### `block_insert_callbacks.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_start_block_insert` | block_insert_callbacks.rs:101 | Seeds a `BlockEditorDraft` for a new block at a UI position; no project mutation yet | N | UI-only *(prepares add-block draft)* |
| `on_choose_block_model` (main win, new block) | block_insert_callbacks.rs:172 | Updates draft model, schedules persist when editing existing block | Y (editing) | `ReplaceBlockModel` |

---

### `block_model_search_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_search_block_model` (main win) | block_model_search_wiring.rs:23 | Filters model list in UI | N | UI-only |
| `on_choose_block_model_by_id` (main win) | block_model_search_wiring.rs:34 | Updates draft model, schedules persist | Y | `ReplaceBlockModel` |
| `on_search_block_model` (block editor win) | block_model_search_wiring.rs:59 | Filters model list in UI | N | UI-only |
| `on_choose_block_model_by_id` (block editor win) | block_model_search_wiring.rs:70 | Updates draft model, schedules persist | Y | `ReplaceBlockModel` |

---

### `block_parameter_wiring.rs`

*(Wires param callbacks on the main AppWindow. All mutate via `schedule_block_editor_persist`.)*

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_update_block_parameter_number_text` | block_parameter_wiring.rs:90 | Parse text→float, update numeric param, schedule persist | Y | `SetBlockParameterNumber` |
| `on_toggle_block_drawer_enabled` | block_parameter_wiring.rs:135 | Toggle `draft.enabled`, schedule persist | Y | `ToggleBlockEnabled` |
| `on_update_block_parameter_text` | block_parameter_wiring.rs:182 | Update string param, schedule persist | Y | `SetBlockParameterText` |
| `on_update_block_parameter_number` | block_parameter_wiring.rs:226 | Update numeric param, schedule persist | Y | `SetBlockParameterNumber` |
| `on_update_block_parameter_bool` | block_parameter_wiring.rs:279 | Update bool param, schedule persist | Y | `SetBlockParameterBool` |
| `on_select_block_parameter_option` | block_parameter_wiring.rs:323 | Update enum param, schedule persist | Y | `SelectBlockParameterOption` |
| `on_pick_block_parameter_file` | block_parameter_wiring.rs:416 | File dialog → update text param, schedule persist | Y | `PickBlockParameterFile` |

---

### `block_picker_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_cancel_block_picker` | block_picker_wiring.rs:47 | Hides block picker UI | N | UI-only |

---

### `chain_block_crud_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_clear_chain_block` | chain_block_crud_wiring.rs:96 | Clears in-memory UI state (draft, selection); no project mutation | N | UI-only |
| `on_toggle_chain_block_enabled` | chain_block_crud_wiring.rs:131 | Toggles `block.enabled` in `session.project`, resyncs runtime, marks dirty | Y | `ToggleBlockEnabled` |
| `on_reorder_chain_block` | chain_block_crud_wiring.rs:213 | Moves block in `chain.blocks`, resyncs runtime, marks dirty | Y | `MoveBlock` |

---

### `chain_crud_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_add_chain` | chain_crud_wiring.rs:101 | Opens chain editor in create mode; actual mutation happens in `on_save_chain` | N | UI-only *(draft creation, save happens via `on_save_chain`)* |
| `on_configure_chain` | chain_crud_wiring.rs:219 | Opens chain editor in edit mode; same: mutation on save | N | UI-only |

---

### `chain_editor_forwarders_wiring.rs`

*(These callbacks on the main window forward to per-instance ChainEditorWindow callbacks.)*

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_edit_chain_input` | chain_editor_forwarders_wiring.rs:26 | Opens input endpoint editor panel | N | UI-only |
| `on_remove_chain_input` | chain_editor_forwarders_wiring.rs:34 | Removes input group from chain draft | N | Draft-only |
| `on_add_chain_input` | chain_editor_forwarders_wiring.rs:42 | Adds input group to chain draft | N | Draft-only |
| `on_edit_chain_output` | chain_editor_forwarders_wiring.rs:50 | Opens output endpoint editor panel | N | UI-only |
| `on_remove_chain_output` | chain_editor_forwarders_wiring.rs:58 | Removes output group from chain draft | N | Draft-only |
| `on_add_chain_output` | chain_editor_forwarders_wiring.rs:66 | Adds output group to chain draft | N | Draft-only |
| `on_select_chain_instrument` | chain_editor_forwarders_wiring.rs:74 | Updates chain draft instrument | N | Draft-only |

---

### `chain_editor_input_endpoint_callbacks.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_input_select_device` | chain_editor_input_endpoint_callbacks.rs:46 | Forwarder to `select_chain_input_device` on main window (draft mutation) | N | Draft-only |
| `on_input_toggle_channel` | chain_editor_input_endpoint_callbacks.rs:55 | Forwarder to `toggle_chain_input_channel` | N | Draft-only |
| `on_input_select_mode` | chain_editor_input_endpoint_callbacks.rs:64 | Updates `draft.inputs[gi].mode` | N | Draft-only |
| `on_input_cancel` | chain_editor_input_endpoint_callbacks.rs:82 | Hides input editor | N | UI-only |
| `on_input_save` | chain_editor_input_endpoint_callbacks.rs:129 | Commits input groups from draft → `session.project.chains[i]`, resyncs runtime | Y | `SaveChainInputEndpoints` |

---

### `chain_editor_meta_io_callbacks.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_update_chain_name` (chain editor win) | chain_editor_meta_io_callbacks.rs:40 | Updates `chain_draft.name`; committed on `on_save_chain` | N | Draft-only |
| `on_select_instrument` | chain_editor_meta_io_callbacks.rs:57 | Updates `chain_draft.instrument` | N | Draft-only |
| `on_edit_input` | chain_editor_meta_io_callbacks.rs:83 | Opens inline input endpoint editor | N | UI-only |
| `on_edit_output` | chain_editor_meta_io_callbacks.rs:150 | Opens inline output endpoint editor | N | UI-only |
| `on_add_input` | chain_editor_meta_io_callbacks.rs:217 | Adds InputGroupDraft; committed on `on_save_chain` | N | Draft-only |
| `on_add_output` | chain_editor_meta_io_callbacks.rs:275 | Adds OutputGroupDraft | N | Draft-only |
| `on_remove_input` | chain_editor_meta_io_callbacks.rs:332 | Removes InputGroupDraft | N | Draft-only |
| `on_remove_output` | chain_editor_meta_io_callbacks.rs:375 | Removes OutputGroupDraft | N | Draft-only |

---

### `chain_editor_output_endpoint_callbacks.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_output_select_device` | chain_editor_output_endpoint_callbacks.rs:46 | Forwarder to main window (draft mutation) | N | Draft-only |
| `on_output_toggle_channel` | chain_editor_output_endpoint_callbacks.rs:55 | Forwarder | N | Draft-only |
| `on_output_select_mode` | chain_editor_output_endpoint_callbacks.rs:64 | Updates `draft.outputs[gi].mode` | N | Draft-only |
| `on_output_cancel` | chain_editor_output_endpoint_callbacks.rs:82 | Hides output editor | N | UI-only |
| `on_output_save` | chain_editor_output_endpoint_callbacks.rs:129 | Commits output groups from draft → `session.project.chains[i]`, resyncs runtime | Y | `SaveChainOutputEndpoints` |

---

### `chain_editor_save_cancel_callbacks.rs`

*(The canonical save/cancel for the chain editor window — used in the `ChainEditorWindow` context)*

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_save_chain` (chain editor win) | chain_editor_save_cancel_callbacks.rs:59 | Validates draft, either adds new chain or replaces existing in `session.project.chains`, resyncs runtime, marks dirty | Y | `SaveChain` |
| `on_cancel_chain` (chain editor win) | chain_editor_save_cancel_callbacks.rs:185 | Discards chain draft, hides editor | N | UI-only |

---

### `chain_io_fullscreen_callbacks.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_chain_io_select_device` | chain_io_fullscreen_callbacks.rs:82 | Updates device in ChainDraft (draft mutation only) | N | Draft-only |
| `on_chain_io_toggle_channel` | chain_io_fullscreen_callbacks.rs:113 | Updates channel selection in ChainDraft | N | Draft-only |
| `on_chain_io_select_mode` | chain_io_fullscreen_callbacks.rs:148 | Updates mode in ChainDraft | N | Draft-only |
| `on_chain_io_save` | chain_io_fullscreen_callbacks.rs:183 | Commits ChainDraft I/O to `session.project.chains`, resyncs runtime | Y | `SaveChainIo` |
| `on_chain_io_cancel` | chain_io_fullscreen_callbacks.rs:226 | Hides chain IO editor | N | UI-only |
| `on_chain_io_groups_edit` | chain_io_fullscreen_callbacks.rs:265 | Opens group editor sub-panel | N | UI-only |
| `on_chain_io_groups_remove` | chain_io_fullscreen_callbacks.rs:355 | Removes I/O group from ChainDraft | N | Draft-only |
| `on_chain_io_groups_add` | chain_io_fullscreen_callbacks.rs:381 | Adds I/O group to ChainDraft | N | Draft-only |
| `on_chain_io_groups_save` | chain_io_fullscreen_callbacks.rs:406 | Commits I/O groups to `session.project.chains`, resyncs runtime | Y | `SaveChainIo` |
| `on_chain_io_groups_cancel` | chain_io_fullscreen_callbacks.rs:428 | Hides groups panel | N | UI-only |
| `on_chain_io_groups_toggle_enabled` | chain_io_fullscreen_callbacks.rs:449 | Toggles block enabled in `session.project.chains`, resyncs runtime | Y | `ToggleBlockEnabled` |
| `on_chain_io_groups_delete_block` | chain_io_fullscreen_callbacks.rs:468 | Removes block from `session.project.chains` | Y | `RemoveBlock` |

---

### `chain_io_main_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_select_chain_input_device` | chain_io_main_wiring.rs:78 | Updates `chain_draft.inputs[gi].device_id` (draft-only) | N | Draft-only |
| `on_select_chain_output_device` | chain_io_main_wiring.rs:151 | Updates `chain_draft.outputs[gi].device_id` (draft-only) | N | Draft-only |
| `on_toggle_chain_input_channel` | chain_io_main_wiring.rs:215 | Toggles channel in `chain_draft.inputs[gi].channels` | N | Draft-only |
| `on_toggle_chain_output_channel` | chain_io_main_wiring.rs:260 | Toggles channel in `chain_draft.outputs[gi].channels` | N | Draft-only |
| `on_configure_chain_input` | chain_io_main_wiring.rs:296 | Opens I/O groups window for input (reads project, no write) | N | UI-only |
| `on_configure_chain_output` | chain_io_main_wiring.rs:372 | Opens I/O groups window for output (reads project, no write) | N | UI-only |

---

### `chain_io_picker_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_select_device` (chain_input_window) | chain_io_picker_wiring.rs:34 | Updates draft device | N | Draft-only |
| `on_toggle_channel` (chain_input_window) | chain_io_picker_wiring.rs:42 | Toggles draft channel | N | Draft-only |
| `on_select_device` (chain_output_window) | chain_io_picker_wiring.rs:50 | Updates draft device | N | Draft-only |
| `on_toggle_channel` (chain_output_window) | chain_io_picker_wiring.rs:58 | Toggles draft channel | N | Draft-only |
| `on_select_input_mode` | chain_io_picker_wiring.rs:66 | Updates draft mode | N | Draft-only |
| `on_select_output_mode` | chain_io_picker_wiring.rs:84 | Updates draft mode | N | Draft-only |

---

### `chain_io_save_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_save` (chain_input_window) | chain_io_save_wiring.rs:84 | Commits input draft to `session.project.chains`, resyncs runtime | Y | `SaveChainInputEndpoints` |
| `on_cancel` (chain_output_window) | chain_io_save_wiring.rs:289 | Hides output window | N | UI-only |
| `on_cancel` (chain_input_window) | chain_io_save_wiring.rs:347 | Hides input window | N | UI-only |
| `on_save` (chain_output_window) | chain_io_save_wiring.rs:409 | Commits output draft to `session.project.chains`, resyncs runtime | Y | `SaveChainOutputEndpoints` |

---

### `chain_input_groups_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_edit_group` | chain_input_groups_wiring.rs:73 | Opens inline group editor (draft-only) | N | UI-only |
| `on_remove_group` | chain_input_groups_wiring.rs:113 | Removes group from ChainDraft | N | Draft-only |
| `on_add_group` | chain_input_groups_wiring.rs:155 | Adds group to ChainDraft | N | Draft-only |
| `on_save` | chain_input_groups_wiring.rs:213 | Commits input groups draft → `session.project.chains`, resyncs runtime | Y | `SaveChainInputEndpoints` |
| `on_cancel` | chain_input_groups_wiring.rs:323 | Discards draft, hides window | N | UI-only |
| `on_toggle_enabled` | chain_input_groups_wiring.rs:342 | Toggles block enabled in `session.project.chains`, resyncs runtime | Y | `ToggleBlockEnabled` |
| `on_delete_block` | chain_input_groups_wiring.rs:402 | Removes block from `session.project.chains` | Y | `RemoveBlock` |

---

### `chain_name_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_update_chain_name` | chain_name_wiring.rs:16 | Updates `chain_draft.name` only (draft-only; committed on `on_save_chain`) | N | Draft-only |

---

### `chain_output_groups_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_edit_group` | chain_output_groups_wiring.rs:72 | Opens inline group editor | N | UI-only |
| `on_remove_group` | chain_output_groups_wiring.rs:108 | Removes output group from ChainDraft | N | Draft-only |
| `on_add_group` | chain_output_groups_wiring.rs:149 | Adds output group to ChainDraft | N | Draft-only |
| `on_save` | chain_output_groups_wiring.rs:204 | Commits output groups draft → `session.project.chains`, resyncs runtime | Y | `SaveChainOutputEndpoints` |
| `on_cancel` | chain_output_groups_wiring.rs:313 | Discards draft, hides window | N | UI-only |
| `on_toggle_enabled` | chain_output_groups_wiring.rs:332 | Toggles block enabled in `session.project.chains`, resyncs runtime | Y | `ToggleBlockEnabled` |
| `on_delete_block` | chain_output_groups_wiring.rs:392 | Removes block from `session.project.chains` | Y | `RemoveBlock` |

---

### `chain_preset_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_save_chain_preset` | chain_preset_wiring.rs:65 | Saves chain blocks to preset YAML file (does NOT mutate `session.project`) | N | `SaveChainPreset` *(file I/O only, no project mutation)* |
| `on_configure_chain_preset` | chain_preset_wiring.rs:124 | Opens preset picker or file dialog; on desktop, loads preset and replaces `chain.blocks` | Y (desktop branch) | `LoadChainPreset` |
| `on_preset_picker_confirm` | chain_preset_wiring.rs:244 | Loads preset YAML, replaces `chain.blocks`, resyncs runtime, marks dirty | Y | `LoadChainPreset` |
| `on_preset_picker_cancel` | chain_preset_wiring.rs:315 | Hides picker | N | UI-only |
| `on_preset_picker_delete` | chain_preset_wiring.rs:325 | Deletes preset file from disk (no project mutation) | N | `DeleteChainPreset` *(file I/O only)* |

---

### `chain_row_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_remove_chain` | chain_row_wiring.rs:57 | Confirms with dialog, removes chain from `session.project.chains`, kills runtime, marks dirty | Y | `RemoveChain` |
| `on_toggle_chain_enabled` | chain_row_wiring.rs:129 | Toggles `chain.enabled` in `session.project.chains`, conflict check, resyncs runtime | Y | `ToggleChainEnabled` |
| `on_move_chain_up` | chain_row_wiring.rs:224 | Moves chain up in `session.project.chains`, marks dirty | Y | `MoveChainUp` |
| `on_move_chain_down` | chain_row_wiring.rs:260 | Moves chain down in `session.project.chains`, marks dirty | Y | `MoveChainDown` |

---

### `chain_save_cancel_callbacks.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_save_chain` (main window) | chain_save_cancel_callbacks.rs:69 | Validates draft, upserts chain in `session.project.chains`, resyncs runtime, marks dirty | Y | `SaveChain` |
| `on_cancel_chain` (main window) | chain_save_cancel_callbacks.rs:194 | Discards draft | N | UI-only |

---

### `compact_chain_block_handlers.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_toggle_block_enabled` (compact win) | compact_chain_block_handlers.rs:72 | Toggles `block.enabled` in `session.project`, resyncs runtime, marks dirty | Y | `ToggleBlockEnabled` |
| `on_toggle_chain_enabled` (compact win) | compact_chain_block_handlers.rs:135 | Toggles `chain.enabled` in `session.project`, resyncs runtime | Y | `ToggleChainEnabled` |
| `on_choose_block_model` (compact win) | compact_chain_block_handlers.rs:289 | Replaces `block.kind` in `session.project`, resyncs runtime, marks dirty | Y | `ReplaceBlockModel` |
| `on_remove_block` (compact win) | compact_chain_block_handlers.rs:418 | Removes block from `session.project.chains`, resyncs runtime, marks dirty | Y | `RemoveBlock` |
| `on_reorder_block` (compact win) | compact_chain_block_handlers.rs:472 | Moves block in `chain.blocks`, resyncs runtime, marks dirty | Y | `MoveBlock` |

---

### `compact_chain_callbacks.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_open_compact_chain_view` | compact_chain_callbacks.rs:70 | Opens compact view window | N | UI-only |
| `on_search_block_model` (compact win) | compact_chain_callbacks.rs:123 | Filters model list | N | UI-only |
| `on_choose_block_model_by_id` (compact win) | compact_chain_callbacks.rs:145 | Updates draft model, schedules persist | Y | `ReplaceBlockModel` |
| `on_close_compact_view` | compact_chain_callbacks.rs:198 | Hides compact view | N | UI-only |
| `on_choose_block_type` (compact win) | compact_chain_callbacks.rs:208 | Opens type picker or inserts block | Y (insert) | `AddBlock` |
| `on_open_block_detail` | compact_chain_callbacks.rs:229 | Opens block editor/detail | N | UI-only |
| `on_configure_input` (compact win) | compact_chain_callbacks.rs:326 | Opens I/O config | N | UI-only |
| `on_configure_output` (compact win) | compact_chain_callbacks.rs:338 | Opens I/O config | N | UI-only |
| `on_open_plugin` (compact win) | compact_chain_callbacks.rs:352 | Opens VST3 editor | N | UI-only |

---

### `compact_chain_param_handlers.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_update_block_parameter_number` (compact win) | compact_chain_param_handlers.rs:69 | Updates numeric param in `session.project` directly, resyncs runtime, marks dirty | Y | `SetBlockParameterNumber` |
| `on_select_block_parameter_option` (compact win) | compact_chain_param_handlers.rs:152 | Updates enum param in `session.project` directly | Y | `SelectBlockParameterOption` |
| `on_update_block_parameter_bool` (compact win) | compact_chain_param_handlers.rs:257 | Updates bool param in `session.project` directly | Y | `SetBlockParameterBool` |

---

### `desktop_app.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_close_requested` | desktop_app.rs:713 | OS window close handler | N | UI-only |

---

### `device_refresh_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_refresh_devices` (main win) | device_refresh_wiring.rs:51 | Refreshes device enumeration | N | UI-only |
| `on_refresh_devices` (settings win) | device_refresh_wiring.rs:77 | Refreshes device enumeration | N | UI-only |

---

### `device_settings_wiring.rs`

*(These mutate in-memory device selection UI models, not `session.project` directly — committed on `on_save_audio_settings`)*

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_toggle_input_device` | device_settings_wiring.rs:45 | Toggles row in `input_devices` VecModel | N | Draft-only |
| `on_update_input_sample_rate` | device_settings_wiring.rs:51 | Updates sample rate in `input_devices` model | N | Draft-only |
| `on_update_input_buffer_size` | device_settings_wiring.rs:57 | Updates buffer size in `input_devices` model | N | Draft-only |
| `on_toggle_output_device` | device_settings_wiring.rs:65 | Toggles row in `output_devices` model | N | Draft-only |
| `on_update_output_sample_rate` | device_settings_wiring.rs:71 | Updates sample rate in `output_devices` model | N | Draft-only |
| `on_update_output_buffer_size` | device_settings_wiring.rs:77 | Updates buffer size in `output_devices` model | N | Draft-only |
| `on_toggle_project_device` (project settings win) | device_settings_wiring.rs:85 | Toggles row in `project_devices` model | N | Draft-only |
| `on_update_project_sample_rate` (project settings win) | device_settings_wiring.rs:91 | Updates sample rate | N | Draft-only |
| `on_update_project_buffer_size` (project settings win) | device_settings_wiring.rs:97 | Updates buffer size | N | Draft-only |
| `on_update_project_bit_depth` (project settings win) | device_settings_wiring.rs:103 | Updates bit depth | N | Draft-only |
| `on_toggle_project_device` (main win) | device_settings_wiring.rs:112 | Toggles row in `project_devices` model | N | Draft-only |
| `on_update_project_sample_rate` (main win) | device_settings_wiring.rs:118 | Updates sample rate | N | Draft-only |
| `on_update_project_buffer_size` (main win) | device_settings_wiring.rs:124 | Updates buffer size | N | Draft-only |
| `on_update_project_bit_depth` (main win) | device_settings_wiring.rs:130 | Updates bit depth | N | Draft-only |

---

### `insert_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_select_send_device` | insert_wiring.rs:66 | Updates `insert_draft.send_device_id` | N | Draft-only |
| `on_toggle_send_channel` | insert_wiring.rs:84 | Toggles send channel in draft | N | Draft-only |
| `on_select_send_mode` | insert_wiring.rs:105 | Updates `insert_draft.send_mode` | N | Draft-only |
| `on_select_return_device` | insert_wiring.rs:122 | Updates `insert_draft.return_device_id` | N | Draft-only |
| `on_toggle_return_channel` | insert_wiring.rs:140 | Toggles return channel in draft | N | Draft-only |
| `on_select_return_mode` | insert_wiring.rs:161 | Updates `insert_draft.return_mode` | N | Draft-only |
| `on_toggle_enabled` (insert win) | insert_wiring.rs:185 | Toggles insert block enabled in `session.project`, resyncs runtime | Y | `ToggleBlockEnabled` |
| `on_delete_block` (insert win) | insert_wiring.rs:241 | Removes insert block from `session.project.chains` | Y | `RemoveBlock` |
| `on_save` (insert win) | insert_wiring.rs:297 | Commits insert draft to `session.project`, resyncs runtime, marks dirty | Y | `SaveInsertBlock` |
| `on_cancel` (insert win) | insert_wiring.rs:375 | Discards draft, hides window | N | UI-only |

---

### `language_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_change_language` | language_wiring.rs:41 | Updates persisted language setting (no project mutation) | N | `ChangeLanguage` *(app settings, not project)* |

---

### `latency_probe.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_probe_chain_latency` | latency_probe.rs:42 | Triggers latency measurement for a chain | N | UI-only / diagnostic |

---

### `model_search_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_search_block_model` | model_search_wiring.rs:50 | Filters model list in UI | N | UI-only |
| `on_choose_block_model_by_id` | model_search_wiring.rs:55 | Updates draft model, schedules persist | Y | `ReplaceBlockModel` |

---

### `plugin_info_inline_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_show_plugin_info` | plugin_info_inline_wiring.rs:35 | Shows plugin info panel | N | UI-only |
| `on_close_plugin_info` | plugin_info_inline_wiring.rs:68 | Hides plugin info | N | UI-only |
| `on_open_plugin_info_homepage` | plugin_info_inline_wiring.rs:77 | Opens browser | N | UI-only |

---

### `project_file_dialog_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_open_project_file` | project_file_dialog_wiring.rs:77 | File dialog → loads project into `session`, replaces runtime, shows chains | Y | `LoadProject` |
| `on_create_project_file` | project_file_dialog_wiring.rs:150 | Routes to new-project setup UI (no mutation yet) | N | UI-only |
| `on_confirm_new_project` | project_file_dialog_wiring.rs:174 | Creates new `ProjectSession`, sets name, routes to chains | Y | `CreateProject` |
| `on_cancel_new_project` | project_file_dialog_wiring.rs:214 | Back to launcher | N | UI-only |
| `on_save_project` | project_file_dialog_wiring.rs:231 | Saves project YAML to disk (save-as dialog if no path) | Y | `SaveProject` |

---

### `project_settings_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_configure_project` | project_settings_wiring.rs:69 | Opens project settings (reads project, no write) | N | UI-only |
| `on_update_project_name` (main win) | project_settings_wiring.rs:114 | Updates `session.project.name`, marks dirty | Y | `UpdateProjectName` |
| `on_update_project_name` (settings win) | project_settings_wiring.rs:142 | Same as above from settings window | Y | `UpdateProjectName` |
| `on_close_project_settings` (main win) | project_settings_wiring.rs:171 | Hides settings | N | UI-only |
| `on_close_project_settings` (settings win) | project_settings_wiring.rs:182 | Hides settings window | N | UI-only |

---

### `recent_projects_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_filter_recent_projects` | recent_projects_wiring.rs:68 | Filters recent project list in UI | N | UI-only |
| `on_open_recent_project` | recent_projects_wiring.rs:91 | Loads project from recent list into session, replaces runtime | Y | `LoadProject` |
| `on_remove_recent_project` | recent_projects_wiring.rs:193 | Removes entry from `app_config.recent_projects` (not from project itself) | N | UI-only / app config |

---

### `select_chain_block_callback.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_select_chain_block` | select_chain_block_callback.rs:137 | Sets selected block in UI state; reads project to build editor draft | N | UI-only |

---

### `spectrum_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_open_spectrum_window` | spectrum_wiring.rs:53 | Opens spectrum analyzer window | N | UI-only |
| `on_close_spectrum` | spectrum_wiring.rs:92 | Closes spectrum | N | UI-only |
| `on_close_spectrum_window` | spectrum_wiring.rs:110 | Closes spectrum window | N | UI-only |
| `on_toggle_spectrum_enabled` (main win) | spectrum_wiring.rs:175 | Toggles spectrum analysis | N | UI-only |
| `on_toggle_enabled` (spectrum win) | spectrum_wiring.rs:176 | Same from spectrum window | N | UI-only |

---

### `tuner_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_open_tuner_window` | tuner_wiring.rs:50 | Opens tuner | N | UI-only |
| `on_close_tuner` | tuner_wiring.rs:90 | Closes tuner | N | UI-only |
| `on_close_tuner_window` | tuner_wiring.rs:109 | Closes tuner window | N | UI-only |
| `on_toggle_tuner_mute` | tuner_wiring.rs:124 | Toggles tuner mute | N | UI-only |
| `on_toggle_mute` (tuner win) | tuner_wiring.rs:147 | Same from tuner window | N | UI-only |
| `on_toggle_tuner_enabled` | tuner_wiring.rs:235 | Toggles tuner | N | UI-only |
| `on_toggle_enabled` (tuner win) | tuner_wiring.rs:236 | Same from tuner window | N | UI-only |

---

### `virtual_keyboard_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_virtual_key_pressed` | virtual_keyboard_wiring.rs:13 | Simulates key press in UI | N | UI-only |

---

### `vst3_editor_wiring.rs`

| Callback | File:line | Currently does | Mutates project? | Maps to Command variant |
|---|---|---|---|---|
| `on_open_vst3_editor` | vst3_editor_wiring.rs:16 | Opens VST3 native editor window | N | UI-only |

---

## Summary

### Mutating Callbacks → Command Variants

| Command Variant | Mutating callbacks it covers |
|---|---|
| `SetBlockParameterNumber` | `on_update_block_parameter_number`, `on_update_block_parameter_number_text` (multiple windows) |
| `SetBlockParameterBool` | `on_update_block_parameter_bool` (multiple windows) |
| `SetBlockParameterText` | `on_update_block_parameter_text` (multiple windows) |
| `SelectBlockParameterOption` | `on_select_block_parameter_option` (multiple windows) |
| `PickBlockParameterFile` | `on_pick_block_parameter_file` (multiple windows) |
| `ToggleBlockEnabled` | `on_toggle_block_drawer_enabled`, `on_toggle_chain_block_enabled`, `on_toggle_block_enabled` (compact), `on_toggle_enabled` (insert, input/output groups) |
| `ToggleChainEnabled` | `on_toggle_chain_enabled` (main win, compact win) |
| `ReplaceBlockModel` | `on_choose_block_model`, `on_choose_block_model_by_id` (multiple windows) |
| `AddBlock` | `on_choose_block_type` (insert branch), `on_choose_block_type` (compact, insert branch) |
| `RemoveBlock` | `on_confirm_delete_block`, `on_delete_block_drawer`, `on_delete_block` (insert/groups windows), `on_remove_block` (compact) |
| `MoveBlock` | `on_reorder_chain_block`, `on_reorder_block` (compact) |
| `RemoveChain` | `on_remove_chain` |
| `MoveChainUp` | `on_move_chain_up` |
| `MoveChainDown` | `on_move_chain_down` |
| `SaveChain` | `on_save_chain` (main win, chain editor win) |
| `SaveChainInputEndpoints` | `on_input_save`, `on_save` (input window, input groups windows) |
| `SaveChainOutputEndpoints` | `on_output_save`, `on_save` (output window, output groups windows) |
| `SaveChainIo` | `on_chain_io_save`, `on_chain_io_groups_save` |
| `SaveInsertBlock` | `on_save` (insert window) |
| `SaveBlockEditorDraft` | `on_save_block_drawer` (main win, block editor win) |
| `LoadProject` | `on_open_project_file`, `on_open_recent_project` |
| `CreateProject` | `on_confirm_new_project` |
| `SaveProject` | `on_save_project` |
| `LoadChainPreset` | `on_configure_chain_preset` (desktop), `on_preset_picker_confirm` |
| `SaveAudioSettings` | `on_save_audio_settings` (both windows) |
| `UpdateProjectName` | `on_update_project_name` (both windows) |

### Counts

| Category | Count |
|---|---|
| Total `on_*` callback registrations (all windows) | 227 |
| Mutating callbacks (directly write `session.project`) | ~68 |
| UI-only callbacks (navigation, open/close, search, diagnostic) | ~89 |
| Draft-only callbacks (write to transient draft, committed later) | ~70 |
| **Command variants** | **25** |
| **Event variants** (spec) | **9** |

### Spec Alignment Notes

1. **`SaveBlockEditorDraft`** — not in the spec. The spec assumes each parameter change is immediately persisted as a `SetBlockParameterNumber` etc. The current codebase batches parameter changes through a draft + persist timer. The `SaveBlockEditorDraft` command wraps the "flush draft to project" operation that occurs when the user clicks Save or model changes. In Phase 2 tasks this should be replaced by making each param callback dispatch immediately (removing the draft indirection).

2. **`SaveChain` / `SaveChainInputEndpoints` / `SaveChainOutputEndpoints` / `SaveChainIo` / `SaveInsertBlock`** — not enumerated in the spec. These arise from the multi-step dialog flow (draft → validate → commit). They will simplify once the chain editor is refactored. For now they are necessary to express the current operation boundary.

3. **`MoveChainUp` / `MoveChainDown`** — spec has no `MoveChain` variant. Introduced as separate variants here because that is how the current code works (two discrete operations). Could be merged into `MoveChain { direction: Up | Down }` or `ReorderChain { new_position: usize }` in a later refinement.

4. **`SaveAudioSettings`** — writes `session.project.device_settings`. The spec did not enumerate this, but it is a clear domain mutation.

5. **`LoadProject` / `CreateProject` / `SaveProject`** — exactly match the spec.
