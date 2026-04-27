#!/usr/bin/env python3
"""Tests for gen_pedal_models.py — uses unittest (stdlib)."""
import unittest
from gen_pedal_models import (
    detect_pattern,
    infer_knobs,
    longest_common_prefix,
    normalize_brand,
    parse_grid_tokens,
    render_enum_rs,
    render_grid_rs,
    split_size_suffix,
)


class TestDetectPattern(unittest.TestCase):
    def test_grid_big_muff(self):
        stems = [
            "ehx_ic_big_muff_v_6_t_2_s_0",
            "ehx_ic_big_muff_v_6_t_2_s_5",
            "ehx_ic_big_muff_v_6_t_3_s_0",
            "ehx_ic_big_muff_v_6_t_3_s_5",
        ]
        p = detect_pattern(stems)
        self.assertEqual(p["kind"], "grid")
        # v=6 is constant → not in active dimensions
        self.assertNotIn("v", p["dimensions"])
        self.assertEqual(set(p["dimensions"].keys()), {"t", "s"})
        self.assertEqual(p["dimensions"]["t"], [2.0, 3.0])
        self.assertEqual(p["dimensions"]["s"], [0.0, 5.0])

    def test_grid_with_size_suffix(self):
        stems = [
            "muff_t_2_s_0",
            "muff_t_2_s_0_feather",
            "muff_t_2_s_5",
            "muff_t_2_s_5_feather",
        ]
        p = detect_pattern(stems)
        self.assertEqual(p["kind"], "grid")
        self.assertEqual(set(p["sizes"]), {"standard", "feather"})

    def test_nominal_hm2_style(self):
        stems = ["hm2_chainsaw", "hm2_medium", "hm2_warm", "hm2_bright"]
        p = detect_pattern(stems)
        self.assertEqual(p["kind"], "nominal")
        self.assertGreaterEqual(len(p["labels"]), 4)

    def test_preset_numbered(self):
        stems = [f"mna_bogner_red_pedal_preset_{i}" for i in range(1, 6)]
        p = detect_pattern(stems)
        self.assertEqual(p["kind"], "preset")
        self.assertEqual(p["count"], 5)

    def test_grid_handles_non_numeric_byp(self):
        stems = [
            "muff_v_6_t_byp_s_0",
            "muff_v_6_t_2_s_0",
            "muff_v_6_t_5_s_0",
        ]
        p = detect_pattern(stems)
        self.assertEqual(p["kind"], "grid")
        # `byp` should be coerced to numeric range minimum
        coords = [e["coords"] for e in p["entries"]]
        # All entries must have a numeric value for `t`
        for c in coords:
            self.assertIsInstance(c["t"], float)


class TestInferKnobs(unittest.TestCase):
    def test_big_muff_uses_brand_specific(self):
        knobs = infer_knobs(
            brand="ehx",
            model="electro-harmonix op-amp big muff",
            letters={"v", "t", "s"},
        )
        self.assertEqual(knobs, {"v": "Volume", "t": "Tone", "s": "Sustain"})

    def test_unknown_brand_falls_back_universal(self):
        knobs = infer_knobs(
            brand="madeupbrand",
            model="random pedal",
            letters={"d", "t", "l"},
        )
        self.assertEqual(knobs, {"d": "Drive", "t": "Tone", "l": "Level"})

    def test_unknown_letter_capitalized(self):
        warnings = []
        knobs = infer_knobs(
            brand="madeupbrand",
            model="x",
            letters={"z"},
            warn=warnings.append,
        )
        self.assertEqual(knobs, {"z": "Z"})
        self.assertTrue(any("z" in w for w in warnings))


