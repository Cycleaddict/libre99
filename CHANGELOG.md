# Changelog

All notable changes to Libre99. The version number is the workspace
`version` in the root `Cargo.toml` — one number shared by the emulator, the
clean-room firmware it embeds, and TI PYTHON's banner.

## Unreleased

- **GSL — the GPL Structured Language** (`crates/libre99-gsl`, binary
  `libre99gsl`, reference [docs/GSL.md](docs/GSL.md)): a high-level language
  over GPL bytecode with both directions implemented. The **compiler** lowers
  GSL (C-flavored statements over the machine's real model — typed vars bound
  to scratchpad/VDP addresses, one condition bit, `asm { }` and `data { }`
  blocks, monolithic files) to `libre99gpl` assembler source and packages
  `.ctg`/GROM images; the **decompiler** turns real cartridges into GSL with
  header-driven entry discovery, a grammar-exact FMT scanner, per-instruction
  re-encode verification, and metadata comments — and refuses to emit a file
  unless it recompiles **byte-identically** to the input payload. Verified:
  the clean-room console GROM, all three original cartridges, Tunnels of
  Doom, and the full 137-cartridge corpus round-trip byte-for-byte
  (`crates/libre99-gsl/tests/`, incl. an `--ignored` full-corpus sweep). The
  GPL assembler grew `assemble_sized` so cartridge GROM space (`>6000+`) can
  be assembled. `assembler/ASSEMBLER.md` moved to `docs/ASSEMBLER.md`.
- **GSL decompiler: semantic annotation** (docs/GSL.md §10.4). Advisory
  analysis layered over the verified output — names and comments only, with
  every address kept in the declarations: documented scratchpad cells become
  named vars (`key_code`, `snd_list_ptr`, `gpl_status`, `gpl_r0`…`gpl_r15`,
  …) sourced from the recon dossiers; VDP vars are annotated with the table
  they land in (screen row/col, sprite attributes, colors, pattern chars),
  tracking literal VDP-register loads; functions get effect-signature
  headers plus `calls:`/`called from:` by name, and single-effect `sub_`s
  are renamed `draw_`/`key_`/`snd_`; statements get machine-contract notes
  (`xml(n)` dispatch targets, VDP register loads with values, sound-list
  stores, the `>D0` sprite terminator, scan loops, `move` source previews);
  data blocks decode strings to literals, render pattern uploads as 8×8
  pixel-art comment bands, and summarize sound lists.
- **GSL: `fmt { }` blocks** (docs/GSL.md §6.2). GPL's FMT screen-format
  sub-language is now a first-class island grammar — `htext`/`vtext`,
  `hchar`/`vchar`, `hmove`/`vmove`, `row`/`col`, `bias` (immediate or
  from-memory), `hstr`, and `repeat (n) { }` loops whose `FEND` loop-back
  word the compiler derives via assembler labels. The decompiler converts
  scanned FMT blocks into `fmt { }` statements when they re-encode
  byte-identically (structural loop-backs, canonical GAS operands) — for
  Tunnels of Doom that turns all 82 raw FMT byte blobs into readable
  statements — and lifts the text they print into `// prints:` function
  headers. Non-canonical blocks still fall back to annotated `BYTE` rows,
  and the whole-file byte-identity gate is unchanged.
- **`libre99probe` — the headless probe shell** (`crates/libre99-probe`,
  manual [docs/PROBE.md](docs/PROBE.md)): a scriptable line-command control
  surface over the emulated console, usable identically by a human at a
  terminal (interactive REPL) and by a program or AI agent on a pipe (echoed,
  fail-fast transcripts). Boots the embedded clean-room firmware headlessly
  (authentic images via the app's own `--system-rom`/`--system-grom`/
  `--disk-dsr` flags), mounts cartridges and disks, and offers: `frames` /
  `settle` / `press` / `hold` / `type` (with the TI SHIFT/FCTN character
  synthesis), the screen decoded to an ASCII grid, dependency-free PNG
  screenshots, side-effect-free `peek`/`vpeek` hex dumps, `poke`/`vpoke`,
  CPU/GROM state panels, a PSG audibility meter, GROM fetch **tracing** and
  GROM+CPU **coverage** recording with per-page summaries and range exports,
  and full **save/load state** checkpoints (the desktop app's portable v3
  format). The machine is deterministic, so a probe script is an exact,
  replayable record of a session — the machine-control layer for the planned
  AI-assisted decompilation-annotation workflows. Pure `std`, depends only on
  `libre99-core`; driven end to end by its own test suite, including a
  corpus-gated Tunnels of Doom walk.
- **`libre99gsl verify` + the `/annotate` skill** — runtime-informed
  annotation of decompilations (docs/GSL.md §13). `verify <in.gsl>
  <against.ctg|.bin>` compiles a (hand- or AI-edited) decompilation and
  byte-compares its payload against the original image — the name-blind
  safety gate that makes editing a decompilation provably unable to change
  its bytes (pinned by a corpus test). On top of it, the repository's first
  checked-in Claude Code skill (`.claude/skills/annotate/SKILL.md`) drives
  the whole workflow: play the cartridge headlessly through `libre99probe`,
  correlate execution traces/coverage with what the screen showed, rename
  and comment with recorded evidence (`observed:`/`likely:`), and maintain
  a cumulative `EXPLORATION NOTES` block (replayable session scripts,
  coverage, renames-with-evidence, unreached code, next-pass hints).
  `.gitignore` now tracks `.claude/skills/` while keeping local Claude
  state untracked; everything continues to work without AI.
