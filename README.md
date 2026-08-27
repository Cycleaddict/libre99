# Libre99

This repository publishes the `observatory-mvp` research branch built on
[Joel Odom's Libre99](https://github.com/joelodom/libre99). The emulator,
firmware, and core toolchain remain upstream work; the Observatory additions
are kept as a separate, reviewable research layer.

A cycle-aware **Texas Instruments TI-99/4A** home-computer emulator in pure
Rust — together with the toolchain of a complete retro-computing platform: a
from-scratch **TMS9900 assembler**, a **GPL toolchain**, an original
**clean-room console firmware** (booted by default), original **Titris**,
**Sokoban**, and **Jaywalker 99** cartridges built end-to-end with the project's
own tools, and a book in progress about programming the machine.

<p align="center">
  <img src="docs/screenshots/title.png" alt="The clean-room (Libre99) master title screen" width="512">
</p>

The emulator models the real chips — the TMS9900 CPU, TMS9918A video processor,
SN76489 sound generator, TMC0430 GROMs, the TMS9901/CRU keyboard interface, and
the TI Disk Controller (FD1771) — and boots real console firmware on them, so
the console's own GPL interpreter draws the title screen and runs cartridges.
Nothing of the TI operating system is reimplemented on the host: get the chips
right, and the firmware does the rest.

## Highlights

- **Faithful chip emulation, verified.** Beam-accurate scanline video
  rendering, hardware-true GROM prefetch semantics, port-aware wait-state
  timing, the TI noise LFSR — cross-checked against Classic99 (the
  hardware-verified reference emulator) and guarded by **500+ tests** across
  the workspace. When a subtle behavior was wrong, the fix shipped with a
  regression test and a written root-cause analysis.
- **Original clean-room firmware, booted by default.** The console ROM (the
  TMS9900 kernel + GPL interpreter) and console GROM (title screen, cartridge
  menu, **TI PYTHON** — a small Python-like language,
  [spec](docs/TI-PYTHON.md) — and a system-information screen) were rewritten
  from scratch as original work and differentially verified against the
  authentic images. **Extended BASIC cartridges run on it end-to-end** (the
  census-built XB substrate). User-supplied authentic TI firmware is one flag
  away (`--system-rom` / `--system-grom`) — still required only for TI BASIC
  itself ([why](docs/KNOWN-ISSUES.md)).
- **Nothing embedded but our own firmware.** The binary carries zero
  third-party bytes; cartridges (`.ctg` containers or raw `.bin` ROM dumps) and
  disks (`.dsk`) load at run time from your own files — the command line or the
  system file chooser (`F9`).
- **A complete author-to-screen toolchain.** `libre99asm` assembles
  Editor/Assembler-dialect TMS9900 source into bootable `.ctg` cartridges
  (`--cartridge <path>` closes the loop), `libre99gpl` builds GPL firmware, and
  three original games prove the pipeline: **Titris** (falling blocks),
  **Sokoban** (the warehouse-keeper puzzle, with twelve credited Microban
  levels), and **Jaywalker 99** (an endless hopper that works the sprite and sound
  hardware) — all gameplay-tested end to end on the emulated console.
- **A pleasant desktop app.** On-screen help overlay with a pictured TI
  keyboard, layout-independent typing (QWERTY/Dvorak/AZERTY all just work),
  native file-chooser media mounting, save states (auto-save/resume + named
  snapshots), screenshots, pause/frame-advance/fast-forward, and a live CPU
  inspector — the overlays drawn by the app itself, no GUI toolkit.

## Observatory research branch

The `observatory-mvp` branch turns Libre99 into an evidence-producing
environment for reconstructing TI-99/4A software. Its purpose is not to guess
at original source. It closes a reproducible loop:

`authentic execution → filtered causal evidence → structural recovery → neutral model → held-out replay → accepted atlas facts`

Observation is optional and remains outside normal emulator behavior. Analysis
runs after execution, and every accepted claim retains an evidence label and a
direct path back to the experiment, recovered instruction, or primary source
that supports it.

### Demonstrated results

| Investigation | Accepted result | Verification |
|---|---|---|
| Parsec frame attribution | All 34 changed VRAM bytes were attributed to their immediate native writers, including PC `>734C` / opcode `>D801` and PC `>7E76` / opcode `>D836`. | Repeated capture produced identical attribution, screenshot, save state, machine state, and write order. |
| Runtime-informed GPL recovery | Trace-discovered entries and explicit inline-operand semantics prevent executed bytes from being silently misclassified. | The accepted five-page GROM recompiles byte-identically. |
| Tunnels of Doom stairs transition | The accepted chain connects input scratchpad `>8375`, predicate cell VRAM `>1D00`, positive/negative GPL branches, transition mutation `>1CF8`, delayed copy to `>10FA`, and the visible `DESCENDING` effect. | Positive, negative, and predeclared held-out cases matched all 22 declared model fields. |
| Per-floor candidate kernel | A neutral model reproduces the 538-byte payload at `>34B8..>36D1`, including random-state evolution and the direct retry contract. | Two accepted authoring cases and the predeclared held-out seed `>A5C3` matched every payload byte and the next seed. |
| Payload consumers | Recovered consumers establish 17×26 geometry, stride 32, neutral byte classes, and cardinal connection masks. | The frozen comparison covered every distinct raw/class/mask tuple without contradiction; unvisited coordinate-specific meanings remain unresolved. |
| Persistent evidence atlas | Append-only packages preserve facts, classifications, uncertainty, routines, state cells, effects, experiments, and their relationships. | Atlas validation rejects missing references and contradictory duplicate facts; compact queries need no bulk trace input. |

### Source-package closure

The same workflow produced a separate, tracked reconstruction package for the
three logical *Tunnels of Doom* payloads. A source-only build—without access to
the original cartridge or disk evidence—reproduces these identities:

| Logical payload | Bytes | SHA-256 |
|---|---:|---|
| Cartridge GROM | 40,960 | `3eb52ac2415744bc69aa61c55704738eb3d0305fc5838a283403ac8f8cd40514` |
| QUEST | 13,056 | `d4fcf8e1335597f78b44bfb88af0848cc7212c33df97796c056b7e3097a682cc` |
| PENNIES | 13,056 | `c1ccde713a7fb6caf746cd15e617ccbb366ea7cc72488d8dbc4025e4e2148bb0` |

One bounded integration run used those generated payloads as the actual
Libre99 inputs and reached the established `NEW DUNGEON` post-load menu at
frame 1,410. This proves source-package closure and runtime compatibility; it
does not claim recovery of the author's original symbols, comments, or
high-level design.

The reconstructed commercial payloads are not distributed from this
repository. Original media, rebuilt binaries, decompilations, traces,
checkpoints, save states, and new experiment screenshots remain outside Git.

### Query the accepted evidence

The public atlas provides a compact, no-context entry point:

```bash
python3 tools/observatory_atlas.py validate
python3 tools/observatory_atlas.py list
python3 tools/observatory_atlas.py query tod.stairs-descend
python3 tools/observatory_atlas.py query tod.floor-kernel-model
python3 tools/observatory_atlas.py query tod.payload-semantics
```

Claims are classified as `source-confirmed`, `observed`, `corroborated`,
`inferred`, or `unresolved`. Neither Libre99 nor another emulator is treated as
an automatic oracle. Exact gameplay names, stocking and content generation,
rendering, broader inter-floor contracts, and a complete generator remain
outside the accepted result.

Start with [START-HERE.md](START-HERE.md), then read
[the observatory MVP](docs/OBSERVATORY-MVP.md),
[the reconstruction sequence](docs/RECONSTRUCTION-NEXT.md), and
[the persistent atlas](docs/ATLAS.md).

## Screenshots

The clean-room firmware and original content:

| Selection menu | TI PYTHON | System information |
|---|---|---|
| ![The master selection menu](docs/screenshots/menu.png) | ![The TI PYTHON REPL](docs/screenshots/ti-python.png) | ![The system-information screen](docs/screenshots/system-info.png) |

| Titris (original cartridge, built with `libre99asm`) | Sokoban (original cartridge, built with `libre99asm`) |
|---|---|
| ![Titris gameplay](docs/screenshots/titris.png) | ![Sokoban gameplay](docs/screenshots/sokoban.png) |

| Jaywalker 99 (original cartridge — 24 sprites and all four sound voices) |
|---|
| ![Jaywalker 99 gameplay](docs/screenshots/jaywalker99.png) |

Third-party titles running on the emulator (historical screenshots — the
commercial cartridge images themselves are **not** part of this repository;
media loads at run time from user-supplied files):

| Parsec | Tunnels of Doom |
|---|---|
| ![Parsec title screen](docs/screenshots/parsec.png) | ![Tunnels of Doom title screen](docs/screenshots/tunnels-of-doom.png) |

Regenerate the our-titles gallery any time with
`cargo run -p libre99-gpl --example readme_gallery` (the two third-party shots
above are static and no longer regenerated).

## Quickstart

You need a **Rust toolchain** (stable, edition 2021+) with Cargo — nothing
else. The clean-room firmware is baked into the binary; cartridges and disks
are files you mount at run time.

```bash
# Boots the bare console to the master title screen.
cargo run --release -p libre99-app
```

A window opens at the master title screen. **Just type** — the keyboard starts
in character mode, so your keystrokes produce the same characters on the TI
regardless of host layout. Press `F9` to mount a cartridge or disk with your
system's file chooser. Useful keys from the start:

| Key | Action |
|---|---|
| `Esc` / `F1` | Help overlay — four tabs, including the full TI keyboard reference (a first run points you here with a `PRESS ESC FOR HELP` banner) |
| `F9` | Mount media — pick any `.ctg`/`.bin` cartridge or `.dsk` disk image |
| `F5` | Reset the console |
| `F10` | Pause / resume |
| `Cmd`+`Q` (macOS) / `Alt`+`F4` | Quit — the session auto-saves and resumes on next launch |

The complete manual — every hotkey, the keyboard modes, the command line,
preferences, save states, file locations — is
**[docs/USER-GUIDE.md](docs/USER-GUIDE.md)**.

```bash
# A few common variations:
cargo run --release -p libre99-app -- --cartridge my.ctg          # mount a cartridge file
cargo run --release -p libre99-app -- --disk vol.dsk              # insert a disk into DSK1
cargo run --release -p libre99-app -- --cartridge build/game.ctg  # run your own libre99asm build

# Boot user-supplied authentic TI firmware instead of the clean-room default
# (required only for TI BASIC itself; Extended BASIC runs on the default):
cargo run --release -p libre99-app -- --system-rom path/to/994aROM.Bin --system-grom path/to/994AGROM.Bin
```

## The pieces

| Piece | What it is | Docs |
|---|---|---|
| `crates/libre99-core` | The emulator core: every chip, the console bus, save states. Pure `std`, **zero third-party dependencies**, `#![forbid(unsafe_code)]`. | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| `crates/libre99-app` | The desktop app: window, audio, input, overlays, media mounting, config, logging. | [docs/USER-GUIDE.md](docs/USER-GUIDE.md) |
| `crates/libre99-asm` | `libre99asm` — a complete two-pass TMS9900 assembler + `.ctg` cartridge packager + disassembler. | [docs/ASSEMBLER.md](docs/ASSEMBLER.md) |
| `crates/libre99-gsl` | `libre99gsl` — **GSL**, a high-level language over GPL: a compiler (GSL → `.ctg`/GROM image via the GPL assembler) and a self-verifying decompiler (real cartridges → GSL, byte-identical on recompile). | [docs/GSL.md](docs/GSL.md) |
| `crates/libre99-probe` | `libre99probe` — the **headless probe shell**: a scriptable line-command control surface over the emulated console (run frames, press keys, read the screen as text, trace/coverage, save states) for humans and AI agents alike. | [docs/PROBE.md](docs/PROBE.md) |
| `original-content/system-roms` | The clean-room console ROM + GROM rewrite (Libre99): original firmware, differentially verified, booted by default. | [original-content/system-roms/README.md](original-content/system-roms/README.md) |
| `original-content/cartridges/titris` | Titris, an original cartridge authored with the project's own assembler. | [its README](original-content/cartridges/titris/README.md) |
| `original-content/cartridges/sokoban` | Sokoban, a second original cartridge — the classic puzzle with twelve credited Microban levels. | [its README](original-content/cartridges/sokoban/README.md) |
| `original-content/cartridges/jaywalker99` | Jaywalker 99, a third original cartridge — an endless hopper that demonstrates the sprite and sound hardware. | [its README](original-content/cartridges/jaywalker99/README.md) |
| `docs/ti99book` | *Programming the TI-99/4A* — a book manuscript in progress, founded on this project's toolchain. | [its README](docs/ti99book/README.md) |
| `third-party/` | **Git-ignored, maintainer-local** TI firmware and commercial media used only by the differential test suites (absent from a fresh checkout; the public suite skips those tests). | [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) |

## Documentation

**Using it**

- **[docs/USER-GUIDE.md](docs/USER-GUIDE.md)** — the emulator's complete user
  manual: command line, keyboard, hotkeys, media, save states, preferences,
  logs, limitations.
- **[docs/ASSEMBLER.md](docs/ASSEMBLER.md)** — the `libre99asm` user
  guide and TMS9900 assembly-language reference.
- **[docs/GSL.md](docs/GSL.md)** — the GSL language reference: the GPL
  high-level language, its compiler, and the verified decompiler.
- **[docs/PROBE.md](docs/PROBE.md)** — the `libre99probe` manual: driving the
  emulator headlessly from scripts, pipes, or an interactive shell.
- **[docs/KNOWN-ISSUES.md](docs/KNOWN-ISSUES.md)** — behaviors that look like
  bugs but are authentic hardware/firmware behavior, plus genuine open issues.

**Understanding and changing it**

- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — the emulated machine, the
  memory map, the crate/module layout, and the run-time data flow.
- **[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)** — building, testing, project
  conventions, documentation policy, and the licensing/IP checklist.
- **[docs/STATUS.md](docs/STATUS.md)** — where the project stands: what is
  built, verified, and remaining.
- **[docs/ROADMAP.md](docs/ROADMAP.md)** — where it goes next, and the design
  principles that keep features modular.
- **[docs/CROSS-VALIDATION.md](docs/CROSS-VALIDATION.md)** — the plan for
  validating the rewritten firmware outside this emulator.
- **[docs/history/](docs/history/)** — executed plans and dated reports,
  preserved as the project's engineering record.

## Status

The emulator core and desktop app are **complete and playable** (the one open
packaging item is a double-clickable macOS `.app` bundle — run via `cargo run`
for now), the clean-room firmware **boots by default**, and the assembler and
GPL toolchains are **complete**. Detail: [docs/STATUS.md](docs/STATUS.md).

## License and provenance

This project's original work — the emulator, the toolchain, the clean-room
firmware, the cartridges, and the documentation — is Copyright © 2026 Joel Odom
and
licensed under the **Modified MIT License with Commons Clause**
(source-available; the right to sell is reserved): see
**[LICENSE.md](LICENSE.md)**.

**No TI firmware or third-party media is tracked in this repository.** The
authentic TI console/DSR firmware and commercial cartridge/disk images used by
the differential test suites live in a **git-ignored** `third-party/`
directory each maintainer supplies locally; they remain the property of their
respective copyright holders and are never distributed with, or embedded in,
this project (policy in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)). This
repository was created 2026-07-06 from an IP-free snapshot of its private
predecessor, so its **history has never contained a proprietary byte** — clean
back to commit 1 ([roadmap](docs/ROADMAP.md)). Hardware references
consulted are credited in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/history/PLAN.md](docs/history/PLAN.md).
