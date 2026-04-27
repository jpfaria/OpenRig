#!/usr/bin/env python3
"""Codegen v2 for NAM gain pedal plugins.

Reads `captures/nam/pedals/<slug>/MANIFEST.yaml` and the `*.nam` files in each
pedal directory, detects the filename pattern (grid / nominal / preset),
auto-infers human-readable knob names from a brand+letter heuristic table,
and writes one `crates/block-gain/src/nam_<slug>.rs` per pedal.

Generated Rust uses `float_parameter` for grid pedals (snap-to-nearest in
`resolve_capture`) or `enum_parameter` for nominal/preset pedals (verbatim
of the existing `nam_boss_hm2_1986.rs` pattern).

Run from repo root:
    python3 tools/gen_pedal_models.py
"""
from __future__ import annotations

import argparse
import re
import sys
from collections import OrderedDict
from pathlib import Path

try:
    import yaml
except ImportError:
    print("error: PyYAML required. Install: pip install pyyaml", file=sys.stderr)
    sys.exit(1)


# ============================================================================
# Constants
# ============================================================================

REPO_ROOT = Path(__file__).resolve().parent.parent
PEDALS_DIR = REPO_ROOT / "captures" / "nam" / "pedals"
OUT_DIR = REPO_ROOT / "crates" / "block-gain" / "src"

GRID_THRESHOLD = 0.8  # ≥80% of files must match the grid regex
NON_NUMERIC_VALUES = {"byp", "bypass", "min", "max", "off", "0db", "unity"}

# Universal letter → knob name fallback. Used when no brand-specific entry matches.
UNIVERSAL_KNOBS = {
    "v": "Volume",
    "t": "Tone",
    "g": "Gain",
    "d": "Drive",
    "l": "Level",
    "b": "Bass",
    "m": "Mid",
    "h": "Treble",
    "p": "Presence",
    "s": "Sustain",
    "a": "Attack",
    "c": "Compression",
    "f": "Filter",
    "o": "Output",
    "r": "Range",
}

