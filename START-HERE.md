# Libre99 Observatory — start here

## What this folder is

This is a clean working branch of Joel Odom's Libre99 repository, based on the
upstream `gsl` branch at commit
`0ff10f666af815be23a0c3045b9c324083faab29`.

- Local branch: `observatory-mvp`
- Upstream remote: `https://github.com/joelodom/libre99.git`
- Upstream code is preserved; observatory work should remain separable enough
  to discuss or contribute upstream.
- The license is Modified MIT with the Commons Clause. Noncommercial hobby and
  research use is permitted; selling derived software or services is not.

Libre99 already supplies the foundation that the prior effort was trying to
reach: a playable TI-99/4A emulator, TMS9900/GPL toolchains, a byte-verifying
GSL decompiler/compiler, and a headless probe with input, screenshots, memory
inspection, GROM tracing, coverage, and checkpoints.

This branch is fork-first: prove the observatory locally without waiting on an
upstream decision. Offer focused emulator defects, chip regressions, and
generally useful GSL/probe improvements upstream as clean commits. Keep
observatory-specific indexing and analysis on this branch unless accepted
upstream. Do not copy the old runtime into Libre99.

## Why this branch exists

The goal is not merely to emulate the TI. It is to expose enough reproducible
runtime evidence for a human and an AI to explain old software substantially
better than static disassembly or a conventional debugger can.

The first proof is a bounded Parsec investigation. The second is one bounded
Tunnels of Doom subsystem. The project does not promise automatic recovery of
perfect original source.

Read [the MVP](docs/OBSERVATORY-MVP.md) and
[the benchmark](docs/BENCHMARK.md) before proposing architecture. The
[foundation audit](docs/FOUNDATION-AUDIT.md) is mandatory before treating
Libre99 chip behavior as settled.

## Current state

The Parsec causal-write POC is complete
([docs/PARSEC-POC.md](docs/PARSEC-POC.md)). Campaign qualification is
complete ([docs/CAMPAIGNS.md](docs/CAMPAIGNS.md)). Gate 2 (ToD
generalization) passed: one stairs-descend view update is causally
attributed through GPL stream + interpreter VDP writes
([docs/TOD-GENERALIZATION.md](docs/TOD-GENERALIZATION.md)). Fresh-start
usability, the final MVP validation gate, passed
([docs/MVP-VALIDATION.md](docs/MVP-VALIDATION.md)). `vtrace` keeps the
GROM stream position and R9 with each recorded VDP write. The observatory
MVP validation is complete. Further observability work is driven by
actual reconstruction questions, not additional generic validation.
The current technical direction is the source-free reconstruction sequence in
[docs/RECONSTRUCTION-NEXT.md](docs/RECONSTRUCTION-NEXT.md). R1 runtime-informed
GPL recovery closed at `babb8d1`: trace-confirmed entries and conservative
leading-`FETCH` inference recover the demonstrated ToD inline-operand boundary,
retain exact raw bytes, roundtrip all five GROM pages byte-identically, and agree
with a held-out replay. R2 mutable-state provenance closed at `b3336ad`: the
bounded `mtrace` recorder attributes selected mutable state to native and GPL
operations without an unbounded machine-wide trace. Across the positive and
held-out-negative stairs checkpoints, it identifies VRAM `>1D00` as the
distinguishing predicate cell, observes `>1CF8` increment on the accepted path,
and observes the later copy to `>10FA`. Exact broader meanings remain
evidence-labeled rather than guessed. R3 persistent game atlas is complete:
append-only JSON packages under `observatory/atlas/` plus one standard-library
command (`tools/observatory_atlas.py`) let a fresh task run
`query tod.stairs-descend` and recover the accepted input, predicate, branch,
mutations, later copy, DESCENDING effect, evidence labels, and retained
uncertainties without rereading traces ([docs/ATLAS.md](docs/ATLAS.md)). Ghidra
was considered there and not adopted. R4 executable subsystem reconstruction
is complete: the neutral stairs model and compact comparison command matched
all 22 declared fields across the accepted positive, rejected negative, and a
predeclared held-out `>1CE8=00` bypass. The held-out runs were deterministic,
and their state mutations, delayed copy, and DESCENDING output agreed with the
frozen prediction ([docs/TOD-STAIRS-MODEL.md](docs/TOD-STAIRS-MODEL.md)). The
post-R4 dungeon-generator reconnaissance is also complete. Two controlled seed
groups, each repeated in a fresh process, locate the outer routine at `>62F4`,
the per-floor entry at `>8246`, the neutral candidate payload
`>34B8..>36D1`, and the reset, seed-dependent-value, and stride writes.
Changing only `>83C0` from `>0000` to `>1234` changes 220 final payload bytes
while each group repeats byte-for-byte. The result is queryable as
`tod.dungeon-generation` and documented in
[docs/TOD-DUNGEON-RECONNAISSANCE.md](docs/TOD-DUNGEON-RECONNAISSANCE.md).
Reconnaissance passed, but full generator reconstruction is a no-go as one
immediate package. The subsequently authorized neutral model of the bounded
per-floor candidate-buffer kernel is now complete. It exactly reproduced both
accepted authoring payloads from kernel-entry seeds `>5207` and `>82F0`. A
prediction for predeclared held-out seed `>A5C3` was frozen before one authentic
run; all 538 bytes, next seed `>AF57`, completion, 1,178 candidate writes, 92
RAND calls, and 31 placement attempts matched. Query
`tod.floor-kernel-model` or read
[docs/TOD-FLOOR-KERNEL-MODEL.md](docs/TOD-FLOOR-KERNEL-MODEL.md).
The bounded payload-consumer semantic decoder is now complete. Byte-verified
GPL recovery establishes the 17×26, stride-32 geometry, neutral class mapping,
and cardinal connection masks around `>A3B5` and `>A5E1`. Existing authoring
evidence agrees at 125 unique consumer-read addresses. The corrected frozen
held-out comparison covers all four distinct raw/class/mask tuples, with zero
contradictions; five duplicate `>6B` coordinates remain `notObserved` and
support no coordinate-specific claim. Query `tod.payload-semantics` or read
[docs/TOD-PAYLOAD-SEMANTICS.md](docs/TOD-PAYLOAD-SEMANTICS.md). Exact
game-level class names, later stocking/content, rendering, and inter-floor
contracts remain unresolved.