class TestHelpers(unittest.TestCase):
    def test_longest_common_prefix(self):
        self.assertEqual(longest_common_prefix(["abc_x", "abc_y"]), "abc_")
        self.assertEqual(longest_common_prefix(["xy", "ab"]), "")
        self.assertEqual(longest_common_prefix([]), "")
        self.assertEqual(longest_common_prefix(["only"]), "")

    def test_split_size_suffix(self):
        self.assertEqual(split_size_suffix("foo_bar_feather"), ("foo_bar", "feather"))
        self.assertEqual(split_size_suffix("foo_bar"), ("foo_bar", "standard"))
        self.assertEqual(split_size_suffix("foo_lite"), ("foo", "lite"))

    def test_parse_grid_tokens(self):
        tokens = parse_grid_tokens("v_6_t_2_s_0")
        self.assertEqual(tokens, [("v", "6"), ("t", "2"), ("s", "0")])

    def test_normalize_brand_ehx(self):
        self.assertEqual(normalize_brand("Electro-Harmonix Op-Amp Big Muff"), "ehx")
        self.assertEqual(normalize_brand("electro harmonix big muff"), "ehx")
        self.assertEqual(normalize_brand("EHX Bass Big Muff"), "ehx")

    def test_normalize_brand_compound(self):
        self.assertEqual(normalize_brand("EarthQuaker Devices Hizumitas"), "earthquaker_devices")
        self.assertEqual(normalize_brand("Pete Cornish G-2"), "pete_cornish")
        self.assertEqual(normalize_brand("Way Huge Pork Pickle"), "way_huge")


class TestRenderGrid(unittest.TestCase):
    def _sample_pedal(self):
        return {
            "slug": "big_muff",
            "label": "Big Muff",
            "make": "Electro-Harmonix Op-Amp Big Muff",
            "display_name": "Big Muff",
            "brand": "ehx",
            "knobs": {"t": "Tone", "s": "Sustain"},
            "dimensions": {"t": [2.0, 7.0], "s": [0.0, 10.0]},
            "entries": [
                {"stem": "muff_v_6_t_2_s_0", "size": "standard", "coords": {"t": 2.0, "s": 0.0}},
                {"stem": "muff_v_6_t_7_s_10", "size": "standard", "coords": {"t": 7.0, "s": 10.0}},
            ],
            "sizes": ["standard"],
        }

    def test_render_grid_contains_required_pieces(self):
        rs = render_grid_rs(self._sample_pedal())
        self.assertIn('pub const MODEL_ID: &str = "nam_big_muff";', rs)
        self.assertIn('const BRAND: &str = "ehx";', rs)
        self.assertIn('float_parameter("tone"', rs)
        self.assertIn('float_parameter("sustain"', rs)
        self.assertIn('NamSize::Standard', rs)
        self.assertIn('fn resolve_capture', rs)
        self.assertIn('#[cfg(test)]', rs)
        self.assertIn("MODEL_DEFINITION: GainModelDefinition", rs)

    def test_render_grid_two_sizes_emits_size_enum_param(self):
        pedal = self._sample_pedal()
        pedal["sizes"] = ["feather", "standard"]
        pedal["entries"].append({"stem": "muff_t_2_s_0_feather", "size": "feather", "coords": {"t": 2.0, "s": 0.0}})
        rs = render_grid_rs(pedal)
        self.assertIn('enum_parameter("size"', rs)
        self.assertIn('NamSize::Feather', rs)


class TestRenderEnum(unittest.TestCase):
    def test_render_enum_smoke(self):
        pedal = {
            "slug": "boss_hm2_legacy",
            "display_name": "HM-2 Legacy",
            "brand": "boss",
            "enum_entries": [
                {"tone_id": "chainsaw", "display_label": "Chainsaw", "model_path": "pedals/x/chainsaw.nam"},
                {"tone_id": "warm", "display_label": "Warm", "model_path": "pedals/x/warm.nam"},
            ],
        }
        rs = render_enum_rs(pedal)
        self.assertIn('pub const MODEL_ID: &str = "nam_boss_hm2_legacy";', rs)
        self.assertIn('enum_parameter', rs)
        self.assertIn('"chainsaw"', rs)
        self.assertIn('"Chainsaw"', rs)


if __name__ == "__main__":
    unittest.main()
