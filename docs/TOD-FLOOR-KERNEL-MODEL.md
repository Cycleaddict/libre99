# Tunnels of Doom bounded candidate-buffer model

## Disposition

**PASS.** `tools/tod_floor_kernel_model.py` is a neutral standard-library model
of the per-floor candidate-buffer kernel at GPL `>8246..>84F1` and the local
helpers it needs through `>86B8`. It reproduced both accepted authoring
payloads exactly, then predicted the one predeclared held-out kernel-entry seed
`>A5C3` before authentic execution. The authentic run matched all 538 output
bytes, the next seed, bounded completion, and every filtered write count.

Model version 2 additionally implements the source-confirmed direct retry at
`>84EB` and refuses to guess the separate post-pass `>857B` predicate. Its
bounded frozen comparison is reported in `docs/TOD-FLOOR-KERNEL-RETRY.md`.

This is not a GPL interpreter or a complete dungeon generator. It excludes the
outer loop at `>62F4`, later consumers and stocking/content code, rendering,
ISR/KSCAN seed stirring, and semantic names for the byte values.

## Boundary and inputs

The modeled output is the 538-byte payload at VRAM `>34B8..>36D1`. Static
decode proved that the kernel also reads immediate neighbors: 32 bytes before
and 32 bytes after that payload cover every one-cell neighbor access and the
reset loop's extra read at `>36D2`. The CLI therefore takes:

- the 16-bit seed present when GPL `>8246` begins;
- the initial 538-byte payload;
- a 602-byte context dump for `>3498..>36F1`, from which only the two 32-byte
  halos are retained; and
- the neutral bytes read at `>1CEC`, `>1CE6`, `>1CE7`, `>1CF8`, `>1CF5`, and
  scratchpad `>833F`, exposed as count/position/control fields.

The accepted floor uses counts `>14`, `>02`, `>02`, position index/limit
`>01/>01`, and control `>09`. The output is the final 538 bytes, the next seed,
and a compact summary of completion, RAND calls, placement attempts, and writes
grouped by GPL operation.

The seed labels `>0000` and `>1234` in the reconnaissance report describe the
state before the menu trigger. Code outside this boundary stirs those words
before `>8246`; the accepted traces prove kernel-entry seeds `>5207` and
`>82F0`, respectively. The model starts at those actual boundary values and
does not pretend to reconstruct the excluded stirring.

## Recovered neutral algorithm

The model directly transcribes the byte-verified GSL control flow:

1. `>8605..>864B` scans offsets `>0000..>021A`. On the accepted first position,
   values at least `>60` are normalized at `>8611` and stored as `>6B` at
   `>863D`; below-`>60` bytes remain unchanged.
2. `>852F..>857A` chooses coordinates in the inclusive ranges `03..1C` and
   `02..12`. Helper `>86B8` maps them to `first * >20 + second - >43`.
   A store at `>8553` occurs only when the selected byte and its four immediate
   neighbors are all `>6B`. The accepted state places one `>68`, twenty `>67`,
   and two `>6A` values.
3. RAND is the console-ROM operation proved at native `>0182/>3860`,
   `>018A/>C802`, and `>019C/>D802`: `seed' = seed * >6FE5 + >7AB9`, and the
   result is the byte-swapped seed modulo `limit + 1`. The shared bounded-range
   helper rejects results outside its requested interval.
4. The two propagation orientations alternate twice. `>8339` stores while the
   index advances by `>0020`; `>83D8` stores while it advances by `>0001`.
   Preserving this alternating order is necessary for exact output.
5. `>8403..>84DF` performs the decoded adjacent-cell adjustments, including
   conditional stores such as `>845F` and `>847E`. The accepted and held-out
   cases return through the post-pass check without a retry.

These are address/value facts. Descriptions such as room, corridor, carving,
or topology remain hypotheses and do not appear in executable behavior.

## Commands

Prediction accepts raw bytes, a prior prediction JSON, or a probe `vpeek`
transcript for the payload. Context accepts 602 raw bytes or a `vpeek` transcript:

```bash
python3 tools/tod_floor_kernel_model.py predict \
  --seed '>A5C3' --payload initial-payload.bin --context initial-context.log \
  --post-pass-result no-retry --output prediction.json --compact

python3 tools/tod_floor_kernel_model.py compare \
  --seed '>A5C3' --payload initial-payload.bin --context initial-context.log \
  --post-pass-result no-retry --actual authentic.log --actual-seed '>AF57'
```

The full prediction JSON contains `payload_hex`; `--compact` omits those bytes
from standard output while retaining the SHA-256 identity and summary.

## Authoring reproduction

No accepted generation was rerun. The existing owner-local evidence produced:

| Reconnaissance lineage | Kernel-entry seed | Final payload SHA-256 | Next seed | Writes | Result |
|---|---:|---|---:|---:|---|
| pre-trigger `>0000` | `>5207` | `c9df1a5878275ef6ebe83e1de5da58ef4765b0dcfcd44b9da7f3bb19e95d67aa` | `>558A` | 1,184 | exact |
| pre-trigger `>1234` | `>82F0` | `07b4583365cda870913caee9eb3689052a75a077f0f18bca041bbe5d0ee65603` | `>23A7` | 1,142 | exact |

The first case matched `474 + 474` reset writes, 23 `>8553` stores, 53
`>8339` stores, and 160 `>83D8` stores. The second matched the same reset and
placement counts, 65/104 stride stores, and one store each at `>845F` and
`>847E`.

## Frozen held-out comparison

The full `>A5C3` prediction was written owner-locally before authentic
execution. Its JSON SHA-256 was
`c031d3659f201273c2dd85973d5caa23cfbcbadf72918230403fc9281470736e`.
It predicted:

- payload SHA-256
  `58f44c03d67b41d19cd966d632ac1020da86f6e7c2430fdf5014d4c97b73e913`;
- next seed `>AF57`;
- 92 RAND calls and 31 placement attempts; and
- 1,178 writes: 474 at `>8611`, 474 at `>863D`, 23 at `>8553`, 47 at
  `>8339`, 159 at `>83D8`, and one at `>845F`.

For the single authentic held-out run, execution was advanced to the first
observed fetch of `>8246`; coverage proved `>852F` had not executed, and the
kernel-entry seed was then set to `>A5C3`. The continuation reached `>84F1`
and the GENERAL STORE. Its 538 bytes and next seed matched exactly. The
filtered trace independently counted the same 1,178 writes, 92 RAND calls, and
31 placement attempts. More precisely, the 92nd seed update wrote `>AF57` at
cycle 128001462; the next seed update occurred only much later, after the
modeled kernel. The seed at the end of that longer continuation is therefore
not a model output. No second held-out seed or correction run was used.

## Evidence labels and retained uncertainty

- **Source-confirmed:** instruction decoding, loop bounds, neighbor offsets,
  coordinate conversion, alternating phase order, stores, and RAND arithmetic.
- **Observed:** authoring and held-out payloads/seeds, native writers, operation
  counts, bounded completion, and visible GENERAL STORE continuation.
- **Inferred:** the payload participates in per-floor construction; precise
  graphical/game meanings are not assigned.
- **Unresolved:** semantic meanings of `>60..>6B`; the payload-level meaning of
  the separate post-pass `>857B` retry condition outside the accepted cases;
  later consumers, stocking/content, and inter-floor contracts.

The direct `>84EB` retry contract is now recovered and independently reported;
none of the original three accepted cases reached it, and their exact results
remain unchanged. A next package should be chosen economically; this result
does not authorize a broad seed campaign or complete-generator rewrite.
