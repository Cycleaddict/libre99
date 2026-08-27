# Legacy evidence boundary

The previous experiment is preserved separately, closed at commit
`052664a2511c66be2651cb8dc6e3b178e0ed8c75` (`process: close TI99-D01`). It is
historical evidence, not this project's build foundation or process. Its host
location is intentionally not part of the public project record.

## Consult selectively

Useful material may include:

- primary-source copies and source-attributed TI research;
- focused hardware behavior tests and authentic boot observations;
- reference-emulator checkouts under `ref emulators/`;
- accepted CPU, VDP, GROM, TMS9901, timing, interrupt, and keyboard findings;
  and
- the D01 feasibility result when comparing direct-runtime behavior.

Verify every imported factual claim against its cited source and the current
Libre99 behavior. Preserve its evidence label. Copy no reference-emulator code
without a separate license review.

## Do not import

- the breadboard stage plan or `ACTIVE-STAGE` workflow;
- relay/autonomous-lane scripts or agent-security mechanisms;
- generic participant facets, checkout/loan protocols, phase buses, or signal
  nets merely because they exist there;
- per-event SHA-256, schema, document, journal, or provenance machinery;
- old projected stage counts or gates; or
- an implementation whose only justification is that the old process accepted
  it.

If a legacy test exposes a real software-visible mismatch, reproduce the
behavior in the smallest Libre99-owned regression and fix it here. Otherwise,
leave the legacy repository untouched.