# Brand+model heuristics ordered most-specific → most-generic. The first
# matcher whose `brand` and (optional) `model_contains` conditions are
# satisfied wins. Letters not present in the chosen mapping fall back to
# UNIVERSAL_KNOBS.
KNOB_HEURISTICS = [
    ({"brand": "ehx", "model_contains": "big muff"},
     {"v": "Volume", "t": "Tone", "s": "Sustain"}),
    ({"brand": "electro-harmonix", "model_contains": "big muff"},
     {"v": "Volume", "t": "Tone", "s": "Sustain"}),
    ({"brand": "ibanez", "model_contains": "tube screamer"},
     {"d": "Drive", "t": "Tone", "l": "Level"}),
    ({"brand": "klon"},
     {"g": "Gain", "t": "Treble", "v": "Output"}),
    ({"brand": "ceriatone", "model_contains": "centura"},
     {"g": "Gain", "t": "Treble", "v": "Output"}),
    ({"brand": "proco", "model_contains": "rat"},
     {"d": "Distortion", "f": "Filter", "v": "Volume"}),
    ({"brand": "fulltone", "model_contains": "ocd"},
     {"d": "Drive", "t": "Tone", "v": "Volume"}),
    ({"brand": "fulltone", "model_contains": "fulldrive"},
     {"v": "Volume", "t": "Tone", "d": "Drive", "b": "Boost"}),
    ({"brand": "boss", "model_contains": "ds-1"},
     {"d": "Distortion", "t": "Tone", "l": "Level"}),
    ({"brand": "boss", "model_contains": "ds-2"},
     {"d": "Distortion", "t": "Tone", "l": "Level"}),
    ({"brand": "boss", "model_contains": "od-3"},
     {"d": "Drive", "t": "Tone", "l": "Level"}),
    ({"brand": "boss", "model_contains": "mt-2"},
     {"d": "Distortion", "h": "High", "l": "Low", "v": "Level", "m": "Mid"}),
    ({"brand": "boss"},
     {"d": "Drive", "t": "Tone", "l": "Level"}),
    ({"brand": "earthquaker", "model_contains": "plumes"},
     {"v": "Volume", "t": "Tone", "g": "Gain", "m": "Mode"}),
    ({"brand": "horizon"},
     {"v": "Volume", "t": "Tone", "a": "Attack", "d": "Drive"}),
    ({"brand": "cornish"},
     {"v": "Volume", "t": "Treble", "b": "Bass", "g": "Gain", "s": "Sustain", "a": "Attack"}),
    ({"brand": "wampler", "model_contains": "tumnus"},
     {"v": "Volume", "t": "Treble", "g": "Gain"}),
    ({"brand": "mxr", "model_contains": "distortion"},
     {"d": "Distortion", "t": "Tone", "o": "Output"}),
    ({"brand": "mxr", "model_contains": "duke of tone"},
     {"v": "Volume", "t": "Tone", "g": "Gain"}),
    ({"brand": "suhr", "model_contains": "riot"},
     {"v": "Volume", "t": "Tone", "d": "Distortion"}),
    ({"brand": "dunlop", "model_contains": "fuzz face"},
     {"v": "Volume", "f": "Fuzz"}),
    ({"brand": "dallas", "model_contains": "rangemaster"},
     {"b": "Boost"}),
    ({"brand": "maestro", "model_contains": "fuzz"},
     {"v": "Volume", "a": "Attack"}),
    ({"brand": "hermida"},
     {"v": "Volume", "g": "Gain", "t": "Tone", "y": "Voice"}),
    ({"brand": "thorpyfx"},
     {"v": "Volume", "t": "Tone", "g": "Gain"}),
    ({"brand": "tech21"},
     {"l": "Level", "b": "Bass", "h": "Treble", "p": "Presence", "d": "Drive", "m": "Mid"}),
    ({"brand": "digitech"},
     {"l": "Level", "h": "High", "o": "Low", "d": "Drive"}),
    ({"brand": "browne"},
     {"v": "Volume", "t": "Tone", "g": "Gain"}),
    ({"brand": "way_huge"},
     {"v": "Volume", "f": "Filter", "g": "Gain"}),
    ({"brand": "way huge"},
     {"v": "Volume", "f": "Filter", "g": "Gain"}),
    # Universal fallback (always last)
    ({}, UNIVERSAL_KNOBS),
]


# ============================================================================
# Pattern detection
# ============================================================================


def longest_common_prefix(strings: list[str]) -> str:
    """Return the longest common prefix among the strings."""
    if not strings:
        return ""
    if len(strings) == 1:
        return ""
    prefix = strings[0]
    for s in strings[1:]:
        while not s.startswith(prefix):
            prefix = prefix[:-1]
            if not prefix:
                return ""
    if prefix and not prefix.endswith("_") and "_" in prefix:
        prefix = prefix.rsplit("_", 1)[0] + "_"
    return prefix


SIZE_SUFFIXES = ("_feather", "_lite", "_nano")


def split_size_suffix(stem: str) -> tuple[str, str]:
    """Strip trailing size suffix. Return (stem_without_suffix, size)."""
    for suf in SIZE_SUFFIXES:
        if stem.endswith(suf):
            return stem[: -len(suf)], suf[1:]  # drop leading underscore
    return stem, "standard"


# Match a knob coordinate in a filename. Several conventions are common in
# Tone3000 captures:
#   `v_6`           → letter, `_`, integer
#   `v_6.5`         → letter, `_`, decimal
#   `g_03_00`       → letter, `_`, decimal-with-underscore (`03_00` = 3.00)
#   `g2`            → letter (no `_`), integer (Cornish, Bad Monkey style)
#   `t_byp`         → letter, `_`, special token
# The value group matches digits with optional `.` or `_` decimal separator,
# or a small set of named tokens (`byp`, `min`, `max`, `off`, ...).
GRID_VALUE_RE = r"\d+(?:[._]\d+)?|byp|bypass|min|max|off|unity|0db"
GRID_TOKEN_RE = re.compile(rf"(?:^|_)([a-z])_?({GRID_VALUE_RE})(?=_|$)")


