---
schema_class: review-entry
schema_version: 1.0.0
review_number: 16
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session Privacy review — Phase 2
  surfaces from the data-flow + telemetry perspective: explain command's
  data handling, integration test fixtures, README's external-link
  posture, no-telemetry-by-construction discipline.
lens: >-
  Privacy (PII handling + telemetry + retention + cross-system data
  flow + operator-data-sovereignty).
source: director-raised
session_note: >-
  Cold-session reviewer mode. Honest scope-dismissal pattern: mdatron is
  local-only by construction. Checking whether Phase 2 introduces any
  telemetry or external data flow.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Privacy easy-dismiss ("no network calls") masks the honest discussion
  of "is operator content exposed to third parties through file paths,
  example fixtures, error reflection?"
---

# Findings

## F1 — Domain-wide honest scope-dismissal: mdatron is local-only (Edge-cases)

**Classification:** dismissed-domain-wide
**Routing:** Privacy-internal observation.

Phase 2 does not introduce:
- Network calls (no telemetry; no analytics; no remote crash reporting)
- Persistent storage of operator content (mdatron is a verify-and-exit
  process; no caching to disk in v0.1.0)
- Logging of operator-content to external destinations
- Crash dumps with content payloads

`mdatron explain CODE` reads from `include_str!` embedded prose; reflects
the operator's argv into error messages on the not-found path; does not
write or transmit anything. `mdatron verify --json` writes its output to
stdout (operator-controlled destination) and stderr (operator-controlled
destination).

## F2 — Argv reflection in `cmd_explain` not-found path is bounded to operator's terminal (Consistency)

**Classification:** dismissed
**Routing:** Privacy-internal observation.

When the operator runs `mdatron explain $WHATEVER`, the value is reflected
into stderr. The stderr stream goes to the operator's terminal (or
operator-chosen destination via shell redirection). No third-party
exposure unless the operator pipes stderr to a third-party (e.g., a CI
log uploader, a chat-bot integration).

For the third-party-pipe case: the operator controls the redirect. mdatron
doesn't introduce a new third-party data flow. Pass.

Cross-reference to Security F1 / Red Team F2: argv reflection has a
control-char-injection concern at the security layer; doesn't carry
privacy concern.

## F3 — README references the operator's project files in examples (Consistency)

**Classification:** dismissed
**Routing:** Privacy-internal observation.

README "First run" + "Schema example" + "Pattern example" sections walk
through hypothetical files (`blog.json`, `post.md`). No real-operator
data; synthetic examples only. The integration test's round-trip uses
the README's own example; no real data captured.

No PII leakage paths. Pass.

## F4 — External URLs in README are operator-trusted destinations (Consistency)

**Classification:** dismissed
**Routing:** Privacy-internal observation.

URLs in scope:
- `https://gist.github.com/dollspace-gay/...` (operator-trusted; the
  methodology author's own gist)
- `https://github.com/magnificentlycursed/...` (operator's own hosting
  pending Phase 6 publish)

Both are operator-trusted; clicking a link in the README doesn't expose
operator-private data. The links are documentation pointers.

## F5 — No findings at this layer (Domain-wide)

**Classification:** dismissed-domain-wide
**Routing:** Privacy-internal closeout.

Substantive privacy-domain findings for Phase 2: zero. The no-telemetry
+ no-persistent-storage + no-third-party-flow discipline is intact.

If a future Phase introduces a telemetry surface (e.g., crash reporting
via the Claude Agent SDK's OpenTelemetry export per DESIGN-OBSERVABILITY),
privacy review would land then — not now. The methodology-spec discipline
(credential-shaped fields structurally rejected; OTel collector redacts;
event-variant schemas exclude credential-shaped fields) is the discipline
that handles telemetry when it arrives.

# Summary

5 findings: 0 resolved-pending, 0 accepted-with-remediation, 5 dismissed
(all). The domain-wide dismissal carries substantive rationale; mdatron's
local-only discipline is unbroken by Phase 2. No hallucinations.
