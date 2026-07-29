# saphyr

**Status:** Approved.

**Pinned version:** `^0.0.11`

## Why this dependency

Position-tracking YAML parse, used ONLY on the schema-violation error path to
resolve a JSON pointer to a precise source line (`MDATRON-E0050` locations,
tracker #65). The happy path uses `serde_yaml_ng`; saphyr runs only when a
finding already exists and a `file:line` is being computed.

**Alternatives considered:**

- Line-counting the raw text: rejected — cannot map a JSON pointer through
  YAML structure (anchors, flow vs block, multi-line scalars) reliably.
- `serde-saphyr` (span-capable serde front end): the eventual convergence
  target (alpha); tracked for a future cycle (#65). saphyr is the lower-level
  piece available today.

## PE supply-chain notes

- **Version pin:** `saphyr = "0.0.11"` (pre-1.0; the error-path-only scope
  bounds the blast radius of an API break).
- **`cargo audit`:** clean at pin time.

## Security notes

- **License:** MIT OR Apache-2.0; compatible.
- **Threat model:** parses adopter YAML on the error path. A panic in the
  pre-1.0 parser is contained: the E0050 resolver runs it inside
  `catch_unwind` and degrades to the block-start line rather than aborting the
  run (tracker #72).

## SO approval

- **Scope justification:** error-path-only, single-purpose (pointer -> line);
  the agent-first diagnostic value (a precise `file:line`) justifies a pre-1.0
  dep held to a narrow surface.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