def parse_grid_tokens(stem: str) -> list[tuple[str, str]]:
    """Extract (letter, raw_value) tuples from a stem.

    Letter may appear at the start of the string or preceded by `_`. The
    value may be: integer, decimal (`.` or `_` separator), or a small set
    of named tokens (`byp`, `min`, `max`).
    """
    return GRID_TOKEN_RE.findall(stem)


def coerce_value(raw: str, dim_numeric_values: list[float]) -> float | None:
    """Convert a captured value to a float coordinate.

    Numeric values parse directly. `_` between digit groups is treated as a
    decimal separator (`03_00` → 3.00). Non-numeric tokens (`byp`, `min`,
    `max`) are mapped to extremes of the numeric range observed for the
    same dimension. Returns None if the value cannot be coerced and no
    numeric siblings exist.
    """
    # Decimal-as-underscore: `03_00` → 3.00
    candidate = raw.replace("_", ".") if "_" in raw and raw.replace("_", "").isdigit() else raw
    try:
        return float(candidate)
    except ValueError:
        pass
    if raw in {"byp", "bypass", "off", "min", "0db"} and dim_numeric_values:
        return min(dim_numeric_values)
    if raw in {"max", "unity"} and dim_numeric_values:
        return max(dim_numeric_values)
    return None


def detect_pattern(stems: list[str]) -> dict:
    """Classify the filename pattern and extract structured info.

    Returns a dict with at least `kind` ∈ {"grid", "nominal", "preset"}.
    For grid: `common_prefix`, `dimensions` (letter → sorted unique numeric
    values), `sizes` (sorted), `entries` (list of {stem, size, coords}).
    For nominal: `labels` (sorted).
    For preset: `count`.
    """
    if not stems:
        return {"kind": "nominal", "labels": []}

    common_prefix = longest_common_prefix(stems)
    stripped = [s[len(common_prefix):] for s in stems]

    # Strip size suffix per stem (uses stripped form for sizing decisions)
    base_and_size = [split_size_suffix(s) for s in stripped]
    sizes = sorted({sz for _, sz in base_and_size})

    # Grid detection runs on the FULL stem (with size suffix removed). Stripping
    # common prefix can swallow `letter_value_` pairs when a coordinate is
    # constant across the pack (e.g. v=6 always), so we must scan the original.
    full_no_size = [split_size_suffix(s)[0] for s in stems]

    grid_results: list[list[tuple[str, str]]] = []
    grid_match_count = 0
    for base in full_no_size:
        tokens = parse_grid_tokens(base)
        grid_results.append(tokens)
        if tokens:
            grid_match_count += 1

    match_ratio = grid_match_count / len(stems)
    distinct_letters = {letter for tokens in grid_results for letter, _ in tokens}

    if match_ratio >= GRID_THRESHOLD and distinct_letters:
        # Build dimensions: collect all numeric values per letter
        per_letter_numeric: dict[str, list[float]] = {l: [] for l in distinct_letters}
        for tokens in grid_results:
            for letter, raw in tokens:
                # Decimal-as-underscore support: `03_00` → 3.00
                norm = raw.replace("_", ".") if "_" in raw and raw.replace("_", "").isdigit() else raw
                try:
                    per_letter_numeric[letter].append(float(norm))
                except ValueError:
                    continue

        # Dimensions: sorted unique numeric values per letter
        dimensions = {
            letter: sorted({v for v in vals})
            for letter, vals in per_letter_numeric.items()
            if vals
        }

        # Drop dimensions that are constant (1 distinct value) — they are not
        # user-controlled (e.g. v=6 fixed for unity gain in Big Muff captures).
        # Also drops dimensions whose ALL tokens are non-numeric (no entry in
        # `dimensions` at all).
        active_dimensions = OrderedDict(
            (letter, vals) for letter, vals in sorted(dimensions.items())
            if len(vals) > 1
        )

        # Each entry: coords ONLY for active dimensions. Non-active letters
        # (constant or all-non-numeric) are ignored without failing the entry.
        entries = []
        for orig_stem, (base, size), tokens in zip(stems, base_and_size, grid_results):
            coords = {}
            ok = True
            for letter, raw in tokens:
                if letter not in active_dimensions:
                    continue
                v = coerce_value(raw, per_letter_numeric[letter])
                if v is None:
                    ok = False
                    break
                coords[letter] = v
            if not ok or len(coords) != len(active_dimensions):
                continue  # entry doesn't cover every active dim — skip
            entries.append({"stem": orig_stem, "size": size, "coords": coords})

        return {
            "kind": "grid",
            "common_prefix": common_prefix,
            "dimensions": active_dimensions,
            "all_dimensions": dict(dimensions),
            "sizes": sizes,
            "entries": entries,
        }

    # Try preset_N: search ORIGINAL stems (the common prefix may have eaten
    # the literal `preset_` part if every file has the same author prefix).
    preset_re = re.compile(r"preset[_-]?(\d+)", re.IGNORECASE)
    preset_count = sum(1 for s in stems if preset_re.search(s))
    if preset_count >= len(stems) * 0.5:
        return {"kind": "preset", "count": preset_count, "stems": stems}

    # Fallback: nominal — use stripped stems as labels
    labels = sorted({s for s in stripped})
    return {"kind": "nominal", "labels": labels, "stems": stems}


