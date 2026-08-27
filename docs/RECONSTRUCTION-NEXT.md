# Source-free reconstruction — next technical work

## Purpose

The observatory MVP is validated. The next work is not another emulator or
observability qualification gate. It is the missing layer between causal runtime
evidence and a rewrite-ready description of software whose source is absent.

Tunnels of Doom is the working case because it crosses cartridge GPL, the console
GPL interpreter, mutable scratchpad state, data-heavy gameplay, and VDP effects.
The game is a consumer of generally useful reconstruction capabilities; its media,
checkpoints, traces, and derived decompilations remain owner-local.

## Current evidence

- The campaign runner can reproduce independent experiments from checkpoints.
- `vtrace` attributes VDP writes to native instruction, GPL stream position, and
  relevant interpreter state.
- The stairs-descend experiment connects input to GPL activity, native interpreter
  writes, and the resulting screen transition.
- R1 closed at `babb8d1`. `libre99gsl --entries` now accepts distilled,
  execution-confirmed GPL instruction starts, and conservative bounded
  leading-`FETCH` analysis reserves proven caller-inline operands without guessing.
- The demonstrated ToD sequence at `>664D..>6651` now recovers reproducibly, all
  five GROM pages roundtrip byte-identically, and a held-out replay reaches the
  predicted instruction boundaries.
- R2 attributes the stairs predicate, branch, mutations, and later copy to the
  executing GPL/native operations across a positive and a held-out-negative
  checkpoint.
- R3 keeps that result queryable: the accepted stairs chain and its evidence
  labels are answered from tracked JSON atlas packages, without rereading traces
  (`docs/ATLAS.md`).
- R4's executable stairs model matched all 22 declared fields across its
  accepted, rejected, and predeclared held-out cases.
- The completed post-R4 dungeon reconnaissance locates the authentic outer and
  per-floor boundaries, the neutral 538-byte candidate payload, controlled RAND
  state, and repeatable reset/value-placement/stride-store phases. Its accepted
  result is `query tod.dungeon-generation`.
- The bounded per-floor candidate-buffer model is complete. It exactly matches
  both authoring payloads and the one frozen `>A5C3` held-out payload, next seed,
  completion, RAND/attempt counts, and per-operation write summary. Its accepted
  result is `query tod.floor-kernel-model`.
- The bounded payload-consumer semantic decoder is complete. Recovered GPL
  establishes the 17×26/stride-32 geometry, neutral class table, and cardinal
  connection masks. Existing authoring reads agree at 125 unique addresses; the
  corrected held-out comparison covers all four distinct tuples without
  contradiction while five duplicate coordinates remain `notObserved`. Its
  accepted result is `query tod.payload-semantics`.

Together these establish the completed bounded reconstruction packages. They do
not establish exact game-level class names, later stocking/content, rendering,
general retry behavior, the complete map generator, or the complete game. A new
economic/technical decision is required before another package begins.

## Tool roles

- **Libre99 and the probe** execute authentic software and produce deterministic,
  filtered runtime evidence.
- **GSL/libre99gsl** remains the byte-authoritative GPL recovery path because it can
  recompile and verify the exact GROM bytes. R1 repaired the demonstrated F-001
  failure with explicit runtime roots and conservative inline-operand semantics;
  later work must preserve that byte-identity gate.
- **The atlas** (`observatory/atlas/` + `tools/observatory_atlas.py`) is where accepted
  evidence persists in queryable form so a later task starts from conclusions and their
  labels instead of from traces. It records claims and pointers, never artifacts.
- **Ghidra** is the intended persistent analysis target for native TMS9900 code and,
  through an importer or a sufficiently mature GPL language module, recovered GPL
  functions, symbols, data types, cross-references, dynamic coverage, call edges, and
  evidence-linked annotations. It does not replace execution or byte-roundtrip proof.

R1 now emits exact-address `trace_XXXX` functions and explicit inline-byte records in a
documented representation that can later feed a headless Ghidra importer. Ghidra was not
made an R1 prerequisite, and **R3 did not adopt it**: the accepted stairs evidence fits in
tracked JSON packages that any task can query with one standard-library command, so an
importer would have added a tool dependency without answering a reconstruction question.
Revisit it only when a real R4 consumer needs native cross-references, types, or coverage
that the atlas cannot express. Do not launch a general Ghidra plug-in project merely
because the platform is available.

