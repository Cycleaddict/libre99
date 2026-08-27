#!/usr/bin/env python3
# Tests for tools/observatory_campaign.py. Licensed under LICENSE.md.

"""Focused tests for the observatory campaign runner."""

from __future__ import annotations

import json
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import observatory_campaign as oc  # noqa: E402


FAKE_PROBE = r"""#!/usr/bin/env python3
import os
import sys
from pathlib import Path

log_path = os.environ.get("FAKE_PROBE_LOG")
script_path = None
args = sys.argv[1:]
i = 0
while i < len(args):
    if args[i] == "--script" and i + 1 < len(args):
        script_path = args[i + 1]
        i += 2
        continue
    i += 1
if not script_path:
    sys.stderr.write("fake-probe: missing --script\n")
    sys.exit(2)

lines = [
    ln.strip()
    for ln in Path(script_path).read_text(encoding="utf-8").splitlines()
    if ln.strip() and not ln.strip().startswith("#")
]
if log_path:
    first = lines[0] if lines else ""
    with open(log_path, "a", encoding="utf-8") as fh:
        fh.write(f"pid={os.getpid()} first={first}\n")

setup = []
seen_load = False
for line in lines:
    if not seen_load:
        if line.startswith("load "):
            seen_load = True
        continue
    if line.startswith("vtrace on"):
        break
    setup.append(line)

def emit(cmd, text=""):
    print(f"> {cmd}")
    if text:
        print(text)

exit_code = 0
for line in lines:
    if line.startswith("load "):
        path = line.split(None, 1)[1]
        if not Path(path).is_file():
            emit(line)
            print(f"error: could not read {path}")
            exit_code = 1
            break
        emit(line, f"state restored from {path} (note: trace/vtrace/cover recording is reset by a load)")
    elif line == "fail" or line.startswith("fail "):
        emit(line)
        print("error: injected failure")
        exit_code = 1
        break
    elif line.startswith("vtrace on"):
        emit(line, "vtrace on for VRAM >0000->3FFF (log restarted)")
    elif line.startswith("cover on"):
        emit(line, "coverage on")
    elif line.startswith("frames "):
        n = line.split()[1]
        emit(line, f"ran {n} frames (session total {n})")
    elif line == "state":
        emit(line, "frame=1 pc=>734C wp=>83E0 st=>C000 grom=>0000 cart=\"PARSEC\"")
    elif line == "regs":
        emit(line, "pc=>734C wp=>83E0 st=>C000 grom=>0000")
    elif line.startswith("peek ") or line.startswith("vpeek "):
        emit(line, ">0000  00 00 00 00")
    elif line.startswith("vtrace save "):
        out = line.split(None, 2)[2]
        setup_key = "\n".join(setup)
        if not setup:
            writes = [
                "cycle=1 frame=0+1 pc=>734C opcode=>D801 r11=>6FEA op=write-data port=>8C00 vram=>0102 old=>08 byte=>10",
                "cycle=2 frame=0+2 pc=>7E76 opcode=>D836 r11=>6466 op=write-data port=>8C00 vram=>1B06 old=>98 byte=>98",
            ]
        elif "joy1-fire" in setup_key:
            writes = [
                "cycle=1 frame=0+1 pc=>734C opcode=>D801 r11=>6FEA op=write-data port=>8C00 vram=>0102 old=>08 byte=>20",
                "cycle=2 frame=0+2 pc=>A000 opcode=>D801 r11=>6FEA op=write-data port=>8C00 vram=>1800 old=>00 byte=>01",
            ]
        else:
            writes = [
                "cycle=1 frame=0+1 pc=>734C opcode=>D801 r11=>6FEA op=write-data port=>8C00 vram=>0102 old=>08 byte=>10",
                "cycle=2 frame=0+2 pc=>7E76 opcode=>D836 r11=>6466 op=write-data port=>8C00 vram=>1B07 old=>0F byte=>03",
            ]
        Path(out).write_text("\n".join(writes) + "\n", encoding="utf-8")
        emit(line, f"wrote {len(writes)} VDP writes to {out}")
    elif line == "quit":
        emit(line)
        break
    else:
        emit(line, "ok")

sys.exit(exit_code)
"""


class CampaignTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.probe = self.root / "fake_probe.py"
        self.probe.write_text(FAKE_PROBE, encoding="utf-8")
        self.probe.chmod(self.probe.stat().st_mode | stat.S_IEXEC)
        self.checkpoint = self.root / "checkpoint.state"
        self.media = self.root / "cart.bin"
        # Distinctive payloads that must never be copied into outputs or the repo.
        self.checkpoint_token = b"UNIQUE-CHECKPOINT-BYTES-9f3a1c\x00\xffMEDIA"
        self.media_token = b"UNIQUE-PARSEC-MEDIA-BYTES-c0ffee\x7f\x00CART"
        self.checkpoint.write_bytes(b"STATE" + self.checkpoint_token + b"END")
        self.media.write_bytes(b"CART" + self.media_token + b"END")
        self.log = self.root / "probe.log"
        os.environ["FAKE_PROBE_LOG"] = str(self.log)

    def tearDown(self):
        os.environ.pop("FAKE_PROBE_LOG", None)
        self.temp.cleanup()

    def manifest_dict(self, experiments, **overrides):
        data = {
            "version": 1,
            "name": "unit-campaign",
            "checkpoint": str(self.checkpoint),
            "media": {"cartridge": str(self.media)},
            "capture_frames": 1,
            "vtrace": {"start": "0000", "end": "3FFF"},
            "observations": ["state"],
            "experiments": experiments,
        }
        data.update(overrides)
        return data

    def write_manifest(self, experiments, **overrides):
        path = self.root / "campaign.json"
        path.write_text(json.dumps(self.manifest_dict(experiments, **overrides)), encoding="utf-8")
        return path

    def run_campaign(self, experiments, only=None, **overrides):
        manifest = self.write_manifest(experiments, **overrides)
        output = self.root / "out"
        code = oc.run_campaign(
            manifest_path=manifest,
            probe=self.probe,
            output_dir=output,
            only=only,
        )
        summary = json.loads((output / "campaign-summary.json").read_text(encoding="utf-8"))
        rows = [
            json.loads(line)
            for line in (output / "summary.jsonl").read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
        return code, output, summary, rows

    def test_manifest_parsing_and_duplicate_run_id_refusal(self):
        parsed = oc.parse_manifest_data(
            self.manifest_dict(
                [
                    {"id": "a", "setup": [], "group": "baseline"},
                    {"id": "b", "setup": ["hold joy1-fire"], "group": "fire"},
                ]
            )
        )
        self.assertEqual(parsed.version, 1)
        self.assertEqual(parsed.name, "unit-campaign")
        self.assertEqual([exp.run_id for exp in parsed.experiments], ["a", "b"])
        self.assertEqual(parsed.experiments[1].setup, ("hold joy1-fire",))

        with self.assertRaises(oc.ManifestError) as ctx:
            oc.parse_manifest_data(
                self.manifest_dict(
                    [
                        {"id": "dup", "setup": []},
                        {"id": "dup", "setup": ["hold joy1-left"]},
                    ]
                )
            )
        self.assertIn("duplicate run id", str(ctx.exception))

        with self.assertRaises(oc.ManifestError):
            oc.parse_manifest_data(self.manifest_dict([{"id": "a", "setup": []}], version=2))

    def test_each_run_gets_a_fresh_probe_process_and_begins_with_load(self):
        code, output, summary, rows = self.run_campaign(
            [
                {"id": "r1", "setup": [], "group": "baseline"},
                {"id": "r2", "setup": ["hold joy1-fire"], "group": "fire"},
            ]
        )
        self.assertEqual(code, 0)
        log = self.log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(log), 2)
        pids = []
        for line, run_id in zip(log, ("r1", "r2")):
            self.assertIn("first=load ", line)
            self.assertTrue(line.startswith("pid="))
            pid = int(line.split()[0].split("=", 1)[1])
            pids.append(pid)
            script = (output / run_id / "script.probe").read_text(encoding="utf-8")
            commands = [
                ln for ln in script.splitlines() if ln.strip() and not ln.strip().startswith("#")
            ]
            self.assertTrue(commands[0].startswith("load "), commands)
        self.assertEqual(len(set(pids)), 2)
        self.assertEqual([row["status"] for row in rows], ["ok", "ok"])
        self.assertEqual(summary["ok"], 2)

    def test_one_failed_run_does_not_prevent_later_runs(self):
        code, output, summary, rows = self.run_campaign(
            [
                {"id": "ok1", "setup": [], "group": "baseline"},
                {"id": "bad", "setup": ["fail"], "group": "fail"},
                {"id": "ok2", "setup": ["hold joy1-left"], "group": "left"},
            ]
        )
        self.assertEqual(code, 1)
        self.assertEqual([row["run_id"] for row in rows], ["ok1", "bad", "ok2"])
        self.assertEqual([row["status"] for row in rows], ["ok", "failed", "ok"])
        self.assertEqual(summary["ok"], 2)
        self.assertEqual(summary["failed"], 1)
        self.assertTrue((output / "bad" / "stdout.txt").read_text(encoding="utf-8"))
        self.assertIn("error: injected failure", (output / "bad" / "stdout.txt").read_text())

    def test_overall_exit_is_nonzero_when_any_run_fails(self):
        code, _, summary, rows = self.run_campaign(
            [
                {"id": "bad", "setup": ["fail"], "group": "fail"},
                {"id": "ok", "setup": [], "group": "baseline"},
            ]
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(summary["failed"], 1)
        self.assertEqual(len(rows), 2)

    def test_only_reruns_exactly_one_named_experiment(self):
        experiments = [
            {"id": "alpha", "setup": [], "group": "baseline"},
            {"id": "beta", "setup": ["hold joy1-fire"], "group": "fire"},
            {"id": "gamma", "setup": ["hold joy1-left"], "group": "left"},
        ]
        code, output, summary, rows = self.run_campaign(experiments, only="beta")
        self.assertEqual(code, 0)
        self.assertEqual([row["run_id"] for row in rows], ["beta"])
        self.assertEqual(summary["only"], "beta")
        self.assertEqual(summary["experiments"], 1)
        self.assertTrue((output / "beta").is_dir())
        self.assertFalse((output / "alpha").exists())
        self.assertFalse((output / "gamma").exists())
        log = self.log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(log), 1)

        with self.assertRaises(oc.ManifestError):
            self.run_campaign(experiments, only="missing")

    def test_equivalent_repeated_results_receive_the_same_causal_digest(self):
        code, _, summary, rows = self.run_campaign(
            [
                {"id": "base-1", "setup": [], "group": "baseline"},
                {"id": "base-2", "setup": [], "group": "baseline"},
            ]
        )
        self.assertEqual(code, 0)
        self.assertEqual(rows[0]["causal_digest"], rows[1]["causal_digest"])
        self.assertTrue(summary["groups"]["baseline"]["deterministic_repeat"])
        payload = oc.causal_payload(
            {
                "total_vdp_writes": 2,
                "state_changing_vdp_writes": 1,
                "writer_pcs": ["734C", "7E76"],
                "changed_vram_addresses": ["0102"],
                "changing_writes": [
                    {"pc": "734C", "vram": "0102", "old": "08", "new": "10"}
                ],
            },
            {"state": "same"},
        )
        self.assertEqual(oc.causal_digest(payload), oc.causal_digest(dict(payload)))

    def test_differing_causal_results_receive_different_digests(self):
        code, _, summary, rows = self.run_campaign(
            [
                {"id": "base", "setup": [], "group": "baseline"},
                {"id": "fire", "setup": ["hold joy1-fire"], "group": "fire"},
            ]
        )
        self.assertEqual(code, 0)
        self.assertNotEqual(rows[0]["causal_digest"], rows[1]["causal_digest"])
        self.assertNotEqual(rows[0]["writer_pcs"], rows[1]["writer_pcs"])
        self.assertIn("A000", rows[1]["writer_pcs"])
        self.assertTrue(summary["between_groups"]["causal_digests"]["varying"])

        left = oc.causal_payload(
            {
                "total_vdp_writes": 1,
                "state_changing_vdp_writes": 1,
                "writer_pcs": ["734C"],
                "changed_vram_addresses": ["0102"],
                "changing_writes": [
                    {"pc": "734C", "vram": "0102", "old": "08", "new": "10"}
                ],
            },
            {},
        )
        right = oc.causal_payload(
            {
                "total_vdp_writes": 1,
                "state_changing_vdp_writes": 1,
                "writer_pcs": ["734C"],
                "changed_vram_addresses": ["0102"],
                "changing_writes": [
                    {"pc": "734C", "vram": "0102", "old": "08", "new": "20"}
                ],
            },
            {},
        )
        self.assertNotEqual(oc.causal_digest(left), oc.causal_digest(right))

    def test_commercial_media_or_checkpoint_bytes_are_never_copied(self):
        repo = TOOLS.parent
        tracked_dirs = (TOOLS, repo / "docs")

        def repo_files():
            files = set()
            for root in tracked_dirs:
                for p in root.rglob("*"):
                    if p.is_file() and "__pycache__" not in p.parts and p.suffix != ".pyc":
                        files.add(p.resolve())
            return files

        before_repo_files = repo_files()

        code, output, _, rows = self.run_campaign(
            [{"id": "r1", "setup": [], "group": "baseline"}]
        )
        self.assertEqual(code, 0)
        checkpoint_sha = rows[0]["checkpoint_sha256"]
        media_sha = rows[0]["media_sha256"]["cartridge"]
        self.assertEqual(checkpoint_sha, oc.sha256_file(self.checkpoint))
        self.assertEqual(media_sha, oc.sha256_file(self.media))

        copied_hashes = []
        for path in output.rglob("*"):
            if not path.is_file():
                continue
            data = path.read_bytes()
            self.assertNotIn(self.checkpoint_token, data, path)
            self.assertNotIn(self.media_token, data, path)
            copied_hashes.append(oc.sha256_file(path))
        self.assertNotIn(checkpoint_sha, copied_hashes)
        self.assertNotIn(media_sha, copied_hashes)

        self.assertEqual(before_repo_files, repo_files())
        self.assertTrue(output.resolve().is_relative_to(self.root.resolve()))
        self.assertFalse(output.resolve().is_relative_to(repo.resolve()))


if __name__ == "__main__":
    unittest.main()
