# Observatory benchmark

## Question

Does runtime evidence materially improve reconstruction quality over static
disassembly/decompilation alone?

The benchmark is designed to answer that question, not to demonstrate that
the emulator can merely run a game.

## Parsec: held-out benchmark

The published [Parsec source-code PDF](https://oratronik.de/atariage/Parsec_Source_Code.pdf)
is the held-out oracle. It must not be shown to the analysis seat until the
blind result is frozen.

Before using it as an oracle, establish the exact relationship between the
listing and the owner-local Parsec binaries. Reassemble when practical and
record address/byte correspondence. A listing from another revision cannot
silently score a binary.

### Pre-register one bounded target

Choose a visible, repeatable behavior such as one scrolling/rendering update.
Record before analysis:

- the input sequence and checkpoint;
- the observed output/state change;
- the address, frame, or event window under study;
- which artifacts the analysis may see; and
- which listing pages/symbols remain hidden.

### Two passes

1. **Static baseline:** use only the binary and existing static tool output.
2. **Observatory pass:** add replayable runtime evidence and the new causal
   queries, without revealing the source listing.

Freeze both reports before opening the oracle.

### Evaluate concrete claims

For the selected behavior, compare:

- instruction and operand decoding;
- routine boundaries and control flow;
- native/GPL transitions where present;
- state variables and data structures;
- memory and VDP effects attributable to the routine;
- the causal explanation of the visible behavior;
- behavior of any reconstructed routine on held-out states;
- unsupported or incorrectly confident claims; and
- human/agent time needed to reach the result.

Report each measure separately. Do not hide tradeoffs in a single composite
score. The MVP passes only if runtime evidence yields a substantial, specific
gain in causal explanation or executable reconstruction over the static pass.

## Tunnels of Doom: generalization test

After the Parsec method works, apply it to one bounded Tunnels of Doom
subsystem. Candidate scope should be selected from a real execution trace and
may include one map representation or one pass of a map transformation.

This is not a claim to recover the complete map generator, combat model, or
30K-scale GPL engine. The test asks whether the same observatory can:

- cross GPL/native and data boundaries without losing execution context;
- expose compact state structures and repeated passes;
- distinguish observation from inference;
- replay a reconstruction against multiple saved states; and
- improve on the existing GSL/probe annotation result.

Libre99's `docs/FINDINGS.md` is prior field evidence, not an oracle to ignore:
`F-001` already records trace-proven code that static tiling misclassified.
The MVP should repair or explicitly bound that failure before making broad
decompilation claims.

## Calibration and failure interpretation

Repository-owned programs with source can calibrate the capture and analysis
plumbing, but they are not substitutes for the held-out game benchmark.

A failed benchmark can mean different things and must say which:

- emulator behavior prevented an authentic run;
- the trace omitted the causal information;
- indexing/querying could not isolate it;
- static recovery misdecoded the executed bytes;
- the AI made unsupported inferences; or
- the oracle did not correspond to the tested binary.

Fix the measured bottleneck. Do not answer failure with an unrelated
architecture or verification layer.
