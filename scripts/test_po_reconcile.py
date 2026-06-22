#!/usr/bin/env python3
"""Tests for po_reconcile.py — translation recovery across key/context renames.

Run: pytest scripts/test_po_reconcile.py
"""
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).parent / "po_reconcile.py"


def _run(po: Path) -> None:
    subprocess.run([sys.executable, str(SCRIPT), str(po)], check=True)


def test_recovers_plain_translation(tmp_path):
    # A renamed key: active entry empty, obsolete entry holds the translation.
    po = tmp_path / "x.po"
    po.write_text(
        'msgid ""\n'
        'msgstr ""\n'
        '"Content-Type: text/plain; charset=UTF-8\\n"\n'
        "\n"
        "#: x.slint:1\n"
        'msgctxt "Dialog"\n'
        'msgid "label-close"\n'
        'msgstr ""\n'
        "\n"
        '#~ msgid "label-close"\n'
        '#~ msgstr "Fechar"\n',
        encoding="utf-8",
    )
    _run(po)
    out = po.read_text(encoding="utf-8")
    assert 'msgstr "Fechar"' in out
    assert "#~" not in out  # obsolete dropped


def test_recovers_quoted_translation_without_double_escaping(tmp_path):
    # The translation (and msgid) contain escaped quotes. Reconcile must copy
    # the already-escaped on-disk form verbatim — re-escaping it produces the
    # invalid `\\"` sequence msgfmt rejects (the bug behind issue #714).
    po = tmp_path / "x.po"
    po.write_text(
        'msgid ""\n'
        'msgstr ""\n'
        '"Content-Type: text/plain; charset=UTF-8\\n"\n'
        "\n"
        "#: x.slint:1\n"
        'msgctxt "ConfirmDeleteBlockDialog"\n'
        'msgid "Excluir \\"{}\\"?"\n'
        'msgstr ""\n'
        "\n"
        '#~ msgid "Excluir \\"{}\\"?"\n'
        '#~ msgstr "\\"{}\\" löschen?"\n',
        encoding="utf-8",
    )
    _run(po)
    out = po.read_text(encoding="utf-8")
    assert '\\\\"' not in out, f"double-escaped quote produced:\n{out}"
    assert 'msgstr "\\"{}\\" löschen?"' in out
    # Must compile cleanly under gettext.
    subprocess.run(
        ["msgfmt", "--check", "-o", "/dev/null", str(po)], check=True
    )
