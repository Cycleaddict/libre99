#!/usr/bin/env python3
# Bounded Tunnels of Doom payload decoder. Licensed under LICENSE.md
# (Modified MIT with Commons Clause). Standard library only.

"""Decode the neutral 17x26 ToD candidate payload using its later consumer."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, Dict, Iterable, List, Mapping, Optional, Sequence

PAYLOAD_BASE = 0x34B8
PAYLOAD_LENGTH = 538
ROWS = 17
COLS = 26
STRIDE = 32
REGION_SIZE = 4
FAILURE_EXIT = 1

_DUMP_LINE = re.compile(r"^>([0-9A-Fa-f]{4})\s+((?:[0-9A-Fa-f]{2}(?:\s+|$))+)")
_CONNECTION_MASKS = {
    0x60: 0x0A,
    0x61: 0x05,
    0x62: 0x0B,
    0x63: 0x0E,
    0x64: 0x0D,
    0x65: 0x07,
    0x66: 0x0F,
    0x67: 0x0F,
    0x68: 0x0F,
    0x69: 0x0F,
    0x6A: 0x0F,
    0x6B: 0x00,
}
_DIRECTIONS = (("north", 0x01), ("east", 0x02), ("south", 0x04), ("west", 0x08))
_EVIDENCE_OPERATIONS = {0xA3BD: ">A3B5", 0xA5F8: ">A5E1"}
_REQUIRED_OPERATIONS = frozenset(_EVIDENCE_OPERATIONS.values())


class DecoderError(ValueError):
    """Invalid input or evidence outside the bounded decoder contract."""


def hex_byte(value: int) -> str:
    return f">{value:02X}"


def hex_word(value: int) -> str:
    return f">{value:04X}"


def load_payload(path: Path) -> bytes:
    raw = path.read_bytes()
    if len(raw) == PAYLOAD_LENGTH:
        return raw
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise DecoderError(
            f"{path}: expected {PAYLOAD_LENGTH} raw bytes or probe/JSON text"
        ) from None
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        parsed = None
    if isinstance(parsed, dict) and isinstance(parsed.get("payload_hex"), str):
        try:
            payload = bytes.fromhex(parsed["payload_hex"])
        except ValueError:
            raise DecoderError(f"{path}: invalid payload_hex") from None
        if len(payload) != PAYLOAD_LENGTH:
            raise DecoderError(
                f"{path}: payload_hex has {len(payload)} bytes, "
                f"expected {PAYLOAD_LENGTH}"
            )
        return payload
    found: Dict[int, int] = {}
    for line in text.splitlines():
        match = _DUMP_LINE.match(line)
        if not match:
            continue
        address = int(match.group(1), 16)
        for offset, byte_text in enumerate(match.group(2).split()):
            found[address + offset] = int(byte_text, 16)
    missing = [
        address
        for address in range(PAYLOAD_BASE, PAYLOAD_BASE + PAYLOAD_LENGTH)
        if address not in found
    ]
    if missing:
        raise DecoderError(
            f"{path}: incomplete >34B8..>36D1 dump; "
            f"first missing {hex_word(missing[0])}"
        )
    return bytes(found[address] for address in range(PAYLOAD_BASE, PAYLOAD_BASE + PAYLOAD_LENGTH))


def _coordinate(row: int, column: int) -> int:
    if not 0 <= row < ROWS or not 0 <= column < COLS:
        raise DecoderError(f"coordinate ({row},{column}) outside 17x26 active grid")
    return row * STRIDE + column


def consumer_class(normalized: int) -> int:
    """Return the exact neutral code produced by GPL >A3B5 for supported values."""
    if normalized not in _CONNECTION_MASKS:
        raise DecoderError(f"unsupported normalized payload value {hex_byte(normalized)}")
    if normalized == 0x6B:
        return 0
    if normalized < 0x67:
        return 1
    return (2, 5, 4, 3)[normalized - 0x67]


def decode_value(raw: int) -> Dict[str, Any]:
    if not 0 <= raw <= 0xFF:
        raise DecoderError(f"raw byte out of range: {raw}")
    normalized = raw & 0xEF
    if normalized not in _CONNECTION_MASKS:
        raise DecoderError(f"unsupported payload value {hex_byte(raw)} after >10 normalization")
    mask = _CONNECTION_MASKS[normalized]
    return {
        "raw": hex_byte(raw),
        "normalized": hex_byte(normalized),
        "ignored_bit_10": bool(raw & 0x10),
        "consumer_class": consumer_class(normalized),
        "connection_mask": hex_byte(mask),
        "connections": {name: bool(mask & bit) for name, bit in _DIRECTIONS},
        "classification": "source-confirmed",
        "consumer_operations": [">A3B5", ">A5E1"],
        "unresolved": "game-level meaning of the raw value and neutral class",
    }


def decode_cell(payload: bytes, row: int, column: int) -> Dict[str, Any]:
    offset = _coordinate(row, column)
    result = {
        "row": row,
        "column": column,
        "offset": offset,
        "vram_address": hex_word(PAYLOAD_BASE + offset),
    }
    result.update(decode_value(payload[offset]))
    return result


def decode_region(payload: bytes, row: int, column: int, height: int, width: int) -> Dict[str, Any]:
    if height <= 0 or width <= 0:
        raise DecoderError("region width and height must be positive")
    _coordinate(row, column)
    _coordinate(row + height - 1, column + width - 1)
    return {
        "row": row,
        "column": column,
        "height": height,
        "width": width,
        "cells": [
            decode_cell(payload, r, c)
            for r in range(row, row + height)
            for c in range(column, column + width)
        ],
    }


def select_heldout_region(payload: bytes) -> Dict[str, Any]:
    best: Optional[tuple[int, int, int]] = None
    for row in range(ROWS - REGION_SIZE + 1):
        for column in range(COLS - REGION_SIZE + 1):
            values = [
                payload[(row + dr) * STRIDE + column + dc]
                for dr in range(REGION_SIZE)
                for dc in range(REGION_SIZE)
            ]
            alphabet_distinct = len({value for value in values if 0x60 <= value <= 0x6B})
            if alphabet_distinct >= 3:
                selected_by = (
                    "first-row-major-4x4-with-at-least-three-distinct-"
                    "60-through-6B-values"
                )
                region = decode_region(payload, row, column, REGION_SIZE, REGION_SIZE)
                break
            distinct = len(set(values))
            if best is None or distinct > best[0]:
                best = (distinct, row, column)
        else:
            continue
        break
    else:
        assert best is not None
        _, row, column = best
        selected_by = "first-row-major-4x4-with-greatest-distinct-value-count"
        region = decode_region(payload, row, column, REGION_SIZE, REGION_SIZE)
    return {
        "format": "libre99-observatory/tod-payload-region-prediction",
        "format_version": 1,
        "payload_sha256": hashlib.sha256(payload).hexdigest(),
        "selection_rule": selected_by,
        "region": region,
    }


def _parse_record(line: str) -> Dict[str, str]:
    return {part.split("=", 1)[0]: part.split("=", 1)[1] for part in line.split() if "=" in part}


def _tuple_key(cell: Mapping[str, Any]) -> str:
    return f"{cell['raw']}/class-{cell['consumer_class']}/{cell['connection_mask']}"


def compare_prediction(
    prediction: Mapping[str, Any], evidence_lines: Iterable[str]
) -> Dict[str, Any]:
    region = prediction.get("region")
    if not isinstance(region, dict) or not isinstance(region.get("cells"), list):
        raise DecoderError("prediction has no decoded region cells")
    observed: Dict[int, Dict[str, Any]] = {}
    for line in evidence_lines:
        record = _parse_record(line)
        if record.get("space") != "vram" or record.get("access") != "read":
            continue
        try:
            grom = int(record.get("grom", "").removeprefix(">"), 16)
            address = int(record.get("addr", "").removeprefix(">"), 16)
            value = int(record.get("byte", "").removeprefix(">"), 16)
        except ValueError:
            continue
        operation = _EVIDENCE_OPERATIONS.get(grom)
        if operation is None:
            continue
        item = observed.setdefault(address, {"values": set(), "operations": set(), "reads": 0})
        item["values"].add(value)
        item["operations"].add(operation)
        item["reads"] += 1

    comparisons: List[Dict[str, Any]] = []
    tuple_coverage: Dict[str, List[str]] = {}
    contradictions: List[Dict[str, Any]] = []
    for cell in region["cells"]:
        try:
            address = int(cell["vram_address"].removeprefix(">"), 16)
            raw = int(cell["raw"].removeprefix(">"), 16)
        except (AttributeError, KeyError, ValueError):
            raise DecoderError("prediction contains a malformed cell address or raw byte") from None
        decoded = decode_value(raw)
        if (
            cell.get("consumer_class") != decoded["consumer_class"]
            or cell.get("connection_mask") != decoded["connection_mask"]
        ):
            raise DecoderError(
                f"prediction cell {cell.get('vram_address', '?')} has a class/mask "
                "inconsistent with its raw byte"
            )
        item = observed.get(address, {"values": set(), "operations": set(), "reads": 0})
        values = sorted(item["values"])
        operations = sorted(item["operations"])
        if not operations:
            status = "notObserved"
        elif values != [raw]:
            status = "contradiction"
        elif set(operations) == _REQUIRED_OPERATIONS:
            status = "observed"
        else:
            status = "partialObservation"
        comparison = {
            "row": cell["row"],
            "column": cell["column"],
            "vram_address": cell["vram_address"],
            "raw": cell["raw"],
            "predicted_consumer_class": cell["consumer_class"],
            "predicted_connection_mask": cell["connection_mask"],
            "observed_raw_values": [hex_byte(value) for value in values],
            "observed_consumer_operations": operations,
            "observed_reads": item["reads"],
            "status": status,
        }
        comparisons.append(comparison)
        if status == "observed":
            tuple_coverage.setdefault(_tuple_key(cell), []).append(cell["vram_address"])
        elif status == "contradiction":
            contradictions.append(comparison)

    required_tuples = sorted({_tuple_key(cell) for cell in region["cells"]})
    distinct_coverage = [
        {
            "decoded_tuple": key,
            "observed": key in tuple_coverage,
            "observed_coordinates": tuple_coverage.get(key, []),
        }
        for key in required_tuples
    ]
    missing = [item for item in distinct_coverage if not item["observed"]]
    passed = not contradictions and not missing
    return {
        "format": "libre99-observatory/tod-payload-region-comparison",
        "format_version": 2,
        "status": "PASS" if passed else "FAIL",
        "acceptance": "distinct-raw-class-mask-coverage-with-no-observed-contradiction",
        "cells": comparisons,
        "distinct_coverage": distinct_coverage,
        "not_observed": [item for item in comparisons if item["status"] == "notObserved"],
        "partial_observations": [
            item for item in comparisons if item["status"] == "partialObservation"
        ],
        "contradictions": contradictions,
        "missing_distinct_coverage": missing,
    }


def _print(value: Mapping[str, Any], as_json: bool) -> None:
    if as_json:
        print(json.dumps(value, sort_keys=True, separators=(",", ":")))
        return
    if "status" in value and "distinct_coverage" in value:
        print(
            f"{value['status']} distinct={len(value['distinct_coverage'])} "
            f"missing={len(value['missing_distinct_coverage'])} "
            f"contradictions={len(value['contradictions'])} "
            f"notObserved={len(value['not_observed'])}"
        )
        for cell in value["cells"]:
            print(
                f"r{cell['row']:02d} c{cell['column']:02d} "
                f"{cell['vram_address']} raw={cell['raw']} "
                f"class={cell['predicted_consumer_class']} "
                f"mask={cell['predicted_connection_mask']} "
                f"status={cell['status']}"
            )
        return
    if "cells" in value:
        for cell in value["cells"]:
            connections = cell.get("connections")
            connected = "?" if connections is None else "".join(
                name[0].upper() for name, present in connections.items() if present
            ) or "-"
            print(
                f"r{cell['row']:02d} c{cell['column']:02d} {cell['vram_address']} "
                f"raw={cell['raw']} class={cell['consumer_class']} "
                f"mask={cell['connection_mask']} connections={connected}"
            )
    else:
        print(json.dumps(value, indent=2, sort_keys=True))


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("decode", "cell", "region"):
        command = sub.add_parser(name)
        command.add_argument("--payload", required=True, type=Path)
        command.add_argument("--json", action="store_true")
    sub.choices["cell"].add_argument("--row", required=True, type=int)
    sub.choices["cell"].add_argument("--column", required=True, type=int)
    sub.choices["region"].add_argument("--row", required=True, type=int)
    sub.choices["region"].add_argument("--column", required=True, type=int)
    sub.choices["region"].add_argument("--height", required=True, type=int)
    sub.choices["region"].add_argument("--width", required=True, type=int)
    select = sub.add_parser("select-heldout")
    select.add_argument("--payload", required=True, type=Path)
    select.add_argument("--output", required=True, type=Path)
    select.add_argument("--json", action="store_true")
    compare = sub.add_parser("compare")
    compare.add_argument("--prediction", required=True, type=Path)
    compare.add_argument("--evidence", required=True, type=Path)
    compare.add_argument("--output", type=Path)
    compare.add_argument("--json", action="store_true")
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "compare":
            prediction = json.loads(args.prediction.read_text(encoding="utf-8"))
            result = compare_prediction(
                prediction, args.evidence.read_text(encoding="utf-8").splitlines()
            )
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
            _print(result, args.json)
            return 0 if result["status"] == "PASS" else FAILURE_EXIT
        payload = load_payload(args.payload)
        if args.command == "decode":
            result = decode_region(payload, 0, 0, ROWS, COLS)
        elif args.command == "cell":
            result = decode_cell(payload, args.row, args.column)
        elif args.command == "region":
            result = decode_region(payload, args.row, args.column, args.height, args.width)
        else:
            result = select_heldout_region(payload)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        _print(result, args.json)
        return 0
    except (DecoderError, OSError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return FAILURE_EXIT


if __name__ == "__main__":
    raise SystemExit(main())