# ============================================================================
# Knob name inference
# ============================================================================


def normalize_brand(make: str) -> str:
    """Normalize a `make` field to a brand token."""
    if not make:
        return ""
    s = make.lower().strip()
    # Take first significant word(s) — handle compound brands
    if s.startswith("electro-harmonix") or s.startswith("electro harmonix"):
        return "ehx"
    if s.startswith("ehx"):
        return "ehx"
    for prefix in ("earthquaker devices", "horizon devices", "way huge",
                   "tech 21", "tech21", "pete cornish", "warm audio",
                   "death by audio", "chase bliss", "analog man", "analogman",
                   "paul cochrane", "paul cochraine", "thorpy fx", "thorpyfx"):
        if s.startswith(prefix):
            return prefix.replace(" ", "_")
    # First word otherwise
    first = re.split(r"\s|-", s)[0]
    return first


def matcher_satisfied(matcher: dict, brand: str, model: str) -> bool:
    """Check if a heuristic matcher's conditions are satisfied."""
    if not matcher:
        return True  # universal fallback
    if "brand" in matcher:
        m_brand = matcher["brand"].lower().replace(" ", "_")
        if brand != m_brand:
            return False
    if "model_contains" in matcher:
        if matcher["model_contains"].lower() not in model.lower():
            return False
    return True


def infer_knobs(brand: str, model: str, letters: set[str], warn=None) -> dict[str, str]:
    """Map letters to display names using KNOB_HEURISTICS + UNIVERSAL_KNOBS."""
    if warn is None:
        def warn(msg):
            print(f"warn: {msg}", file=sys.stderr)

    chosen = None
    for matcher, mapping in KNOB_HEURISTICS:
        if matcher_satisfied(matcher, brand, model):
            chosen = mapping
            break
    if chosen is None:
        chosen = UNIVERSAL_KNOBS

    out = {}
    for letter in sorted(letters):
        if letter in chosen:
            out[letter] = chosen[letter]
        elif letter in UNIVERSAL_KNOBS:
            out[letter] = UNIVERSAL_KNOBS[letter]
        else:
            out[letter] = letter.upper()
            warn(f"unknown knob letter '{letter}' for brand={brand} model={model[:50]!r} — using literal '{letter.upper()}'")
    return out


