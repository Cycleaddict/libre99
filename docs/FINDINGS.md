# Field findings — from heavy real-world toolchain use

Concrete defects and improvement opportunities in Libre99 surfaced by
sustained use of the toolchain (currently: a long-running annotation
campaign over the Tunnels of Doom decompilation — the standing worked
example of docs/GSL.md §13 — driving `libre99gsl`, `libre99gpl`, and
`libre99probe` hard for hours at a time). Each entry records the symptom,
the evidence, and a suggested direction, for later triage into
[ROADMAP.md](ROADMAP.md) / [KNOWN-ISSUES.md](KNOWN-ISSUES.md) work.
Entries are appended as they are found; nothing here blocks current use —
byte-identity and emulation held up throughout.

---

## F-001 · GSL decompiler misses function entries reached only via computed dispatch (2026-07-28)

**Symptom.** In dense GPL regions the decompiler emits too few `fn`
boundaries: real routines fall inside a neighboring function's span, or
inside `d_…` "data" blocks. In the ToD decompilation the combat region is
the worst case — routines at `>AE6D` (a to-hit computation), `>AFAE`
(target selection), `>AE9E` (roster drawing), `>B1F5` (a precondition
check) have no `fn` of their own, and the 632-byte block `d_827A` is
emitted as data while GROM fetch traces show it executing heavily (it is
the dungeon corridor digger).

**Why (hypothesis).** Entry discovery seeds from header vectors and
direct `CALL`/branch targets. Targets reached only through `CASE`
dispatch tables, computed branches, or call tables are never seeded, so
their code is swallowed by the preceding span or classified as data.

**Cost.** Byte-identity is unaffected (the payload round-trips), but
readability and annotation suffer badly: annotators had to re-derive
boundaries by hand (`libre99gsl compile --format grom`, then
`libre99gpl dis` seeded from probe-trace addresses).

**Suggested direction.**
1. Decode `CASE`/dispatch tables during entry discovery and harvest their
   targets (static fix — covers most of it).
2. Add **trace-assisted decompilation**: `libre99gsl decompile
   --entries FILE` accepting a list of known-executed GROM addresses
   (directly consumable from `libre99probe`'s `trace save` / `cover save`
   output) to seed additional entries. The recompile-identity gate stays
   unchanged, so the worst a bad seed can do is fail the gate.

**Root cause confirmed (2026-07-28, follow-up).** A full manual
re-disassembly of the worst 632-byte "data" block (boundaries seeded from
GROM fetch traces; all 122 trace-proven instruction starts landed on the
recovered listing's boundaries) pinned the tiler's failure mode: **GPL's
inline-argument idiom** — `CALL` to a routine whose first act is `FETCH`,
consuming operand bytes from the *caller's* instruction stream (the ToD
cartridge uses this seven distinct ways: RAND-range bounds, a mode byte, a
message id, a cell selector, a delay constant, …). The tiler, unaware the
callee consumes stream bytes, resumes decoding at the wrong offset, the
stream de-synchronizes, and whole regions get classified as data; a
secondary artefact is off-by-a-few function boundaries (a spurious leading
instruction absorbed from the neighboring stream). Concrete fix now
available: detect FETCH-at-entry callees statically, record "consumes N
inline bytes" per callee, and have the tiler skip operands at each call
site — combined with (1) this would have decoded the entire region. The
same idiom likely explains most `d_…` blocks that traces show executing.

**Two further confirmed instances (2026-07-29, same campaign).** (a) A
`CALL` whose callee fetches one inline message-id byte de-syncs the tiler
so thoroughly that the *following real instruction* — bytes
`B6 AF 11 1C 04`, i.e. `OR V@>111C,>04` — decompiles as the nonsense
statement `w_8311 /= 0x1C04`: not dropped, *mis-decoded*, which is worse
for an annotator because it reads as plausible code. (b) A function whose
first statement is `CALL >E00E` + inline byte is silently truncated after
that call (the tiler treats the inline byte as the start of the next
tile and gives up), hiding ~80 bytes of real logic that a GROM-trace
seeded re-disassembly recovered. Both were caught only because runtime
traces contradicted the listing — reinforcing both suggested directions
(decode dispatch tables; accept trace-seeded entries) and adding a third:
model the small set of known inline-argument callees (`>E00E` message
print being the most common) directly in the tiler.

## F-002 · Probe: no raw binary memory export (2026-07-28)

**Symptom.** Extracting large VRAM/CPU regions for offline analysis means
screen-scraping `vpeek`/`peek` hex output and reassembling bytes with
awk/xxd. It works, but it is error-prone (echoed `> vpeek …` command
lines match naive parsers' address regexes — this bit us once) and slow
for 16 KiB dumps.

**Suggested direction.** A `vsave FILE [ADDR LEN]` command (and `msave`
for CPU space, perhaps `gsave` for GROM) writing raw bytes. Deterministic
binary artifacts would make diff-based workflows (before/after a game
action) one command each, complementing `shot`/`save`/`trace save`.

## F-003 · Verify: should the console ISR perturb the GPL RAND seed? (2026-07-28)

**Symptom.** Under libre99, GPL `RAND` streams are identical from cold
boot regardless of elapsed idle time: two otherwise-identical probe
scripts that idle 7 vs 601 extra frames before the same keystroke produce
byte-identical random outcomes. The seed word (`>83C0`) evidently
advances only on `RAND` calls in our clean-room firmware.

**Question to resolve.** Does the authentic console ISR (or KSCAN) stir
`>83C0` over time? If yes, our firmware has a fidelity gap: on real
hardware random outcomes would vary with wall-clock timing, while under
libre99 they cannot. Check Classic99's ISR/`RAND` handling and the
authentic ROM disassembly; if confirmed, add the stir to the clean-room
ISR with a focused regression test (and note the determinism change in
docs/PROBE.md, since replayable scripts currently rely on it — an
opt-out may be warranted for the probe).

**Note.** Deterministic replay is genuinely useful for testing; if the
stir is authentic, consider making the probe default to a fixed seed with
a `--stir-clock` (or `seed N`) escape hatch so session scripts stay
reproducible.
