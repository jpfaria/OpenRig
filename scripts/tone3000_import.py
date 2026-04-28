#!/usr/bin/env python3
"""Bulk-import Tone3000 NAM amp/preamp captures into the OpenRig catalog.

For each target spec (make_name + dest_kind + slug + display_name + brand)
the script:
  1. Calls Tone3000 `search_tones_a2` and picks the top tone(s) by
     downloads_count (filters: platform=nam, has_model_with_url=true,
     gear matches dest_kind).
  2. Fetches `/tones?id=eq.<id>&select=*,models(*)` to get the .nam
     files in the pack.
  3. Downloads each .nam to `captures/nam/{amps,preamp}/<slug>/`.
     De-duplicates by `(name+size)` and caps at MAX_CAPTURES_PER_MODEL
     to keep the catalog manageable.
  4. Codegens `crates/block-{amp,preamp}/src/nam_<slug>.rs` with a single
     `enum_parameter("capture", ...)` that lists every downloaded file.

The script does NOT touch the build registry — `build.rs` auto-detects
new modules with `MODEL_DEFINITION`.

Usage:
    python3 scripts/tone3000_import.py specs.json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
import unicodedata
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

# Anonymous Supabase JWT exposed by the Tone3000 frontend — same key for
# every browser session. Public by design (PostgREST `anon` role).
ANON = (
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9."
    "eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Imd6eWJpdW9weGtkeGJ5dG5vamRzIiwicm9sZSI6ImFub24iLCJpYXQiOjE3MzgwODIxNjUsImV4cCI6MjA1MzY1ODE2NX0."
    "Gq66BJXjtLsqP2nAGXm9Xb9PAjoeZalWUj66K4nmVSU"
)
API = "https://api.tone3000.com"
SEARCH_URL = f"{API}/rest/v1/rpc/search_tones_a2"
TONES_URL = f"{API}/rest/v1/tones"

HEADERS = {
    "apikey": ANON,
    "authorization": f"Bearer {ANON}",
    "content-type": "application/json",
    "content-profile": "public",
}

MAX_CAPTURES_PER_MODEL = 8
TIMEOUT = 30
SLEEP_BETWEEN = 0.2


# --- helpers ---------------------------------------------------------------

def slugify(value: str) -> str:
    value = unicodedata.normalize("NFKD", value)
    value = value.encode("ascii", "ignore").decode("ascii")
    value = re.sub(r"[^A-Za-z0-9]+", "_", value).strip("_").lower()
    return re.sub(r"_+", "_", value) or "x"


def http_post(url: str, body: dict[str, Any]) -> Any:
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode("utf-8"),
        headers=HEADERS,
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
        return json.loads(resp.read())


def http_get(url: str) -> Any:
    req = urllib.request.Request(url, headers=HEADERS, method="GET")
    with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
        return json.loads(resp.read())


def http_get_bytes(url: str) -> bytes:
    req = urllib.request.Request(url, method="GET")
    with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
        return resp.read()


def search_make(make_name: str, page_size: int = 50) -> list[dict[str, Any]]:
    body = {
        "query_term": "",
        "page_number": 1,
        "page_size": page_size,
        "order_by": "trending",
        "tag_names": None,
        "make_names": [make_name],
        "gear_filters": None,
        "is_calibrated": False,
        "size_filters": None,
        "usernames": None,
    }
    return http_post(SEARCH_URL, body) or []


def fetch_tone(tone_id: int) -> dict[str, Any] | None:
    qs = urllib.parse.urlencode({"id": f"eq.{tone_id}", "select": "*,models(*)"})
    url = f"{TONES_URL}?{qs}"
    data = http_get(url) or []
    return data[0] if data else None


# --- selection -------------------------------------------------------------

def is_amp_pack(tone: dict[str, Any]) -> bool:
    """`full-rig` (amp+cab) or `amp` (NAM-captured full amp) gear."""
    return tone.get("gear") in ("full-rig", "amp") and tone.get("platform") == "nam"


def is_preamp_pack(tone: dict[str, Any]) -> bool:
    """Preamp/head captures — used as preamp blocks (no cab)."""
    return tone.get("gear") in ("preamp", "head") and tone.get("platform") == "nam"


def select_models(models: list[dict[str, Any]], limit: int) -> list[dict[str, Any]]:
    """Keep `standard` size, drop duplicates (same name), cap at `limit`.

    `feather`, `lite`, `nano` sizes are quality-reduced versions of the
    same capture for low-CPU setups — we ship `standard` and let users
    choose the size at runtime via the NAM block size discovery (issue
    #336 conventions). Also dedupes on lowercased name.
    """
    seen: set[str] = set()
    out: list[dict[str, Any]] = []
    # Prefer standard size first
    ordered = sorted(
        models,
        key=lambda m: (m.get("size") != "standard", m.get("position") or 9999),
    )
    for m in ordered:
        if m.get("is_deleted"):
            continue
        url = m.get("model_url")
        if not url or not url.endswith(".nam"):
            continue
        key = slugify((m.get("name") or "").lower())
        if not key or key in seen:
            continue
        seen.add(key)
        out.append(m)
        if len(out) >= limit:
            break
    return out


# --- codegen ---------------------------------------------------------------

# Minimal-coupling template: a single `capture` enum parameter exposing
# every downloaded .nam by its source name. No knob inference from
# filename — keeps the script generic across packs with arbitrary naming.
AMP_TEMPLATE = '''\
use anyhow::{{anyhow, Result}};
use crate::registry::{{AmpBackendKind, AmpModelDefinition}};
use nam::{{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{{NamPluginParams, DEFAULT_PLUGIN_PARAMS}},
}};
use block_core::param::{{enum_parameter, required_string, ModelParameterSchema, ParameterSet}};
use block_core::{{AudioChannelLayout, BlockProcessor}};

pub const MODEL_ID: &str = "{model_id}";
pub const DISPLAY_NAME: &str = "{display_name}";
const BRAND: &str = "{brand}";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
{capture_rows}
];

pub fn model_schema() -> ModelParameterSchema {{
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some({default_key}),
        &[
{enum_options}
        ],
    )];
    schema
}}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {{
    let path = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {{
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{{}}' has no capture '{{}}'", MODEL_ID, key))
}}

fn schema() -> Result<ModelParameterSchema> {{
    Ok(model_schema())
}}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {{
    build_processor_for_model(params, sample_rate, layout)
}}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {{
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
}};

pub fn validate_params(params: &ParameterSet) -> Result<()> {{
    resolve_capture(params).map(|_| ())
}}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {{
    let path = resolve_capture(params)?;
    Ok(format!("model='{{}}'", path))
}}
'''

PREAMP_TEMPLATE = '''\
use anyhow::{{anyhow, Result}};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{{plugin_params_from_set_with_defaults, NamPluginParams}},
}};
use block_core::param::{{enum_parameter, required_string, ModelParameterSchema, ParameterSet}};
use block_core::{{AudioChannelLayout, BlockProcessor}};

pub const MODEL_ID: &str = "{model_id}";
pub const DISPLAY_NAME: &str = "{display_name}";
const BRAND: &str = "{brand}";

pub const NAM_PLUGIN_DEFAULTS: NamPluginParams = NamPluginParams {{
    input_level_db: 0.0,
    output_level_db: 0.0,
    noise_gate_threshold_db: -80.0,
    noise_gate_enabled: true,
    eq_enabled: true,
    bass: 5.0,
    middle: 5.0,
    treble: 5.0,
}};

const CAPTURES: &[(&str, &str, &str)] = &[
{capture_rows}
];

pub fn model_schema() -> ModelParameterSchema {{
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some({default_key}),
        &[
{enum_options}
        ],
    )];
    schema
}}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {{
    let path = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = nam::resolve_nam_capture(path)?;
    build_processor_with_assets_for_layout(&model_path, None, plugin_params, sample_rate, layout)
}}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {{
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("preamp '{{}}' has no capture '{{}}'", MODEL_ID, key))
}}

fn schema() -> Result<ModelParameterSchema> {{
    Ok(model_schema())
}}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {{
    build_processor_for_model(params, sample_rate, layout)
}}

pub const MODEL_DEFINITION: PreampModelDefinition = PreampModelDefinition {{
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: PreampBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
}};

pub fn validate_params(params: &ParameterSet) -> Result<()> {{
    resolve_capture(params).map(|_| ())
}}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {{
    let path = resolve_capture(params)?;
    Ok(format!("asset_id='{{}}'", path))
}}
'''


def rust_str(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


def render_template(template: str, *, model_id: str, display_name: str, brand: str,
                    captures: list[tuple[str, str, str]]) -> str:
    """`captures` is a list of `(key, label, path)` tuples."""
    if not captures:
        raise ValueError(f"no captures for {model_id}")
    rows = "\n".join(
        f'    ("{rust_str(k)}", "{rust_str(lbl)}", "{rust_str(p)}"),'
        for (k, lbl, p) in captures
    )
    enum_opts = "\n".join(
        f'            ("{rust_str(k)}", "{rust_str(lbl)}"),'
        for (k, lbl, _) in captures
    )
    default_key = f'"{rust_str(captures[0][0])}"'
    return template.format(
        model_id=model_id,
        display_name=rust_str(display_name),
        brand=rust_str(brand),
        capture_rows=rows,
        enum_options=enum_opts,
        default_key=default_key,
    )


# --- main pipeline ---------------------------------------------------------

def import_one(spec: dict[str, Any], repo_root: Path, *, dry_run: bool = False) -> dict[str, Any]:
    make = spec["make"]
    kind = spec["kind"]            # "amp" | "preamp"
    slug = spec["slug"]            # e.g. "fender_hot_rod_deluxe"
    display = spec["display"]
    brand = spec["brand"]
    pick_tone_ids = spec.get("tone_ids")  # explicit override; else top-by-downloads
    max_captures = spec.get("max_captures", MAX_CAPTURES_PER_MODEL)

    print(f"\n=== {make}  →  nam_{slug}  ({kind}) ===")

    candidates = search_make(make)
    if pick_tone_ids:
        tones = [t for t in candidates if t["id"] in pick_tone_ids]
    else:
        if kind == "amp":
            tones = [t for t in candidates if is_amp_pack(t) and t.get("has_model_with_url")]
        else:
            tones = [t for t in candidates
                     if (is_preamp_pack(t) or is_amp_pack(t)) and t.get("has_model_with_url")]
        tones.sort(key=lambda t: t.get("downloads_count") or 0, reverse=True)
        tones = tones[:1]  # default: just the top pack

    if not tones:
        print("  ! no usable tone packs found; skipping")
        return {"skipped": True, "reason": "no packs"}

    selected_models: list[tuple[dict[str, Any], dict[str, Any]]] = []
    for t in tones:
        full = fetch_tone(t["id"])
        if not full:
            continue
        kept = select_models(full.get("models") or [], max_captures - len(selected_models))
        for m in kept:
            selected_models.append((t, m))
        if len(selected_models) >= max_captures:
            break
        time.sleep(SLEEP_BETWEEN)

    if not selected_models:
        print("  ! no models in selected tone(s); skipping")
        return {"skipped": True, "reason": "no models"}

    bucket = "amps" if kind == "amp" else "preamp"
    captures_dir = repo_root / "captures" / "nam" / bucket / slug

    # Skip if a previous run already produced captures for this slug — re-running
    # the pipeline must NOT pollute the directory with `_2.nam` duplicates. To
    # re-import, delete the directory and the matching .rs file first.
    rs_crate = "block-amp" if kind == "amp" else "block-preamp"
    rs_existing = repo_root / "crates" / rs_crate / "src" / f"nam_{slug}.rs"
    if captures_dir.exists() and any(captures_dir.glob("*.nam")) and rs_existing.exists():
        print(f"  ↷ already imported (captures dir + .rs exist) — skipping")
        return {"skipped": True, "reason": "already imported"}

    captures_dir.mkdir(parents=True, exist_ok=True)

    capture_entries: list[tuple[str, str, str]] = []
    seen_keys: set[str] = set()

    for tone, model in selected_models:
        url = model["model_url"]
        raw_name = model.get("name") or model["model_url"].rsplit("/", 1)[-1]
        size = (model.get("size") or "standard").lower()
        # short, stable, filesystem-safe filename
        base = slugify(raw_name)[:60] or f"capture_{model['id']}"
        if size != "standard":
            base = f"{base}_{size}"
        # avoid collisions on disk
        filename = f"{base}.nam"
        idx = 2
        while (captures_dir / filename).exists():
            filename = f"{base}_{idx}.nam"
            idx += 1
        target = captures_dir / filename

        if not dry_run:
            print(f"  ↓ {filename}  ({raw_name[:50]})")
            data = http_get_bytes(url)
            target.write_bytes(data)
            time.sleep(SLEEP_BETWEEN)

        # enum key — short, stable across re-runs
        key = slugify(raw_name)[:32] or f"c{model['id']}"
        if size != "standard":
            key = f"{key}_{size}"
        if key in seen_keys:
            key = f"{key}_{model['id']}"
        seen_keys.add(key)

        # human label — keep original casing/spacing
        label = (raw_name or filename)[:60].strip() or "Capture"
        rel_path = f"{bucket}/{slug}/{filename}"
        capture_entries.append((key, label, rel_path))

    if not capture_entries:
        return {"skipped": True, "reason": "no downloads"}

    # codegen
    crate = "block-amp" if kind == "amp" else "block-preamp"
    rs_path = repo_root / "crates" / crate / "src" / f"nam_{slug}.rs"
    template = AMP_TEMPLATE if kind == "amp" else PREAMP_TEMPLATE
    src = render_template(
        template,
        model_id=f"nam_{slug}",
        display_name=display,
        brand=brand,
        captures=capture_entries,
    )
    if not dry_run:
        rs_path.write_text(src)

    print(f"  ✓ wrote {len(capture_entries)} captures + {rs_path.relative_to(repo_root)}")
    return {
        "ok": True,
        "captures": len(capture_entries),
        "rs_path": str(rs_path.relative_to(repo_root)),
        "captures_dir": str(captures_dir.relative_to(repo_root)),
        "tone_ids": [t["id"] for (t, _) in selected_models],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("specs", help="JSON file with list of import specs")
    parser.add_argument("--repo-root", default=".",
                        help="Path to repo root (default: cwd)")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--only",
                        help="Comma-separated slugs to process; default: all")
    args = parser.parse_args()

    specs = json.loads(Path(args.specs).read_text())
    repo_root = Path(args.repo_root).resolve()

    if args.only:
        wanted = set(s.strip() for s in args.only.split(","))
        specs = [s for s in specs if s["slug"] in wanted]

    summary = []
    for spec in specs:
        try:
            result = import_one(spec, repo_root, dry_run=args.dry_run)
        except Exception as e:
            print(f"  !! error on {spec.get('slug')}: {e}")
            result = {"error": str(e)}
        summary.append({"slug": spec["slug"], **result})

    print("\n=== SUMMARY ===")
    for r in summary:
        print(json.dumps(r))
    return 0


if __name__ == "__main__":
    sys.exit(main())