## Technical sequence

These are work packages, not process stages. Do not add another generic validation
gate between them. Each package is driven and accepted by an authentic reconstruction
question.

### R1 — Runtime-informed GPL recovery

**Complete at `babb8d1` (2026-08-26).** The accepted implementation adds
trace-confirmed entry seeds, conservative leading-`FETCH` inline-byte inference,
conflict refusal, exact-address output, focused regression coverage, a byte-identical
five-page ToD roundtrip, and a held-out authentic replay. Raw GROM fetch logs still need
distillation into confirmed instruction starts; automate that later only if it becomes a
measured campaign bottleneck.

Make executed GPL recoverable as correctly bounded code when static tiling is defeated
by computed dispatch or inline arguments. Start from F-001 and the existing ToD traces.
Prefer the smallest combination of trace-discovered entries and explicit inline-operand
semantics that preserves GSL byte-identical roundtrip.

Pass when one previously hidden or misdecoded ToD region is recovered reproducibly,
roundtrips byte-identically, and a held-out execution reaches instruction boundaries
predicted by the recovered representation. **Passed at `babb8d1`.**

### R2 — Mutable-state provenance

**Complete (2026-08-27).**

Attribute filtered CPU RAM, scratchpad, GROM-related, and VRAM state changes to the
executing native/GPL operation that caused them. Capture only selected ranges and
windows; do not add an unbounded all-machine event stream.

Pass when the stairs-only predicate and floor-transition state for the existing ToD
experiment can be located and explained across the positive and held-out-negative
checkpoints.

The accepted bounded capture found the distinguishing state and its executing path:
GPL `>669B` sees the same down-arrow key (`>8375=0A`) in both cases, but the positive
checkpoint has VDP `>1D00=06` and the negative has `>1D00=05`. Only `06` passes the
second comparison at `>66A7`; the positive path then increments `>1CF8` (`00→01`) at
GPL `>66C7`, stores `>1D00=05` at `>66CB`, and later copies `>1CF8` to `>10FA` at
`>A798`. The native data-port reader/writer PCs and every byte access are retained in
the owner-local filtered evidence. Exact game-level names for `>1D00`, `>1CE8`, and
`>10FE` remain evidence-labeled rather than guessed. See `TOD-GENERALIZATION.md`.

### R3 — Persistent game atlas

**Complete (2026-08-26).**

Use campaigns to accumulate an execution map, state map, effect map, data-layout map,
and evidence-labeled routine ledger across a bounded gameplay scenario set. The AI
selects and clusters candidate subsystems; the owner is not expected to guess function
addresses one at a time.

Pass when a fresh task can query the atlas and reproduce why a selected input/state
transition was assigned to a candidate subsystem without rereading full traces.
If Ghidra is adopted here, the same result must be reproducible by a scripted/headless
import rather than existing only in one operator's GUI project. **Passed**; Ghidra was
not adopted.

The accepted implementation is append-only JSON atlas packages under
`observatory/atlas/` plus one standard-library command, `tools/observatory_atlas.py`
(`validate`, `list`, `query`). Packages contribute stable typed entities, evidence
references, factual assertions, semantic hypotheses, and ordered relationships; later
experiments append packages instead of rewriting accepted ones. Facts stay separate from
semantic names, all five evidence labels are preserved, and `validate` refuses
unresolved references, duplicate ids, unsupported kinds/classifications, and
contradictory facts sharing a subject/predicate/context while allowing identical
corroborating assertions. `query tod.stairs-descend` reconstructs the R1/R2 stairs chain
— input, `>8375` key byte, the `>1D00` predicate with its positive `06` and negative
`05`, the accepted continuation at `>66A7`, the rejected branch at `>66AC` to `>663F`,
the `>1CF8`/`>1D00` mutations at `>66C7`/`>66CB`, the later `>10FA` copy at `>A798`, the
native `>08B0`/`>08CE`/`>1D2A` boundary, and the DESCENDING effect — with its labels and
retained uncertainties, from any working directory and without reading a trace. The
atlas stores no commercial bytes, media paths, traces, or owner paths; owner-local
evidence is named logically. See `docs/ATLAS.md`.

