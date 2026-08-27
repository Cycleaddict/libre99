# Observatory campaigns

A campaign is a batch of independent `libre99probe` experiments from one
checkpoint. Each experiment starts a **fresh probe process**, restores the
same save state, applies that run's setup commands, records a filtered VDP
write trace for a bounded number of frames, and writes a compact result row.

This is experiment automation around the existing probe. It is not a new
emulator, trace format, or analysis database.

Python 3 (standard library only) is required for the runner. It is observatory
tooling, not part of the Cargo workspace.

## Command

```bash
python3 tools/observatory_campaign.py \
  --manifest CAMPAIGN.json \
  --probe target/release/libre99probe \
  --output OUTPUT_DIR
```

`--only RUN_ID` reproduces one experiment from the same manifest, still in a
fresh process, into the given output directory.

The overall command exits `0` when every selected experiment succeeds. A
failed experiment writes a failure row, keeps its stdout/stderr, and lets the
remaining experiments run; the campaign then exits `1`. Manifest or path
errors exit `2`. There is no retry.

## Manifest v1

One JSON object:

| Field | Meaning |
|---|---|
| `version` | Must be `1`. |
| `name` | Campaign name, copied into every result row. |
| `checkpoint` | Save-state path. SHA-256 is recorded; bytes are never copied. |
| `media` | Optional object of label → owner-local path (cartridge, ROM, …). Identities are hashed; bytes are never copied. |
| `capture_frames` | Frames to run after setup, with `vtrace` armed. |
| `vtrace.start` / `vtrace.end` | Inclusive 14-bit VRAM filter, hex (`0000`…`3FFF`) or integer. |
| `coverage` | Optional. If `true`, issue `cover on` for the capture. Default `false`. |
| `observations` | Optional probe commands run after the capture: `state`, `regs`, `peek`, `vpeek` (with their usual arguments). |
| `experiments` | Non-empty list. Duplicate `id` values are refused. |

Each experiment:

| Field | Meaning |
|---|---|
| `id` | Unique run id and directory name. |
| `setup` | Probe commands after `load`, before `vtrace on`. May be empty. |
| `group` | Optional repeat/group label for deterministic-repeat checks. |

Setup must use existing probe commands (`hold joy1-fire`, `frames 2`, …).
`press` itself runs frames; prefer `hold` when the capture should see a
joystick state.

Example:

```json
{
  "version": 1,
  "name": "example",
  "checkpoint": "/path/to/frozen.state",
  "media": {"cartridge": "/path/to/game.ctg"},
  "capture_frames": 1,
  "vtrace": {"start": "0000", "end": "3FFF"},
  "observations": ["state"],
  "experiments": [
    {"id": "baseline-01", "setup": [], "group": "baseline"},
    {"id": "joy1-fire-01", "setup": ["hold joy1-fire"], "group": "joy1-fire"}
  ]
}
```

## What each experiment does

1. `load` the checkpoint.
2. Run that experiment's setup commands.
3. `vtrace on START END`.
4. `cover on` only when requested.
5. `frames N`.
6. Configured observation commands.
7. `vtrace save` the filtered provenance log.
8. `quit`.

`load` clears probe recording, so tracing is armed **after** restore.

## Output

`OUTPUT_DIR` receives:

- `summary.jsonl` — exactly one JSON object per experiment, in run order.
- `campaign-summary.json` — counts, per-group deterministic-repeat results,
  and which writer PCs, changed VRAM addresses, and causal digests vary
  between groups.
- `<run_id>/` — `script.probe`, `stdout.txt`, `stderr.txt`, `vtrace.txt`.

Each JSONL row includes at least: campaign, run id, status, emulator Git
commit, checkpoint SHA-256, optional media SHA-256 identities, exact setup
commands, capture frame count, VDP filter, probe exit status, duration, total
VDP writes, state-changing VDP writes (`old != new`), distinct writer PCs,
distinct changed VRAM addresses, observation results, relative evidence
paths, and a compact causal-result digest.

The digest is SHA-256 of a canonical JSON payload of those meaningful result
fields (including the changing write tuples). It exists only to compare
repeats and name an artifact. It is not a signature, journal, or provenance
chain.

Owner-local commercial media, checkpoints, and campaign output stay outside
Git. The runner records hashes and host paths; it does not copy those bytes
into the repository or the output tree.

## Tests

```bash
python3 -m unittest tools/test_observatory_campaign.py
```

The unit suite drives a tiny fake probe. It does not boot the emulator a
hundred times.
