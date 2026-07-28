---
name: annotate
description: >-
  Enrich a GSL decompilation (a .gsl produced by libre99gsl decompile) with
  runtime evidence: play the cartridge headlessly with libre99probe, then
  improve symbol names and comments while proving the file still recompiles
  byte-identically (libre99gsl verify). Use when asked to annotate, enrich,
  or explore a decompiled TI-99/4A cartridge.
---

# Annotate a GSL decompilation with runtime evidence

You will enrich a decompiled `.gsl` file with meaningful names and comments
**earned by actually running the cartridge** — without changing a single byte
of what the file compiles to.

**Arguments**: `<decompilation.gsl> [cartridge] [hints file]`.
- The cartridge (`.ctg`/`.bin`) is the image the decompilation came from; its
  basename is recorded in the `.gsl` header comment. If its path isn't given
  and isn't in the hints, ask the user.
- The hints file is optional free text: where companion media lives (data
  disks), what the program is, what to focus on, standing orders ("don't
  touch the sound driver"). Honor it throughout, and re-read it each pass.

## The four iron rules

1. **The file must stay byte-identical.** After every batch of edits run
   `./target/release/libre99gsl verify <file.gsl> <cartridge>`. It recompiles
   the file and byte-compares the payload against the original image. If it
   fails, fix or revert the batch before doing anything else; never finish
   with a failing verify. (The check is name-blind — renames and comments
   cannot break it; touching real statements or data can.)
2. **Addresses are ground truth — never remove them.** Every declaration
   keeps its `@ 0xNNNN`, every function header keeps its `// >NNNN:` line.
   Renamed **functions keep the address suffix** (`sub_C41E` →
   `store_menu_C41E`); **variables may take plain names** (`b_8340` →
   `party_gold`) because their declaration pins the address — the same
   convention the decompiler's own static renames use.
3. **Evidence discipline.** Rename only what you *observed*: the trace or
   coverage placed execution there, the screen showed it, memory changed in
   step with it. Prefix comments `observed:` for session facts and `likely:`
   for inference. No evidence → no rename; a plausible guess is worse than an
   honest `sub_XXXX`.
4. **Copyright.** Decompilations of commercial cartridges — and everything
   derived from them (the enriched `.gsl`, evidence files, session scripts,
   hints) — stay **outside this repository**. Never commit them; keep them
   next to the `.gsl` or in scratch space.

## Step 0 — set up

- `cargo build --release -p libre99-probe -p libre99-gsl`
- Read the hints file, the `.gsl` header comment (the first ~60 lines), and
  the **tail** of the file: an existing `EXPLORATION NOTES` block means this
  is a later pass — read it fully and build on it. Passes are cumulative;
  never discard earlier sessions, renames, or notes.
- Baseline: run `verify` on the untouched file. It must pass before you edit
  anything (if it doesn't, stop and tell the user).
- Pick a scratch directory for session scripts, evidence dumps, screenshots.

## Step 1 — index the file (don't read all of it)

The file may be hundreds of KB. Grep, don't read end to end:

- `grep -n '^fn ' file.gsl` → the function index: every name and `@ 0xNNNN`
  address. A function's byte span runs from its address to the next one's —
  this index is what turns trace/coverage addresses into function names.
- The `// >NNNN:` headers already carry the static analysis: effects
  (`formats screen text`, `reads keyboard`, …), `// prints:`, `// calls:` /
  `// called from:`. Your job is the layer static analysis cannot reach:
  what the code *means* in the running program.

## Step 2 — play (the evidence loop)

`libre99probe` is the control surface — `docs/PROBE.md` is the manual. Two
driving modes, both good:

- **Script replay**: keep `session-N.txt`, append commands, rerun
  `./target/release/libre99probe <cart> --script session-N.txt`. The
  emulator is deterministic, so the script *is* the session — and the
  replayable proof of every claim you make from it.
- **Checkpoints**: `save`/`load` state files to branch-explore (checkpoint
  before a menu, try option 1, `load`, try option 2) without replaying.

The loop is: `screen` → decide → `press`/`type` → `settle` → repeat. For
custom-font or sprite-heavy screens where the ASCII decode is unreadable,
`shot file.png` and view the image. A standard opening:

```
frames 180        # power-up to the master title
press space       # any key → selection menu
settle
press 2           # the cartridge's first program
settle
```

Evidence channels, and when to use each:

- `cover on` at session start and leave it on (it's cheap). At session end,
  `cover` for the summary and `cover save <file>` for the ranges. Bucketed
  against the Step-1 function index this yields the coverage measure:
  *N of M functions observed executing*.
- `trace on` only around a moment — "what runs when I press B here?" — then
  `trace summary` / `trace save`, since the log grows fast; `trace on`
  restarts it. Fetches at `>6000`+ are the cartridge's own code executing.
- `peek` scratchpad ranges before/after an action to see which cells changed
  — the raw material for variable names (`observed: decremented each combat
  round`).
- `audio 30` to confirm an action beeped (sound-driver attribution).
- The strongest naming evidence combines channels: *function X executed
  while the screen said Y* (trace window + `screen` in the same moment).

Plan sessions around the hints. Probe menus and verbs systematically — save
states make it cheap to try every option of every menu. Reaching all code is
intractable; make reasonable, hint-guided efforts, then *document* what was
not reached and why rather than guessing.

## Step 3 — edit

- **Renames**: whole-word search-and-replace across the file — declaration,
  call sites, and `calls:`/`called from:` cross-references all use the same
  spelling, and a missed site is a compile error that `verify` will catch.
- **Comments**: terse and in place. Explain what the code *means* in the
  game ("`observed:` prints the store menu"), not what the instructions do.
  Use `likely:` sparingly and honestly.
- Batch small (one subsystem at a time) and run `verify` after each batch,
  so a mistake is easy to bisect.

## Step 4 — the EXPLORATION NOTES block

Maintain exactly one block comment at the very end of the file — append it
on the first pass, update it (bump the pass number, merge sessions and
renames) on later passes:

```
// =====================================================================
// EXPLORATION NOTES — pass 2 (3 sessions) — by the /annotate skill
// =====================================================================
// COVERAGE
//   functions observed executing: 212/344 (62%)
//   cart GROM addresses read: 18412/40960
// SESSIONS (deterministic keystroke scripts — replay to reproduce)
//   1 "boot to party creation" (session-1.txt):
//     frames 180 / press space / settle / press 2 / settle / press 1 / ...
//   2 "store + combat" (session-2.txt): ...
// RENAMES (one per line, with evidence)
//   sub_C41E -> store_menu_C41E    observed: executed while screen read
//                                  "GENERAL STORE"; draws its menu
//   b_8340   -> party_gold         observed: fell 250->175 on a purchase
// UNREACHED (and why)
//   >D9A2->DB00 cassette error path — needs a mid-load cassette fault
// NEXT HINTS (for the user's next pass)
//   - a data disk in DSK1 would open everything past "LOAD DATA FROM"
```

The block doubles as the machine-readable record: scripts verbatim, one
rename per line. It is also how the next pass (and the user) knows where
diminishing returns set in.

## Step 5 — finish and report

- Final `verify` must print `verify OK` — quote that line in your report.
- Report to the user: coverage numbers (and their delta from the previous
  pass), the rename count with the two or three best examples, notable
  discoveries about how the program works, what stayed unreached, and the
  NEXT HINTS suggestions.
