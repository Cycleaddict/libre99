#!/usr/bin/env python3
# Tests for tools/tod_floor_kernel_model.py. Licensed under LICENSE.md.

"""Focused tests for the bounded ToD candidate-buffer kernel model."""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import tod_floor_kernel_model as model  # noqa: E402


def inputs(seed: int = 0, payload: bytes | None = None) -> model.KernelInputs:
    return model.KernelInputs(
        seed=seed,
        payload=payload if payload is not None else bytes([0x6B]) * model.PAYLOAD_LENGTH,
        context_before=bytes(model.CONTEXT_BEFORE),
        context_after=bytes(model.CONTEXT_AFTER),
        control_833f=0x00,
    )


class RandomTest(unittest.TestCase):
    def test_console_rand_lcg_and_byte_swapped_modulus(self) -> None:
        kernel = model.Kernel(inputs(0x5207))
        self.assertEqual(0x0B, kernel._rand(0x1E))
        self.assertEqual(0xE3FC, kernel.seed)
        self.assertEqual(1, kernel.summary.random_calls)

    def test_bounded_range_rejects_until_value_is_inside(self) -> None:
        kernel = model.Kernel(inputs(0x0000))
        self.assertEqual(0x15, kernel._rand_between(0x03, 0x1C))
        self.assertEqual(0x7AB9, kernel.seed)


class LayoutTest(unittest.TestCase):
    def test_coordinate_conversion_matches_recovered_bounds(self) -> None:
        self.assertEqual(0, model.Kernel._index(0x02, 0x03))
        self.assertEqual(537, model.Kernel._index(0x12, 0x1C))

    def test_reset_preserves_below_60_bytes_and_normalizes_the_rest(self) -> None:
        payload = bytearray([0x6B]) * model.PAYLOAD_LENGTH
        payload[7] = 0x20
        kernel = model.Kernel(inputs(payload=bytes(payload)))
        kernel._reset(0x02)
        self.assertEqual(0x20, kernel._read(7))
        self.assertTrue(all(kernel._read(i) == 0x6B for i in range(model.PAYLOAD_LENGTH) if i != 7))
        self.assertEqual(model.PAYLOAD_LENGTH - 1, kernel.summary.writes[">8611"])
        self.assertEqual(model.PAYLOAD_LENGTH - 1, kernel.summary.writes[">863D"])

    def test_one_cell_context_is_required_and_not_returned_as_payload(self) -> None:
        with self.assertRaises(model.ModelError):
            model.KernelInputs(0, bytes(model.PAYLOAD_LENGTH), b"", bytes(model.CONTEXT_AFTER))
        result = model.predict(inputs())
        self.assertEqual(model.PAYLOAD_LENGTH, len(result.payload))


class RetryTest(unittest.TestCase):
    def test_direct_retry_resets_mode_01_and_continues_at_8283(self) -> None:
        result = model.predict(
            model.KernelInputs(
                seed=0,
                payload=bytes([0x6B]) * model.PAYLOAD_LENGTH,
                context_before=bytes(model.CONTEXT_BEFORE),
                context_after=bytes(model.CONTEXT_AFTER),
                value_67_count=0,
                value_6a_count=0,
                value_69_count=0,
                position_index=1,
                position_limit=1,
                control_833f=0x09,
                max_direct_retries=1,
            )
        )
        self.assertFalse(result.summary["completed"])
        self.assertEqual("direct-retry-bound-at->84EB", result.summary["termination"])
        self.assertEqual(1, result.summary["restarts"])
        self.assertEqual(2, len(result.summary["direct_retry_triggers"]))
        self.assertEqual(
            [">84EB", ">8605 mode >01", ">8283", ">84EB"],
            result.summary["control_flow"],
        )
        first = result.summary["direct_retry_triggers"][0]
        self.assertEqual(">68", first["value"])
        self.assertEqual(first, result.summary["direct_retry_triggers"][1])
        self.assertEqual(">00", result.inputs["value_67_count"])
        self.assertEqual(">00", result.inputs["value_6a_count"])

    def test_post_pass_checker_must_not_be_silently_assumed(self) -> None:
        with self.assertRaisesRegex(model.ModelError, ">857B is unresolved"):
            model.predict(
                model.KernelInputs(
                    seed=0,
                    payload=bytes([0x6B]) * model.PAYLOAD_LENGTH,
                    context_before=bytes(model.CONTEXT_BEFORE),
                    context_after=bytes(model.CONTEXT_AFTER),
                    value_67_count=0,
                    value_6a_count=0,
                    value_69_count=0,
                    position_index=0,
                    position_limit=0,
                    control_833f=0x09,
                )
            )


