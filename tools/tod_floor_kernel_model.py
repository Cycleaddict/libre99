#!/usr/bin/env python3
# Bounded Tunnels of Doom per-floor candidate-buffer model. Licensed under
# LICENSE.md (Modified MIT with Commons Clause). Standard library only.

"""Neutral executable model of GPL >8246..>84F1 and required local helpers.

This is a direct behavioral transcription of the byte-verified GPL recovery,
not a GPL interpreter and not a complete dungeon generator.  It models only
the 538-byte candidate payload at VRAM >34B8..>36D1 and the console GPL RAND
state consumed while that bounded routine executes.

The executable deliberately uses address/value terminology.  Meanings for the
candidate byte alphabet are not established by the accepted evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Mapping, Optional, Sequence, Tuple

MODEL_ID = "tod-floor-candidate-kernel"
MODEL_VERSION = 2
PAYLOAD_BASE = 0x34B8
PAYLOAD_LENGTH = 538
CONTEXT_BEFORE = 32
CONTEXT_AFTER = 32
CONTEXT_BASE = PAYLOAD_BASE - CONTEXT_BEFORE
CONTEXT_LENGTH = CONTEXT_BEFORE + PAYLOAD_LENGTH + CONTEXT_AFTER

FAILURE_EXIT = 1
USAGE_EXIT = 2


class ModelError(ValueError):
    """Invalid input or a bounded-model invariant failure."""


def hex_byte(value: int) -> str:
    return f">{value:02X}"


def hex_word(value: int) -> str:
    return f">{value:04X}"


def parse_word(value: Any, field_name: str) -> int:
    if isinstance(value, bool):
        raise ModelError(f"{field_name}: expected a 16-bit word, found boolean")
    if isinstance(value, int):
        result = value
    elif isinstance(value, str):
        text = value.strip()
        try:
            if text.startswith(">"):
                result = int(text[1:], 16)
            elif text.lower().startswith("0x"):
                result = int(text[2:], 16)
            else:
                result = int(text, 10)
        except ValueError:
            raise ModelError(f"{field_name}: invalid word {value!r}") from None
    else:
        raise ModelError(f"{field_name}: expected a 16-bit word")
    if not 0 <= result <= 0xFFFF:
        raise ModelError(f"{field_name}: word out of range: {result}")
    return result


def parse_byte(value: Any, field_name: str) -> int:
    result = parse_word(value, field_name)
    if result > 0xFF:
        raise ModelError(f"{field_name}: byte out of range: {result}")
    return result


@dataclass(frozen=True)
class KernelInputs:
    """State proved to affect the bounded candidate-payload result."""

    seed: int
    payload: bytes
    context_before: bytes
    context_after: bytes
    value_67_count: int = 0x14
    value_6a_count: int = 0x02
    value_69_count: int = 0x02
    position_index: int = 0x01
    position_limit: int = 0x01
    control_833f: int = 0x09
    post_pass_retry: Optional[bool] = None
    max_direct_retries: int = 1

    def __post_init__(self) -> None:
        object.__setattr__(self, "seed", parse_word(self.seed, "seed"))
        if not isinstance(self.payload, bytes) or len(self.payload) != PAYLOAD_LENGTH:
            length = len(self.payload) if isinstance(self.payload, (bytes, bytearray)) else "non-bytes"
            raise ModelError(f"payload: expected exactly {PAYLOAD_LENGTH} bytes, found {length}")
        for name, expected in (("context_before", CONTEXT_BEFORE), ("context_after", CONTEXT_AFTER)):
            value = getattr(self, name)
            if not isinstance(value, bytes) or len(value) != expected:
                length = len(value) if isinstance(value, (bytes, bytearray)) else "non-bytes"
                raise ModelError(f"{name}: expected exactly {expected} bytes, found {length}")
        for name in (
            "value_67_count",
            "value_6a_count",
            "value_69_count",
            "position_index",
            "position_limit",
            "control_833f",
            "max_direct_retries",
        ):
            object.__setattr__(self, name, parse_byte(getattr(self, name), name))
        if self.post_pass_retry is not None and not isinstance(self.post_pass_retry, bool):
            raise ModelError("post_pass_retry: expected true, false, or unresolved")


@dataclass
class PhaseSummary:
    writes: Dict[str, int] = field(default_factory=dict)
    random_calls: int = 0
    placement_attempts: int = 0
    restarts: int = 0
    completed: bool = True
    termination: str = "return->84F1"
    direct_retry_triggers: List[Dict[str, Any]] = field(default_factory=list)
    control_flow: List[str] = field(default_factory=list)

    def write(self, address: int) -> None:
        key = hex_word(address)
        self.writes[key] = self.writes.get(key, 0) + 1

    def as_dict(self) -> Dict[str, Any]:
        return {
            "completed": self.completed,
            "termination": self.termination,
            "random_calls": self.random_calls,
            "placement_attempts": self.placement_attempts,
            "restarts": self.restarts,
            "direct_retry_triggers": list(self.direct_retry_triggers),
            "control_flow": list(self.control_flow),
            "writes_by_gpl_operation": dict(sorted(self.writes.items())),
            "candidate_writes": sum(self.writes.values()),
        }


@dataclass(frozen=True)
class KernelResult:
    payload: bytes
    next_seed: int
    summary: Mapping[str, Any]
    inputs: Mapping[str, Any] = field(default_factory=dict)

    def as_dict(self, include_payload: bool = True) -> Dict[str, Any]:
        result: Dict[str, Any] = {
            "format": "libre99-observatory/tod-floor-kernel-prediction",
            "format_version": 1,
            "model": MODEL_ID,
            "model_version": MODEL_VERSION,
            "payload_range": ">34B8..>36D1",
            "payload_length": len(self.payload),
            "payload_sha256": hashlib.sha256(self.payload).hexdigest(),
            "next_seed": hex_word(self.next_seed),
            "inputs": dict(self.inputs),
            "summary": dict(self.summary),
        }
        if include_payload:
            result["payload_hex"] = self.payload.hex().upper()
        return result


class Kernel:
    def __init__(self, inputs: KernelInputs):
        self.data = bytearray(inputs.context_before + inputs.payload + inputs.context_after)
        self.seed = inputs.seed
        self.inputs = inputs
        self.summary = PhaseSummary()

    def _input_identity(self) -> Dict[str, Any]:
        initial = bytes(self.inputs.context_before + self.inputs.payload + self.inputs.context_after)
        return {
            "seed": hex_word(self.inputs.seed),
            "payload_sha256": hashlib.sha256(self.inputs.payload).hexdigest(),
            "context_3498_36f1_sha256": hashlib.sha256(initial).hexdigest(),
            "value_67_count": hex_byte(self.inputs.value_67_count),
            "value_6a_count": hex_byte(self.inputs.value_6a_count),
            "value_69_count": hex_byte(self.inputs.value_69_count),
            "position_index": hex_byte(self.inputs.position_index),
            "position_limit": hex_byte(self.inputs.position_limit),
            "control_833f": hex_byte(self.inputs.control_833f),
            "post_pass_retry": self.inputs.post_pass_retry,
            "max_direct_retries": self.inputs.max_direct_retries,
        }

    def _read(self, index: int) -> int:
        storage = index + CONTEXT_BEFORE
        if not 0 <= storage < len(self.data):
            raise ModelError(f"candidate index outside bounded reset span: {index}")
        return self.data[storage]

    def _write(self, index: int, value: int, operation: int) -> None:
        storage = index + CONTEXT_BEFORE
        if not 0 <= storage < len(self.data):
            raise ModelError(
                f"GPL {hex_word(operation)} wrote outside the declared one-cell context: offset {index}"
            )
        self.data[storage] = value & 0xFF
        if 0 <= index < PAYLOAD_LENGTH:
            self.summary.write(operation)

    @staticmethod
    def _index(first: int, second: int) -> int:
        # Direct result of >A3A6 followed by >86B3: first*32 + second - >43.
        return first * 0x20 + second - 0x43

    def _rand(self, limit: int) -> int:
        self.seed = (self.seed * 0x6FE5 + 0x7AB9) & 0xFFFF
        self.summary.random_calls += 1
        swapped = ((self.seed & 0xFF) << 8) | (self.seed >> 8)
        return swapped % (limit + 1)

    def _rand_between(self, low: int, high: int) -> int:
        if high <= low:
            return high
        if high <= 0x05:
            limit = 0x05
        elif high <= 0x0F:
            limit = 0x0F
        elif high <= 0x1E:
            limit = 0x1E
        elif high <= 0x3C:
            limit = 0x3C
        else:
            limit = 0x7D
        while True:
            value = self._rand(limit)
            if low <= value <= high:
                return value

    def _count_6b_neighbors(self, index: int) -> int:
        return sum(self._read(index + delta) == 0x6B for delta in (-0x20, 0x20, -1, 1))

    def _reset(self, mode: int) -> None:
        for index in range(0x021B):
            value = self._read(index)
            if value < 0x60:
                continue
            value &= 0xEF
            self._write(index, value, 0x8611)
            if mode == 0x02:
                value = 0x6B
            elif value == 0x69:
                if mode == 0x00:
                    value = 0x68
                    self._write(index, value, 0x8628)
                    continue
                value = 0x6B
            elif value == 0x68 and mode == 0x01:
                continue
            else:
                value = 0x6B
            self._write(index, value, 0x863D)

    def _place(self, value: int, count: int) -> None:
        placed = 0
        while placed < count:
            self.summary.placement_attempts += 1
            second = self._rand_between(0x03, 0x1C)
            first = self._rand_between(0x02, 0x12)
            index = self._index(first, second)
            if self._read(index) != 0x6B or self._count_6b_neighbors(index) != 4:
                continue
            self._write(index, value, 0x8553)
            placed += 1

    def _vertical_pass(self, passes: int = 2) -> None:
        b8308 = passes
        steps = 0
        while b8308:
            second = 0x03
            while second <= 0x1C:
                first = 0x02
                b8302 = 0
                b8303 = 0
                while first <= 0x12:
                    steps += 1
                    if steps > 100000:
                        raise ModelError(
                            f"vertical control did not converge at ({first:02X},{second:02X}) "
                            f"tracker=({b8302:02X},{b8303:02X})"
                        )
                    restart_same = False
                    index = self._index(first, second)
                    value = self._read(index)
                    hit = value == 0x60
                    if not hit and 0x67 <= value < 0x6B:
                        hit = True
                        if self._count_6b_neighbors(index) >= 3:
                            b8303 = (b8303 + 1) & 0xFF
                    if hit:
                        if b8303 != 0 and b8302 != 0:
                            b8303 = b8302
                            write_index = self._index(b8302, second)
                            while True:
                                value = self._read(write_index)
                                if value == 0x6B:
                                    value = 0x61
                                elif value == 0x61:
                                    b8302 = 0
                                    b8303 = 0
                                    restart_same = True
                                    break
                                elif value == 0x63:
                                    if b8303 != first:
                                        value = 0x62
                                elif value == 0x60:
                                    if b8303 == b8302:
                                        value = 0x64
                                    elif b8303 == first:
                                        value = 0x63
                                    else:
                                        value = 0x62
                                self._write(write_index, value, 0x8339)
                                write_index += 0x20
                                b8303 = (b8303 + 1) & 0xFF
                                if b8303 > first:
                                    b8302 = 0
                                    b8303 = 0
                                    restart_same = True
                                    break
                        if not restart_same:
                            b8302 = first
                    if restart_same:
                        continue
                    first += 1
                second += 1
            b8308 -= 1

    def _horizontal_pass(self, passes: int = 2) -> None:
        b8308 = passes
        steps = 0
        while b8308:
            first = 0x02
            while first <= 0x12:
                second = 0x03
                b8302 = 0
                b8303 = 0
                while second <= 0x1C:
                    steps += 1
                    if steps > 100000:
                        raise ModelError(
                            f"horizontal control did not converge at ({first:02X},{second:02X}) "
                            f"tracker=({b8302:02X},{b8303:02X})"
                        )
                    restart_same = False
                    index = self._index(first, second)
                    value = self._read(index)
                    hit = value == 0x61
                    if not hit and value >= 0x67 and value != 0x6B:
                        hit = True
                        if self._count_6b_neighbors(index) >= 3:
                            b8303 = (b8303 + 1) & 0xFF
                    if hit:
                        if b8303 != 0 and b8302 != 0:
                            b8303 = b8302
                            write_index = self._index(first, b8302)
                            while True:
                                value = self._read(write_index)
                                if value == 0x6B:
                                    value = 0x60
                                elif value == 0x60:
                                    b8302 = 0
                                    b8303 = 0
                                    restart_same = True
                                    break
                                elif value == 0x65:
                                    if second != b8303:
                                        value = 0x62
                                elif value in (0x66, 0x61):
                                    if b8303 == b8302:
                                        value = 0x66
                                    elif second == b8303:
                                        value = 0x65
                                    else:
                                        value = 0x62
                                self._write(write_index, value, 0x83D8)
                                write_index += 1
                                b8303 = (b8303 + 1) & 0xFF
                                if b8303 > second:
                                    b8302 = 0
                                    b8303 = 0
                                    restart_same = True
                                    break
                        if not restart_same:
                            b8302 = second
                    if restart_same:
                        continue
                    second += 1
                first += 1
            b8308 -= 1

    def _cleanup(self) -> Optional[Dict[str, Any]]:
        for first in range(0x02, 0x13):
            for second in range(0x03, 0x1D):
                index = self._index(first, second)
                original = self._read(index)
                if self.inputs.control_833f == 0x09 and 0x67 <= original <= 0x6A:
                    neighbors = self._count_6b_neighbors(index)
                    if neighbors == 4:
                        if original != 0x67:
                            return {
                                "branch": ">84EB",
                                "payload_address": hex_word(PAYLOAD_BASE + index),
                                "value": hex_byte(original),
                                "neighbor_addresses": [
                                    hex_word(PAYLOAD_BASE + index + delta)
                                    for delta in (-0x20, 0x20, -1, 1)
                                ],
                                "neighbor_value": ">6B",
                            }
                        self._write(index, 0x6B, 0x842C)
                if not 0x68 <= original <= 0x6A:
                    continue
                changes = (
                    (-0x20, 0x60, 0x64, 0x845F),
                    (-0x20, 0x63, 0x62, 0x846F),
                    (0x20, 0x60, 0x63, 0x847E),
                    (0x20, 0x64, 0x62, 0x848E),
                    (-1, 0x61, 0x66, 0x849D),
                    (-1, 0x65, 0x62, 0x84AC),
                    (1, 0x61, 0x65, 0x84BB),
                    (1, 0x66, 0x62, 0x84CA),
                )
                for delta, before, after, operation in changes:
                    neighbor = index + delta
                    if self._read(neighbor) == before:
                        self._write(neighbor, after, operation)
        return None

    def _continuation_8283(self) -> None:
        """>8283 reloads counts and restarts placement without placing >68."""

        self._place(0x67, self.inputs.value_67_count)
        self._place(0x6A, self.inputs.value_6a_count)
        if self.inputs.position_index != self.inputs.position_limit:
            self._place(0x69, self.inputs.value_69_count)

    def run(self) -> KernelResult:
        # The accepted floor has position_index=1.  >8246 calls reset mode 2,
        # places one >68 value, then continues at >8283 for the other values.
        if self.inputs.position_index == 0x01:
            self._reset(0x02)
            self._place(0x68, 0x01)
        else:
            self._reset(0x00)
        self._continuation_8283()
        while True:
            for _ in range(2):
                self._vertical_pass(1)
                self._horizontal_pass(1)
            trigger = self._cleanup()
            if trigger is not None:
                self.summary.direct_retry_triggers.append(trigger)
                self.summary.control_flow.append(">84EB")
                if self.summary.restarts >= self.inputs.max_direct_retries:
                    self.summary.completed = False
                    self.summary.termination = "direct-retry-bound-at->84EB"
                    break
                # CALL >8605 is followed by its inline mode byte >01.  The
                # reset itself consumes no RAND state and returns to >8283.
                self.summary.control_flow.extend([">8605 mode >01", ">8283"])
                self._reset(0x01)
                self.summary.restarts += 1
                self._continuation_8283()
                continue

            if self.inputs.control_833f == 0x09:
                if self.inputs.post_pass_retry is None:
                    raise ModelError(
                        "post-pass checker >857B is unresolved; supply an accepted no-retry result explicitly"
                    )
                if self.inputs.post_pass_retry:
                    raise ModelError(
                        "post-pass checker >857B requested retry; its payload predicate remains unresolved"
                    )
            break
        start = CONTEXT_BEFORE
        return KernelResult(
            bytes(self.data[start : start + PAYLOAD_LENGTH]),
            self.seed,
            self.summary.as_dict(),
            self._input_identity(),
        )


_DUMP_LINE = re.compile(r"^>([0-9A-Fa-f]{4})\s+((?:[0-9A-Fa-f]{2}(?:\s+|$))+)")


def load_payload(path: Path) -> bytes:
    raw = path.read_bytes()
    if len(raw) == PAYLOAD_LENGTH:
        return raw
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise ModelError(f"{path}: expected {PAYLOAD_LENGTH} raw bytes or a probe/JSON text result") from None
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        parsed = None
    if isinstance(parsed, dict) and isinstance(parsed.get("payload_hex"), str):
        try:
            payload = bytes.fromhex(parsed["payload_hex"])
        except ValueError:
            raise ModelError(f"{path}: invalid payload_hex") from None
        if len(payload) != PAYLOAD_LENGTH:
            raise ModelError(f"{path}: payload_hex has {len(payload)} bytes, expected {PAYLOAD_LENGTH}")
        return payload
    found: Dict[int, int] = {}
    for line in text.splitlines():
        match = _DUMP_LINE.match(line)
        if not match:
            continue
        address = int(match.group(1), 16)
        for offset, byte_text in enumerate(match.group(2).split()):
            found[address + offset] = int(byte_text, 16)
    missing = [address for address in range(PAYLOAD_BASE, PAYLOAD_BASE + PAYLOAD_LENGTH) if address not in found]
    if missing:
        raise ModelError(
            f"{path}: no complete probe dump for >34B8..>36D1; first missing {hex_word(missing[0])}"
        )
    return bytes(found[address] for address in range(PAYLOAD_BASE, PAYLOAD_BASE + PAYLOAD_LENGTH))


def load_context(path: Path) -> Tuple[bytes, bytes]:
    raw = path.read_bytes()
    if len(raw) == CONTEXT_LENGTH:
        context = raw
    else:
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            raise ModelError(
                f"{path}: expected {CONTEXT_LENGTH} raw bytes or a probe vpeek transcript"
            ) from None
        found: Dict[int, int] = {}
        for line in text.splitlines():
            match = _DUMP_LINE.match(line)
            if not match:
                continue
            address = int(match.group(1), 16)
            for offset, byte_text in enumerate(match.group(2).split()):
                found[address + offset] = int(byte_text, 16)
        missing = [address for address in range(CONTEXT_BASE, CONTEXT_BASE + CONTEXT_LENGTH) if address not in found]
        if missing:
            raise ModelError(
                f"{path}: no complete probe dump for >3498..>36F1; first missing {hex_word(missing[0])}"
            )
        context = bytes(found[address] for address in range(CONTEXT_BASE, CONTEXT_BASE + CONTEXT_LENGTH))
    return context[:CONTEXT_BEFORE], context[-CONTEXT_AFTER:]


def predict(inputs: KernelInputs) -> KernelResult:
    return Kernel(inputs).run()


def compare(result: KernelResult, actual: bytes, actual_seed: Optional[int]) -> Dict[str, Any]:
    first = next((i for i, (left, right) in enumerate(zip(result.payload, actual)) if left != right), None)
    payload_match = first is None and len(actual) == PAYLOAD_LENGTH
    seed_match = actual_seed is None or result.next_seed == actual_seed
    output: Dict[str, Any] = {
        "format": "libre99-observatory/tod-floor-kernel-comparison",
        "format_version": 1,
        "status": "PASS" if payload_match and seed_match else "FAIL",
        "payload_match": payload_match,
        "predicted_payload_sha256": hashlib.sha256(result.payload).hexdigest(),
        "actual_payload_sha256": hashlib.sha256(actual).hexdigest(),
        "predicted_next_seed": hex_word(result.next_seed),
        "actual_next_seed": hex_word(actual_seed) if actual_seed is not None else None,
        "next_seed_match": seed_match if actual_seed is not None else None,
        "summary": dict(result.summary),
    }
    if first is not None:
        output["first_mismatch"] = {
            "offset": first,
            "vram_address": hex_word(PAYLOAD_BASE + first),
            "predicted": hex_byte(result.payload[first]),
            "actual": hex_byte(actual[first]),
        }
    return output


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("predict", "compare"):
        command = sub.add_parser(name)
        command.add_argument("--seed", required=True, help="kernel-entry seed word")
        command.add_argument("--payload", required=True, type=Path, help="538 raw bytes, prediction JSON, or probe vpeek transcript")
        command.add_argument("--context", required=True, type=Path, help="602-byte >3498..>36F1 raw context or probe transcript")
        command.add_argument("--value-67-count", default=">14")
        command.add_argument("--value-6a-count", default=">02")
        command.add_argument("--value-69-count", default=">02")
        command.add_argument("--position-index", default=">01")
        command.add_argument("--position-limit", default=">01")
        command.add_argument("--control-833f", default=">09")
        command.add_argument(
            "--post-pass-result",
            choices=("unresolved", "no-retry", "retry"),
            default="unresolved",
            help="explicit accepted result of separate checker >857B",
        )
        command.add_argument("--max-direct-retries", default="1")
    sub.choices["predict"].add_argument("--output", type=Path, help="write full prediction JSON here")
    sub.choices["predict"].add_argument("--compact", action="store_true", help="omit payload_hex from stdout")
    sub.choices["compare"].add_argument("--actual", required=True, type=Path, help="authentic raw payload or probe transcript")
    sub.choices["compare"].add_argument("--actual-seed", help="authentic kernel-exit seed word")
    return parser


def _inputs(args: argparse.Namespace) -> KernelInputs:
    before, after = load_context(args.context)
    return KernelInputs(
        seed=parse_word(args.seed, "seed"),
        payload=load_payload(args.payload),
        context_before=before,
        context_after=after,
        value_67_count=parse_byte(args.value_67_count, "value_67_count"),
        value_6a_count=parse_byte(args.value_6a_count, "value_6a_count"),
        value_69_count=parse_byte(args.value_69_count, "value_69_count"),
        position_index=parse_byte(args.position_index, "position_index"),
        position_limit=parse_byte(args.position_limit, "position_limit"),
        control_833f=parse_byte(args.control_833f, "control_833f"),
        post_pass_retry={"unresolved": None, "no-retry": False, "retry": True}[
            args.post_pass_result
        ],
        max_direct_retries=parse_byte(args.max_direct_retries, "max_direct_retries"),
    )


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = _parser()
    args = parser.parse_args(argv)
    try:
        result = predict(_inputs(args))
        if args.command == "predict":
            full = result.as_dict(include_payload=True)
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(full, indent=2) + "\n", encoding="utf-8")
            shown = result.as_dict(include_payload=not args.compact)
            print(json.dumps(shown, indent=2, sort_keys=True))
            return 0
        actual_seed = parse_word(args.actual_seed, "actual_seed") if args.actual_seed else None
        comparison = compare(result, load_payload(args.actual), actual_seed)
        print(json.dumps(comparison, indent=2, sort_keys=True))
        return 0 if comparison["status"] == "PASS" else FAILURE_EXIT
    except (ModelError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return FAILURE_EXIT


if __name__ == "__main__":
    raise SystemExit(main())
