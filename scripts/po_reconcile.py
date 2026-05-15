#!/usr/bin/env python3
"""Recover human translations across key/context renames in a gettext .po.

slint-tr-extractor scopes every @tr() by its Slint component (msgctxt).
Renaming a key or moving a string to another component orphans the old
entry as obsolete (#~) and leaves the new entry untranslated, so it leaks
the raw key to the UI — the root cause of issue #446.

UI labels are context-independent ("Inputs" is "Inputs" in every window),
so this fills any active, empty msgstr from another entry that shares the
same msgid — preferring an active translation, falling back to an obsolete
one. Entries already translated are never touched. Obsolete (#~) entries
are dropped on output once mined for translations, so the catalog can't
re-accumulate the dead weight that masked issue #446. Dropping is done
here rather than via `msgattrib --no-obsolete` because some non-GNU
msgattrib builds wipe active msgstr. Single arg: the .po path.
"""
import re
import sys

MSGID = re.compile(r'^msgid "(.*)"$', re.M)
MSGSTR = re.compile(r'^msgstr "(.*)"$', re.M)


def main(path: str) -> None:
    blocks = open(path, encoding="utf-8").read().split("\n\n")

    active: dict[str, str] = {}
    obsolete: dict[str, str] = {}
    for blk in blocks:
        is_obsolete = blk.lstrip().startswith("#~")
        text = re.sub(r"(?m)^#~ ?", "", blk) if is_obsolete else blk
        mid, mstr = MSGID.search(text), MSGSTR.search(text)
        if not mid or not mstr or not mid.group(1) or not mstr.group(1):
            continue
        bucket = obsolete if is_obsolete else active
        bucket.setdefault(mid.group(1), mstr.group(1))

    out = []
    for blk in blocks:
        if blk.lstrip().startswith("#~"):
            continue  # drop obsolete (already mined into `obsolete` above)
        mid = MSGID.search(blk)
        if mid and mid.group(1) and re.search(r'^msgstr ""$', blk, re.M):
            repl = active.get(mid.group(1)) or obsolete.get(mid.group(1))
            if repl:
                escaped = repl.replace("\\", "\\\\").replace('"', '\\"')
                blk = re.sub(
                    r'^msgstr ""$', f'msgstr "{escaped}"', blk, flags=re.M
                )
        out.append(blk)

    open(path, "w", encoding="utf-8").write("\n\n".join(out))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: po_reconcile.py <path-to-.po>")
    main(sys.argv[1])
