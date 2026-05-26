#!/usr/bin/env python3
"""Convert the official Facebook Research Demucs v4 (htdemucs) PyTorch
checkpoint into a single ONNX file consumable by OpenRig's
`feature-stems` crate when built with `--features real-htdemucs`.

Usage
-----
    pipx install demucs           # or: pip install demucs onnx torch
    python scripts/convert-htdemucs-to-onnx.py

The output is written to the OS data dir so the running app picks it
up without any extra config:

    macOS   ~/Library/Application Support/OpenRig/models/htdemucs/htdemucs.onnx
    Linux   ~/.local/share/OpenRig/models/htdemucs/htdemucs.onnx
    Windows %APPDATA%\\OpenRig\\models\\htdemucs\\htdemucs.onnx

Override the destination with `--out PATH`.

Input contract — `(batch=1, channels=2, samples)` float32 at 44.1 kHz.
Output contract — `(batch=1, stems=4, channels=2, samples)` float32 in
the canonical order `[drums, bass, vocals, other]`.

Conversion notes
----------------
- Uses `torch.onnx.export` with `opset_version=17` (needed for the STFT
  ops Demucs v4 uses internally).
- Dynamic axis on the sample dimension so the Rust caller can feed any
  chunk length.
- The official `htdemucs` weights are public via the `demucs` package
  (it downloads them on first use into `~/.cache/torch/hub/checkpoints`).
"""

from __future__ import annotations

import argparse
import platform
import sys
from pathlib import Path


def default_out() -> Path:
    system = platform.system()
    if system == "Darwin":
        root = Path.home() / "Library" / "Application Support"
    elif system == "Windows":
        appdata = Path(os.environ.get("APPDATA", Path.home()))  # noqa: F821 — guarded import below
        root = appdata
    else:
        root = Path.home() / ".local" / "share"
    return root / "OpenRig" / "models" / "htdemucs" / "htdemucs.onnx"


def main() -> int:
    import os  # noqa: PLC0415 — guard so the script can be parsed everywhere

    global default_out  # type: ignore[misc]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out",
        type=Path,
        default=None,
        help="Output .onnx path (default: OS data dir, OpenRig convention).",
    )
    parser.add_argument(
        "--chunk-frames",
        type=int,
        default=44_100 * 5,
        help="Fixed sample axis used as the dummy input during export.",
    )
    args = parser.parse_args()

    out: Path = args.out or default_out()
    out.parent.mkdir(parents=True, exist_ok=True)

    try:
        import torch
        from demucs.pretrained import get_model
    except ImportError as err:
        print(
            f"missing dependency ({err}). Install with:\n"
            "    pipx install demucs\n"
            "    pip install onnx torch\n",
            file=sys.stderr,
        )
        return 1

    print(f"Loading htdemucs weights via the `demucs` package…")
    bag = get_model("htdemucs")
    model = bag.models[0]
    model.eval()

    dummy = torch.zeros(1, 2, args.chunk_frames, dtype=torch.float32)

    print(f"Exporting to {out}…")
    torch.onnx.export(
        model,
        dummy,
        out,
        input_names=["mix"],
        output_names=["stems"],
        opset_version=17,
        dynamic_axes={
            "mix": {2: "samples"},
            "stems": {3: "samples"},
        },
        do_constant_folding=True,
    )

    print(f"OK. Run OpenRig with `--features real-htdemucs` to pick it up.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
