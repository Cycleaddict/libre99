# GSL — the GPL Structured Language

GSL is this project's high-level language for **GPL** (the TI-99/4A's Graphics
Programming Language bytecode). One `.gsl` source file describes a complete
GROM image or cartridge; the toolchain lives in
[`crates/libre99-gsl`](../crates/libre99-gsl) and works in both directions:

* **`libre99gsl compile`** — `.gsl` → GPL bytecode, packaged as a raw GROM
  image (`.bin`) or a `ti99sim` `.ctg` cartridge. The compiler does not encode
  bytes itself: it lowers GSL to **`libre99gpl` assembler source** and runs the
  real assembler ([ASSEMBLER.md](ASSEMBLER.md) documents the shared front end),
  so inline `asm { }` blocks are aligned with the standalone assembler by
  construction.
* **`libre99gsl decompile`** — a real `.ctg` or `.bin` → readable `.gsl` with
  functions, statements, `data { }` blocks, and metadata comments. The
  decompiler **verifies its own output**: it recompiles the generated GSL and
  requires the result to be byte-identical to the input payload before it will
  write the file (§10).

GSL is deliberately *not* a general-purpose language. It is an honest surface
for the GPL machine: memory-to-memory statements over three address spaces, a
single condition bit, no register file, no stack frames, no heap. Anything the
machine can express that GSL cannot is written (or decompiled) as inline
assembly or data — a `.gsl` file can therefore represent **any** GROM byte
stream, which is what makes decompile → edit → recompile safe.

---

## 1. Quick start

```gsl
// beep.gsl — a minimal GPL cartridge: standard header + a key-wait beep loop.
format ctg;
cartridge "BEEP";

origin 0x6000;
data header {
    0xAA, 0x01, 0x01, 0x00,      // >AA valid, version, programs, reserved
    word 0x0000,                 // power-up list: none
    word 0x6010,                 // program list
    word 0x0000, word 0x0000,    // DSR list, subprogram list
    word 0x0000, 0x00, 0x00,     // interrupt list; pad to the node at >6010
    word 0x0000, word 0x6021,    // list node: next = none, entry = main
    0x04, "BEEP",                // name length, name
}

var key:   byte @ cpu[0x8375];   // KSCAN result (>FF = no key)
var sndp:  word @ cpu[0x83CC];   // ISR sound-list pointer
var sndgo: byte @ cpu[0x83CE];   // ISR sound trigger

fn main() @ 0x6021 {
    all(0x20);                   // clear the screen to spaces
    back(0x17);
loop:
    scan();
    if (key == 0xFF) goto loop;  // wait for a key
    sndp = tone;                 // arm the ISR sound list (a bare data name
    sndgo = 0x01;                // is an immediate — the block's address)
    goto loop;
}

data tone @ 0x6040 {             // [n bytes][sound bytes][frames], 0 ends
    0x02, 0x8E, 0x0F, 0x04,
    0x01, 0x9F, 0x00,
}
```

```sh
cargo run -p libre99-gsl -- compile beep.gsl -o beep.ctg
cargo run -p libre99-gsl -- decompile beep.ctg -o beep-back.gsl
cargo run -p libre99-gsl -- roundtrip beep.ctg        # decompile+recompile+diff
```

---

## 2. Lexical structure

* **Comments** — `// line comment` and `/* block comment */` (non-nesting),
  allowed anywhere between tokens. Inside `asm { }` bodies the *assembler's*
  comment rules apply instead (`*` in column 1, trailing `;`).
* **Numbers** — `0x6A3F` hex, `123` decimal, `'A'` character (its ASCII code).
  The assembler's `>6A3F` form is **not** GSL syntax (it appears only inside
  `asm { }` bodies).
* **Identifiers** — `[A-Za-z_][A-Za-z0-9_]*`, case-sensitive. Keywords
  (`fn`, `var`, `data`, `goto`, `move`, …) are reserved. Identifiers beginning
  with `_G` are reserved for compiler-generated labels.
