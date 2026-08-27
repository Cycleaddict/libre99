# Observatory persistent game atlas

The atlas is the R3 layer between causal runtime evidence and a rewrite-ready
description. It holds **accepted** reconstruction facts as ordinary JSON so a
fresh task can recover a causal chain and its evidence labels **without
rereading raw traces**.

It is not a database, a framework, or a trace store. There is no server, no
index, and no schema migration machinery: a package is a file, the tool reads
the files, and Git reviews the diff.

Python 3 (standard library only) is required. Like the campaign runner, this is
observatory tooling, not part of the Cargo workspace.

## Commands

```bash
python3 tools/observatory_atlas.py validate
python3 tools/observatory_atlas.py list
python3 tools/observatory_atlas.py query tod.stairs-descend
python3 tools/observatory_atlas.py query tod.stairs-fallback
python3 tools/observatory_atlas.py query tod.stairs-descend --json
python3 tools/observatory_atlas.py query tod.dungeon-generation
python3 tools/observatory_atlas.py query tod.payload-semantics
```

The default atlas root is the tracked directory `observatory/atlas/`, derived
from the script's own location, so every command works from any working
directory. `--root DIR` (before the subcommand) points at another root.

`query` accepts either a **subsystem id** (`tod.stairs-descend`) or a **query
id** (`q.tod.stairs-descend`). Both resolve to the same answer while a
subsystem has exactly one query. The same applies to
`tod.dungeon-generation` / `q.tod.dungeon-generation` and
`tod.payload-semantics` / `q.tod.payload-semantics`.

Exit codes: `0` success, `1` an invalid atlas (unresolved reference, duplicate
id, unsupported kind or classification, contradictory facts), `2` an unknown
query identity. `query` refuses to answer from an atlas that does not validate.

## Packages are append-only

Every `observatory/atlas/*.json` file is one package. Files load in sorted
order and their contents merge; a package name prefix (`0001-`, `0002-`, …)
keeps that order readable.

- A later experiment **adds a package**. It does not edit an accepted one.
- A package may reference entities published by an earlier package — that is
  how `0002-tod-stairs-visible-effect.json` attaches the DESCENDING effect to
  the routine seeded by `0001-tod-stairs-r2-causal-chain.json`.
- Ids are global across packages and immutable once accepted. Leave gaps in
  `order` values so an appended package can slot into an existing chain.
- Correcting an accepted claim is a real change with a real diff: append the
  better-evidenced assertion under a new context, or edit the wrong package and
  say so in the commit. Contradicting it silently is refused by `validate`.

## Schema (`atlas_version: 1`)

A package is a JSON object with `atlas_version`, `package`, optional `title`,
`summary`, `seeded_from`, and these sections, all optional lists:

| Section | Purpose |
|---|---|
| `entities` | The stable typed identities everything else points at. |
| `evidence` | Where a claim comes from. |
| `facts` | Factual assertions about an entity. |
| `hypotheses` | Semantic names, meanings, and retained uncertainty. |
| `relationships` | Ordered edges between entities. |
| `queries` | Named questions a fresh task can ask. |

**Facts and hypotheses are deliberately separate.** A fact says what was
observed, confirmed, or corroborated; a hypothesis says what it might *mean*.
That split is what keeps a broad name from being promoted into evidence.

### Classifications

The five evidence labels from `AGENTS.md`, unchanged:

`observed`, `source-confirmed`, `corroborated`, `inferred`, `unresolved`.

Facts may carry any of the five. Hypotheses may carry only `inferred` or
`unresolved` — a semantic claim that is directly observable belongs in `facts`.

### Entities

| Field | Meaning |
|---|---|
| `id` | Stable global identity. |
| `kind` | `subsystem`, `experiment`, `input`, `routine`, `operation`, `state-cell`, `effect`. |
| `name` | Short human name. |
| `subsystem` | The owning `subsystem` entity. Required except on a subsystem. |
| `order` | Optional integer, for readable output within a kind. |
| `address` | Optional TI address, in the usual `>ABCD` form. |
| `space` | Optional address space (`scratchpad`, `vram`, `cartridge-grom`, …). |
| `detail` | Optional note, including what is *not* bounded. |

### Evidence

| Field | Meaning |
|---|---|
| `id` | Stable global identity. |
| `kind` | `primary-documentation`, `tracked-document`, `owner-local-experiment`, `held-out-source`. |
| `ref` | Repo-relative document path, public source, or **logical** owner-local experiment name. |
| `detail` | Optional description of the run or section. |
| `counts` | Optional object of the accepted counts already printed in tracked docs. |

### Facts

`id`, `subject` (entity id), `predicate`, `value`, optional `context`,
`classification`, `evidence` (at least one id), optional `note`.

`context` is an `experiment` entity id. It is what lets the positive and
held-out-negative runs disagree without contradicting each other: the seed uses
`tod.experiment.positive-stairs` and `tod.experiment.negative-stairs`.
Validation rejects an unknown or non-experiment context, so a typo cannot evade
the contradictory-fact check.

### Hypotheses

`id`, `subject`, `statement`, `classification` (`inferred`/`unresolved`),
optional `alternatives` (the readings the evidence does not yet separate),
optional `resolves_when` (the experiment that would settle it), optional
`evidence`.

### Relationships

`id`, `order`, `from`, `kind`, `to`, `classification`, optional `context`,
`detail`, `evidence`.

Kinds: `part-of`, `executed-by`, `reads`, `compares`, `continues-to`,
`branches-to`, `writes`, `precedes`, `causes`.

`executed-by` records the GPL/native boundary: the GPL operation is the causal
agent, the native console-ROM operation performs the access.

### Queries

