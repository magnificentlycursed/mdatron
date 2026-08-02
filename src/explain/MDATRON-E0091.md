# MDATRON-E0091 — invented-label-scheme

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A letter-plus-number cluster in governed prose matches no allowed label scheme. The letter-cluster incident is the evidence: ad-hoc schemes (SEC-F3, AIE-F2, M4) recurred past written correction until nothing mechanical policed them. The engine detects the cluster shape; the adopter's allowlist says which schemes are sanctioned.

Structured **reference-IDs** under long-standing spec conventions — `REQ-<n>`, `AC-<n>`, `ADR-<n>`, `RFC-<n>`, `Q<n>`/`Q-<n>` — are exempt **by default** (#159): those IDs *reference* numbered spec items rather than coin a label scheme, so the engine's default allow-set covers them out of the box (unioned with your `label_schemes.allow` whenever the cluster scan is active). This flag therefore fires on a cluster that is neither a sanctioned local scheme *nor* one of those standard reference-ID forms.

## How to fix

- **A sanctioned local scheme** (e.g. a project's `C<n>` constraint IDs, outside the conservative default set): add its regex to `label_schemes.allow` in `.mdatron/vocabulary.yaml` (e.g. `^C\d+$`). Your patterns union with the engine defaults, so you list only what the defaults don't already cover.
- **A genuine ad-hoc coinage**: rewrite the prose to use a sanctioned scheme, or register the term.