* Declarations and statements end with `;`; blocks use `{ }`.

**Constant expressions** (in `const`, addresses, immediates) support
`+ - * / % & | ^ << >> ~ ( )` with C precedence, evaluated by the compiler in
64-bit arithmetic and range-checked where used. They may reference `const`
names, not vars or labels.

---

## 3. Program structure and placement

A `.gsl` file is a flat, ordered list of top-level items — **monolithic by
design**; there is no import mechanism. Items are placed at a running
**location counter** in the 64 KiB GROM address space:

| Item | Meaning |
|---|---|
| `format ctg;` \| `format grom;` \| `format grom24;` \| `format rombin;` | Default output container (§9). |
| `cartridge "TITLE";` | `.ctg` banner title. |
| `cru 0x0000;` | `.ctg` CRU base (default 0). |
| `origin 0x6000;` | Set the location counter. |
| `grompage 0xA000;` | Declare a GROM page as present even if nothing is placed there (§9). |
| `const NAME = expr;` | Named constant. |
| `var NAME: byte @ cpu[0x8350];` | Variable binding (§4). Emits no bytes. |
| `fn NAME() [@ addr] { … }` | A function (§5). |
| `data NAME [@ addr] { … }` | Raw bytes (§8). |
| `asm { … }` | Verbatim assembler lines (§7). |
| `rom N { … }` | 8 KiB TMS9900 ROM bank `N` for `.ctg` output — raw bytes only, zero-padded to 8 KiB; GSL does not compile CPU code. |

`fn`/`data` accept an **address pin** `@ addr`. A pin moves the location
counter *forward* to `addr` (backward is an error); the gap is **zero-filled**.
Unpinned items are laid out contiguously. Everything not covered by an item
assembles as `0x00` — which is why the decompiler can elide all-zero regions.

Functions, variables, constants, labels, and data names share **one flat,
file-wide namespace** (they become symbols of the underlying assembly; inline
`asm` labels live in the same namespace and can be referenced from GSL and
vice versa).

---

## 4. Types, variables, and places

GPL operates on **bytes** and big-endian **words** in two writable address
spaces — CPU RAM (`cpu`, which is the `>8300` scratchpad plus any expansion
RAM) and VDP RAM (`vdp`) — plus read-only GROM (`grom`). A *place* is anything
a GPL instruction can address (the hardware calls the encoding GAS):

| GSL place | GPL operand | Meaning |
|---|---|---|
| `name` | `@>8350` / `V@>0300` | A declared var (its space/address/width). |
| `cpu[0x8350]` / `vdp[0x0300]` | `@>8350` / `V@>0300` | Anonymous direct reference. |
| `*p`, `*cpu[0x8372]` | `*@>8372` | CPU-indirect: the word at the CPU cell is the effective CPU address. |
| `vdp[*p]`, `vdp[*0x8356]` | `*V@>8356` | VDP-indirect: the word at the **CPU** cell is a VDP address. |
| `place(ix)` | `@>8300(@>83E0)` | Indexed: the byte at CPU cell `ix` (a `>83xx` cell) is added to the address. |
| `byte(place)` / `word(place)` | — | Width cast (§4.1). |
| `grom[0x1600]`, `grom[*p]` | `G@>1600`, `G*@p` | GROM source (in `move` only): immediate address, or address held in CPU cell `p`. |
| `gram[0x9800]` | — | GRAM destination (in `move` only; no assembler mnemonic — compiled via the byte path). |
| `vreg(1)` | `#1` | Starting VDP register (in `move` only). |

Variables bind a name to a space, address, and width:

```gsl
var cnt:  byte @ cpu[0x8350];
var wp:   word @ cpu[0x8346];
var line: byte @ vdp[0x02E0];
```

Two vars may overlap the same address (e.g. a `byte` and a `word` view of one
cell) — aliasing is idiomatic GPL and explicitly allowed. Index cells (`ix`
above) must lie in `0x8300–0x83FF`; indexed and `gram`/`grom`-computed places
require compile-time-known addresses (a var or constant, not a forward label).