`id`, `subsystem`, `question`, `answer`. The `answer` is a compact orientation
paragraph; the reconstruction itself comes from the entities, facts, chain,
hypotheses, and evidence the query collects.

## What `validate` refuses

| Refusal | Message names |
|---|---|
| Duplicate id, in one package or across packages | the id and both packages/files |
| Reference to an unknown entity or evidence id | the owning record and the bad reference |
| A `context` that is not an experiment entity id | the owning record and the target |
| An entity or query whose `subsystem` is not a subsystem | the record and the target |
| Unsupported kind or classification | the record and the offending value |
| Missing or empty required field | the record and the field |
| Two facts with the same `subject`/`predicate`/`context` but different `value` | the key and both fact ids |
| A fact with no evidence reference | the fact id |

Identical corroborating assertions may coexist: the seed asserts the FCTN+X key
binding twice with the same value, once `observed` from execution and once
`corroborated` from the published manual.

## Query output

Text output is compact and deterministic — the same atlas always renders the
same bytes. It prints, in order: the atlas root and its packages, the query,
subsystem, question, and answer; then `entities`, `facts` (each with its label,
context, note, and evidence ids), the ordered `causal chain`, `semantics and
retained uncertainty`, and the `evidence` actually cited.

`--json` emits the same content as one compact line with sorted keys, for later
machine import (about 30 KB for the seeded stairs query).

## Evidence boundary

The atlas stores **claims and pointers**, never artifacts:

- No commercial bytes, media paths, cartridge/disk/ROM file names, traces,
  screenshots, save states, or derived decompilation.
- No absolute or `~`-relative owner paths. Owner-local evidence is named
  logically (`tod-mvp/mtrace-stairs-positive`), and the numbers stored are the
  accepted counts and digest prefixes already printed in tracked documentation.
- A test enforces the path/media half of this boundary over every tracked
  package.

Where the underlying evidence lives, and how to reproduce it, stays in
`docs/CAMPAIGNS.md` and `docs/TOD-GENERALIZATION.md`.

## Seeded content

Eight append-only packages preserve the accepted stairs and dungeon-generator
reconstruction results:

- `0001-tod-stairs-r2-causal-chain.json` — the input, the `>8375` key byte, the
  `>1D00` predicate with its positive `06` and negative `05`, the accepted
  continuation from `>66A7`, the rejected branch at `>66AC` to `>663F`, the
  `>1CF8 00→01` mutation at `>66C7`, the `>1D00 06→05` store at `>66CB`, the
  later `>10FA 00→01` copy at `>A798`, the native `>08B0/>D013`,
  `>08CE/>D020`, and `>1D2A/>D802` boundary, and the retained uncertainties for
  the `>1D00` enum and `>1CE8`/`>10FE` semantics.
- `0002-tod-stairs-visible-effect.json` — the DESCENDING interstitial, its
  console-ROM writer PCs, the gate-2 campaign digests, and the held-out manual
  corroboration.
- `0003-tod-stairs-r4-model.json` — the executable stairs-model boundary, its
  three-case comparison, and the held-out `>1CE8=00` bypass result.
- `0004-tod-dungeon-reconnaissance.json` — the bounded generation boundary,
  `>34B8..>36D1` candidate payload, controlled seed, exact GPL/native mutators,
  deterministic two-seed experiment, phase ordering, and retained economic and
  semantic uncertainty.
- `0005-tod-floor-kernel-model.json` — the neutral executable kernel boundary,
  exact two-case authoring reproduction, frozen `>A5C3` prediction, single
  authentic held-out comparison, and retained retry/semantic uncertainty.
- `0006-tod-payload-semantics.json` — the source-confirmed 17×26/stride-32
  structural decoder, exact neutral class and cardinal-mask tables, 125-address
  authoring comparison, corrected four-tuple held-out result, five duplicate
  `notObserved` coordinates, and retained game-level semantic uncertainty.
- `0007-tod-floor-kernel-retry.json` — the source-confirmed direct retry
  predicate, mode-`>01` reset and `>8283` continuation, frozen bounded case,
  exact authentic comparison, and separate unresolved `>857B` predicate.
- `0008-tod-stairs-fallback.json` — the source-confirmed `>8018`/`>96EA`
  acknowledgement contract, all fourteen callers, frozen single authentic
  comparison, and model-v2 input boundary.

The atlas answers six queries: `tod.stairs-descend`, `tod.stairs-fallback`,
`tod.dungeon-generation`, `tod.floor-kernel-model`, and
`tod.payload-semantics`, plus `tod.floor-kernel-retry`.

## Fresh-task invocation

A task with no other context can reconstruct the accepted stairs evidence with:

```bash
python3 tools/observatory_atlas.py validate
python3 tools/observatory_atlas.py list
python3 tools/observatory_atlas.py query tod.stairs-descend
python3 tools/observatory_atlas.py query tod.stairs-fallback
python3 tools/observatory_atlas.py query tod.dungeon-generation
python3 tools/observatory_atlas.py query tod.floor-kernel-model
python3 tools/observatory_atlas.py query tod.floor-kernel-retry
python3 tools/observatory_atlas.py query tod.payload-semantics
```

That is the whole entry path. Do not open a trace, a checkpoint, or an
owner-local artifact to answer a question the atlas already answers; if the
atlas cannot answer it, that gap is the next experiment.

## Tests

```bash
python3 -m unittest tools/test_observatory_atlas.py
```

The suite validates and queries the tracked packages, runs the principal
queries from an unrelated working directory to prove the answers come only
from the atlas, and checks that invalid references, contradictory facts,
duplicate ids, and unsupported kinds/classifications fail with the offender
named.
