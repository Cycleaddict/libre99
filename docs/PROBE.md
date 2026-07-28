# libre99probe — the headless probe shell

`libre99probe` is a **headless, scriptable control surface over the emulated
console**: it boots the same machine the desktop app boots — no window — and
executes plain line commands against it. Run frames, press keys, read the
screen back as text, take PNG screenshots, peek and poke memory, trace GROM
fetches, record execution coverage, and checkpoint/restore complete machine
states.

It is built for two users at once:

- **a human at a terminal** — an interactive REPL with a prompt, `help`, and
  forgiving errors;
- **an AI agent (or any program) on a pipe** — the same commands, echoed into
  a self-describing transcript, with fail-fast errors and deterministic
  output.

The second user is the point. The probe is the machine-control layer for the
GSL decompiler's planned annotation workflows: an agent plays a cartridge
headlessly — read `screen`, decide, `press` — while `trace`/`cover` record
which code actually ran, and `save`/`load` checkpoint the exploration. The
emulator has no wall clock and no host randomness, so **a session script is
an exact, replayable record**: the same commands from the same boot always
produce the same machine, byte for byte.

```
$ printf 'frames 180\npress space\nsettle\nscreen\nquit\n' \
    | libre99probe tunnelsofdoom.ctg
> cart tunnelsofdoom.ctg
mounted "TUNNELS OF DOOM" (0 ROM bank(s), 5 GROM page(s)); console reset
> frames 180
ran 180 frames (session total 180)
> press space
pressed space (+30 frames)
> settle
settled after 30 frames (screen unchanged for 30)
> screen
screen 32x24 (graphics1), name table at VRAM >0000
   +--------------------------------+
 0 |                                |
 1 |  ..... TEXAS INSTRUMENTS       |
...
 9 |    2 FOR TUNNELS OF DOOM       |
...
```

## Invocation

```
libre99probe [CARTRIDGE] [options]
```

| Option | Meaning |
|---|---|
| `CARTRIDGE` | Mount this cartridge at startup (`.ctg` container or raw `.bin` ROM dump) — equivalent to a leading `cart` command. |
| `--script FILE` | Run `FILE`'s commands before reading stdin. The first error stops the run with exit code 1. |
| `--disk FILE.dsk` | Mount `FILE` in DSK1 at startup. |
| `--system-rom FILE` | Boot this console ROM instead of the embedded clean-room one. |
| `--system-grom FILE` | Boot this console GROM instead of the embedded clean-room one. |
| `--disk-dsr FILE` | Install this disk-controller DSR instead of the clean-room one. |
| `--version`, `--help` | The usual. |

By default the probe boots the **embedded clean-room firmware** — the same
committed artifacts the desktop app embeds — so it works from a fresh
checkout with no third-party bytes. The override flags accept user-supplied
authentic TI images, exactly like the desktop app's flags of the same names.

After `--script` (if given), the probe reads commands from **stdin**:

- At a **terminal**, it is an interactive REPL — `probe>` prompt (on stderr,
  so redirected stdout stays a clean transcript), errors are printed and the
  session continues.
- On a **pipe**, every non-empty command is echoed to stdout as `> command`
  before its output, so the transcript alone tells the whole story; the first
  error prints `error: …` and exits 1 (fail fast, so a driving program can't
  keep issuing commands against a state it misjudged).

Exit codes: `0` success (including `quit`), `1` a command failed, `2` a
startup problem (bad flag, unreadable file).

## The command language

One command per line. `#` starts a comment; blank lines are ignored.
Addresses and bytes are **hex** in any of three spellings (`>8370`, `0x8370`,
`8370`); counts (frames, lengths, drive numbers) are **decimal**. Commands
that reject bad input leave the machine untouched.

### Machine control

| Command | What it does |
|---|---|
| `frames N` (alias `f N`) | Run `N` frames. 60 frames = 1 emulated second. |
| `settle [MAX]` | Run until the screen has been unchanged for 30 consecutive frames (cap `MAX`, default 600). Reports either `settled after N frames` or `screen still changing` — the standard way to wait for the firmware to finish drawing. Screens with animation never settle; use the cap. |
| `reset` | Console reset (the CPU re-vectors through `>0000`). Memory and media stay. |
| `press KEY [KEY …]` | Tap keys **together** (a chord): hold 3 frames, release, settle 27 — e.g. `press 2`, `press fctn =` (the TI QUIT chord). |
| `hold KEY [KEY …]` / `release [KEY …\|all]` | Raw switch control for anything the `press` cadence can't express (joystick steering, long holds). Bare `release` lets go of everything held. |
| `type TEXT` | Type `TEXT` one character at a time, synthesizing SHIFT/FCTN as the TI keyboard requires (`"` is FCTN+P, `+` is SHIFT+=, …). Quote the whole text (`type "A B"`) to type leading/trailing spaces. `keys` lists the key names; an untypeable character is an error. |

### Observation

