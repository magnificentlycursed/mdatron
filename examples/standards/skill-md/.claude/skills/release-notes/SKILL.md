---
name: release-notes
description: Draft release notes from the merged pull requests since the last tag.
when_to_use: When the user asks for release notes, a changelog entry, or "what shipped".
argument-hint: "[since-tag]"
context: fork
agent: general-purpose
effort: medium
allowed-tools: Bash(git log:*) Bash(gh pr list:*) Read
---

# Release notes

Collect the merged pull requests since `$ARGUMENTS` (or the latest tag),
group them by label, and write one line per change in the imperative mood.