# ============================================================================
# Slug helpers
# ============================================================================


def normalize_brand_for_rust(make: str) -> str:
    """Brand string used in MODEL_DEFINITION's `brand` field."""
    b = normalize_brand(make)
    return b or "unknown"


def slugify(s: str) -> str:
    s = (s or "").lower()
    s = re.sub(r"\(.*?\)", "", s)
    s = re.sub(r"[^a-z0-9]+", "_", s)
    return re.sub(r"_+", "_", s).strip("_")


def display_name_from_label(label: str) -> str:
    return re.sub(r"\s*\([^)]*\)\s*", "", label or "").strip() or label or "Unnamed"


def knob_param_name(letter: str, display: str) -> str:
    """Lowercase identifier for the parameter (e.g. 'tone', 'sustain')."""
    return slugify(display) or letter


def rust_str_lit(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


# ============================================================================
# Render — grid pedals
# ============================================================================


def render_grid_rs(pedal: dict) -> str:
    """Render the Rust source for a grid-pattern pedal.

    `pedal` dict keys (consumed):
      slug, label, make, knobs (letter → display_name),
      dimensions (letter → sorted [values]), entries (stem, size, coords),
      sizes (list)
    """
    slug = pedal["slug"]
    model_id = f"nam_{slug}"
    display_name = pedal["display_name"]
    brand = pedal["brand"]
    knobs = pedal["knobs"]              # letter → display
    dimensions = pedal["dimensions"]    # letter → sorted [values]
    entries = pedal["entries"]
    sizes = pedal["sizes"]

    # Sort knob letters in stable order: alphabetical
    letters = sorted(knobs.keys())
    n = len(letters)

    # Build Rust struct fields
    coord_fields = ",\n    ".join(f"{knob_param_name(l, knobs[l])}: f32" for l in letters)
    coord_struct = f"#[derive(Clone, Copy)]\nstruct GridCapture {{\n    {coord_fields},\n    size: NamSize,\n    model_path: &'static str,\n}}"

    # NamSize enum
    size_variants = [s.capitalize() for s in sizes]
    size_enum = "#[derive(Clone, Copy, PartialEq, Eq)]\nenum NamSize {\n    " + ",\n    ".join(size_variants) + ",\n}"

    # Range constants per dimension
    range_consts_lines = []
    for l in letters:
        vals = dimensions[l]
        param = knob_param_name(l, knobs[l])
        range_consts_lines.append(f"const {param.upper()}_MIN: f32 = {vals[0]:.1f};")
        range_consts_lines.append(f"const {param.upper()}_MAX: f32 = {vals[-1]:.1f};")
    range_consts = "\n".join(range_consts_lines)

    # CAPTURES const
    capture_lines = []
    for e in entries:
        coord_assigns = ", ".join(f"{knob_param_name(l, knobs[l])}: {e['coords'][l]:.1f}" for l in letters if l in e["coords"])
        size_variant = e["size"].capitalize()
        path = f"pedals/{slug}/{e['stem']}.nam"
        capture_lines.append(f"    GridCapture {{ {coord_assigns}, size: NamSize::{size_variant}, model_path: {rust_str_lit(path)} }},")
    captures_block = "\n".join(capture_lines)

    # Schema parameters
    schema_param_lines = []
    for l in letters:
        param = knob_param_name(l, knobs[l])
        display = knobs[l]
        schema_param_lines.append(
            f'        float_parameter({rust_str_lit(param)}, {rust_str_lit(display)}, '
            f'Some({rust_str_lit("Pedal")}), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),'
        )
    if len(sizes) > 1:
        size_options = ", ".join(f"({rust_str_lit(s)}, {rust_str_lit(s.capitalize())})" for s in sizes)
        default_size = "standard" if "standard" in sizes else sizes[0]
        schema_param_lines.append(
            f'        enum_parameter({rust_str_lit("size")}, {rust_str_lit("Model Size")}, '
            f'Some({rust_str_lit("Capture")}), Some({rust_str_lit(default_size)}), '
            f'&[{size_options}]),'
        )
    schema_params = "\n".join(schema_param_lines)

    # resolve_capture body
    knob_reads = []
    coord_calc = []
    for l in letters:
        param = knob_param_name(l, knobs[l])
        knob_reads.append(
            f'    let {param}_pct = required_f32(params, {rust_str_lit(param)})'
            f'.map_err(anyhow::Error::msg)?;'
        )
        coord_calc.append(
            f'    let {param} = {param.upper()}_MIN + ({param}_pct / 100.0) * ({param.upper()}_MAX - {param.upper()}_MIN);'
        )
    if len(sizes) > 1:
        size_arms = "\n        ".join(f'{rust_str_lit(s)} => NamSize::{s.capitalize()},' for s in sizes)
        size_resolve = f'''    let size_str = required_string(params, {rust_str_lit("size")}).map_err(anyhow::Error::msg)?;
    let size = match size_str.as_str() {{
        {size_arms}
        other => return Err(anyhow!("unknown size '{{}}'", other)),
    }};
    let candidates = CAPTURES.iter().filter(|c| c.size == size);'''
    else:
        only_size = sizes[0].capitalize()
        size_resolve = f'    let _size = NamSize::{only_size};\n    let candidates = CAPTURES.iter().filter(|c| c.size == NamSize::{only_size});'

    distance_terms = " + ".join(f"(c.{knob_param_name(l, knobs[l])} - {knob_param_name(l, knobs[l])}).powi(2)" for l in letters)

    resolve_fn = f'''fn resolve_capture(params: &ParameterSet) -> Result<&'static GridCapture> {{
{chr(10).join(knob_reads)}
{chr(10).join(coord_calc)}
{size_resolve}
    candidates
        .min_by(|a, b| {{
            let da = {distance_terms.replace("c.", "a.")};
            let db = {distance_terms.replace("c.", "b.")};
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        }})
        .ok_or_else(|| anyhow!("no capture matches"))
}}'''

    # Inline per-pedal tests omitted — coverage comes from the workspace's
    # registry tests in `block-gain/src/registry.rs::tests`, which iterate
    # every MODEL_DEFINITION (schema, validate with defaults, build).
    tests_block = ""

    multi_size = len(sizes) > 1
    extra_imports = "enum_parameter, " if multi_size else ""
    extra_imports2 = "required_string, " if multi_size else ""
    return f'''use anyhow::{{anyhow, Result}};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{{NamPluginParams, DEFAULT_PLUGIN_PARAMS}},
}};
use block_core::param::{{
    {extra_imports}float_parameter, required_f32, {extra_imports2}
    ModelParameterSchema, ParameterSet, ParameterUnit,
}};
use block_core::{{AudioChannelLayout, BlockProcessor, ModelAudioMode}};

pub const MODEL_ID: &str = {rust_str_lit(model_id)};
pub const DISPLAY_NAME: &str = {rust_str_lit(display_name)};
const BRAND: &str = {rust_str_lit(brand)};

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

{coord_struct}

{size_enum}

{range_consts}

const CAPTURES: &[GridCapture] = &[
{captures_block}
];

pub fn model_schema() -> ModelParameterSchema {{
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
{schema_params}
    ];
    schema
}}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {{
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}}

pub fn validate_params(params: &ParameterSet) -> Result<()> {{
    resolve_capture(params).map(|_| ())
}}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {{
    let capture = resolve_capture(params)?;
    Ok(format!("model='{{}}'", capture.model_path))
}}

{resolve_fn}

fn schema() -> Result<ModelParameterSchema> {{
    Ok(model_schema())
}}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {{
    build_processor_for_model(params, sample_rate, layout)
}}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {{
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
}};

{tests_block}'''


# ============================================================================
# Render — nominal/preset pedals (enum dropdown)
# ============================================================================


def render_enum_rs(pedal: dict) -> str:
    """Render an enum-based pedal (nominal labels or numbered presets)."""
    slug = pedal["slug"]
    model_id = f"nam_{slug}"
    display_name = pedal["display_name"]
    brand = pedal["brand"]
    entries = pedal["enum_entries"]  # list of {tone_id, display_label, model_path}

    if not entries:
        raise ValueError(f"no entries for enum pedal {slug}")

    default_tone = entries[0]["tone_id"]

    capture_lines = []
    for e in entries:
        capture_lines.append(
            f'    NamCapture {{ tone: {rust_str_lit(e["tone_id"])}, model_path: {rust_str_lit(e["model_path"])} }},'
        )
    captures_block = "\n".join(capture_lines)

    option_lines = []
    for e in entries:
        option_lines.append(f'            ({rust_str_lit(e["tone_id"])}, {rust_str_lit(e["display_label"])}),')
    options_block = "\n".join(option_lines)

    return f'''use anyhow::{{anyhow, Result}};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{{NamPluginParams, DEFAULT_PLUGIN_PARAMS}},
}};
use block_core::param::{{enum_parameter, required_string, ModelParameterSchema, ParameterSet}};
use block_core::{{AudioChannelLayout, BlockProcessor}};

pub const MODEL_ID: &str = {rust_str_lit(model_id)};
pub const DISPLAY_NAME: &str = {rust_str_lit(display_name)};
const BRAND: &str = {rust_str_lit(brand)};

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {{
    tone: &'static str,
    model_path: &'static str,
}}

const CAPTURES: &[NamCapture] = &[
{captures_block}
];

pub fn model_schema() -> ModelParameterSchema {{
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some({rust_str_lit(default_tone)}),
        &[
{options_block}
        ],
    )];
    schema
}}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {{
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}}

pub fn validate_params(params: &ParameterSet) -> Result<()> {{
    resolve_capture(params).map(|_| ())
}}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {{
    let capture = resolve_capture(params)?;
    Ok(format!("model='{{}}'", capture.model_path))
}}

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {{
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{{}}' does not support tone='{{}}'", MODEL_ID, tone))
}}

fn schema() -> Result<ModelParameterSchema> {{
    Ok(model_schema())
}}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {{
    build_processor_for_model(params, sample_rate, layout)
}}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {{
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
}};
'''


# ============================================================================
# Orchestration
# ============================================================================


def title_case(s: str) -> str:
    return " ".join(w.capitalize() for w in s.replace("_", " ").split())


def process_pedal(slug: str, dry_run: bool = False) -> tuple[str, str]:
    """Read MANIFEST + .nam files, classify, render, write the .rs file.

    Returns (status, message).
    status ∈ {"grid", "enum", "skip", "error"}.
    """
    pedal_dir = PEDALS_DIR / slug
    mf_path = pedal_dir / "MANIFEST.yaml"
    if not mf_path.exists():
        return "skip", f"no MANIFEST.yaml"
    mf = yaml.safe_load(mf_path.read_text())
    label = mf.get("label") or slug
    make = mf.get("make") or label

    nam_files = sorted(pedal_dir.glob("*.nam"))
    if not nam_files:
        return "skip", "no .nam files"

    stems = [f.stem for f in nam_files]
    pattern = detect_pattern(stems)

    brand = normalize_brand_for_rust(make)
    display = display_name_from_label(label)

    out_path = OUT_DIR / f"nam_{slug}.rs"

    if pattern["kind"] == "grid":
        if not pattern["dimensions"]:
            # All dimensions constant — degenerate, fall back to enum
            pattern = {"kind": "nominal", "labels": stems, "stems": stems}
        else:
            knobs = infer_knobs(brand, label, set(pattern["dimensions"].keys()))
            pedal = {
                "slug": slug,
                "label": label,
                "make": make,
                "display_name": display,
                "brand": brand,
                "knobs": knobs,
                "dimensions": pattern["dimensions"],
                "entries": pattern["entries"],
                "sizes": pattern["sizes"],
            }
            content = render_grid_rs(pedal)
            if not dry_run:
                out_path.write_text(content)
            return "grid", f"{len(pattern['entries'])} captures, knobs={list(knobs.values())}, sizes={pattern['sizes']}"

    if pattern["kind"] == "preset":
        # Build enum_entries from preset numbering
        enum_entries = []
        seen_ids = set()
        for stem in stems:
            m = re.search(r"preset[_-]?(\d+)", stem, re.IGNORECASE)
            n = m.group(1) if m else stem
            tone_id = f"preset_{n}"
            i = 0
            while tone_id in seen_ids:
                i += 1
                tone_id = f"preset_{n}_{i}"
            seen_ids.add(tone_id)
            enum_entries.append({
                "tone_id": tone_id,
                "display_label": f"Preset {n}",
                "model_path": f"pedals/{slug}/{stem}.nam",
            })
        pedal = {"slug": slug, "display_name": display, "brand": brand, "enum_entries": enum_entries}
        content = render_enum_rs(pedal)
        if not dry_run:
            out_path.write_text(content)
        return "enum", f"{len(enum_entries)} preset entries"

    # Nominal fallback
    common_prefix = longest_common_prefix(stems)
    enum_entries = []
    seen_ids = set()
    for stem in stems:
        stripped = stem[len(common_prefix):] or stem
        tone_id = slugify(stripped)
        i = 0
        base = tone_id
        while tone_id in seen_ids:
            i += 1
            tone_id = f"{base}_{i}"
        seen_ids.add(tone_id)
        enum_entries.append({
            "tone_id": tone_id,
            "display_label": title_case(stripped) or stem,
            "model_path": f"pedals/{slug}/{stem}.nam",
        })
    pedal = {"slug": slug, "display_name": display, "brand": brand, "enum_entries": enum_entries}
    content = render_enum_rs(pedal)
    if not dry_run:
        out_path.write_text(content)
    return "enum", f"{len(enum_entries)} nominal entries"


def main():
    parser = argparse.ArgumentParser(description="Generate NAM gain pedal Rust modules from captures/")
    parser.add_argument("--dry-run", action="store_true", help="Don't write files, just report")
    parser.add_argument("slugs", nargs="*", help="Specific pedal slugs (default: all tone3000-source)")
    args = parser.parse_args()

    idx_path = PEDALS_DIR / "INDEX.yaml"
    if not idx_path.exists():
        print(f"error: {idx_path} not found", file=sys.stderr)
        sys.exit(1)
    idx = yaml.safe_load(idx_path.read_text())
    tone3000_slugs = [p["slug"] for p in idx["pedals"] if p.get("source") == "tone3000"]

    if args.slugs:
        target = [s for s in args.slugs if s in tone3000_slugs]
        if not target:
            print(f"error: none of {args.slugs} are tone3000-source pedals", file=sys.stderr)
            sys.exit(1)
    else:
        target = tone3000_slugs

    print(f"Processing {len(target)} pedals...")
    counts = {"grid": 0, "enum": 0, "skip": 0, "error": 0}
    for slug in target:
        try:
            status, msg = process_pedal(slug, dry_run=args.dry_run)
        except Exception as e:
            status = "error"
            msg = f"{type(e).__name__}: {e}"
        counts[status] = counts.get(status, 0) + 1
        glyph = {"grid": "✓", "enum": "○", "skip": "⊘", "error": "✗"}[status]
        print(f"  {glyph} {slug:<40s} [{status}] {msg}")

    print()
    print(f"Summary: grid={counts['grid']}, enum={counts['enum']}, skip={counts['skip']}, error={counts['error']}")
    if counts["error"] > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