- The `Esc`/`F1` **help overlay was redesigned** to the approved four-tab
  "quiet terminal" design (per `design_handoff_help_redesign`): solid black
  backdrop, hairline rules and whitespace instead of cards, a single cyan
  chrome accent (amber/green reserved for the keyboard map's SHIFT/FCTN
  semantics), larger type throughout, and OS-correct shortcut labels. The
  Media & State tab folded into Hotkeys as a "loading & saving" note — four
  tabs (`1`–`4` jump; the footer now also hints `←`/`→`). The embedded fonts
  now interpret point sizes as em sizes (CSS-like), so design-handoff values
  render true; the unused Silkscreen Bold face was dropped.
- **Jaywalker 99**, a third original demonstration cartridge
  (`original-content/cartridges/jaywalker99/`): an endless-hopper arcade game —
  a fledgling blue jay crossing procedurally generated roads, rivers, and
  rail lines — built, like Titris and Sokoban, entirely with the project's
  own assembler and gameplay-tested end to end on the emulated console.
  Where the first two cartridges are character-graphics puzzles, Jaywalker 99
  works the arcade hardware: up to 24 simultaneous 16×16 sprites at
  independent sub-pixel speeds (early-clock edge slide-ins, priority-aware
  four-per-line budgeting), color-table and pattern-table animation (the
  flashing level-crossing signal, the shimmering river), and a table-driven
  driver for all four SN76489 voices. Its regression suite
  (`crates/libre99-asm/tests/jaywalker99.rs`) plays the game headlessly —
  movement, scoring, scrolling, every hazard and death, the hawk, sounds on
  the real PSG, and a 5,000-frame random-input soak.

## 0.1.0 — 2026-07-07 — the first public release (early testing)

The first source-available drop of **Libre99**: a from-scratch TI-99/4A
emulator in pure Rust that **contains and executes no Texas Instruments
bytes** — it boots this project's own clean-room firmware by default, in a
repository whose history has been IP-clean from commit 1.

**The machine** (`libre99-core`, pure `std`, zero dependencies)

- TMS9900 CPU — all instructions, status flags, interrupts, cycle-aware
  timing; conformance-tested.
- TMS9918A VDP — all modes, sprites, **beam-accurate scanline rendering**.
- TMC0430 GROM array, TMS9901 + CRU + keyboard matrix, SN76489 PSG.
- Cartridge loader (`.ctg`, bank switching) — byte-exact across a 137-image
  test corpus.
- TI Disk Controller (FD1771) with a **clean-room disk DSR** that reads *and
  writes* by default.

**The firmware** (clean-room, boots by default)

- Original console ROM + GROM: title screen, selection menu, GPL
  interpreter, KSCAN, ISR, DSRLNK, FMT, floating point — differentially
  verified against the authentic firmware; the 137-cart health panel passes
  with **zero waivers**.
- **TI PYTHON v1** — an original Python-like mini-language in TI BASIC's
  menu slot ([spec](docs/TI-PYTHON.md)).
- **Extended BASIC runs end-to-end** on the clean-room pair (the XB
  substrate). TI BASIC itself is the one thing that still needs
  user-supplied authentic ROMs (`--system-rom` / `--system-grom`).

**The desktop app** (`libre99`)

- Zero embedded media — the console boots bare; mount any `.ctg`/`.dsk` via
  the command line or the OS-native file chooser (`F9`); disks mount and
  eject **live**, no reboot.
- **Disk persistence that never touches your files**: writes stay in
  memory, survive eject/remount and save states, and export on demand
  (`F4`) through the native save dialog.
- **Save states**: the automatic resume state (auto-save on exit, resume at
  launch, `F6`/`F8`) plus user-named snapshot files (`Shift`+`F6`/`F8`) —
  atomic writes, portable format (v3) across Windows and macOS.
- Five-tab help overlay (`Esc`/`F1`) at native resolution, a first-run
  `PRESS ESC FOR HELP` banner, speed control (pause / frame advance /
  fast-forward), PNG screenshots, a live CPU inspector, character and
  positional keyboard mapping, `--version`.

**The toolchain and original content**

- `libre99asm` — a from-scratch, Editor/Assembler-compatible TMS9900
  assembler that emits bootable cartridges ([guide](docs/ASSEMBLER.md)).
- `libre99gpl` — GPL assembler/decoder/disassembler; builds the console GROM.
- Two original, playable cartridges built by that toolchain: **Titris** and
  **Sokoban**.

**Assurance**

- 500+ tests across four crates; public CI (tests + clippy) green on
  Windows and macOS from a clean checkout, with zero proprietary bytes.
- Authentic-image comparisons run only on development machines, loading
  user-supplied images at run time from the git-ignored `third-party/`
  (tests skip green when absent).
