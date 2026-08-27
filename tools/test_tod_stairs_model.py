#!/usr/bin/env python3
# Tests for tools/tod_stairs_model.py. Licensed under LICENSE.md.

"""Focused tests for the bounded ToD stairs-descend behavioral model."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import tod_stairs_model as tsm  # noqa: E402

# The positive spawn checkpoint recorded in the accepted atlas.
POSITIVE = {
    "key_code": 0x0A,
    "vram_1d00": 0x06,
    "vram_1ce8": 0x01,
    "vram_10fe": 0x00,
    "vram_1cf8": 0x00,
    "vram_10fa": 0x00,
}


def state(**changes: object) -> tsm.StairsState:
    fields = dict(POSITIVE)
    fields.update(changes)
    return tsm.StairsState.from_mapping(fields)


def addresses(result) -> tuple:
    return tuple(result.gpl_operations)


class AcceptedPathTest(unittest.TestCase):
    def test_positive_atlas_state_is_accepted_at_66c7(self) -> None:
        result = tsm.predict_immediate(state())
        self.assertEqual(tsm.STATUS_ACCEPTED, result.status)
        self.assertEqual(tsm.REASON_ACCEPTED, result.reason)
        self.assertEqual(0x66C7, result.branch)
        self.assertEqual(tsm.EFFECT_DESCENDING, result.visible_effect)
        self.assertTrue(result.delayed_copy_pending)
        # Immediate mutations only: the counter increments, the predicate cell
        # is consumed, and the delayed destination is untouched so far.
        self.assertEqual(0x01, result.state.vram_1cf8)
        self.assertEqual(0x05, result.state.vram_1d00)
        self.assertEqual(0x00, result.state.vram_10fa)
        self.assertEqual(0x00, result.before.vram_1cf8)
        self.assertEqual(
            (0x669B, 0x66A0, 0x66A7, 0x66AE, 0x66B5, 0x66C7, 0x66CB), addresses(result)
        )

    def test_equality_at_66b5_takes_the_transition_path(self) -> None:
        # >10FE == >1CF8 is the observed positive case; the comparison accepts it.
        result = tsm.predict_immediate(state(vram_10fe=0x03, vram_1cf8=0x03))
        self.assertEqual(tsm.STATUS_ACCEPTED, result.status)
        self.assertEqual(0x04, result.state.vram_1cf8)
        self.assertNotIn(0x8018, addresses(result))

    def test_native_boundary_is_metadata_on_the_accepted_path(self) -> None:
        result = tsm.predict_immediate(state())
        boundary = [
            (item["address"], item["opcode"]) for item in result.native_operations
        ]
        self.assertEqual(
            [(">08B0", ">D013"), (">08CE", ">D020"), (">1D2A", ">D802")], boundary
        )


class RejectionTest(unittest.TestCase):
    def test_predicate_rejection_branches_to_663f(self) -> None:
        before = state(vram_1d00=0x05)
        result = tsm.predict_immediate(before)
        self.assertEqual(tsm.STATUS_REJECTED, result.status)
        self.assertEqual(tsm.REASON_PREDICATE, result.reason)
        self.assertEqual(0x663F, result.branch)
        self.assertEqual(tsm.EFFECT_NONE, result.visible_effect)
        self.assertFalse(result.delayed_copy_pending)
        self.assertEqual(before, result.state)  # no in-boundary mutation
        self.assertEqual(
            (0x669B, 0x66A0, 0x66A7, 0x66AC, 0x663F), addresses(result)
        )

    def test_input_rejection_branches_to_66e5(self) -> None:
        before = state(key_code=0x0B)
        result = tsm.predict_immediate(before)
        self.assertEqual(tsm.STATUS_REJECTED, result.status)
        self.assertEqual(tsm.REASON_INPUT, result.reason)
        self.assertEqual(0x66E5, result.branch)
        self.assertEqual(before, result.state)
        # The predicate cell is never consulted once the key fails.
        self.assertEqual((0x669B, 0x66E5), addresses(result))
        self.assertEqual(
            [">08B0"], [item["address"] for item in result.native_operations]
        )


class PredicateVariantTest(unittest.TestCase):
    def test_1d00_04_is_accepted_by_the_first_comparison(self) -> None:
        result = tsm.predict_immediate(state(vram_1d00=0x04))
        self.assertEqual(tsm.STATUS_ACCEPTED, result.status)
        self.assertEqual(0x66C7, result.branch)
        self.assertEqual(0x05, result.state.vram_1d00)
        self.assertNotIn(0x66A7, addresses(result))

    def test_1ce8_not_01_accepts_through_the_66b3_bypass(self) -> None:
        # The values that would otherwise fail the >10FE/>1CF8 comparison and
        # the acknowledgement fallback must not be consulted at all.
        result = tsm.predict_immediate(
            state(vram_1ce8=0x00, vram_10fe=0x00, vram_1cf8=0x7F)
        )
        self.assertEqual(tsm.STATUS_ACCEPTED, result.status)
        self.assertEqual(0x66C7, result.branch)
        self.assertEqual(0x80, result.state.vram_1cf8)
        self.assertIn(0x66B3, addresses(result))
        self.assertNotIn(0x66B5, addresses(result))
        self.assertNotIn(0x8018, addresses(result))


class UnsignedComparisonTest(unittest.TestCase):
    def test_high_bit_operand_compares_unsigned_and_accepts(self) -> None:
        # Signed arithmetic would read >80 as negative and take the fallback.
        result = tsm.predict_immediate(state(vram_10fe=0x80, vram_1cf8=0x7F))
        self.assertEqual(tsm.STATUS_ACCEPTED, result.status)
        self.assertEqual(0x80, result.state.vram_1cf8)
        self.assertNotIn(0x8018, addresses(result))

    def test_lower_operand_takes_the_acknowledgement_path(self) -> None:
        result = tsm.predict_immediate(
            state(vram_10fe=0x7F, vram_1cf8=0x80),
            tsm.FallbackInput(scan_key_code=0x20),
        )
        self.assertIn(0x8018, addresses(result))
        self.assertEqual(tsm.STATUS_REJECTED, result.status)
        self.assertEqual(0x80, result.state.vram_1cf8)


class AcknowledgementContractTest(unittest.TestCase):
    def _fallback(self, **changes: object) -> tsm.StairsState:
        return state(vram_10fe=0x00, vram_1cf8=0x01, **changes)

    def test_authentic_contract_rejects_after_new_key(self) -> None:
        before = self._fallback()
        result = tsm.predict_immediate(
            before,
            tsm.FallbackInput(
                scratch_83a1=0,
                vdp_timer=0x8D,
                vram_1d01=0x01,
                scan_key_code=0x20,
            ),
        )
        self.assertEqual(tsm.STATUS_REJECTED, result.status)
        self.assertEqual(tsm.REASON_ACKNOWLEDGED, result.reason)
        self.assertEqual(0x663F, result.branch)
        self.assertEqual(0x01, result.state.vram_1cf8)
        self.assertEqual(0x06, result.state.vram_1d00)
        self.assertEqual(0x20, result.state.key_code)
        self.assertEqual(tsm.EFFECT_MESSAGE_ACK, result.visible_effect)
        self.assertEqual(0x02, result.fallback.initial_backdrop)
        self.assertEqual(0x03, result.fallback.final_backdrop)
        self.assertEqual("reset", result.fallback.return_condition)
        self.assertIn(0x96EA, addresses(result))
        self.assertIn(0x96FF, addresses(result))
        self.assertIn(0x9710, addresses(result))
        self.assertEqual(0x663F, addresses(result)[-1])
        self.assertEqual(
            [">08B0", ">08CE", ">1CAA"],
            [item["address"] for item in result.native_operations],
        )

    def test_zero_83a1_without_new_key_stops_in_scan_wait(self) -> None:
        before = self._fallback()
        result = tsm.predict_immediate(before, tsm.FallbackInput(vdp_timer=0x40))
        self.assertEqual(tsm.STATUS_UNRESOLVED, result.status)
        self.assertEqual(tsm.REASON_ACK_INPUT_MISSING, result.reason)
        self.assertEqual(0x96FF, result.branch)
        self.assertEqual(tsm.EFFECT_UNRESOLVED, result.visible_effect)
        self.assertFalse(result.delayed_copy_pending)
        self.assertEqual(before, result.state)
        self.assertEqual(0x0C, result.fallback.initial_backdrop)
        self.assertIsNone(result.fallback.return_condition)
        self.assertEqual(0x96FF, addresses(result)[-1])

    def test_nonzero_83a1_clears_and_returns_without_scan(self) -> None:
        before = self._fallback()
        result = tsm.predict_immediate(
            before, tsm.FallbackInput(scratch_83a1=0x7F)
        )
        self.assertEqual(tsm.STATUS_REJECTED, result.status)
        self.assertFalse(result.fallback.waited_for_input)
        self.assertEqual(0, result.fallback.scratch_83a1_after)
        self.assertIsNone(result.fallback.initial_backdrop)
        self.assertNotIn(0x96FF, addresses(result))

    def test_1d01_02_selects_final_backdrop_06(self) -> None:
        result = tsm.predict_immediate(
            self._fallback(),
            tsm.FallbackInput(vram_1d01=0x02, scan_key_code=0x0D),
        )
        self.assertEqual(0x06, result.fallback.final_backdrop)
        self.assertNotIn(0x970E, addresses(result))

    def test_omitted_fallback_context_is_unresolved_not_guessed(self) -> None:
        result = tsm.predict_immediate(self._fallback())
        self.assertEqual(tsm.STATUS_UNRESOLVED, result.status)
        self.assertEqual(tsm.REASON_ACK_INPUT_MISSING, result.reason)
        self.assertIsNone(result.branch)
        self.assertIsNone(result.fallback)


class ByteWrapTest(unittest.TestCase):
    def test_counter_increment_wraps_at_ff(self) -> None:
        result = tsm.predict_immediate(state(vram_10fe=0xFF, vram_1cf8=0xFF))
        self.assertEqual(tsm.STATUS_ACCEPTED, result.status)
        self.assertEqual(0x00, result.state.vram_1cf8)
        self.assertEqual(0x00, tsm.apply_delayed(result).state.vram_10fa)


class DelayedPhaseTest(unittest.TestCase):
    def test_delayed_copy_assigns_1cf8_into_10fa_at_a798(self) -> None:
        immediate = tsm.predict_immediate(state())
        delayed = tsm.apply_delayed(immediate)
        self.assertEqual(0xA798, delayed.address)
        self.assertEqual(tsm.DELAYED_STATUS_COMPLETE, delayed.status)
        self.assertEqual(0x01, delayed.state.vram_10fa)
        self.assertEqual(0x01, delayed.state.vram_1cf8)
        # The delayed phase touches nothing else in the boundary.
        self.assertEqual(immediate.state.vram_1d00, delayed.state.vram_1d00)
        self.assertEqual(0x00, delayed.before.vram_10fa)

    def test_delayed_copy_requires_an_accepted_pending_result(self) -> None:
        for rejected in (
            tsm.predict_immediate(state(vram_1d00=0x05)),
            tsm.predict_immediate(state(key_code=0x00)),
            tsm.predict_immediate(state(vram_10fe=0x00, vram_1cf8=0x01)),
        ):
            with self.assertRaises(tsm.DelayedPhaseError) as caught:
                tsm.apply_delayed(rejected)
            self.assertIn(">A798", str(caught.exception))

    def test_predict_applies_the_delayed_phase_only_on_request(self) -> None:
        _, without = tsm.predict(state())
        _, with_delayed = tsm.predict(state(), with_delayed=True)
        self.assertIsNone(without)
        self.assertEqual(0x01, with_delayed.state.vram_10fa)
        _, none_pending = tsm.predict(state(vram_1d00=0x05), with_delayed=True)
        self.assertIsNone(none_pending)


class InputValidationTest(unittest.TestCase):
    def test_bytes_are_range_checked(self) -> None:
        for bad in (256, -1, 0x100):
            with self.assertRaises(tsm.ModelError) as caught:
                state(vram_1cf8=bad)
            self.assertIn("vram_1cf8", str(caught.exception))

    def test_booleans_and_nonsense_are_refused(self) -> None:
        for bad in (True, None, 1.5, "ten"):
            with self.assertRaises(tsm.ModelError) as caught:
                state(key_code=bad)
            self.assertIn("key_code", str(caught.exception))

    def test_missing_and_unknown_fields_are_named(self) -> None:
        fields = dict(POSITIVE)
        del fields["vram_10fa"]
        with self.assertRaises(tsm.ModelError) as caught:
            tsm.StairsState.from_mapping(fields)
        self.assertIn("vram_10fa", str(caught.exception))

        fields = dict(POSITIVE)
        fields["vram_9999"] = 0
        with self.assertRaises(tsm.ModelError) as caught:
            tsm.StairsState.from_mapping(fields)
        self.assertIn("vram_9999", str(caught.exception))

    def test_hex_and_decimal_byte_forms_agree(self) -> None:
        self.assertEqual(
            tsm.predict_immediate(state(key_code=">0A")).status,
            tsm.predict_immediate(state(key_code="0x0a")).status,
        )
        self.assertEqual(0x0A, tsm.parse_byte("10", "key_code"))
        self.assertEqual(0x0A, tsm.parse_byte(">0A", "key_code"))


class NeutralNamingTest(unittest.TestCase):
    """The model reconstructs behavior; it does not name game semantics."""

    GUESSES = (
        "floor",
        "depth",
        "level",
        "dungeon",
        "party",
        "player",
        "quest",
        "monster",
        "room",
        "map",
    )

    def test_state_fields_stay_address_based(self) -> None:
        self.assertEqual(
            (
                "key_code",
                "vram_1d00",
                "vram_1ce8",
                "vram_10fe",
                "vram_1cf8",
                "vram_10fa",
            ),
            tsm.BYTE_FIELDS,
        )

    def test_operation_roles_claim_no_game_meaning(self) -> None:
        for address, role in tsm.GPL_OPERATION_ROLES.items():
            for guess in self.GUESSES:
                self.assertNotIn(
                    guess,
                    role.lower(),
                    f"{tsm.hex_address(address)} role guesses {guess!r}: {role}",
                )

    def test_the_callee_is_declared_as_a_tail_trampoline(self) -> None:
        role = tsm.GPL_OPERATION_ROLES[0x8018]
        self.assertIn("tail trampoline", role)
        self.assertNotIn("predicate", role)

    def test_prediction_is_deterministic(self) -> None:
        first = tsm.prediction_dict(*tsm.predict(state(), with_delayed=True))
        second = tsm.prediction_dict(*tsm.predict(state(), with_delayed=True))
        self.assertEqual(
            json.dumps(first, sort_keys=True), json.dumps(second, sort_keys=True)
        )


class CliTest(unittest.TestCase):
    def _run(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(TOOLS / "tod_stairs_model.py"), *args],
            capture_output=True,
            text=True,
            check=False,
        )

    POSITIVE_ARGS = (
        "predict",
        "--key-code",
        ">0A",
        "--vram-1d00",
        ">06",
        "--vram-1ce8",
        ">01",
        "--vram-10fe",
        ">00",
        "--vram-1cf8",
        ">00",
        "--vram-10fa",
        ">00",
    )

    def test_predict_json_carries_the_whole_prediction(self) -> None:
        done = self._run(*self.POSITIVE_ARGS, "--delayed", "--json")
        self.assertEqual(0, done.returncode, done.stderr)
        payload = json.loads(done.stdout)
        self.assertEqual("accepted", payload["status"])
        self.assertEqual(">66C7", payload["branch"])
        self.assertEqual("DESCENDING", payload["visible_effect"])
        self.assertEqual(">01", payload["immediate"]["vram_1cf8"])
        self.assertEqual(">05", payload["immediate"]["vram_1d00"])
        self.assertEqual(">00", payload["immediate"]["vram_10fa"])
        self.assertEqual(">01", payload["delayed"]["state"]["vram_10fa"])
        self.assertEqual(">A798", payload["delayed"]["address"])
        self.assertEqual(
            [">08B0", ">08CE", ">1D2A"],
            [item["address"] for item in payload["native_operations"]],
        )

    def test_predict_text_reports_the_unresolved_refusal(self) -> None:
        done = self._run(
            "predict", "--key-code", ">0A", "--vram-1d00", ">06",
            "--vram-1ce8", ">01", "--vram-10fe", ">00", "--vram-1cf8", ">01",
        )
        self.assertEqual(0, done.returncode, done.stderr)
        self.assertIn("unresolved", done.stdout)
        self.assertIn("new_scan_key_not_supplied", done.stdout)
        self.assertIn(">8018", done.stdout)

    def test_predict_json_reports_the_authentic_fallback_contract(self) -> None:
        done = self._run(
            "predict", "--key-code", ">0A", "--vram-1d00", ">06",
            "--vram-1ce8", ">01", "--vram-10fe", ">00",
            "--vram-1cf8", ">01", "--scratch-83a1", ">00",
            "--vdp-timer", ">8D", "--vram-1d01", ">01",
            "--scan-key-code", ">20", "--json",
        )
        self.assertEqual(0, done.returncode, done.stderr)
        payload = json.loads(done.stdout)
        self.assertEqual("rejected", payload["status"])
        self.assertEqual(">663F", payload["branch"])
        self.assertEqual("reset", payload["fallback"]["return_condition"])
        self.assertEqual(">02", payload["fallback"]["initial_backdrop"])
        self.assertEqual(">03", payload["fallback"]["final_backdrop"])

    def test_predict_refuses_an_out_of_range_byte(self) -> None:
        done = self._run("predict", "--key-code", "300")
        self.assertEqual(tsm.USAGE_EXIT, done.returncode)
        self.assertIn("key_code", done.stderr)

    def _case_file(self, tmp: Path, expected: dict) -> Path:
        path = tmp / "cases.json"
        path.write_text(
            json.dumps(
                {
                    "format": tsm.COMPARISON_FORMAT,
                    "version": tsm.COMPARISON_VERSION,
                    "cases": [
                        {
                            "name": "synthetic-positive",
                            "input": dict(POSITIVE),
                            "expected": expected,
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        return path

    def test_compare_passes_a_matching_case(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._case_file(
                Path(tmp),
                {
                    "status": "accepted",
                    "branch": ">66C7",
                    "visible_effect": "DESCENDING",
                    "immediate": {"vram_1cf8": ">01", "vram_1d00": ">05"},
                    "delayed": {"vram_10fa": ">01"},
                },
            )
            done = self._run("compare", str(path))
        self.assertEqual(0, done.returncode, done.stderr)
        self.assertIn("case 1 synthetic-positive", done.stdout)
        self.assertIn("PASS immediate.vram_1cf8 = >01", done.stdout)
        self.assertIn("PASS delayed.vram_10fa = >01", done.stdout)
        self.assertIn("0 fail", done.stdout)
        self.assertNotIn("FAIL", done.stdout)

    def test_compare_fails_and_names_the_field(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._case_file(
                Path(tmp),
                {"status": "accepted", "immediate": {"vram_1cf8": ">02"}},
            )
            done = self._run("compare", str(path))
        self.assertEqual(tsm.FAILURE_EXIT, done.returncode)
        self.assertIn("FAIL immediate.vram_1cf8: observed >02, model >01", done.stdout)
        self.assertIn("1 failing case(s)", done.stdout)

    def test_compare_refuses_an_unversioned_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cases.json"
            path.write_text(json.dumps({"cases": []}), encoding="utf-8")
            done = self._run("compare", str(path))
        self.assertEqual(tsm.USAGE_EXIT, done.returncode)
        self.assertIn("format", done.stderr)

    def test_compare_refuses_an_unknown_expected_field(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._case_file(Path(tmp), {"screen_bytes": "..."})
            done = self._run("compare", str(path))
        self.assertEqual(tsm.USAGE_EXIT, done.returncode)
        self.assertIn("screen_bytes", done.stderr)

    def test_compare_refuses_a_missing_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            done = self._run("compare", str(Path(tmp) / "absent.json"))
        self.assertEqual(tsm.USAGE_EXIT, done.returncode)
        self.assertIn("absent.json", done.stderr)

    def test_compare_reports_a_delayed_expectation_with_no_pending_copy(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cases.json"
            rejected = dict(POSITIVE, vram_1d00=0x05)
            path.write_text(
                json.dumps(
                    {
                        "format": tsm.COMPARISON_FORMAT,
                        "version": tsm.COMPARISON_VERSION,
                        "cases": [
                            {
                                "name": "synthetic-negative",
                                "input": rejected,
                                "expected": {"delayed": {"vram_10fa": ">01"}},
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            done = self._run("compare", str(path))
        self.assertEqual(tsm.FAILURE_EXIT, done.returncode)
        self.assertIn("no pending copy", done.stdout)


if __name__ == "__main__":
    unittest.main()
