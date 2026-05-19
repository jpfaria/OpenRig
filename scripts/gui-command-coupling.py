#!/usr/bin/env python3
# Measures GUI<->business coupling: every desktop GUI callback that does
# NOT route through the Command/dispatcher (`.dispatch(`). The rule is
# "GUI has no business logic — everything via Command". This is the
# objective metric; run it yourself, don't trust a "done" claim. The
# number must only go DOWN. Goal: 0.
#
#   python3 scripts/gui-command-coupling.py            # summary + count
#   python3 scripts/gui-command-coupling.py --list     # every offender
#
# Exit code = number of non-dispatch callbacks (0 = goal reached), so
# CI / a hook can assert it never increases.
import re, glob, os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
files = sorted(
    f for f in glob.glob(os.path.join(root, "crates/adapter-gui/src/**/*.rs"), recursive=True)
    if "test" not in os.path.basename(f)
)
cb = re.compile(r"\.on_([a-z0-9_]+)\(")
offenders, dispatch = [], 0
for f in files:
    s = open(f, encoding="utf-8", errors="replace").read()
    ms = list(cb.finditer(s))
    seen = set()
    for i, m in enumerate(ms):
        name = m.group(1)
        body = s[m.start():(ms[i + 1].start() if i + 1 < len(ms) else len(s))]
        key = (f, name)
        if key in seen:
            continue
        seen.add(key)
        if ".dispatch(" in body:
            dispatch += 1
        else:
            offenders.append(f"{os.path.relpath(f, root)} :: on_{name}")

total = dispatch + len(offenders)
print(f"GUI callbacks: {total}  |  via Command: {dispatch}  |  "
      f"NOT via Command (coupling): {len(offenders)}")
if "--list" in sys.argv:
    for o in offenders:
        print("  " + o)
sys.exit(min(len(offenders), 250))