The later dungeon reconnaissance appends its accepted data-layout and phase facts in
`0004-tod-dungeon-reconnaissance.json`. Future work should continue to append packages,
not perform a completeness sweep.

### R4 — Executable subsystem reconstruction

**Complete (2026-08-26).**

Recover one meaningful ToD subsystem into control flow, data layout, pseudocode, and an
executable behavioral model. Compare it with the original across declared checkpoints,
inputs, and held-out states. Behavioral equivalence is the objective; original comments
or byte-identical source are not.

Pass when the replacement predicts the original subsystem's outputs and state changes
across the declared matrix, with uncertainties retained rather than silently guessed.

Start from `query tod.stairs-descend`: the accepted chain, its labels, and its retained
uncertainties are the specification to reproduce, and the `resolves_when` notes name the
experiments that would settle the remaining semantics. Facts a reconstruction newly
establishes belong in an appended atlas package, not only in a report.

The accepted model is `tools/tod_stairs_model.py`, documented in
`docs/TOD-STAIRS-MODEL.md` and appended to the atlas as
`0003-tod-stairs-r4-model.json`. It models the exact bounded decision beginning at
GPL `>669B`, the `>1D00`/`>1CE8`/`>10FE`/`>1CF8` conditions, immediate writes at
`>66C7` and `>66CB`, and the separately applied `>A798` copy to `>10FA`. The
G-007 later recovered `>8018`/`>96EA` as a shared acknowledgement trampoline,
not a game-state predicate. Model version 2 replaces the former external
Boolean with concrete `>83A1`, `vdp_timer`, `>1D01`, and new-SCAN-key input;
see `docs/TOD-STAIRS-FALLBACK.md` and query `tod.stairs-fallback`.

The frozen three-case matrix matched all 22 compared authentic fields. The accepted
positive and rejected later-room evidence came from R2. A new predeclared held-out
case changed only `>1CE8` from `01` to `00`; authentic execution took `>66AE` to
the `>66B3` bypass, did not read `>10FE`, performed `>1CF8 00→01` and
`>1D00 06→05`, copied `01` to `>10FA` eight frames later, and displayed
DESCENDING. Two fresh runs produced byte-identical filtered state and name-table VDP
evidence. The model's focused suite and compact `predict`/`compare` commands are
standard-library only and run outside the emulator.

After R4, the owner/controller assessment authorized one bounded dungeon-generator
reconnaissance, not full reconstruction. That reconnaissance is now complete; it did
not require new observation tooling.

### Post-R4 dungeon reconnaissance

**Complete (2026-08-27).** See `docs/TOD-DUNGEON-RECONNAISSANCE.md` and run
`python3 tools/observatory_atlas.py query tod.dungeon-generation`.

The source-free path is viable for a bounded subsystem: byte-verified static recovery,
filtered runtime evidence, the atlas, and a neutral executable model agreed across an
authentic positive, negative, and unseen bypass. This does not yet show that the same
cost/benefit holds for the dungeon generator or a broad game rewrite.

Two fresh runs at restored seed `>0000` and two at controlled seed `>1234` were
byte-identical within each group. The outer routine at `>62F4` calls through `>8002`
at `>63E3` into the per-floor routine at `>8246`. GPL operations at `>8611` and
`>863D` perform an invariant reset of the candidate VRAM payload `>34B8..>36D1`;
`>8553` performs 23 seed-dependent value stores; and `>8339`/`>83D8` perform
seed-dependent stride-32/stride-1 stores. Native `>1D2A` / opcode `>D802`
executes every filtered candidate write. Changing only the random word at `>83C0`
changes 220 final payload bytes, and both runs proceed to the GENERAL STORE.

The economic decision is **narrow**: full generator reconstruction is not economical
as one immediate package.

### Bounded per-floor candidate-buffer model

**Complete (2026-08-27).** See `docs/TOD-FLOOR-KERNEL-MODEL.md` and run
`python3 tools/observatory_atlas.py query tod.floor-kernel-model`.

