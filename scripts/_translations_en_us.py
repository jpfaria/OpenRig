#!/usr/bin/env python3
"""
Populate en-US/adapter-gui.po with English translations for every msgstr.

This is a one-shot helper to translate the gettext catalog after the initial
extraction. Each future @tr() addition will need a manual entry here.

The same approach as for locales/en-US.yml on the Rust side: keys are the
pt-BR source strings, values are English. Symbols/punctuation that don't
need translation are left empty (gettext falls back to msgid).
"""
import re
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PO = REPO / "crates" / "adapter-gui" / "translations" / "en-US" / "adapter-gui.po"

# Keys are pt-BR source strings exactly as they appear in @tr(...).
# Empty value = leave msgstr "" (gettext returns msgid, e.g. for "OK", "+",
# numeric tick labels, single glyphs).
TRANSLATIONS = {
    # Header / launcher
    "OpenRig": "",
    "Buscar projetos": "Search projects",
    "Novo Projeto": "New Project",
    "Idioma": "Language",
    "🌐  {}": "🌐  {}",

    # Project setup
    "Nome do projeto": "Project name",
    "Ex: Guitarra ao vivo": "e.g. Live Guitar",

    # Project chains / chain row
    "Entrada": "Input",
    "Saída": "Output",
    "Entradas": "Inputs",
    "Saidas": "Outputs",
    "LAT": "LAT",
    "MUTE": "MUTE",
    "Carregar Preset": "Load Preset",

    # Compact chain view
    "IN": "IN",
    "OUT": "OUT",
    "NO ACTIVE INPUTS": "NO ACTIVE INPUTS",
    "NO ACTIVE OUTPUTS": "NO ACTIVE OUTPUTS",

    # Confirm delete dialog
    "Excluir bloco": "Delete block",
    "Cancelar": "Cancel",
    "Excluir": "Delete",

    # Model picker
    "Buscar modelos...": "Search models...",
    "Nenhum modelo encontrado": "No models found",

    # Chain editor
    "Nome": "Name",
    "Instrumento": "Instrument",

    # Chain endpoint editor / chain insert editor
    "Dispositivo": "Device",
    "Modo": "Mode",
    "Canais": "Channels",
    "Envio": "Send",
    "Retorno": "Return",

    # Project settings / device settings
    "Dispositivos de áudio": "Audio devices",
    "Buffer": "Buffer",
    "Bits": "Bits",
    "Hz": "Hz",

    # Tuner / spectrum
    "TUNER": "TUNER",
    "SPECTRUM": "SPECTRUM",
    "Enable a chain with a configured input to see its tuners here.":
        "Enable a chain with a configured input to see its tuners here.",
    "Power on the spectrum and enable a chain with a configured output.":
        "Power on the spectrum and enable a chain with a configured output.",

    # Plugin info
    "Abrir Plugin": "Open Plugin",
    "Description": "Description",
    "License:": "License:",
    "Open Homepage": "Open Homepage",

    # Symbols / glyphs / numeric ticks — leave untranslated
    "–": "",
    "−": "",
    "+": "",
    "▼": "",
    "▴": "",
    "▾": "",
    "🔍": "",
    "✕": "",
    "✓": "",
    "›": "",
    "20": "",
    "100": "",
    "1k": "",
    "10k": "",
    "20k": "",
}


def main():
    text = PO.read_text(encoding="utf-8")

    # Walk every msgid → msgstr block and inject translation when known.
    # Pattern: msgid "X"\nmsgstr "Y"  (Y currently empty)
    pattern = re.compile(r'msgid "((?:[^"\\]|\\.)*)"\nmsgstr ""', re.MULTILINE)

    def replace(m):
        key = m.group(1)
        if key in TRANSLATIONS:
            value = TRANSLATIONS[key]
            if value:
                return f'msgid "{key}"\nmsgstr "{value}"'
        return m.group(0)

    new_text, n = pattern.subn(replace, text)
    PO.write_text(new_text, encoding="utf-8")
    print(f"populated {sum(1 for v in TRANSLATIONS.values() if v)} translations into {PO.relative_to(REPO)}")


if __name__ == "__main__":
    main()
