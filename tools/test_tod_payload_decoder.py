#!/usr/bin/env python3
# Tests for tools/tod_payload_decoder.py. Licensed under LICENSE.md.

from __future__ import annotations

import io
import json
import os
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import tod_payload_decoder as decoder  # noqa: E402


class ConsumerDecodeTest(unittest.TestCase):
    def test_geometry_uses_17_rows_26_columns_and_stride_32(self) -> None:
        payload = bytes([0x6B]) * decoder.PAYLOAD_LENGTH
        first = decoder.decode_cell(payload, 0, 0)
        last = decoder.decode_cell(payload, 16, 25)
        self.assertEqual((0, ">34B8"), (first["offset"], first["vram_address"]))
        self.assertEqual((537, ">36D1"), (last["offset"], last["vram_address"]))

    def test_recovered_class_and_connection_table(self) -> None:
        expected = {
            0x60: (1, 0x0A), 0x61: (1, 0x05), 0x62: (1, 0x0B),
            0x63: (1, 0x0E), 0x64: (1, 0x0D), 0x65: (1, 0x07),
            0x66: (1, 0x0F), 0x67: (2, 0x0F), 0x68: (5, 0x0F),
            0x69: (4, 0x0F), 0x6A: (3, 0x0F), 0x6B: (0, 0x00),
        }
        actual = {
            value: (
                decoder.decode_value(value)["consumer_class"],
                int(decoder.decode_value(value)["connection_mask"][1:], 16),
            )
            for value in expected
        }
        self.assertEqual(expected, actual)

    def test_proven_bit_10_normalization_and_unsupported_values(self) -> None:
        normalized = decoder.decode_value(0x70)
        self.assertEqual(">60", normalized["normalized"])
        self.assertTrue(normalized["ignored_bit_10"])
        with self.assertRaises(decoder.DecoderError):
            decoder.decode_value(0x20)
        with self.assertRaises(decoder.DecoderError):
            decoder.decode_value(0x6C)


class SelectionTest(unittest.TestCase):
    def test_first_qualifying_region_is_selected_row_major(self) -> None:
        payload = bytearray([0x6B] * decoder.PAYLOAD_LENGTH)
        payload[0:3] = bytes((0x60, 0x61, 0x62))
        selected = decoder.select_heldout_region(bytes(payload))
        self.assertEqual((0, 0), (selected["region"]["row"], selected["region"]["column"]))
        self.assertIn("at-least-three", selected["selection_rule"])


class ComparisonTest(unittest.TestCase):
    @staticmethod
    def record(operation: str, address: str, value: str) -> str:
        grom = "A3BD" if operation == ">A3B5" else "A5F8"
        return f"grom=>{grom} space=vram access=read addr={address} byte={value}"

    def test_duplicate_unreached_coordinate_is_not_observed_but_tuple_passes(self) -> None:
        payload = bytes([0x60] * decoder.PAYLOAD_LENGTH)
        prediction = {"region": decoder.decode_region(payload, 0, 0, 1, 2)}
        lines = [self.record(operation, ">34B8", ">60") for operation in (">A3B5", ">A5E1")]
        comparison = decoder.compare_prediction(prediction, lines)
        self.assertEqual("PASS", comparison["status"])
        self.assertEqual("notObserved", comparison["cells"][1]["status"])
        self.assertEqual(1, len(comparison["not_observed"]))

    def test_missing_distinct_tuple_fails(self) -> None:
        payload = bytearray([0x60] * decoder.PAYLOAD_LENGTH)
        payload[1] = 0x61
        prediction = {"region": decoder.decode_region(bytes(payload), 0, 0, 1, 2)}
        lines = [self.record(operation, ">34B8", ">60") for operation in (">A3B5", ">A5E1")]
        comparison = decoder.compare_prediction(prediction, lines)
        self.assertEqual("FAIL", comparison["status"])
        self.assertEqual(1, len(comparison["missing_distinct_coverage"]))

    def test_observed_raw_contradiction_fails(self) -> None:
        payload = bytes([0x60] * decoder.PAYLOAD_LENGTH)
        prediction = {"region": decoder.decode_region(payload, 0, 0, 1, 1)}
        lines = [self.record(">A3B5", ">34B8", ">61"), self.record(">A5E1", ">34B8", ">61")]
        comparison = decoder.compare_prediction(prediction, lines)
        self.assertEqual("FAIL", comparison["status"])
        self.assertEqual("contradiction", comparison["cells"][0]["status"])

    def test_prediction_class_and_mask_must_match_its_raw_byte(self) -> None:
        payload = bytes([0x60] * decoder.PAYLOAD_LENGTH)
        prediction = {"region": decoder.decode_region(payload, 0, 0, 1, 1)}
        prediction["region"]["cells"][0]["connection_mask"] = ">05"
        with self.assertRaises(decoder.DecoderError):
            decoder.compare_prediction(prediction, [])

    def test_comparison_has_compact_text_output(self) -> None:
        payload = bytes([0x60] * decoder.PAYLOAD_LENGTH)
        prediction = {"region": decoder.decode_region(payload, 0, 0, 1, 1)}
        lines = [
            self.record(operation, ">34B8", ">60")
            for operation in (">A3B5", ">A5E1")
        ]
        output = io.StringIO()
        with redirect_stdout(output):
            decoder._print(decoder.compare_prediction(prediction, lines), False)
        self.assertIn("PASS distinct=1", output.getvalue())
        self.assertIn("status=observed", output.getvalue())


class AcceptedOwnerLocalEvidenceTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        configured = os.environ.get("LIBRE99_TOD_EVIDENCE_ROOT")
        if not configured:
            raise unittest.SkipTest("LIBRE99_TOD_EVIDENCE_ROOT is not set")
        cls.root = Path(configured)
        cls.author = cls.root / "dungeon-recon-2026-08-27"
        cls.heldout = cls.root / "floor-kernel-model-2026-08-27"
        cls.semantics = cls.root / "payload-semantics-2026-08-27"
        required = [
            cls.author / "a1.log",
            cls.author / "a1.mtrace",
            cls.heldout / "a5c3-authentic.mtrace",
            cls.semantics / "a5c3-region-prediction.json",
        ]
        missing = [str(path) for path in required if not path.is_file()]
        if missing:
            raise unittest.SkipTest(f"owner-local evidence missing: {', '.join(missing)}")

    def test_authoring_consumer_reads_match_recorded_payload(self) -> None:
        payload = decoder.load_payload(self.author / "a1.log")
        observed: dict[int, set[int]] = {}
        by_operation: dict[str, set[int]] = {">A3BD": set(), ">A5F8": set()}
        for line in (self.author / "a1.mtrace").read_text(encoding="utf-8").splitlines():
            record = decoder._parse_record(line)
            if record.get("grom") not in (">A3BD", ">A5F8"):
                continue
            address = int(record["addr"][1:], 16)
            by_operation[record["grom"]].add(address)
            observed.setdefault(address, set()).add(int(record["byte"][1:], 16))
        self.assertEqual(124, len(by_operation[">A3BD"]))
        self.assertEqual(125, len(by_operation[">A5F8"]))
        self.assertEqual(125, len(observed))
        for address, values in observed.items():
            self.assertEqual({payload[address - decoder.PAYLOAD_BASE]}, values)

    def test_frozen_heldout_region_passes_distinct_coverage_rule(self) -> None:
        prediction = json.loads(
            (self.semantics / "a5c3-region-prediction.json").read_text(
                encoding="utf-8"
            )
        )
        evidence = (self.heldout / "a5c3-authentic.mtrace").read_text(encoding="utf-8").splitlines()
        comparison = decoder.compare_prediction(prediction, evidence)
        self.assertEqual("PASS", comparison["status"])
        self.assertEqual(4, len(comparison["distinct_coverage"]))
        self.assertTrue(all(item["observed"] for item in comparison["distinct_coverage"]))
        self.assertEqual(5, len(comparison["not_observed"]))
        self.assertEqual(
            {
                (0, 0, ">34B8"),
                (2, 2, ">34FA"),
                (2, 3, ">34FB"),
                (3, 2, ">351A"),
                (3, 3, ">351B"),
            },
            {
                (item["row"], item["column"], item["vram_address"])
                for item in comparison["not_observed"]
            },
        )
        self.assertTrue(
            all(
                (item["raw"], item["predicted_consumer_class"], item["predicted_connection_mask"])
                == (">6B", 0, ">00")
                for item in comparison["not_observed"]
            )
        )
        self.assertEqual([], comparison["partial_observations"])
        self.assertEqual([], comparison["contradictions"])
        self.assertEqual([], comparison["missing_distinct_coverage"])


if __name__ == "__main__":
    unittest.main()