**Bare names.** In an operand position, a bare name that is a declared var is
a *memory* operand (its contents); any other bare name — a `const`, or a
function/label/data symbol — is an *immediate* (the const's value, or the
symbol's address, resolved by the assembler): `sndp = tone;` loads the data
block's address. Spell `cpu[name]`/`vdp[name]` to force a memory
interpretation of a constant address.

### 4.1 Operation width

Every statement compiles to a byte op or a word op (GPL's `W` bit — `ST` vs
`DST`, `INC` vs `DINC` …). The width is taken from its operands: a declared
var fixes it; `byte(…)`/`word(…)` casts fix it; a bare immediate does not.
Mixed widths are a compile error (make an aliased var or cast). When nothing
claims a width the operation is a **byte** op, so a word op through only
anonymous or indirect places needs a cast: `word(*p) = 0x0080;`.

---

## 5. Functions, control flow, and the condition bit

`fn name() { … }` places a label and its statements. GPL has no frames or
parameters — `CALL` pushes a return address on the interpreter's internal
stack, `RTN` pops it; arguments travel through scratchpad cells by convention.
Accordingly:

* `name();` compiles to `CALL name`; `call(expr);` calls a numeric address.
* `return;` is `RTN`; `returnc;` is `RTNC` (returns preserving the condition
  bit).
* **Control falls off the end of a function into whatever follows.** The
  compiler does not insert an implicit `return` (real GPL firmware relies on
  fall-through), so end a function explicitly unless you mean it.
* `goto target;` is `B target` (16-bit absolute). Targets are labels, `fn`
  names, or constant addresses.
* `label:` prefixes any statement (or stands alone at the end of a block).

The machine has **one condition bit**, written by compares/tests and consumed
by the short conditional branches `BS` (branch if set) / `BR` (branch if
reset). GSL spells the fused forms:

```gsl
if (cnt == 0x09) goto done;      // CEQ cnt,>09  then BS done
if (cnt != 0x09) goto more;      // CEQ cnt,>09  then BR more
```

| Condition | GPL op | branches on |
|---|---|---|
| `a == b` / `a != b` | `CEQ` | set / reset |
| `a == 0` / `a != 0` | `CZ` (bare `0` only — §6.1) | set / reset |
| `a > b` / `a <= b` | `CGT` (signed) | set / reset |
| `a >= b` / `a < b` | `CGE` (signed) | set / reset |
| `a h> b` / `a h<= b` | `CH` (unsigned "high") | set / reset |
| `a h>= b` / `a h< b` | `CHE` | set / reset |
| `(a & b) == 0` / `(a & b) != 0` | `CLOG` (sets the bit when `a AND b = 0`) | set / reset |
| `cond()` / `!cond()` | — (bare `BS` / `BR`) | current condition bit |
| `carry()`, `ovf()`, `gt()`, `h()` (negatable) | `CARRY`/`OVF`/`GT`/`H` | status-bit loads |

Notes:

* `BS`/`BR` reach only within the current 8 KiB GROM slot (a hardware
  property); the assembler rejects cross-slot conditional gotos. `goto` (`B`)
  reaches anywhere.
* A compare may stand alone — `test(a > b);` emits just the `CGT`; a later
  `if (cond()) goto l;` consumes it. `test(gt());` etc. emit the bare status
  ops. This is how the decompiler renders non-adjacent compare/branch pairs.
* `if (…) { … } else { … }` and `while (…) { … }` are compiler-only sugar over
  the same conditions (the decompiler emits flat `if … goto`). `case(x);`
  is GPL `CASE`: it skips `2·x` bytes — by convention a table of 2-byte `BR`
  instructions follows.

---

## 6. Statements — the opcode mapping

Each GPL opcode family has exactly one GSL spelling, so source recompiles to
the bytes it was decompiled from. `P`, `Q` are places; `e` a constant
expression; word forms are chosen by operand width (§4.1).

| GSL | GPL | | GSL | GPL |
|---|---|---|---|---|
| `P = Q;` | `ST`/`DST` | | `P++;` | `INC` |
| `P = 0;` | `CLR` (§6.1) | | `P--;` | `DEC` |
| `P += Q;` | `ADD` | | `inct(P);` | `INCT` (P += 2) |
| `P -= Q;` | `SUB` | | `dect(P);` | `DECT` (P -= 2) |
| `P *= Q;` | `MUL` | | `abs(P);` | `ABS` |
| `P /= Q;` | `DIV` | | `neg(P);` | `NEG` (P = -P) |
| `P &= Q;` | `AND` | | `inv(P);` | `INV` (P = ~P) |
| `P \|= Q;` | `OR` | | `swap(P, Q);` | `EX` |
| `P ^= Q;` | `XOR` | | `push(P);` | `PUSH` |
| `P <<= Q;` | `SLL` | | `fetch(P);` | `FETCH` (next inline byte at the caller) |
| `P >>= Q;` | `SRA` (arithmetic) | | `case(P);` | `CASE` |
| `P >>>= Q;` | `SRL` (logical) | | `rotr(P, Q);` | `SRC` (rotate right) |

Intrinsic statements for the named opcodes:

| GSL | GPL | effect |
|---|---|---|
| `scan();` | `SCAN` | keyboard scan (result in `>8375` etc.) |
| `back(e);` | `BACK` | border/backdrop color |
| `all(e);` | `ALL` | fill the screen with character `e` |
| `rand(e);` | `RAND` | random number into `>8378`, max `e` |
| `xml(e);` | `XML` | escape to a machine-code routine (ROM table) |
| `parse(e);` | `PARSE` | BASIC parser assist |
| `exit();` | `EXIT` | software reset to the master title screen |
| `cont();` `exec();` `rtnb();` `rtgr();` | `CONT`/`EXEC`/`RTNB`/`RTGR` | interpreter/BASIC linkage |

**Block move** — `move(dst, src, count);` (memcpy argument order; the
assembler surface is `MOVE count,src,dst`):

* `dst`: a place, `vreg(e)` (VDP registers upward from `e`), or `gram[e]`.
* `src`: a place, `grom[e]`, or `grom[*P]` (GROM address held in CPU cell).
* `count`: a constant expression (immediate) or a place (count read at run
  time).

```gsl
move(vdp[0x0022], grom[LOGO], 5);      // GROM -> screen
move(vreg(0), grom[0x0451], 8);        // GROM -> VDP registers 0-7
move(vdp[*sp], name_buf, len);         // CPU -> VDP, runtime count
```

### 6.1 Canonical spellings (byte determinism)

A few GPL operations overlap in meaning; GSL keeps them distinct so that
compilation is deterministic, byte for byte:

* The bare literal `0` is special: `P = 0;` emits `CLR`, `== 0`/`!= 0` emit
  `CZ`. Any other zero spelling (`0x00`, `0x0000`) emits the general
  `ST`/`CEQ` immediate forms.
* `P++;` (`INC`) and `P += 1;` (`ADD` with immediate 1) are different
  instructions; likewise `inct(P);` vs `P += 2;`.
* `<=`, `<`, `h<=`, `h<` and `!=` do not have their own opcodes — they are the
  reset-branch (`BR`) readings of `CGT`/`CGE`/`CH`/`CHE`/`CEQ`.

The decompiler always emits the spelling that regenerates the original opcode,
and falls back to raw bytes for anything non-canonical (§10).

---

## 7. Inline assembly

```gsl
asm {
* comment lines and blank lines are fine (assembler syntax)
PUCALL  DST  @>8372,>0080
        DST  *@>8372,PUDONE
        XML  >1A
}
```

The block's lines are spliced **verbatim** into the generated assembly and
processed by the `libre99gpl` assembler — same mnemonics, directives (`BYTE`,
`DATA`, `TEXT`, `BSS`, `EVEN`, `GROM`/`AORG`, `EQU`), expressions (`>` hex,
strict left-to-right arithmetic), and restrictions (indexed operand *syntax*
is rejected by the assembler; GSL statements compile indexed places through
the byte path instead). Labels defined in `asm` blocks are ordinary symbols:
GSL `goto`/`call` can target them, and asm operands can name GSL functions,
data blocks, and vars (vars are emitted as `EQU`s). Instruction statements and
`asm` blocks may be interleaved freely inside a `fn`; top-level `asm` blocks
are allowed between items.

---

## 8. Data blocks

```gsl
data font @ 0x1000 {
    0x00, 0x38, 0x44, 0x44, 0x7C, 0x44, 0x44, 0x00,   // 'A'
    word 0x1234,          // one big-endian word
    "PRESS",              // ASCII bytes
    'Q',
}
```

Items are bytes (constant expressions, range-checked), `word e` pairs, ASCII
strings, and characters, separated by commas (a trailing comma is fine).
The block's name is its address symbol — usable as a `goto`/`move` target or
in constant expressions via `const`. Interleave `data` between functions to
mirror real GROM layouts (headers, sound lists, name tables).

---

## 9. Containers and output formats

| `format` | Output | Contents |
|---|---|---|
| `ctg` | `ti99sim` V1 `.ctg` | GROM pages touched by the program (plus `grompage` declarations), `rom N { }` banks, `cartridge` title, `cru` base. The RLE framing is normalized, so a rebuilt `.ctg` matches the original **payload-for-payload** (title, CRU, every bank/page byte) rather than file-byte-for-file-byte. |
| `grom` | raw `.bin` | The GROM space from the first touched page to the last, zero-filled between. |
| `grom24` | raw 24 KiB `.bin` | Exactly `>0000–5FFF` — the console-GROM layout (`994AGROM.Bin` replacement). |
| `rombin` | raw `.bin` | The `rom N { }` banks concatenated — a loose CPU-ROM dump. |

The CLI `--format` flag overrides the declaration. GROM pages are 8 KiB
(`>0000`, `>2000`, … `>E000`); cartridge software normally lives at `>6000`
upward, the console OS at `>0000–5FFF`.

---

## 10. The decompiler

`libre99gsl decompile` turns a `.ctg` or `.bin` into GSL:

1. **Container** — `.ctg` banner → pages/banks/title/CRU. A bare `.bin`
   starting with `>AA` is a GROM dump (24 KiB exactly → console at `>0000`;
   otherwise based at `>6000`; override with `--base`), or a CPU-ROM dump with
   `--rom`.
2. **Discovery** — standard `>AA` GROM headers on every page yield power-up,
   program, DSR, subprogram and interrupt entries (with names); on console
   images `>0020` is the boot entry. Code is traced from these roots through
   `B`/`BR`/`BS`/`CALL` targets and fall-through, with a grammar-exact `FMT`
   scanner (nested `RPTB`, GAS string operands, `FEND` loopback addresses —
   per `rom/RECON.md` §7).
3. **Emission** — traced instructions become statements (§6); everything else
   becomes `data` (all-zero spans are elided). `CALL` targets and header
   entries become `fn`s — named from the header program names (`prog_*`) or
   their address (`sub_6123`); branch targets become labels (`L_6134`).
   Scratchpad/VDP cells become vars (`b_833F`, `w_8340`, `vb_0300`, …).
4. **Semantic annotation** — advisory analysis layered on top (names and
   comments only; the addresses in every declaration remain the ground truth):
   - vars at documented machine cells take their standard names —
     `key_code` (`>8375`), `kscan_mode`, `joy_x`/`joy_y`, `gpl_status`,
     `vdp_timer`, `snd_list_ptr`, `rand_seed`, `gpl_r0`…`gpl_r15`, … — with
     the cell's meaning as the declaration comment (sourced from the recon
     dossiers under `original-content/system-roms/`);
   - VDP vars are annotated with the table they land in (screen row/col,
     sprite attribute list, color table, pattern-table char), using the
     console-default layout refined by any literal VDP-register loads the
     code performs;
   - each `fn` gets a header with its observed effects (formats screen text,
     writes VDP/GRAM, reads keyboard/joystick, drives sound, uses random,
     XML escapes) plus `calls:` / `called from:` lists by name; when a
     neutral `sub_` function shows **exactly one** kind of effect it is
     renamed `draw_XXXX` / `key_XXXX` / `snd_XXXX` (the address stays in the
     name);
   - statements get trailing notes where the machine contract says more than
     the spelling: `xml(n)` names the console ROM routine or vector it
     dispatches to, `move(vreg(n), …)` names the VDP register and shows the
     literal value loaded, sound-list stores to `>83CC` are flagged, the
     `>D0` sprite-list terminator is called out, branches back to a `scan()`
     are marked as scan loops, and literal `move` sources show `d_XXXX+off`
     plus an ASCII preview when the bytes are text;
   - `data` blocks decode what the code told us about them: printable runs
     become string literals, bytes uploaded to a pattern table render as
     8×8 pixel-art comment bands above their rows, sound lists get a
     block/duration summary, and each chunk lists the functions that `move`
     from it.