class ComparisonTest(unittest.TestCase):
    def test_first_mismatch_names_offset_and_vram_address(self) -> None:
        payload = bytes(model.PAYLOAD_LENGTH)
        result = model.KernelResult(payload, 0x1234, {"completed": True})
        actual = bytearray(payload)
        actual[9] = 0x55
        comparison = model.compare(result, bytes(actual), 0x1234)
        self.assertEqual("FAIL", comparison["status"])
        self.assertEqual(9, comparison["first_mismatch"]["offset"])
        self.assertEqual(">34C1", comparison["first_mismatch"]["vram_address"])

    def test_exact_state_and_seed_pass(self) -> None:
        payload = bytes(range(256)) * 2 + bytes(range(26))
        result = model.KernelResult(payload, 0xABCD, {"completed": True})
        comparison = model.compare(result, payload, 0xABCD)
        self.assertEqual("PASS", comparison["status"])
        self.assertTrue(comparison["payload_match"])
        self.assertTrue(comparison["next_seed_match"])


class AcceptedOwnerLocalEvidenceTest(unittest.TestCase):
    """Exact commercial-byte checks run only when the owner points at evidence."""

    @classmethod
    def setUpClass(cls) -> None:
        configured = os.environ.get("LIBRE99_TOD_DUNGEON_EVIDENCE")
        if not configured:
            raise unittest.SkipTest("LIBRE99_TOD_DUNGEON_EVIDENCE is not set")
        cls.root = Path(configured)
        required = ("initial-context.log", "a1.log", "b1.log")
        missing = [name for name in required if not (cls.root / name).is_file()]
        if missing:
            raise unittest.SkipTest(f"owner-local evidence missing: {', '.join(missing)}")

    def _case(self, name: str, seed: int, next_seed: int, digest: str, writes: int) -> None:
        before, after = model.load_context(self.root / "initial-context.log")
        initial_payload = model.load_payload(self.root / "initial-context.log")
        expected_payload = model.load_payload(self.root / f"{name}.log")
        result = model.predict(
            model.KernelInputs(seed, initial_payload, before, after, post_pass_retry=False)
        )
        self.assertEqual(expected_payload, result.payload)
        self.assertEqual(next_seed, result.next_seed)
        self.assertEqual(digest, hashlib.sha256(result.payload).hexdigest())
        self.assertEqual(writes, result.summary["candidate_writes"])

    def test_seed_0000_lineage_exact_payload_and_kernel_seed(self) -> None:
        self._case("a1", 0x5207, 0x558A, "c9df1a5878275ef6ebe83e1de5da58ef4765b0dcfcd44b9da7f3bb19e95d67aa", 1184)

    def test_seed_1234_lineage_exact_payload_and_kernel_seed(self) -> None:
        self._case("b1", 0x82F0, 0x23A7, "07b4583365cda870913caee9eb3689052a75a077f0f18bca041bbe5d0ee65603", 1142)


class CliTest(unittest.TestCase):
    def test_compact_prediction_omits_payload_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = root / "payload.bin"
            context = root / "context.bin"
            payload.write_bytes(bytes([0x6B]) * model.PAYLOAD_LENGTH)
            context.write_bytes(bytes(model.CONTEXT_LENGTH))
            completed = subprocess.run(
                [
                    sys.executable,
                    str(TOOLS / "tod_floor_kernel_model.py"),
                    "predict",
                    "--seed",
                    ">0000",
                    "--payload",
                    str(payload),
                    "--context",
                    str(context),
                    "--compact",
                    "--control-833f",
                    ">00",
                ],
                check=True,
                text=True,
                capture_output=True,
            )
            self.assertIn('"payload_sha256"', completed.stdout)
            self.assertNotIn('"payload_hex"', completed.stdout)


if __name__ == "__main__":
    unittest.main()
