# Desktop Window Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the OpenRig desktop GUI to the approved multi-window architecture while keeping shared form content reusable for the future touch mode.

**Architecture:** Desktop switches from inline config/edit flows to independent windows, while touch keeps those same forms embedded in the main window. Shared editor content must be extracted so catalog-driven behavior and schema rendering stay single-sourced.

**Tech Stack:** Rust, Slint, OpenRig project/stage catalog, desktop/touch container split

---

### Task 1: Lock the shell structure

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/pages/project_chains.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/pages/project_launcher.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/app-window.slint`

- [ ] Remove the logo from non-launcher screens.
- [ ] Move the project name into the chains header area.
- [ ] Move `Nova chain` to the top-right of the chains panel.
- [ ] Keep the launcher branding unchanged.
- [ ] Commit only these shell changes.

### Task 2: Introduce reusable chain endpoint forms

**Files:**
- Create: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/components/chain_input_form.slint`
- Create: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/components/chain_output_form.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/models.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`

- [ ] Extract shared input form content for device, channels, sample rate, and buffer.
- [ ] Extract shared output form content for device, channels, sample rate, and buffer.
- [ ] Keep runtime apply behavior immediate.
- [ ] Commit the shared endpoint forms.

### Task 3: Add `In` and `Out` chips to chains

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/pages/project_chains.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`

- [ ] Render `In` chip at the start of the chain line.
- [ ] Render `Out` chip at the end of the chain line.
- [ ] Add hover tooltip data for device, sample rate, buffer, and channels.
- [ ] Add click handlers that open input/output config entry points.
- [ ] Commit the endpoint chip behavior.

### Task 4: Add desktop input/output windows and touch inline hosts

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/desktop_main.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/touch_main.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/app-window.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`

- [ ] Create desktop window hosts for input and output forms.
- [ ] Keep touch hosts inline in the main window.
- [ ] Share the exact same form content across both hosts.
- [ ] Commit container split for endpoint config.

### Task 5: Refactor chain config to metadata only

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/pages/track_editor.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`

- [ ] Remove direct input/output editing from chain config.
- [ ] Show input/output summaries only.
- [ ] Add launch points to the dedicated input/output windows.
- [ ] Commit the chain config refactor.

### Task 6: Extract stage form content from the current drawer

**Files:**
- Create: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/components/stage_editor_form.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/pages/project_chains.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`

- [ ] Move the add/edit stage form content into a reusable shared component.
- [ ] Keep the inline type picker in the chains screen.
- [ ] Remove desktop dependence on the inline drawer form.
- [ ] Commit the extracted stage form.

### Task 7: Add desktop stage windows

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/desktop_main.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/touch_main.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/app-window.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`

- [ ] Keep add-type picking inline on desktop.
- [ ] After type selection, open stage editor in a separate desktop window.
- [ ] Allow multiple stage windows open simultaneously.
- [ ] Keep touch inline using the same shared form component.
- [ ] Commit the desktop stage window behavior.

### Task 8: Set the app icon to the logomark

**Files:**
- Modify: platform-specific app/window configuration files discovered during implementation
- Reuse: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/assets/openrig-logomark.svg`

- [ ] Locate the current desktop app icon configuration path.
- [ ] Apply the logomark-only asset.
- [ ] Verify the app/window icon path is wired correctly.
- [ ] Commit the icon change.

### Task 9: Regression verification

**Files:**
- Modify as needed after failures

- [ ] Verify launcher still opens projects and creates new ones.
- [ ] Verify chains screen shell matches the approved layout.
- [ ] Verify `In` and `Out` chips open dedicated windows and apply changes immediately.
- [ ] Verify chain config opens metadata only and launches endpoint windows.
- [ ] Verify adding/editing stage works on desktop with separate windows.
- [ ] Verify touch still renders the same forms inline.
- [ ] Run the relevant test/build commands.
- [ ] Commit the verification fixes.
