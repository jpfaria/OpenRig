#!/usr/bin/env python3
# Rule (user-defined, not interpreted): opening/closing a screen =
# screen rule (allowed in GUI). ANY action that CHANGES STATE = business
# rule = must go through Command/dispatcher. This counts GUI callbacks
# that change state WITHOUT dispatching a Command. Pure open/close-view
# callbacks are excluded. Number must only go DOWN. Goal: 0.
#
#   python3 scripts/gui-command-coupling.py            # count
#   python3 scripts/gui-command-coupling.py --list     # offenders
#
# Exit code = offender count (0 = goal), so CI/a hook can assert it
# never increases.
import re, glob, os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
files = sorted(
    f for f in glob.glob(os.path.join(root, "crates/adapter-gui/src/**/*.rs"), recursive=True)
    if "test" not in os.path.basename(f)
)
cb = re.compile(r"\.on_([a-z0-9_]+)\(")
# A callback CHANGES STATE if its body mutates the model / persists /
# runs a domain mutation. (Opening a window, set_* on a Slint property,
# show/hide = screen, not state.)
state_re = re.compile(
    r"session\.(project|rig)|\.borrow_mut\(\)\.(?!as_ref)|RigCommand|sync_synthetic|"
    r"switch_and_project|save_project_session|save_chain_|load_preset|migrate_legacy|"
    r"set_input_sources|\.remove_input\(|add_preset_to_input|remove_preset_from_input|"
    r"add_scene_to_input|remove_last_scene|write_back_|register_recent|persist|"
    r"FilesystemStorage|save_gui_audio|set_project_dirty"
)
offenders, ok = [], 0
for f in files:
    s = open(f, encoding="utf-8", errors="replace").read()
    ms = list(cb.finditer(s))
    seen = set()
    for i, m in enumerate(ms):
        name = m.group(1)
        if (f, name) in seen:
            continue
        seen.add((f, name))
        body = s[m.start():(ms[i + 1].start() if i + 1 < len(ms) else len(s))]
        if ".dispatch(" in body:
            ok += 1
        elif state_re.search(body):
            offenders.append(f"{os.path.relpath(f, root)} :: on_{name}")
        # else: screen-only (open/close/render) — allowed, not counted.

print(f"state-changing GUI callbacks via Command: {ok}  |  "
      f"changing state WITHOUT Command (business in GUI): {len(offenders)}")
if "--list" in sys.argv:
    for o in offenders:
        print("  " + o)
sys.exit(min(len(offenders), 250))
