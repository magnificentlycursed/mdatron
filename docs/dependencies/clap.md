# clap

**Status:** Approved.

**Pinned version:** `^4.5` (workspace.dependencies with `["derive"]` feature; resolves to 4.5.x or 4.6.x latest)

## Why this dependency

CLI argument parsing for the `mdatron` binary subcommand surface (`verify`, `explain`, future `init` / `registry` / `schema` / `skill-preamble` / `verify hook` / `verify test-error-catalog` / `lsp-serve` / `mcp-serve`). Per DESIGN-MDATRON § CLI surface, the subcommand structure with `#[derive(Subcommand)]` lands on clap.

**Alternatives considered:**

- `argh`: rejected — Google-stewarded; smaller community; less feature-coverage for derive-based subcommand structures
- `lexopt` / hand-rolled parsing: rejected — would re-implement ~200 LoC of subcommand dispatch + help-text generation; clap is the de-facto standard
- `bpaf`: interesting alternative; rejected for v0.1.0 — smaller adoption; not a v0.1.0 blocker but reconsider at v1.0

## PE supply-chain notes

- **Version pin discipline:** workspace-level `clap = { version = "4.5", features = ["derive"] }`; resolves to 4.6.1 at pin time.
- **Maintainer trust:** clap-rs/clap GitHub org; multiple active maintainers; ~25k reverse deps.
- **Transitive deps:** brings `anstream` + `anstyle` (terminal-color management) + `clap_lex` + `strsim` + proc-macro toolkit. Standard CLI ecosystem deps; no archived deps.
- **`cargo audit`:** no known CVEs at pin time.

## Security notes

- **CVE history:** clap 4.x has had no CVEs since the 4.0 release.
- **License:** MIT OR Apache-2.0; compatible with mdatron's MIT license.
- **Threat model:** parses user-controlled CLI arguments (operator-controlled). The threat is argument-injection — clap's parser does not execute argument content; the binary's own logic is responsible for sanitizing argument values (e.g., the path-confinement discipline applies to `--files` values).

## SO approval

- **Operator-attribution:** Solution Owner confirms clap is the appropriate CLI library for mdatron's subcommand structure. The derive feature pulls in proc-macros at compile time; runtime cost is minimal.
- **Scope justification:** mdatron will eventually ship ~10 subcommands. Hand-rolling that surface would cost ~500-1000 LoC; clap absorbs that complexity. Dep cost is proportionate.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