Upstream's known field findings remain the longer backlog:

- `F-001`: resolved for the demonstrated R1 failure at `babb8d1`; explicit
  trace entries and bounded leading-`FETCH` inference prevent its observed
  inline operands from desynchronizing the GSL decompiler. Static harvesting
  of every computed-dispatch table remains optional future automation, not an
  open R1 blocker.
- `F-002`: the probe lacks raw binary exports for CPU/VDP/GROM regions.
- `F-003`: authentic GPL random-seed behavior is unresolved. The controlled ToD
  RAND path does not test whether the authentic ISR/KSCAN stirs the seed while
  idle.

Baseline verified on 2026-08-27: `cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings` both pass.

## Next controller session

Do not repeat the completed MVP, R1-R4, dungeon reconnaissance, bounded
candidate-kernel model, or payload-semantic decoder. Recover the current
baseline and preserve their accepted reports and atlas packages:

1. Confirm branch, HEAD, remotes, and clean worktree.
2. Read `docs/RECONSTRUCTION-NEXT.md`, `docs/ATLAS.md`,
   `docs/TOD-GENERALIZATION.md`, and the F-001 resolution in
   `docs/FINDINGS.md`.
3. Preserve R1 at `babb8d1`, R2 at `b3336ad`, the accepted R3 atlas, R4 stairs
   model, dungeon reconnaissance, floor-kernel model, and payload-semantic
   decoder. Do not repeat their traces, model authoring, or authentic held-out
   work unless a new execution reproduces a defect.
4. Query `tod.payload-semantics` before consulting owner-local evidence. It is
   the accepted compact entry point for geometry, class/mask tables, authoring
   coverage, held-out identity and result, and retained uncertainty.
5. Do not start another seed, floor, region, direct-cell, stocking/content,
   inter-floor, rendering, or complete-generator package without a newly
   bounded reconstruction question and explicit cost decision.

The Codex controller invokes the other seats; the owner does not copy prompts.
Do not create a stage, relay, or new generic validation gate.

### Next package

No subsequent reconstruction package is authorized by the completed decoder.
The next controller should stop after recovering the accepted atlas result
unless the owner supplies a new bounded technical question. Do not infer a
campaign from the retained uncertainties.

## Legacy evidence

An earlier independent experiment is preserved separately and is not a
dependency. See [Legacy evidence](docs/LEGACY-EVIDENCE.md) before consulting
it. New implementation work must be reproducible from this checkout and its
documented public inputs alone.