The standard-library model covers GPL `>8246..>84F1` and the required local
helpers through `>86B8`. Its neutral inputs are the kernel-entry seed, the 538-byte
candidate payload, one-cell VRAM context, and the proven count/position/control
bytes. It implements the decoded reset, RAND-driven accepted stores, alternating
stride-32/stride-1 passes, and adjacent-cell cleanup.

The existing `>0000`/`>1234` lineages were not rerun: their kernel-entry seeds
`>5207`/`>82F0` reproduced all bytes and next seeds exactly. The `>A5C3`
prediction was frozen owner-locally before one authentic boundary-injected run;
all 538 bytes (SHA-256
`58f44c03d67b41d19cd966d632ac1020da86f6e7c2430fdf5014d4c97b73e913`),
next seed `>AF57`, completion through `>84F1`, 1,178 writes, 92 RAND calls, and
31 placement attempts matched. No extra seed was run. Exact byte meanings,
general `>857B` retry behavior, later consumers, stocking/content, and inter-floor
contracts remain unresolved.

### Bounded payload-consumer semantics package

**Complete (2026-08-27).** See `docs/TOD-PAYLOAD-SEMANTICS.md` and run
`python3 tools/observatory_atlas.py query tod.payload-semantics`.

The owner selected the highest-information boundary: recover how the later
consumer around `>A3B5` interprets the neutral 538-byte payload, especially values
`>60..>6B`, as structural cells and connections. Use existing generated floors only.
One floor supplies authoring evidence; one region from another existing floor is
predeclared before consulting its semantic evidence and serves as the sole held-out
comparison. The output is a compact neutral decoder and appended atlas package, not a
renderer, stocking model, inter-floor contract, seed campaign, or complete generator.

The bounded comparison established the 17×26, stride-32 mapping and the
neutral decoder table. The frozen 4×4 region contains four distinct raw values:
`>60`, `>61`, `>67`, and `>6B`. Authentic consumer execution reached at least one
instance of every value and contradicted none of their predicted class or connection
mask. Five unvisited coordinates are additional `>6B` instances, so their status is
`notObserved`, not mismatch. The owner corrected the package acceptance rule to
require coverage of each distinct decoded value/class/mask in the held-out region,
not execution of every duplicate coordinate. This does not authorize a
coordinate-specific claim for an unvisited cell. The completed standard-library
decoder rejects unsupported values rather than inventing semantics, the report
preserves the source-confirmed/observed/unresolved boundary, and atlas package
`0006` makes the result queryable without loading payloads or traces. No
additional floor, seed, region, or direct-cell campaign was run.

Exact game-level class names, coordinate-specific roles for unvisited cells,
stocking/content, rendering, inter-floor contracts, and general retry behavior
remain unresolved. They do not authorize a follow-on campaign by themselves.
A new package requires a separately bounded technical question and cost decision.

## Engineering constraints

- Keep observation optional and behavior-neutral when disabled.
- Keep analysis and indexing outside the emulator hot loop.
- Extend existing Libre99/GSL/probe surfaces; do not revive the old runtime or relay.
- Use primary documentation and original source first, authentic execution second,
  independent implementations as corroboration, and label inference.
- Add focused tests for reproduced behavior plus one authentic owner-local execution.
- Keep public changes separable and generally useful; keep commercial artifacts out of
  Git.
- F18A support is a future concrete execution target, not part of R1-R4 unless an actual
  reconstruction consumer requires it.

## Agent responsibilities

- **Codex controller:** owns the technical question, repository state, scope, diff
  review, acceptance execution, and commit decision.
- **Grok reviewer:** independently challenges material design/evidence decisions and
  audits completed behavioral or public-interface changes read-only.
- **Ox Alpha builder:** implements a bounded controller-authored task and tests it; it
  does not commit, push, broaden scope, or refactor unrelated working code.
- **Authentic execution:** decides whether the capability answers the reconstruction
  question. Agreement among models never replaces it.

Use the exact invocation and prompt templates in `AGENT-COMMANDS.md`. No owner prompt
shuttling, custom relay, or mandatory multi-agent ceremony is required.