| Command | What it does |
|---|---|
| `screen` | The VDP name table decoded as a bordered ASCII grid with row numbers — 32 or 40 columns by mode. Bytes are shown as ASCII where printable, `.` elsewhere: exactly right for the standard character set, approximate for games with custom fonts (use `shot` for those). |
| `shot FILE.png` | A real screenshot: 512×384 indexed PNG (2× the native 256×192), written dependency-free. What the player would see, sprites and custom fonts included. |
| `state` | A one-look health panel: session frame count, CPU PC/WP/ST, GROM address, cartridge, VDP mode + registers, and a screen fingerprint (distinct byte values + the most common one — a "bonkers" screen is a flood of one value). |
| `regs` | CPU PC/WP/ST, workspace registers R0–R15, GROM address. |
| `peek ADDR [LEN]` | Hex+ASCII dump of CPU address space, **side-effect-free** (RAM/ROM regions; device ports and the paged cartridge/DSR windows read as `00` here by design — peeking must never disturb the machine). Default 32 bytes, max 4096. |
| `vpeek ADDR [LEN]` | The same for the VDP's 16 KiB VRAM (addresses wrap at `>3FFF`). |
| `poke ADDR B [B …]` / `vpoke ADDR B [B …]` | Write bytes to CPU RAM / VDP RAM (experiments; `poke` writes only writable RAM regions). |
| `audio [FRAMES]` | Run frames (default 60) while measuring the PSG's output; reports RMS/peak and an audible/silent verdict — "did that keypress beep?" without a sound card. |

### Media

| Command | What it does |
|---|---|
| `cart FILE` | Mount a cartridge (`.ctg` or raw `.bin`) and reset the console — the same cold restart a real cartridge insertion needs. |
| `disk N FILE.dsk` | Mount a disk image in DSK`N` (1–3), **live** — no reset, like a real floppy. Writes stay in the emulated machine (and in save states); the host file is never touched. |
| `eject N` | Empty drive `N`. |

### Evidence: tracing, coverage, checkpoints

These are the probe's reason to exist — the channels that turn "the AI played
the game" into machine-checkable facts about which code ran.

| Command | What it does |
|---|---|
| `trace on` / `trace off` | Record every GROM fetch (address, byte). GPL code *is* fetched from GROM, so fetches at `>6000`+ are literally the cartridge's own instructions executing — the GPL execution trace. `on` restarts the log. The log grows without bound (~thousands of fetches per busy frame); trace the moments you care about, not whole sessions. |
| `trace` / `trace tail [N]` | Log size / the last `N` fetches (default 32). |
| `trace summary` | Fetch counts per 256-byte page, cart vs. console — a fast "what ran just now". |
| `trace save FILE` | The full log, one `>ADDR BYTE` per line, for offline analysis (e.g. bucketing by a decompilation's function ranges). |
| `cover on` / `cover off` | Record **coverage bitmaps**: every distinct GROM address read and every distinct CPU PC executed. Compact where the trace is huge — leave coverage on for a whole session to measure how much of a cartridge was reached. |
| `cover` (or `cover summary`) | Counts and range counts for both bitmaps. |
| `cover save FILE` | The bitmaps as inclusive address ranges (`grom >6000->6123` / `cpu >0024->0100` lines). |
| `save FILE` | A complete machine save state — the same self-contained format v3 as the desktop app (RAM, VRAM, GROM image, cartridge, mounted disks, chip latches). |
| `load FILE` | Restore one, exactly. Note: trace/cover recording does not survive a load (re-arm afterwards). Save states from the desktop app load here and vice versa. |

The `save`/`load` pair is the branch-exploration tool: checkpoint before a
menu, try option 1, `load`, try option 2 — no replaying the session.

### Session

| Command | What it does |
|---|---|
| `source FILE` | Run a command script (its transcript is echoed; nesting up to 8 deep). Errors report `FILE:LINE`. |
| `echo TEXT` | Print `TEXT` — labels and markers inside transcripts. |
| `keys` | The key-name reference: letters, digits, `enter space = . , ; /`, `fctn shift ctrl`, `joy1-up` … `joy2-fire`. |
| `help` | The built-in command summary. |
| `quit` / `exit` | End the session. |

## Patterns

**Boot to the menu** (the opening of nearly every session):

```
frames 180        # power-up to the master title screen
press space       # any key leaves the title
settle            # wait for the selection menu to draw
screen            # list the menu — cartridges appear by name
press 2           # launch the cartridge's first program
settle
```

**Watch what a keypress executes** — trace only the moment:

```
trace on
press 1
settle
trace summary     # which GROM pages ran (>6000+ = the cartridge's own code)
trace save /tmp/press1.trace
trace off
```

**Measure how much of a cartridge a session reached** — coverage stays cheap
for hours:

```
cover on
# ... play ...
cover             # e.g. "grom: 9041 addresses read (cart >6000+: 7812 ...)"
cover save /tmp/session.cover
```

**Drive it from a program** (how an AI agent uses it): keep a growing script
file, or hold the process open on a pipe and alternate write-command /
read-reply. Determinism means the whole session can also be re-run from the
script at any time — `libre99probe cart.ctg --script session.txt` reproduces
the machine exactly, and a `save` at the end makes the next run instant.

## What the probe is not

It executes no AI logic and never modifies the host beyond the files you name
in `shot`/`save`/`trace save`/`cover save` commands. It is the mechanical
layer only — the annotation workflow that sits *on top of* it (the
`/annotate` Claude Code skill, which compiles a GSL decompilation, plays the
result through this shell, and enriches the source with the evidence) is
documented in [GSL.md](GSL.md) §13 and is always optional.

## Where it lives

`crates/libre99-probe` — pure `std`, depending only on `libre99-core`. The
session engine is the library (`Session::exec`, one string in, one reply
out); the binary is a thin REPL over it, so tests drive the real command
language end to end (`crates/libre99-probe/tests/shell.rs`, including a
corpus-gated Tunnels of Doom walk that skips green without the third-party
media). The embedded firmware is the committed clean-room artifacts under
`original-content/system-roms/` — the staleness gates that keep those fresh
(`committed_bin` tests) therefore guard the probe too.
