# glob

**Status:** Approved.

**Pinned version:** `^0.3`

## Why this dependency

Cross-file index resolution (mdatron-core::dsl::index) compiles `keys:` block
declarations like:

```yaml
keys:
  - name: domain-prompts
    source: .claude/commands/vsdd-domain-*.md
    select: $.frontmatter
    indexed_by: $.domain_slug
```

The `source` field is either a single file path or a glob pattern; the glob crate
expands the pattern into a list of matching file paths under the project root.

## Alternatives considered

- `walkdir` + custom matcher: rejected — re-implements ~150 LoC of glob semantics
- `globset` (newer): rejected — designed for matching a set of globs against a path
  stream; our use case is the inverse (expand one glob to its matching files)
- Hand-rolled: rejected — globs with `**` recursion are subtle; standard impl is safer

## PE supply-chain notes

- Workspace dep: `glob = "0.3"`; resolves to 0.3.x
- Maintainer: rust-lang-nursery; effectively part of the Rust ecosystem governance
- Zero transitive deps
- License: MIT OR Apache-2.0; compatible with mdatron's MIT
- `cargo audit`: no known CVEs

## Security notes

- Glob patterns operate on filesystem paths; path-confinement discipline
  (BOUNDARY-PREAMBLE § 7) is enforced by the index module: every resolved path is
  validated under the project root before being read. PR-injected glob patterns
  that escape the project root return `MDATRON-E0011: key-source-parent-traversal`.

## Attribution

Single-agent claude-opus-4-7 investigation per the attribution-honesty discipline
(vsdd-cli crosslink issue #6).
