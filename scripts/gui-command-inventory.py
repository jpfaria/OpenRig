#!/usr/bin/env python3
# Full inventory of every desktop GUI callback: whether it already goes
# through Command (and which), whether it changes state without one
# (the work left), or is screen-only. Mechanical — derived from source,
# not hand-written, so it can't overclaim. Re-run anytime:
#   python3 scripts/gui-command-inventory.py
import re, glob, os

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
files = sorted(
    f for f in glob.glob(os.path.join(root, "crates/adapter-gui/src/**/*.rs"), recursive=True)
    if "test" not in os.path.basename(f)
)
cb = re.compile(r"\.on_([a-z0-9_]+)\(")
cmd = re.compile(r"\.dispatch\(\s*Command::([A-Za-z0-9_]+)")
state_re = re.compile(
    r"session\.(project|rig)|\.borrow_mut\(\)\.(?!as_ref)|RigCommand|sync_synthetic|"
    r"switch_and_project|save_project_session|save_chain_|load_preset|migrate_legacy|"
    r"set_input_sources|\.remove_input\(|add_preset_to_input|remove_preset_from_input|"
    r"add_scene_to_input|remove_last_scene|write_back_|register_recent|persist|"
    r"FilesystemStorage|save_gui_audio|set_project_dirty"
)
done, todo, screen = [], [], []
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
        rel = os.path.relpath(f, root).replace("crates/adapter-gui/src/", "")
        cmds = sorted(set(cmd.findall(body)))
        if cmds:
            done.append((rel, name, "→ Command::" + ", Command::".join(cmds)))
        elif state_re.search(body):
            tok = state_re.search(body).group(0)
            todo.append((rel, name, f"muta estado direto ({tok}) — falta virar Command"))
        else:
            screen.append((rel, name, "abrir/fechar tela / render — regra de tela (ok)"))


def dump(title, rows):
    print(f"\n## {title} ({len(rows)})")
    for rel, n, d in sorted(rows):
        print(f"- `on_{n}`  [{rel}]  {d}")


print(f"# Inventário GUI↔Command  —  via Command: {len(done)} | "
      f"FALTA (estado sem Command): {len(todo)} | tela-only: {len(screen)}")
dump("JÁ via Command", done)
dump("FALTA — altera estado SEM Command (o trabalho)", todo)
dump("Tela-only (regra de tela — fora do escopo)", screen)
