# MDATRON-E0091 — invented-label-scheme

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A letter-plus-number cluster in governed prose matches no allowed label scheme. The letter-cluster incident is the evidence: ad-hoc schemes (SEC-F3, AIE-F2, M4) recurred past written correction until nothing mechanical policed them. The engine detects the cluster shape; the adopter's allowlist says which schemes are sanctioned.

## How to fix

Add the scheme's regex to label_schemes.allow in .mdatron/vocabulary.yaml if it is sanctioned, or rewrite the prose to use a sanctioned scheme.
