#!/usr/bin/env python3
# Observatory campaign runner. Licensed under LICENSE.md (Modified MIT with
# Commons Clause). Standard library only; wraps libre99probe, it does not
# replace it.

"""Repeatable libre99probe campaigns: one fresh process per experiment."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Iterable, List, Mapping, Optional, Sequence, Tuple

VTRACE_RE = re.compile(
    r"pc=>(?P<pc>[0-9A-Fa-f]+)"
    r".*?"
    r"vram=>(?P<vram>[0-9A-Fa-f]+)"
    r".*?"
    r"old=>(?P<old>[0-9A-Fa-f]+)"
    r".*?"
    r"byte=>(?P<byte>[0-9A-Fa-f]+)"
)

ALLOWED_OBSERVATIONS = ("state", "regs", "peek", "vpeek")
RUN_TIMEOUT_SEC = 120
USAGE_EXIT = 2
FAILURE_EXIT = 1


class ManifestError(ValueError):
    """Invalid campaign manifest."""


@dataclass(frozen=True)
class Experiment:
    run_id: str
    setup: Tuple[str, ...]
    group: Optional[str]


@dataclass(frozen=True)
class Manifest:
    version: int
    name: str
    checkpoint: Path
    media: Tuple[Tuple[str, Path], ...]
    capture_frames: int
    vtrace_start: int
    vtrace_end: int
    coverage: bool
    observations: Tuple[str, ...]
    experiments: Tuple[Experiment, ...]


@dataclass(frozen=True)
class VdpWrite:
    pc: str
    vram: str
    old: str
    new: str

    @property
    def changed(self) -> bool:
        return self.old != self.new


def hex4(value: int) -> str:
    return f"{value:04X}"


def hex_byte(value: str) -> str:
    return f"{int(value, 16):02X}"


def parse_hex_addr(value: Any, name: str, maximum: int = 0x3FFF) -> int:
    if isinstance(value, bool) or value is None:
        raise ManifestError(f"{name} must be a hex address or integer")
    if isinstance(value, int):
        n = value
    else:
        text = str(value).strip().lower()
        if text.startswith(">"):
            text = text[1:]
        elif text.startswith("0x"):
            text = text[2:]
        try:
            n = int(text, 16)
        except ValueError as exc:
            raise ManifestError(f"{name} is not a hex address: {value!r}") from exc
    if n < 0 or n > maximum:
        raise ManifestError(f"{name} must be in 0..{maximum:04X}, got {n:#06x}")
    return n


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def causal_digest(payload: Mapping[str, Any]) -> str:
    return hashlib.sha256(canonical_json(payload).encode("utf-8")).hexdigest()


def _require_str(value: Any, name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ManifestError(f"{name} must be a non-empty string")
    return value.strip()


def _as_object(value: Any, name: str) -> Dict[str, Any]:
    if not isinstance(value, dict):
        raise ManifestError(f"{name} must be a JSON object")
    return value


def _command_list(value: Any, name: str) -> Tuple[str, ...]:
    if value is None:
        return ()
    if not isinstance(value, list):
        raise ManifestError(f"{name} must be a list of command strings")
    commands = []
    for i, item in enumerate(value):
        if not isinstance(item, str) or not item.strip():
            raise ManifestError(f"{name}[{i}] must be a non-empty command string")
        if "\n" in item or "\r" in item:
            raise ManifestError(f"{name}[{i}] must be a single line")
        commands.append(item.strip())
    return tuple(commands)


def _validate_observation(command: str) -> str:
    verb = command.split(None, 1)[0].lower()
    if verb not in ALLOWED_OBSERVATIONS:
        raise ManifestError(
            f"observation command {command!r} must start with "
            + ", ".join(ALLOWED_OBSERVATIONS)
        )
    return command


def _safe_run_id(run_id: str) -> str:
    if run_id in {".", ".."} or "/" in run_id or "\\" in run_id:
        raise ManifestError(f"run id {run_id!r} is not a safe directory name")
    return run_id


def parse_manifest_data(data: Any) -> Manifest:
    obj = _as_object(data, "manifest")
    version = obj.get("version")
    if version != 1:
        raise ManifestError("manifest version must be 1")

    name = _require_str(obj.get("name") or obj.get("campaign"), "name")
    checkpoint = Path(_require_str(obj.get("checkpoint"), "checkpoint"))

    media_items: List[Tuple[str, Path]] = []
    media = obj.get("media")
    if media is None:
        media = {}
    if not isinstance(media, dict):
        raise ManifestError("media must be an object of label -> path")
    for label, path in media.items():
        if not isinstance(label, str) or not label.strip():
            raise ManifestError("media labels must be non-empty strings")
        media_items.append((label.strip(), Path(_require_str(path, f"media.{label}"))))

    capture_frames = obj.get("capture_frames")
    if not isinstance(capture_frames, int) or isinstance(capture_frames, bool) or capture_frames < 1:
        raise ManifestError("capture_frames must be a positive integer")

    vtrace = obj.get("vtrace")
    if vtrace is None:
        vtrace = obj.get("vdp_filter")
    vtrace_obj = _as_object(vtrace, "vtrace")
    start = parse_hex_addr(vtrace_obj.get("start", vtrace_obj.get("lo")), "vtrace.start")
    end = parse_hex_addr(vtrace_obj.get("end", vtrace_obj.get("hi")), "vtrace.end")
    if start > end:
        raise ManifestError("vtrace.start must be <= vtrace.end")

    coverage = obj.get("coverage", False)
    if not isinstance(coverage, bool):
        raise ManifestError("coverage must be a boolean")

    observations = tuple(
        _validate_observation(cmd) for cmd in _command_list(obj.get("observations"), "observations")
    )

    raw_experiments = obj.get("experiments")
    if not isinstance(raw_experiments, list) or not raw_experiments:
        raise ManifestError("experiments must be a non-empty list")

    experiments: List[Experiment] = []
    seen = set()
    for i, item in enumerate(raw_experiments):
        rec = _as_object(item, f"experiments[{i}]")
        run_id = _safe_run_id(_require_str(rec.get("id") or rec.get("run_id"), f"experiments[{i}].id"))
        if run_id in seen:
            raise ManifestError(f"duplicate run id: {run_id}")
        seen.add(run_id)
        group = rec.get("group") or rec.get("repeat")
        if group is not None:
            group = _require_str(group, f"experiments[{i}].group")
        experiments.append(
            Experiment(
                run_id=run_id,
                setup=_command_list(rec.get("setup"), f"experiments[{i}].setup"),
                group=group,
            )
        )

    return Manifest(
        version=1,
        name=name,
        checkpoint=checkpoint,
        media=tuple(media_items),
        capture_frames=capture_frames,
        vtrace_start=start,
        vtrace_end=end,
        coverage=coverage,
        observations=observations,
        experiments=tuple(experiments),
    )


def load_manifest(path: Path) -> Manifest:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise ManifestError(f"manifest is not valid JSON: {exc}") from exc
    except OSError as exc:
        raise ManifestError(f"could not read manifest {path}: {exc}") from exc
    return parse_manifest_data(data)


def parse_vtrace(text: str) -> List[VdpWrite]:
    writes: List[VdpWrite] = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        match = VTRACE_RE.search(line)
        if not match:
            continue
        writes.append(
            VdpWrite(
                pc=f"{int(match.group('pc'), 16):04X}",
                vram=f"{int(match.group('vram'), 16):04X}",
                old=hex_byte(match.group("old")),
                new=hex_byte(match.group("byte")),
            )
        )
    return writes


def vtrace_stats(writes: Sequence[VdpWrite]) -> Dict[str, Any]:
    changing = [w for w in writes if w.changed]
    writer_pcs = sorted({w.pc for w in writes})
    changed_vram = sorted({w.vram for w in changing})
    changing_writes = [
        {"pc": w.pc, "vram": w.vram, "old": w.old, "new": w.new} for w in changing
    ]
    return {
        "total_vdp_writes": len(writes),
        "state_changing_vdp_writes": len(changing),
        "writer_pcs": writer_pcs,
        "changed_vram_addresses": changed_vram,
        "changing_writes": changing_writes,
    }


def parse_transcript(stdout: str) -> List[Dict[str, str]]:
    blocks: List[Dict[str, str]] = []
    current: Optional[Dict[str, List[str]]] = None
    for line in stdout.splitlines():
        if line.startswith("> "):
            if current is not None:
                blocks.append(
                    {"cmd": current["cmd"], "output": "\n".join(current["output"]).rstrip()}
                )
            current = {"cmd": line[2:], "output": []}
        elif current is not None:
            current["output"].append(line)
    if current is not None:
        blocks.append({"cmd": current["cmd"], "output": "\n".join(current["output"]).rstrip()})
    return blocks


def observation_results(stdout: str, observations: Sequence[str]) -> Dict[str, str]:
    blocks = parse_transcript(stdout)
    remaining = list(blocks)
    results: Dict[str, str] = {}
    for command in observations:
        found = None
        for i, block in enumerate(remaining):
            if block["cmd"] == command:
                found = i
                break
        if found is None:
            results[command] = ""
            continue
        results[command] = remaining[found]["output"]
        remaining = remaining[found + 1 :]
    return results


def causal_payload(stats: Mapping[str, Any], observations: Mapping[str, str]) -> Dict[str, Any]:
    return {
        "changed_vram_addresses": list(stats.get("changed_vram_addresses") or []),
        "changing_writes": list(stats.get("changing_writes") or []),
        "observations": observations,
        "state_changing_vdp_writes": stats.get("state_changing_vdp_writes", 0),
        "total_vdp_writes": stats.get("total_vdp_writes", 0),
        "writer_pcs": list(stats.get("writer_pcs") or []),
    }


def emulator_git_commit(start_dirs: Iterable[Path]) -> str:
    tried = []
    for start in start_dirs:
        path = start.resolve()
        if path.is_file():
            path = path.parent
        for candidate in (path, *path.parents):
            if candidate in tried:
                continue
            tried.append(candidate)
            try:
                result = subprocess.run(
                    ["git", "rev-parse", "HEAD"],
                    cwd=str(candidate),
                    capture_output=True,
                    text=True,
                    check=False,
                )
            except OSError:
                continue
            if result.returncode == 0:
                commit = result.stdout.strip()
                if commit:
                    return commit
    return "unknown"


def build_probe_script(
    manifest: Manifest,
    experiment: Experiment,
    checkpoint: Path,
    vtrace_path: Path,
) -> str:
    lines = [
        f"# campaign={manifest.name} run_id={experiment.run_id}",
        f"load {checkpoint}",
    ]
    lines.extend(experiment.setup)
    lines.append(f"vtrace on {hex4(manifest.vtrace_start)} {hex4(manifest.vtrace_end)}")
    if manifest.coverage:
        lines.append("cover on")
    lines.append(f"frames {manifest.capture_frames}")
    lines.extend(manifest.observations)
    lines.append(f"vtrace save {vtrace_path}")
    lines.append("quit")
    return "\n".join(lines) + "\n"


def relative_to(path: Path, root: Path) -> str:
    try:
        return path.resolve().relative_to(root.resolve()).as_posix()
    except ValueError:
        return str(path)


def empty_stats() -> Dict[str, Any]:
    return {
        "total_vdp_writes": None,
        "state_changing_vdp_writes": None,
        "writer_pcs": [],
        "changed_vram_addresses": [],
        "changing_writes": [],
    }


def run_probe(probe: Path, script_path: Path, timeout: int) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(probe), "--script", str(script_path)],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def run_experiment(
    *,
    manifest: Manifest,
    experiment: Experiment,
    probe: Path,
    output_dir: Path,
    checkpoint: Path,
    checkpoint_sha256: str,
    media_sha256: Mapping[str, str],
    emulator_commit: str,
    timeout: int = RUN_TIMEOUT_SEC,
) -> Dict[str, Any]:
    run_dir = output_dir / experiment.run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    script_path = run_dir / "script.probe"
    stdout_path = run_dir / "stdout.txt"
    stderr_path = run_dir / "stderr.txt"
    vtrace_path = run_dir / "vtrace.txt"

    script = build_probe_script(manifest, experiment, checkpoint, vtrace_path.resolve())
    script_path.write_text(script, encoding="utf-8")

    evidence = {
        "script": relative_to(script_path, output_dir),
        "stdout": relative_to(stdout_path, output_dir),
        "stderr": relative_to(stderr_path, output_dir),
        "vtrace": relative_to(vtrace_path, output_dir),
    }

    row: Dict[str, Any] = {
        "campaign": manifest.name,
        "run_id": experiment.run_id,
        "group": experiment.group,
        "status": "failed",
        "emulator_git_commit": emulator_commit,
        "checkpoint_sha256": checkpoint_sha256,
        "media_sha256": dict(media_sha256),
        "setup": list(experiment.setup),
        "capture_frames": manifest.capture_frames,
        "vdp_filter": {
            "start": hex4(manifest.vtrace_start),
            "end": hex4(manifest.vtrace_end),
        },
        "coverage": manifest.coverage,
        "probe_exit_status": None,
        "duration_sec": None,
        "observations": {},
        "evidence": evidence,
        "causal_digest": None,
    }
    row.update(empty_stats())

    started = time.monotonic()
    try:
        proc = run_probe(probe, script_path, timeout)
    except subprocess.TimeoutExpired as exc:
        duration = round(time.monotonic() - started, 6)
        stdout = exc.stdout or b""
        stderr = exc.stderr or b""
        if isinstance(stdout, str):
            stdout_text = stdout
        else:
            stdout_text = stdout.decode("utf-8", errors="replace")
        if isinstance(stderr, str):
            stderr_text = stderr
        else:
            stderr_text = stderr.decode("utf-8", errors="replace")
        stdout_path.write_text(stdout_text, encoding="utf-8")
        stderr_path.write_text(stderr_text, encoding="utf-8")
        row["duration_sec"] = duration
        row["error"] = f"probe timed out after {timeout}s"
        return row
    except OSError as exc:
        row["duration_sec"] = round(time.monotonic() - started, 6)
        row["error"] = f"could not launch probe: {exc}"
        stderr_path.write_text(row["error"] + "\n", encoding="utf-8")
        stdout_path.write_text("", encoding="utf-8")
        return row

    duration = round(time.monotonic() - started, 6)
    stdout_text = proc.stdout.decode("utf-8", errors="replace")
    stderr_text = proc.stderr.decode("utf-8", errors="replace")
    stdout_path.write_text(stdout_text, encoding="utf-8")
    stderr_path.write_text(stderr_text, encoding="utf-8")
    row["probe_exit_status"] = proc.returncode
    row["duration_sec"] = duration

    if proc.returncode != 0:
        row["error"] = f"probe exited {proc.returncode}"
        return row

    if not vtrace_path.is_file():
        row["error"] = "probe did not write a vtrace file"
        return row

    writes = parse_vtrace(vtrace_path.read_text(encoding="utf-8", errors="replace"))
    stats = vtrace_stats(writes)
    observations = observation_results(stdout_text, manifest.observations)
    payload = causal_payload(stats, observations)
    row.update(stats)
    row["observations"] = observations
    row["causal_digest"] = causal_digest(payload)
    row["status"] = "ok"
    return row


def select_experiments(manifest: Manifest, only: Optional[str]) -> Tuple[Experiment, ...]:
    if only is None:
        return manifest.experiments
    chosen = [exp for exp in manifest.experiments if exp.run_id == only]
    if not chosen:
        known = ", ".join(exp.run_id for exp in manifest.experiments)
        raise ManifestError(f"unknown run id {only!r}; known: {known}")
    return tuple(chosen)


def analyze_campaign(rows: Sequence[Mapping[str, Any]]) -> Dict[str, Any]:
    grouped: Dict[str, List[Mapping[str, Any]]] = {}
    for row in rows:
        key = row.get("group") or row["run_id"]
        grouped.setdefault(str(key), []).append(row)

    groups: Dict[str, Any] = {}
    for name in sorted(grouped):
        runs = grouped[name]
        ok_rows = [r for r in runs if r.get("status") == "ok"]
        failed_rows = [r for r in runs if r.get("status") != "ok"]
        digest_map: Dict[str, List[str]] = {}
        for row in ok_rows:
            digest = row.get("causal_digest")
            if digest:
                digest_map.setdefault(digest, []).append(row["run_id"])
        setups = sorted({tuple(r.get("setup") or []) for r in runs})
        writer_pcs = sorted({pc for r in ok_rows for pc in r.get("writer_pcs") or []})
        changed_vram = sorted(
            {addr for r in ok_rows for addr in r.get("changed_vram_addresses") or []}
        )
        groups[name] = {
            "runs": len(runs),
            "ok": len(ok_rows),
            "failed": len(failed_rows),
            "setups": [list(s) for s in setups],
            "identical_setup": len(setups) == 1,
            "causal_digests": digest_map,
            "deterministic_repeat": len(failed_rows) == 0
            and len(ok_rows) > 0
            and len(digest_map) == 1,
            "writer_pcs": writer_pcs,
            "changed_vram_addresses": changed_vram,
        }

    ok_groups = {name: g for name, g in groups.items() if g["ok"]}
    pc_sets = {name: set(g["writer_pcs"]) for name, g in ok_groups.items()}
    vram_sets = {name: set(g["changed_vram_addresses"]) for name, g in ok_groups.items()}
    digest_sets = {name: set(g["causal_digests"]) for name, g in ok_groups.items()}

    def vary(sets: Mapping[str, set]) -> Dict[str, Any]:
        if not sets:
            return {"union": [], "common": [], "varying": [], "by_group": {}}
        union = sorted(set().union(*sets.values())) if sets else []
        common = sorted(set.intersection(*sets.values())) if sets else []
        varying = [item for item in union if item not in common]
        return {
            "union": union,
            "common": common,
            "varying": varying,
            "by_group": {name: sorted(values) for name, values in sorted(sets.items())},
        }

    return {
        "groups": groups,
        "between_groups": {
            "writer_pcs": vary(pc_sets),
            "changed_vram_addresses": vary(vram_sets),
            "causal_digests": vary(digest_sets),
        },
    }


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run_campaign(
    *,
    manifest_path: Path,
    probe: Path,
    output_dir: Path,
    only: Optional[str] = None,
    timeout: int = RUN_TIMEOUT_SEC,
) -> int:
    manifest = load_manifest(manifest_path)
    experiments = select_experiments(manifest, only)

    if not probe.is_file():
        raise ManifestError(f"probe binary not found: {probe}")
    checkpoint = manifest.checkpoint.expanduser()
    if not checkpoint.is_file():
        raise ManifestError(f"checkpoint not found: {checkpoint}")
    media_paths = []
    for label, path in manifest.media:
        resolved = path.expanduser()
        if not resolved.is_file():
            raise ManifestError(f"media.{label} not found: {resolved}")
        media_paths.append((label, resolved))

    checkpoint = checkpoint.resolve()
    probe = probe.resolve()
    output_dir = output_dir.expanduser().resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    checkpoint_sha256 = sha256_file(checkpoint)
    media_sha256 = {label: sha256_file(path) for label, path in media_paths}
    emulator_commit = emulator_git_commit((Path.cwd(), probe))

    jsonl_path = output_dir / "summary.jsonl"
    rows: List[Dict[str, Any]] = []
    any_failed = False

    with jsonl_path.open("w", encoding="utf-8") as jsonl:
        for experiment in experiments:
            row = run_experiment(
                manifest=manifest,
                experiment=experiment,
                probe=probe,
                output_dir=output_dir,
                checkpoint=checkpoint,
                checkpoint_sha256=checkpoint_sha256,
                media_sha256=media_sha256,
                emulator_commit=emulator_commit,
                timeout=timeout,
            )
            jsonl.write(canonical_json(row) + "\n")
            jsonl.flush()
            rows.append(row)
            if row.get("status") != "ok":
                any_failed = True

    analysis = analyze_campaign(rows)
    summary = {
        "campaign": manifest.name,
        "manifest": str(manifest_path.resolve()),
        "emulator_git_commit": emulator_commit,
        "checkpoint": str(checkpoint),
        "checkpoint_sha256": checkpoint_sha256,
        "media_sha256": media_sha256,
        "capture_frames": manifest.capture_frames,
        "vdp_filter": {
            "start": hex4(manifest.vtrace_start),
            "end": hex4(manifest.vtrace_end),
        },
        "experiments": len(rows),
        "ok": sum(1 for row in rows if row.get("status") == "ok"),
        "failed": sum(1 for row in rows if row.get("status") != "ok"),
        "only": only,
        "groups": analysis["groups"],
        "between_groups": analysis["between_groups"],
    }
    write_json(output_dir / "campaign-summary.json", summary)
    return FAILURE_EXIT if any_failed else 0


def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a libre99probe observatory campaign from a JSON manifest."
    )
    parser.add_argument("--manifest", required=True, help="campaign manifest JSON")
    parser.add_argument("--probe", required=True, help="path to libre99probe")
    parser.add_argument("--output", required=True, help="output directory")
    parser.add_argument("--only", metavar="RUN_ID", help="reproduce one experiment")
    return parser.parse_args(argv)


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(argv)
    try:
        return run_campaign(
            manifest_path=Path(args.manifest),
            probe=Path(args.probe),
            output_dir=Path(args.output),
            only=args.only,
        )
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return USAGE_EXIT


if __name__ == "__main__":
    sys.exit(main())
