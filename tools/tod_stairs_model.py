#!/usr/bin/env python3
# Bounded Tunnels of Doom stairs-descend behavioral model. Licensed under
# LICENSE.md (Modified MIT with Commons Clause). Standard library only.

"""Executable behavioral model of the bounded ToD stairs-descend subsystem.

This is a reconstruction model, not an emulator and not a GPL runtime. It
predicts, for the frozen R4 boundary only:

* the original GPL decision starting at ``>669B``,
* the immediate state mutations at ``>66C7``/``>66CB``,
* the separately applied delayed copy at ``>A798``,
* the visible-effect classification (DESCENDING, message/acknowledgement, or
  nothing),
* and the recovered ``>8018``/``>96EA`` acknowledgement contract.

Everything outside that boundary — the screen renderer, the map,
the party, combat, and the rest of the GPL machine — is deliberately absent.
The native interpreter accesses (``>08B0``/``>D013``, ``>08CE``/``>D020``,
``>1D2A``/``>D802``) are reported as metadata; they are not emulated.

State names are neutral and address-based on purpose. ``>1D00``, ``>1CE8``,
``>10FE``, ``>1CF8``, and ``>10FA`` have no confirmed game-level meaning, and
this model does not invent one. See
``docs/TOD-STAIRS-MODEL.md``.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Mapping, Optional, Sequence, Tuple

MODEL_ID = "tod-stairs-descend"
MODEL_VERSION = 2

COMPARISON_FORMAT = "libre99-observatory/tod-stairs-comparison"
COMPARISON_VERSION = 1

FAILURE_EXIT = 1
USAGE_EXIT = 2

# --- frozen constants ------------------------------------------------------

BYTE_FIELDS: Tuple[str, ...] = (
    "key_code",
    "vram_1d00",
    "vram_1ce8",
    "vram_10fe",
    "vram_1cf8",
    "vram_10fa",
)

KEY_DOWN_ARROW = 0x0A
ELIGIBLE_PREDICATE_VALUES: Tuple[int, ...] = (0x04, 0x06)
CONSUMED_PREDICATE_VALUE = 0x05
EXTERNAL_MESSAGE_ID = 0x2D

ADDR_KEY_COMPARE = 0x669B
ADDR_PREDICATE_FIRST = 0x66A0
ADDR_PREDICATE_SECOND = 0x66A7
ADDR_REJECT_BRANCH = 0x66AC
ADDR_REJECT_TARGET = 0x663F
ADDR_SECONDARY_COMPARE = 0x66AE
ADDR_SECONDARY_BYPASS = 0x66B3
ADDR_COUNTER_COMPARE = 0x66B5
ADDR_MESSAGE_CALL = 0x66BE
ADDR_EXTERNAL_CALL = 0x8018
ADDR_ACK_ENTRY = 0x96EA
ADDR_ACK_TIMER = 0x96F3
ADDR_ACK_SCAN = 0x96FF
ADDR_ACK_FINAL_BACKDROP_ALT = 0x970E
ADDR_ACK_RETURN = 0x9710
ADDR_ACK_RETURN_BRANCH = 0x66C5
ADDR_ACCEPT_TARGET = 0x66C7
ADDR_PREDICATE_STORE = 0x66CB
ADDR_INPUT_REJECT_TARGET = 0x66E5
ADDR_DELAYED_COPY = 0xA798

GPL_OPERATION_ROLES: Dict[int, str] = {
    ADDR_KEY_COMPARE: "compare the key byte with the down-arrow code",
    ADDR_PREDICATE_FIRST: "first eligibility comparison (accepts 04)",
    ADDR_PREDICATE_SECOND: "second eligibility comparison (requires 06)",
    ADDR_REJECT_BRANCH: "rejected-path branch",
    ADDR_REJECT_TARGET: "rejected-path continuation target",
    ADDR_SECONDARY_COMPARE: "compare >1CE8 with 01",
    ADDR_SECONDARY_BYPASS: "secondary-flag bypass branch to the accepted target",
    ADDR_COUNTER_COMPARE: "compare >10FE with >1CF8",
    ADDR_MESSAGE_CALL: "CALL message routine with inline byte >2D",
    ADDR_EXTERNAL_CALL: "condition-reset tail trampoline to acknowledgement routine",
    ADDR_ACK_ENTRY: "acknowledgement entry; clear nonzero >83A1 or enter wait",
    ADDR_ACK_TIMER: "select waiting backdrop from vdp_timer bit >40",
    ADDR_ACK_SCAN: "SCAN until a new key sets condition",
    ADDR_ACK_FINAL_BACKDROP_ALT: "select final backdrop >03 when >1D01 is not >02",
    ADDR_ACK_RETURN: "ordinary RTN clearing condition",
    ADDR_ACK_RETURN_BRANCH: "BR-on-condition-reset to >663F after acknowledgement",
    ADDR_ACCEPT_TARGET: "INC of >1CF8, the accepted target",
    ADDR_PREDICATE_STORE: "ST of >05 into >1D00",
    ADDR_INPUT_REJECT_TARGET: "input-rejection continuation target",
    ADDR_DELAYED_COPY: "ST copying >1CF8 into >10FA (delayed phase)",
}

# Interpreter boundary metadata only. This model never executes these.
NATIVE_KEY_READ: Mapping[str, str] = {
    "address": ">08B0",
    "opcode": ">D013",
    "role": "native GPL value-loader byte read of the key code",
}
NATIVE_VDP_READ: Mapping[str, str] = {
    "address": ">08CE",
    "opcode": ">D020",
    "role": "native VDP data-port read of a GPL comparison operand",
}
NATIVE_VDP_WRITE: Mapping[str, str] = {
    "address": ">1D2A",
    "opcode": ">D802",
    "role": "native VDP data-port write performing a GPL store",
}
NATIVE_SCAN_WRITE: Mapping[str, str] = {
    "address": ">1CAA",
    "opcode": ">D800",
    "role": "native SCAN/KSCAN boundary write of the detected key byte",
}

STATUS_ACCEPTED = "accepted"
STATUS_REJECTED = "rejected"
STATUS_UNRESOLVED = "unresolved"

REASON_ACCEPTED = "accepted"
REASON_INPUT = "input"
REASON_PREDICATE = "predicate"
REASON_ACKNOWLEDGED = "acknowledgement_complete"
REASON_ACK_INPUT_MISSING = "new_scan_key_not_supplied"

EFFECT_DESCENDING = "DESCENDING"
EFFECT_NONE = "NONE"
EFFECT_MESSAGE_ACK = "MESSAGE_2D_ACK"
EFFECT_UNRESOLVED = "UNRESOLVED"

DELAYED_STATUS_COMPLETE = "complete"
DELAYED_REASON_COMPLETE = "copy_at_A798_complete"


# --- errors ----------------------------------------------------------------


class ModelError(ValueError):
    """Invalid model input, or a phase used outside its precondition."""


class DelayedPhaseError(ModelError):
    """The delayed copy was requested without an accepted pending result."""


class ComparisonFileError(ModelError):
    """The owner-local comparison case list is malformed."""


# --- byte helpers ----------------------------------------------------------


def hex_byte(value: int) -> str:
    return f">{value:02X}"


def hex_address(address: int) -> str:
    return f">{address:04X}"


def parse_byte(value: Any, field: str) -> int:
    """Accept ``10``, ``">0A"``, or ``"0x0A"``; refuse anything outside 0..255."""

    if isinstance(value, bool):
        raise ModelError(f"{field}: expected a byte 0..255, found a boolean")
    if isinstance(value, int):
        number = value
    elif isinstance(value, str):
        text = value.strip()
        if not text:
            raise ModelError(f"{field}: expected a byte 0..255, found an empty string")
        try:
            if text.startswith(">"):
                number = int(text[1:], 16)
            elif text[:2].lower() == "0x":
                number = int(text[2:], 16)
            else:
                number = int(text, 10)
        except ValueError:
            raise ModelError(
                f"{field}: {value!r} is not a byte; use 10, '>0A', or '0x0A'"
            ) from None
    else:
        raise ModelError(
            f"{field}: expected a byte 0..255, found {type(value).__name__}"
        )
    if not 0 <= number <= 0xFF:
        raise ModelError(f"{field}: byte out of range 0..255: {number}")
    return number


def parse_optional_byte(value: Any, field: str) -> Optional[int]:
    """Parse an optional byte; ``None`` means no new SCAN input is modeled."""

    return None if value is None else parse_byte(value, field)


# --- state -----------------------------------------------------------------


@dataclass(frozen=True)
class StairsState:
    """The six explicit bytes the bounded subsystem reads or writes."""

    key_code: int
    vram_1d00: int
    vram_1ce8: int
    vram_10fe: int
    vram_1cf8: int
    vram_10fa: int

    def __post_init__(self) -> None:
        for field in BYTE_FIELDS:
            object.__setattr__(
                self, field, parse_byte(getattr(self, field), field)
            )

    @classmethod
    def from_mapping(cls, mapping: Any, where: str = "state") -> "StairsState":
        if not isinstance(mapping, dict):
            raise ModelError(f"{where}: expected a JSON object of byte fields")
        unknown = sorted(set(mapping) - set(BYTE_FIELDS))
        if unknown:
            raise ModelError(f"{where}: unknown field(s): {', '.join(unknown)}")
        missing = [field for field in BYTE_FIELDS if field not in mapping]
        if missing:
            raise ModelError(f"{where}: missing field(s): {', '.join(missing)}")
        return cls(
            **{
                field: parse_byte(mapping[field], f"{where}.{field}")
                for field in BYTE_FIELDS
            }
        )

    def with_changes(self, **changes: int) -> "StairsState":
        unknown = sorted(set(changes) - set(BYTE_FIELDS))
        if unknown:
            raise ModelError(f"state: unknown field(s): {', '.join(unknown)}")
        return dataclasses.replace(self, **changes)

    def as_dict(self) -> Dict[str, str]:
        return {field: hex_byte(getattr(self, field)) for field in BYTE_FIELDS}


# --- results ---------------------------------------------------------------


@dataclass(frozen=True)
class FallbackInput:
    """Concrete state/input read by ``>96EA`` after the ``>8018`` transfer."""

    scratch_83a1: int = 0
    vdp_timer: int = 0
    vram_1d01: int = 0
    scan_key_code: Optional[int] = None

    def __post_init__(self) -> None:
        for field in ("scratch_83a1", "vdp_timer", "vram_1d01"):
            object.__setattr__(self, field, parse_byte(getattr(self, field), field))
        object.__setattr__(
            self,
            "scan_key_code",
            parse_optional_byte(self.scan_key_code, "scan_key_code"),
        )

    @classmethod
    def from_mapping(cls, mapping: Any, where: str = "fallback") -> "FallbackInput":
        if not isinstance(mapping, dict):
            raise ModelError(f"{where}: expected a JSON object")
        fields = {"scratch_83a1", "vdp_timer", "vram_1d01", "scan_key_code"}
        unknown = sorted(set(mapping) - fields)
        if unknown:
            raise ModelError(f"{where}: unknown field(s): {', '.join(unknown)}")
        return cls(**mapping)


@dataclass(frozen=True)
class FallbackResult:
    """The bounded acknowledgement effects, separate from stairs state bytes."""

    scratch_83a1_before: int
    scratch_83a1_after: int
    vdp_timer: int
    vram_1d01: int
    scan_key_code: Optional[int]
    waited_for_input: bool
    initial_backdrop: Optional[int]
    final_backdrop: Optional[int]
    return_condition: Optional[str]

    def as_dict(self) -> Dict[str, Any]:
        return {
            "entered": ">96EA",
            "scratch_83a1_before": hex_byte(self.scratch_83a1_before),
            "scratch_83a1_after": hex_byte(self.scratch_83a1_after),
            "vdp_timer": hex_byte(self.vdp_timer),
            "vram_1d01": hex_byte(self.vram_1d01),
            "scan_key_code": (
                hex_byte(self.scan_key_code) if self.scan_key_code is not None else None
            ),
            "waited_for_input": self.waited_for_input,
            "initial_backdrop": (
                hex_byte(self.initial_backdrop)
                if self.initial_backdrop is not None
                else None
            ),
            "final_backdrop": (
                hex_byte(self.final_backdrop)
                if self.final_backdrop is not None
                else None
            ),
            "return_condition": self.return_condition,
        }


@dataclass(frozen=True)
class ImmediateResult:
    """The predicted GPL decision and its immediate in-boundary mutations."""

    status: str
    reason: str
    branch: Optional[int]
    before: StairsState
    state: StairsState
    visible_effect: str
    delayed_copy_pending: bool
    fallback: Optional[FallbackResult]
    gpl_operations: Tuple[int, ...]
    native_operations: Tuple[Mapping[str, str], ...]

    def as_dict(self) -> Dict[str, Any]:
        return {
            "model": MODEL_ID,
            "model_version": MODEL_VERSION,
            "status": self.status,
            "reason": self.reason,
            "branch": hex_address(self.branch) if self.branch is not None else None,
            "visible_effect": self.visible_effect,
            "delayed_copy_pending": self.delayed_copy_pending,
            "fallback": self.fallback.as_dict() if self.fallback is not None else None,
            "before": self.before.as_dict(),
            "immediate": self.state.as_dict(),
            "gpl_operations": _operations_as_list(self.gpl_operations),
            "native_operations": [dict(item) for item in self.native_operations],
        }


@dataclass(frozen=True)
class DelayedResult:
    """The separately applied copy at ``>A798``."""

    status: str
    reason: str
    address: int
    before: StairsState
    state: StairsState
    gpl_operations: Tuple[int, ...]
    native_operations: Tuple[Mapping[str, str], ...]

    def as_dict(self) -> Dict[str, Any]:
        return {
            "status": self.status,
            "reason": self.reason,
            "address": hex_address(self.address),
            "before": self.before.as_dict(),
            "state": self.state.as_dict(),
            "gpl_operations": _operations_as_list(self.gpl_operations),
            "native_operations": [dict(item) for item in self.native_operations],
        }


def _operations_as_list(addresses: Sequence[int]) -> List[Dict[str, str]]:
    return [
        {"address": hex_address(address), "role": GPL_OPERATION_ROLES[address]}
        for address in addresses
    ]


def prediction_dict(
    immediate: ImmediateResult, delayed: Optional[DelayedResult] = None
) -> Dict[str, Any]:
    payload = immediate.as_dict()
    payload["delayed"] = delayed.as_dict() if delayed is not None else None
    return payload


# --- the model ------------------------------------------------------------


def predict_immediate(
    state: StairsState, fallback: Optional[FallbackInput] = None
) -> ImmediateResult:
    """Predict the original decision and its immediate mutations.

    Mirrors the frozen R4 control flow plus the recovered G-007 fallback. When
    ``>83A1`` is zero and no new SCAN key is supplied, the model stops in the
    proven wait rather than inventing input timing.
    """

    if not isinstance(state, StairsState):
        raise ModelError("state: expected a StairsState")
    if fallback is not None and not isinstance(fallback, FallbackInput):
        raise ModelError("fallback: expected a FallbackInput")

    ops: List[int] = [ADDR_KEY_COMPARE]
    natives: List[Mapping[str, str]] = [NATIVE_KEY_READ]

    def finish(
        status: str,
        reason: str,
        branch: Optional[int],
        result_state: StairsState,
        visible_effect: str,
        pending: bool,
        fallback_result: Optional[FallbackResult] = None,
    ) -> ImmediateResult:
        return ImmediateResult(
            status=status,
            reason=reason,
            branch=branch,
            before=state,
            state=result_state,
            visible_effect=visible_effect,
            delayed_copy_pending=pending,
            fallback=fallback_result,
            gpl_operations=tuple(ops),
            native_operations=tuple(natives),
        )

    def accept() -> ImmediateResult:
        ops.extend((ADDR_ACCEPT_TARGET, ADDR_PREDICATE_STORE))
        natives.append(NATIVE_VDP_WRITE)
        mutated = state.with_changes(
            vram_1cf8=(state.vram_1cf8 + 1) & 0xFF,
            vram_1d00=CONSUMED_PREDICATE_VALUE,
        )
        return finish(
            STATUS_ACCEPTED,
            REASON_ACCEPTED,
            ADDR_ACCEPT_TARGET,
            mutated,
            EFFECT_DESCENDING,
            True,
        )

    if state.key_code != KEY_DOWN_ARROW:
        ops.append(ADDR_INPUT_REJECT_TARGET)
        return finish(
            STATUS_REJECTED,
            REASON_INPUT,
            ADDR_INPUT_REJECT_TARGET,
            state,
            EFFECT_NONE,
            False,
        )

    ops.append(ADDR_PREDICATE_FIRST)
    natives.append(NATIVE_VDP_READ)
    if state.vram_1d00 != ELIGIBLE_PREDICATE_VALUES[0]:
        # 04 is accepted by the first comparison; every other value reaches the
        # second one, which requires 06.
        ops.append(ADDR_PREDICATE_SECOND)
        if state.vram_1d00 != ELIGIBLE_PREDICATE_VALUES[1]:
            ops.extend((ADDR_REJECT_BRANCH, ADDR_REJECT_TARGET))
            return finish(
                STATUS_REJECTED,
                REASON_PREDICATE,
                ADDR_REJECT_TARGET,
                state,
                EFFECT_NONE,
                False,
            )

    ops.append(ADDR_SECONDARY_COMPARE)
    if state.vram_1ce8 != 0x01:
        ops.append(ADDR_SECONDARY_BYPASS)
        return accept()

    ops.append(ADDR_COUNTER_COMPARE)
    if state.vram_10fe >= state.vram_1cf8:  # unsigned; both are bytes
        return accept()

    ops.extend((ADDR_MESSAGE_CALL, ADDR_EXTERNAL_CALL, ADDR_ACK_ENTRY))
    if fallback is None:
        return finish(
            STATUS_UNRESOLVED,
            REASON_ACK_INPUT_MISSING,
            None,
            state,
            EFFECT_UNRESOLVED,
            False,
        )

    if fallback.scratch_83a1 != 0:
        # >96EA clears this byte and returns immediately. Ordinary RTN clears
        # condition, so the original caller's >66C5 branch rejects.
        result = FallbackResult(
            scratch_83a1_before=fallback.scratch_83a1,
            scratch_83a1_after=0,
            vdp_timer=fallback.vdp_timer,
            vram_1d01=fallback.vram_1d01,
            scan_key_code=None,
            waited_for_input=False,
            initial_backdrop=None,
            final_backdrop=None,
            return_condition="reset",
        )
        ops.extend((ADDR_ACK_RETURN, ADDR_ACK_RETURN_BRANCH, ADDR_REJECT_TARGET))
        return finish(
            STATUS_REJECTED,
            REASON_ACKNOWLEDGED,
            ADDR_REJECT_TARGET,
            state,
            EFFECT_MESSAGE_ACK,
            False,
            result,
        )

    initial_backdrop = 0x0C if fallback.vdp_timer & 0x40 else 0x02
    partial = FallbackResult(
        scratch_83a1_before=0,
        scratch_83a1_after=0,
        vdp_timer=fallback.vdp_timer,
        vram_1d01=fallback.vram_1d01,
        scan_key_code=fallback.scan_key_code,
        waited_for_input=True,
        initial_backdrop=initial_backdrop,
        final_backdrop=None,
        return_condition=None,
    )
    ops.extend((ADDR_ACK_TIMER, ADDR_ACK_SCAN))
    if fallback.scan_key_code is None:
        return finish(
            STATUS_UNRESOLVED,
            REASON_ACK_INPUT_MISSING,
            ADDR_ACK_SCAN,
            state,
            EFFECT_UNRESOLVED,
            False,
            partial,
        )

    final_backdrop = 0x06 if fallback.vram_1d01 == 0x02 else 0x03
    if fallback.vram_1d01 != 0x02:
        ops.append(ADDR_ACK_FINAL_BACKDROP_ALT)
    ops.extend((ADDR_ACK_RETURN, ADDR_ACK_RETURN_BRANCH, ADDR_REJECT_TARGET))
    natives.append(NATIVE_SCAN_WRITE)
    result = dataclasses.replace(
        partial, final_backdrop=final_backdrop, return_condition="reset"
    )
    return finish(
        STATUS_REJECTED,
        REASON_ACKNOWLEDGED,
        ADDR_REJECT_TARGET,
        state.with_changes(key_code=fallback.scan_key_code),
        EFFECT_MESSAGE_ACK,
        False,
        result,
    )


def apply_delayed(immediate: ImmediateResult) -> DelayedResult:
    """Apply the later copy at ``>A798``: ``vram_10fa = vram_1cf8``."""

    if not isinstance(immediate, ImmediateResult):
        raise ModelError("delayed: expected an ImmediateResult")
    if not immediate.delayed_copy_pending:
        raise DelayedPhaseError(
            "delayed: the copy at >A798 requires an accepted pending result; "
            f"this result is {immediate.status} ({immediate.reason})"
        )
    before = immediate.state
    return DelayedResult(
        status=DELAYED_STATUS_COMPLETE,
        reason=DELAYED_REASON_COMPLETE,
        address=ADDR_DELAYED_COPY,
        before=before,
        state=before.with_changes(vram_10fa=before.vram_1cf8),
        gpl_operations=(ADDR_DELAYED_COPY,),
        native_operations=(NATIVE_VDP_READ, NATIVE_VDP_WRITE),
    )


def predict(
    state: StairsState,
    fallback: Optional[FallbackInput] = None,
    with_delayed: bool = False,
) -> Tuple[ImmediateResult, Optional[DelayedResult]]:
    """Immediate phase, plus the delayed copy when it is requested and pending."""

    immediate = predict_immediate(state, fallback)
    delayed = (
        apply_delayed(immediate)
        if with_delayed and immediate.delayed_copy_pending
        else None
    )
    return immediate, delayed


# --- comparison against independently observed in-boundary outputs ---------

EXPECTED_KEYS = ("status", "reason", "branch", "visible_effect", "immediate", "delayed")


@dataclass(frozen=True)
class FieldCheck:
    field: str
    expected: str
    actual: str

    @property
    def passed(self) -> bool:
        return self.expected == self.actual


@dataclass(frozen=True)
class CaseResult:
    index: int
    name: str
    checks: Tuple[FieldCheck, ...]

    @property
    def passed(self) -> bool:
        return all(check.passed for check in self.checks)


def load_comparison_cases(path: Path) -> List[Mapping[str, Any]]:
    """Read a versioned owner-local case list; the file itself stays untracked."""

    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise ComparisonFileError(f"{path.name}: cannot read: {exc}") from None
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ComparisonFileError(f"{path.name}: invalid JSON: {exc}") from None
    if not isinstance(data, dict):
        raise ComparisonFileError(f"{path.name}: expected a JSON object")
    if data.get("format") != COMPARISON_FORMAT:
        raise ComparisonFileError(
            f"{path.name}: field 'format' must be {COMPARISON_FORMAT!r}, "
            f"found {data.get('format')!r}"
        )
    if data.get("version") != COMPARISON_VERSION:
        raise ComparisonFileError(
            f"{path.name}: field 'version' must be {COMPARISON_VERSION}, "
            f"found {data.get('version')!r}"
        )
    cases = data.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ComparisonFileError(f"{path.name}: field 'cases' must be a non-empty list")
    for index, case in enumerate(cases, start=1):
        if not isinstance(case, dict):
            raise ComparisonFileError(f"{path.name}: case {index} is not an object")
    return cases


def _expected_state_checks(
    prefix: str,
    expected: Any,
    actual: Optional[StairsState],
    where: str,
    unavailable: str,
) -> List[FieldCheck]:
    if not isinstance(expected, dict) or not expected:
        raise ComparisonFileError(
            f"{where}: '{prefix}' must be a non-empty object of byte fields"
        )
    unknown = sorted(set(expected) - set(BYTE_FIELDS))
    if unknown:
        raise ComparisonFileError(f"{where}: '{prefix}' unknown field(s): {', '.join(unknown)}")
    checks: List[FieldCheck] = []
    for field in BYTE_FIELDS:
        if field not in expected:
            continue
        want = hex_byte(parse_byte(expected[field], f"{where}.{prefix}.{field}"))
        got = hex_byte(getattr(actual, field)) if actual is not None else unavailable
        checks.append(FieldCheck(f"{prefix}.{field}", want, got))
    return checks


def evaluate_case(case: Mapping[str, Any], index: int) -> CaseResult:
    where = f"case {index}"
    name = case.get("name", f"case-{index}")
    if not isinstance(name, str) or not name.strip():
        raise ComparisonFileError(f"{where}: field 'name' must be a non-empty string")
    where = f"case {index} ({name})"

    unknown_case_keys = sorted(set(case) - {"name", "input", "expected"})
    if unknown_case_keys:
        raise ComparisonFileError(
            f"{where}: unknown field(s): {', '.join(unknown_case_keys)}"
        )

    raw_input = case.get("input")
    if not isinstance(raw_input, dict):
        raise ComparisonFileError(f"{where}: field 'input' must be an object")
    fields = dict(raw_input)
    raw_fallback = fields.pop("fallback", None)
    fallback = (
        FallbackInput.from_mapping(raw_fallback, f"{where}.input.fallback")
        if raw_fallback is not None
        else None
    )
    state = StairsState.from_mapping(fields, f"{where}.input")

    expected = case.get("expected")
    if not isinstance(expected, dict) or not expected:
        raise ComparisonFileError(f"{where}: field 'expected' must be a non-empty object")
    unknown_expected = sorted(set(expected) - set(EXPECTED_KEYS))
    if unknown_expected:
        raise ComparisonFileError(
            f"{where}: 'expected' unknown field(s): {', '.join(unknown_expected)}"
        )

    immediate, delayed = predict(state, fallback, with_delayed=True)

    checks: List[FieldCheck] = []
    if "status" in expected:
        checks.append(FieldCheck("status", str(expected["status"]), immediate.status))
    if "reason" in expected:
        checks.append(FieldCheck("reason", str(expected["reason"]), immediate.reason))
    if "branch" in expected:
        want = expected["branch"]
        want_text = (
            "none"
            if want is None
            else hex_address(parse_address(want, f"{where}.expected.branch"))
        )
        got_text = (
            "none" if immediate.branch is None else hex_address(immediate.branch)
        )
        checks.append(FieldCheck("branch", want_text, got_text))
    if "visible_effect" in expected:
        checks.append(
            FieldCheck(
                "visible_effect",
                str(expected["visible_effect"]),
                immediate.visible_effect,
            )
        )
    if "immediate" in expected:
        checks.extend(
            _expected_state_checks(
                "immediate", expected["immediate"], immediate.state, where, "(none)"
            )
        )
    if "delayed" in expected:
        checks.extend(
            _expected_state_checks(
                "delayed",
                expected["delayed"],
                delayed.state if delayed is not None else None,
                where,
                "(no pending copy)",
            )
        )
    if not checks:
        raise ComparisonFileError(f"{where}: 'expected' declares no comparable field")
    return CaseResult(index=index, name=name, checks=tuple(checks))


def parse_address(value: Any, field: str) -> int:
    if isinstance(value, bool):
        raise ModelError(f"{field}: expected an address, found a boolean")
    if isinstance(value, int):
        number = value
    elif isinstance(value, str):
        text = value.strip()
        try:
            if text.startswith(">"):
                number = int(text[1:], 16)
            elif text[:2].lower() == "0x":
                number = int(text[2:], 16)
            else:
                number = int(text, 16)
        except ValueError:
            raise ModelError(f"{field}: {value!r} is not an address") from None
    else:
        raise ModelError(f"{field}: expected an address, found {type(value).__name__}")
    if not 0 <= number <= 0xFFFF:
        raise ModelError(f"{field}: address out of range >0000..>FFFF: {number}")
    return number


def render_comparison(results: Sequence[CaseResult]) -> str:
    lines: List[str] = []
    passed_fields = 0
    failed_fields = 0
    failed_cases = 0
    for result in results:
        lines.append(f"case {result.index} {result.name}")
        for check in result.checks:
            if check.passed:
                passed_fields += 1
                lines.append(f"  PASS {check.field} = {check.actual}")
            else:
                failed_fields += 1
                lines.append(
                    f"  FAIL {check.field}: observed {check.expected}, "
                    f"model {check.actual}"
                )
        if not result.passed:
            failed_cases += 1
    total_fields = passed_fields + failed_fields
    lines.append(
        f"summary: {len(results)} case(s), {total_fields} field(s), "
        f"{passed_fields} pass, {failed_fields} fail, "
        f"{failed_cases} failing case(s)"
    )
    return "\n".join(lines) + "\n"


def compare_file(path: Path) -> Tuple[str, int]:
    cases = load_comparison_cases(path)
    results = [evaluate_case(case, index) for index, case in enumerate(cases, start=1)]
    text = render_comparison(results)
    status = 0 if all(result.passed for result in results) else FAILURE_EXIT
    return text, status


# --- command line ----------------------------------------------------------


def _predict_text(
    immediate: ImmediateResult, delayed: Optional[DelayedResult]
) -> str:
    lines = [
        f"model: {MODEL_ID} v{MODEL_VERSION} (bounded reconstruction, not an emulator)",
        f"status: {immediate.status} ({immediate.reason})",
        "branch: "
        + (hex_address(immediate.branch) if immediate.branch is not None else "none"),
        f"visible effect: {immediate.visible_effect}",
        f"delayed copy pending: {'yes' if immediate.delayed_copy_pending else 'no'}",
    ]
    if immediate.fallback is not None:
        fallback = immediate.fallback.as_dict()
        lines.append(
            "fallback: entered >96EA, return condition "
            + str(fallback["return_condition"])
        )
        lines.append(
            "fallback backdrop: "
            f"{fallback['initial_backdrop']} -> {fallback['final_backdrop']}"
        )
    lines.append("")
    lines.append("state")
    before = immediate.before.as_dict()
    after = immediate.state.as_dict()
    final = delayed.state.as_dict() if delayed is not None else after
    width = max(len(field) for field in BYTE_FIELDS)
    for field in BYTE_FIELDS:
        mark = "" if before[field] == final[field] else "  <- changed"
        middle = (
            f" -> {after[field]} -> {final[field]}"
            if delayed is not None
            else f" -> {after[field]}"
        )
        lines.append(f"  {field:<{width}} {before[field]}{middle}{mark}")
    if delayed is not None:
        lines.append(f"  delayed phase {hex_address(delayed.address)}: {delayed.reason}")
    elif immediate.delayed_copy_pending:
        lines.append("  delayed phase >A798: pending, not applied (pass --delayed)")

    lines.append("")
    lines.append("gpl operations")
    for item in _operations_as_list(immediate.gpl_operations):
        lines.append(f"  {item['address']}  {item['role']}")
    if delayed is not None:
        for item in _operations_as_list(delayed.gpl_operations):
            lines.append(f"  {item['address']}  {item['role']}")

    lines.append("")
    lines.append("native interpreter boundary (metadata; not emulated)")
    seen = []
    for item in list(immediate.native_operations) + list(
        delayed.native_operations if delayed is not None else ()
    ):
        if item not in seen:
            seen.append(item)
    for item in seen:
        lines.append(f"  {item['address']} {item['opcode']}  {item['role']}")
    return "\n".join(lines) + "\n"


def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Bounded behavioral model of the Tunnels of Doom stairs-descend "
            "decision, its immediate mutations, and the delayed copy at >A798."
        )
    )
    sub = parser.add_subparsers(dest="command", required=True)

    predict_cmd = sub.add_parser("predict", help="predict one state transition")
    for field in BYTE_FIELDS:
        predict_cmd.add_argument(
            "--" + field.replace("_", "-"),
            dest=field,
            default="0x00",
            help=f"{field} byte (10, '>0A', or '0x0A'); default >00",
        )
    predict_cmd.add_argument("--scratch-83a1", default=None)
    predict_cmd.add_argument("--vdp-timer", default=None)
    predict_cmd.add_argument("--vram-1d01", default=None)
    predict_cmd.add_argument("--scan-key-code", default=None)
    predict_cmd.add_argument(
        "--delayed",
        action="store_true",
        help="also apply the delayed copy at >A798 when one is pending",
    )
    predict_cmd.add_argument(
        "--json", action="store_true", help="emit compact JSON instead of text"
    )

    compare_cmd = sub.add_parser(
        "compare", help="compare predictions with an owner-local observed case list"
    )
    compare_cmd.add_argument("file", help="versioned JSON case list (owner-local)")
    return parser.parse_args(argv)


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(argv)
    try:
        if args.command == "predict":
            state = StairsState.from_mapping(
                {field: getattr(args, field) for field in BYTE_FIELDS}, "input"
            )
            fallback_values = (args.scratch_83a1, args.vdp_timer, args.vram_1d01)
            fallback = None
            if any(value is not None for value in fallback_values) or args.scan_key_code is not None:
                fallback = FallbackInput(
                    scratch_83a1=args.scratch_83a1 or 0,
                    vdp_timer=args.vdp_timer or 0,
                    vram_1d01=args.vram_1d01 or 0,
                    scan_key_code=args.scan_key_code,
                )
            immediate, delayed = predict(state, fallback, with_delayed=args.delayed)
            if args.json:
                sys.stdout.write(
                    json.dumps(
                        prediction_dict(immediate, delayed),
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                    + "\n"
                )
            else:
                sys.stdout.write(_predict_text(immediate, delayed))
            return 0

        text, status = compare_file(Path(args.file))
        sys.stdout.write(text)
        return status
    except ComparisonFileError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return USAGE_EXIT
    except ModelError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return USAGE_EXIT


if __name__ == "__main__":
    sys.exit(main())
