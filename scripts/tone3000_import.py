#!/usr/bin/env python3
"""Tone3000 download + metadata dump.

Scope is intentionally limited: this script ONLY fetches assets and
captures metadata from Tone3000. It does NOT generate the final Rust
model files — those require per-pack semantic analysis (axes vary
across packs: mic position × distance × preamp, channel × gain knob,
voicing × cab-load, etc) and are written by hand via IA review.

For each target spec (make_name + dest_kind + slug + display_name + brand)
the script:
  1. Resolves the source tone(s) via Tone3000 RPC (`search_tones_a2`)
     OR by explicit `tone_ids` list in the spec.
  2. Fetches `/tones?id=eq.<id>&select=*,models(*)` to enumerate the
     .nam / .wav models in each pack.
  3. Downloads files to `captures/nam/{amps,preamp}/<slug>/` or
     `captures/ir/cabs/<slug>/`. Skips bytes already on disk.
  4. Writes a metadata dump alongside each pack (`_metadata.json`) so
     the IA review step has the original capture names + sizes available
     when crafting the .rs file.
  5. Emits a stub `nam_<slug>.rs` / `ir_<slug>.rs` with a single
     `enum_parameter("capture", ...)` as a STARTING POINT only. The
     stub WILL need manual restructuring per pack — usually splitting
     into 2-3 `enum_parameter`s once the axes are clear from the names.

The script does NOT touch the build registry — `build.rs` auto-detects
new modules with `MODEL_DEFINITION`.

Per-pack manual review checklist:
  • Open the `.rs` stub the script produced
  • Open the `_metadata.json` next to the .nam/.wav files
  • Identify the axes by inspecting the capture names
  • Rewrite CAPTURES tuple as (axis1, axis2, ..., file_path)
  • Replace the single `enum_parameter("capture", ...)` with one per axis
  • Update resolve_capture lookup accordingly
  • See `ir_marshall_1960tv_greenback.rs` for a 3-axis canonical example

Usage:
    python3 scripts/tone3000_import.py specs.json
    python3 scripts/tone3000_import.py specs.json --regen-only   # refresh stubs without re-downloading
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
    """Fetch tone metadata + its models. Big packs (>300 models) cause
    PostgREST to 500 on the embedded `select=*,models(*)`, so we fall
    back to two slim queries."""
    qs = urllib.parse.urlencode({"id": f"eq.{tone_id}", "select": "*,models(*)"})
    try:
        data = http_get(f"{TONES_URL}?{qs}") or []
        return data[0] if data else None
    except urllib.error.HTTPError as e:
        if e.code != 500:
            raise
        # Fallback: separate queries
        tone_qs = urllib.parse.urlencode({"id": f"eq.{tone_id}", "select": "*"})
        tones = http_get(f"{TONES_URL}?{tone_qs}") or []
        if not tones:
            return None
        tone = tones[0]
        # Fetch models in pages of 100, prefer standard size
        models: list[dict[str, Any]] = []
        for offset in range(0, 1000, 100):
            mqs = urllib.parse.urlencode({
                "tone_id": f"eq.{tone_id}",
                "select": "id,name,size,model_url,position,is_deleted,tone_id",
                "size": "eq.standard",
                "order": "position.asc",
                "limit": "100",
                "offset": str(offset),
            })
            chunk = http_get(f"{API}/rest/v1/models?{mqs}") or []
            models.extend(chunk)
            if len(chunk) < 100:
                break
        tone["models"] = models
        return tone


# --- selection -------------------------------------------------------------

def is_amp_pack(tone: dict[str, Any]) -> bool:
    """`full-rig` (amp+cab) or `amp` (NAM-captured full amp) gear."""
    return tone.get("gear") in ("full-rig", "amp") and tone.get("platform") == "nam"


def is_preamp_pack(tone: dict[str, Any]) -> bool:
    """Preamp/head captures — used as preamp blocks (no cab)."""
    return tone.get("gear") in ("preamp", "head") and tone.get("platform") == "nam"


def is_cab_pack(tone: dict[str, Any]) -> bool:
    """IR cab pack."""
    return tone.get("gear") == "ir" and tone.get("platform") == "ir"


def select_models(
    models: list[dict[str, Any]],
    limit: int,
    *,
    expected_ext: str = ".nam",
) -> list[dict[str, Any]]:
    """Keep preferred size, drop duplicates (same name), cap at `limit`.

    For NAM packs: prefer `size=standard` (quality-reduced `feather`/
    `lite`/`nano` sizes are low-CPU variants of the same capture).
    For IR packs: `size` is None — order by `position`. Filters by
    `expected_ext` so a `.nam` model in an IR pack (or vice-versa)
    won't sneak in. Dedupes on lowercased name.
    """
    seen: set[str] = set()
    out: list[dict[str, Any]] = []
    # Prefer standard size first (no-op for IRs where size is None)
    ordered = sorted(
        models,
        key=lambda m: (m.get("size") != "standard", m.get("position") or 9999),
    )
    for m in ordered:
        if m.get("is_deleted"):
            continue
        url = m.get("model_url")
        if not url or not url.endswith(expected_ext):
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

CAB_TEMPLATE = '''\
use anyhow::{{anyhow, bail, Result}};
use ir::{{build_mono_ir_processor_from_wav, IrAsset}};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{{enum_parameter, required_string, ModelParameterSchema, ParameterSet}};
use block_core::{{AudioChannelLayout, ModelAudioMode, BlockProcessor}};

pub const MODEL_ID: &str = "{model_id}";
pub const DISPLAY_NAME: &str = "{display_name}";
const BRAND: &str = "{brand}";

const CAPTURES: &[(&str, &str, &str)] = &[
{capture_rows}
];

pub fn model_schema() -> ModelParameterSchema {{
    ModelParameterSchema {{
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "capture",
            "Capture",
            Some("Cab"),
            Some({default_key}),
            &[
{enum_options}
            ],
        )],
    }}
}}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {{
    match layout {{
        AudioChannelLayout::Mono => {{
            let path = resolve_capture(params)?;
            let wav_path = ir::resolve_ir_capture(path)?;
            let ir = IrAsset::load_from_wav(&wav_path)?;
            if ir.channel_count() != 1 {{
                bail!(
                    "cab model '{{}}' capture must be mono, got {{}} channels",
                    MODEL_ID,
                    ir.channel_count()
                );
            }}
            let processor = build_mono_ir_processor_from_wav(&wav_path, sample_rate)?;
            Ok(BlockProcessor::Mono(processor))
        }}
        AudioChannelLayout::Stereo => bail!(
            "cab model '{{}}' currently expects mono processor layout",
            MODEL_ID
        ),
    }}
}}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {{
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("cab '{{}}' has no capture '{{}}'", MODEL_ID, key))
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

pub const MODEL_DEFINITION: CabModelDefinition = CabModelDefinition {{
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: CabBackendKind::Ir,
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

KIND_CONFIG = {
    "amp":    {"captures_root": "captures/nam/amps",    "crate": "block-amp",    "asset_subpath": "amps",   "ext": ".nam", "rs_prefix": "nam_"},
    "preamp": {"captures_root": "captures/nam/preamp",  "crate": "block-preamp", "asset_subpath": "preamp", "ext": ".nam", "rs_prefix": "nam_"},
    "cab":    {"captures_root": "captures/ir/cabs",     "crate": "block-cab",    "asset_subpath": "cabs",   "ext": ".wav", "rs_prefix": "ir_"},
}


def import_one(spec: dict[str, Any], repo_root: Path, *, dry_run: bool = False, regen_only: bool = False) -> dict[str, Any]:
    make = spec["make"]
    kind = spec["kind"]            # "amp" | "preamp" | "cab"
    slug = spec["slug"]            # e.g. "fender_hot_rod_deluxe"
    display = spec["display"]
    brand = spec["brand"]
    pick_tone_ids = spec.get("tone_ids")  # explicit override; else top-by-downloads
    max_captures = spec.get("max_captures", MAX_CAPTURES_PER_MODEL)

    if kind not in KIND_CONFIG:
        return {"error": f"unknown kind '{kind}'"}
    cfg = KIND_CONFIG[kind]
    rs_prefix = cfg["rs_prefix"]

    print(f"\n=== {make}  →  {rs_prefix}{slug}  ({kind}) ===")

    if pick_tone_ids:
        # Direct ID fetch — bypass make_names search entirely. The make
        # field on the spec doesn't have to match a Tone3000 canonical
        # make name when explicit tone_ids are supplied.
        tones = []
        for tid in pick_tone_ids:
            t = fetch_tone(tid)
            if t:
                # Stub the fields used downstream by select_models / dedup
                tones.append({
                    "id": tid,
                    "downloads_count": 0,
                    "gear": t.get("gear"),
                    "platform": t.get("platform"),
                    "has_model_with_url": True,
                    "_prefetched": t,
                })
            time.sleep(SLEEP_BETWEEN)
    else:
        candidates = search_make(make)
        if kind == "amp":
            tones = [t for t in candidates if is_amp_pack(t) and t.get("has_model_with_url")]
        elif kind == "preamp":
            tones = [t for t in candidates
                     if (is_preamp_pack(t) or is_amp_pack(t)) and t.get("has_model_with_url")]
        else:  # cab
            tones = [t for t in candidates if is_cab_pack(t) and t.get("has_model_with_url")]
        tones.sort(key=lambda t: t.get("downloads_count") or 0, reverse=True)
        tones = tones[:1]  # default: just the top pack

    if not tones:
        print("  ! no usable tone packs found; skipping")
        return {"skipped": True, "reason": "no packs"}

    selected_models: list[tuple[dict[str, Any], dict[str, Any]]] = []
    for t in tones:
        full = t.get("_prefetched") or fetch_tone(t["id"])
        if not full:
            continue
        kept = select_models(
            full.get("models") or [],
            max_captures - len(selected_models),
            expected_ext=cfg["ext"],
        )
        for m in kept:
            selected_models.append((t, m))
        if len(selected_models) >= max_captures:
            break
        if "_prefetched" not in t:
            time.sleep(SLEEP_BETWEEN)

    if not selected_models:
        print("  ! no models in selected tone(s); skipping")
        return {"skipped": True, "reason": "no models"}

    captures_dir = repo_root / cfg["captures_root"] / slug

    # Skip if a previous run already produced captures for this slug — re-running
    # the pipeline must NOT pollute the directory with `_2.<ext>` duplicates. To
    # re-import, delete the directory and the matching .rs file first.
    rs_crate = cfg["crate"]
    rs_existing = repo_root / "crates" / rs_crate / "src" / f"{rs_prefix}{slug}.rs"
    if not regen_only and captures_dir.exists() and any(captures_dir.glob(f"*{cfg['ext']}")) and rs_existing.exists():
        print(f"  ↷ already imported (captures dir + .rs exist) — skipping")
        return {"skipped": True, "reason": "already imported"}

    captures_dir.mkdir(parents=True, exist_ok=True)

    # --- key derivation: drop tokens that appear in EVERY name ---
    # Tone3000 curators repeat the same boring stuff in every capture
    # name ("M25 LL 1960TV 4x12 SM57 1.50in 0.0in OA30 SA73" has 6 tokens
    # identical across 8 captures). Naive slug + truncate produces opaque
    # keys. We tokenise on whitespace, find the case-insensitive token
    # set that appears in *every* name, and drop those — leaving only the
    # distinguishing pieces. Order is preserved per-name so the result
    # is still readable.
    raw_names = [m.get("name") or m["model_url"].rsplit("/", 1)[-1]
                 for (_, m) in selected_models]
    token_lists = [n.split() for n in raw_names]

    common_tokens: set[str] = set()
    if len(token_lists) > 1:
        common_tokens = set(t.lower() for t in token_lists[0])
        for tl in token_lists[1:]:
            common_tokens &= set(t.lower() for t in tl)
        # Don't strip *everything* — need at least one distinguishing
        # token left in each name. Bail if any name would be fully
        # consumed.
        if any(all(t.lower() in common_tokens for t in tl) for tl in token_lists):
            common_tokens = set()

    capture_entries: list[tuple[str, str, str]] = []
    seen_keys: set[str] = set()

    for (tone, model), raw_name, tokens in zip(selected_models, raw_names, token_lists):
        url = model["model_url"]
        size = (model.get("size") or "standard").lower()
        # short, stable, filesystem-safe filename
        base = slugify(raw_name)[:60] or f"capture_{model['id']}"
        if size != "standard":
            base = f"{base}_{size}"
        # avoid collisions on disk
        ext = cfg["ext"]
        filename = f"{base}{ext}"
        idx = 2
        while (captures_dir / filename).exists():
            filename = f"{base}_{idx}{ext}"
            idx += 1
        target = captures_dir / filename

        if not dry_run and not regen_only:
            print(f"  ↓ {filename}  ({raw_name[:50]})")
            data = http_get_bytes(url)
            target.write_bytes(data)
            time.sleep(SLEEP_BETWEEN)
        elif regen_only and not target.exists():
            # Regen mode: bytes should already be on disk. If a file is
            # missing for whatever reason (rare), fall back to download.
            print(f"  ↓ {filename}  (regen-only fallback download)")
            data = http_get_bytes(url)
            target.write_bytes(data)
            time.sleep(SLEEP_BETWEEN)

        # enum key — slug only the *distinguishing* tokens
        core_tokens = [t for t in tokens if t.lower() not in common_tokens] if common_tokens else tokens
        if not core_tokens:
            core_tokens = tokens
        key = slugify(" ".join(core_tokens))[:40] or slugify(raw_name)[:40] or f"c{model['id']}"
        if size != "standard":
            key = f"{key}_{size}"
        if key in seen_keys:
            key = f"{key}_{model['id']}"
        seen_keys.add(key)

        # human label — show only the distinguishing tokens (in original
        # casing). Falls back to the full raw_name when no common tokens
        # were found. This mirrors the key-derivation logic so the UI's
        # dropdown shows short, scannable options instead of repeating
        # the brand/model in every row.
        if common_tokens:
            label_tokens = [t for t in tokens if t.lower() not in common_tokens]
            label = " ".join(label_tokens) if label_tokens else raw_name
        else:
            label = raw_name
        label = (label or filename)[:48].strip() or "Capture"
        rel_path = f"{cfg['asset_subpath']}/{slug}/{filename}"
        capture_entries.append((key, label, rel_path))

    if not capture_entries:
        return {"skipped": True, "reason": "no downloads"}

    # Dump per-pack metadata for the IA review step. Includes raw_name +
    # size so the human/IA reviewing the .rs has the original capture
    # info (Tone3000 names like "M25 LL 1960TV 4x12 SM57 1.50in 0.0in OA30 SA73"
    # are how axes are recovered).
    metadata = {
        "slug": slug,
        "kind": kind,
        "display": display,
        "brand": brand,
        "tone_ids": [t["id"] for t in tones],
        "captures": [
            {
                "key": k,
                "label": lbl,
                "rel_path": p,
                "raw_name": rn,
                "size": (m.get("size") or "standard"),
            }
            for ((k, lbl, p), rn, (_, m)) in zip(
                capture_entries, raw_names, selected_models
            )
        ],
    }
    if not dry_run:
        (captures_dir / "_metadata.json").write_text(
            json.dumps(metadata, indent=2)
        )

    # codegen
    crate = cfg["crate"]
    rs_path = repo_root / "crates" / crate / "src" / f"{rs_prefix}{slug}.rs"
    template_map = {"amp": AMP_TEMPLATE, "preamp": PREAMP_TEMPLATE, "cab": CAB_TEMPLATE}
    template = template_map[kind]
    src = render_template(
        template,
        model_id=f"{rs_prefix}{slug}" if kind != "cab" else slug,
        display_name=display,
        brand=brand,
        captures=capture_entries,
    )
    if not dry_run and not rs_path.exists():
        # Only write the stub when there isn't already a hand-tuned file
        # at this path. The pipeline must NOT clobber a manual multi-axis
        # rewrite — that's the whole point of the per-pack IA review.
        rs_path.write_text(src)
    elif not dry_run and rs_path.exists():
        print(f"  ⓘ {rs_path.relative_to(repo_root)} exists — preserving (likely IA-tuned)")

    print(f"  ✓ {len(capture_entries)} captures + metadata at {captures_dir.relative_to(repo_root)}/_metadata.json")
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
    parser.add_argument("--regen-only", action="store_true",
                        help="Only regenerate .rs files; skip downloads if bytes already on disk")
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
            result = import_one(spec, repo_root, dry_run=args.dry_run, regen_only=args.regen_only)
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