5. **Verification** — every decoded instruction is re-encoded and compared
   against the original bytes; any mismatch (non-canonical encodings, opcodes
   without GSL spellings such as `IO`/`COINC`/`SWGR`/`XGPL`, `FMT` blocks,
   GRAM moves) is emitted as raw `asm { BYTE … }` / `data` with a comment
   naming the decoded form. Finally the **whole generated file is compiled
   and byte-compared** against the input payload; the `.gsl` is written only
   if it reproduces the input exactly, and its header comment records the
   verdict and coverage statistics.

The result: **decompiled output is functionally equivalent by construction**
(identical bytes ⇒ identical behavior), regardless of how much of the image
the tracer understood. Discovery quality affects only *readability* — how much
appears as statements rather than data. Metadata comments carry the
decompilation record: input provenance, per-function origin (which header,
which callers), data cross-references from `move` sources, ASCII gutters, and
decoded `FMT` listings.

Limits worth knowing: bytes consumed via `FETCH` after a `CALL` are untracked
(they tile as code — harmless, occasionally ugly); banked/duplicate GROM pages
in a `.ctg` are rejected; TMS9900 `rom` banks are carried as data, never
decompiled (GSL is a GPL tool — see `libre99asm`'s disassembler for CPU code).

---

## 11. CLI reference

```text
libre99gsl compile   <in.gsl> -o <out>   [--format ctg|grom|grom24|rombin]
libre99gsl decompile <in.ctg|in.bin> -o <out.gsl>
                     [--base 0xNNNN]   GROM base for headerless .bin dumps
                     [--rom]           treat a .bin as a CPU-ROM dump
libre99gsl roundtrip <in.ctg|in.bin> [--keep <out.gsl>]
```

`roundtrip` decompiles, recompiles, and byte-compares in memory, printing a
per-page/per-bank verdict — the same check the test suite runs over the
committed images and the local cartridge corpus (`third-party/cartridges/`,
via `libre99-core`'s `third_party` gate).

---

## 12. Non-goals (v1)

* No expressions over multiple operations (`a = b + c;`) — GPL is two-address;
  write two statements. No `for`, `break`, `continue`, functions with
  parameters, or recursion (the machine has none of these).
* No floating point, strings-as-values, or BASIC linkage beyond the raw
  `parse`/`cont`/`exec`/`rtnb` opcodes.
* No `FMT` statement syntax — format blocks round-trip as raw bytes with
  decoded comments. A structured `fmt { }` sub-language is a natural v2 item.
* The compiler never *chooses* between equivalent encodings: what you write is
  the opcode you get (§6.1). Optimization is the author's job.
