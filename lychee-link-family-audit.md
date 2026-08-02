---
title: "lychee + markdownlint — link/anchor resolution reference for mdatron's link family (validation + gaps)"
tags: ["lychee", "markdownlint", "architecture", "reference", "mdatron", "link-family", "design-doc"]
sources:
  - url: "https://github.com/lycheeverse/lychee"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://lychee.cli.rs/recipes/anchors/"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://github.com/DavidAnson/markdownlint/blob/main/doc/md051.md"
    title: ""
    accessed_at: "2026-08-02"
contributors: ["79Ig"]
created: 2026-08-02
updated: 2026-08-02
---


## Design Specification

### what this is

A reference-architecture review of **lychee** (lycheeverse/lychee, the Rust link checker) and **markdownlint** (MD051 link-fragments) for **mdatron**'s `link` (#145) and `marker` (#147) families — to validate or challenge the shipped design decisions. Grounded in lychee source (`lychee-lib/src/extract/markdown.rs` + `extract/fragments.rs`), lychee.cli.rs, markdownlint MD051/MD042/MD059, and mdatron `src/link.rs` + `src/markup.rs`. Completes the reference-arch set ([[schematron-lineage-audit]], [[sarif-envelope-audit]], [[ruff-registry-audit]], [[cedar-opa-dsl-audit]]).

### lychee architecture

Extraction is **parser-based** (`pulldown-cmark`, not regex): it extracts *every* CommonMark link form — inline, all reference forms (`[t][]`, `[t]`), autolinks, email, **images** (`Tag::Image`), opt-in wikilinks. Raw strings → a `Uri` model (http/https/file/mailto) → checkable `Request`s. Async checker (`reqwest`+`tokio`, `--max-concurrency 128`, retries, rate-limiting). `Status`: Ok/Failed/Excluded/Cached/Timeout. `--cache` → `.lycheecache`.

### resolution — the load-bearing comparison

- **lychee's DEFAULT is document-relative — identical to mdatron.** A relative link resolves against the *containing file's directory*, maps to `file://`, and lychee **verifies the file exists on disk**. Root-relative (`/foo`) is **opt-in** via `--root-dir`, *not* the default. `--base-url` is a separate deployed-site mode.
- **`..` and confinement:** lychee imposes **no sandbox** — `../README.md` and even `../../../etc/passwd` resolve freely. mdatron does the same document-relative `..` collapse (`../README.md` legit) but **adds** a confinement layer lychee lacks (absolute → E0010, tree-escape → E0011, symlink → E0012). **This is a genuine mdatron value-add, not a gap** — it's a conformance engine on a governed tree, not a general checker.
- **Verdict: mdatron's document-relative resolution is the industry-standard model, full stop.** Neither lychee's default nor markdownlint suggests root-relative as the default.

### fragments / anchors

- lychee verifies `#fragment` against the target's headings (`--include-fragments`) — the same "resolve file, extract fragment set, test membership" flow mdatron uses.
- **Slug algorithm:** lychee's `GithubHeadingIdGenerator` mimics GitHub's *real* algorithm, tracing it to GitHub's Ruby `html-pipeline` `toc_filter.rb` — and *explicitly rejects* `github-slugger` as having discrepancies. It handles **duplicate-heading `-1`/`-2` disambiguation (stateful), `{#custom-id}`, `<span id>`/`<a name>` HTML anchors, emoji variation selectors, ZWJ, percent-encoding.**
- **markdownlint MD051 uses the same GitHub algorithm and cites the same `html-pipeline` source** — the two mature tools agree on the reference implementation. mdatron's `slugify` is **ASCII-identical** to it.

### external / offline, and fenced-code

- lychee splits by scheme; **`--offline` = "only check local files, block network"** — *precisely* mdatron's permanent, unconditional posture. mdatron's `is_external` defers any `scheme:`/`//host`. (Minor divergence: mdatron defers `file:` URLs where lychee resolves them — negligible.)
- **Fenced-code links are skipped — confirmed convention.** lychee tracks `inside_code_block` and suppresses extraction inside fenced blocks *and inline code* (`Event::Code`); pulldown also emits `[x](y)` inside a fence as literal Text. mdatron skips **fenced** blocks via its line-based `non_fenced_lines` — but see the bug below.

### markdownlint (secondary)

- **MD051** (link-fragments): same-file scope, GitHub slug, recognizes `{#custom-id}` + HTML `id=` + `<a name>` + GitHub specials (`#top`, `#L20`). Closest analog to mdatron **E0111**.
- **MD042** (no-empty-links `[t]()`/`[t](#)`): mdatron has **no analog** (silently skips empty dests).
- **MD059** (descriptive text): pure quality lint, out of family scope.

### mapping → mdatron link + marker

**Validated (mdatron got these right):**
1. **Document-relative resolution with on-disk existence** = lychee's exact default, CommonMark-correct.
2. **GitHub heading-slug** = the same algorithm family lychee *and* MD051 use (both cite `html-pipeline`); mdatron is ASCII-identical to the reference.
3. **Defer-all-external / never-network** = lychee's `--offline`, made permanent.
4. **Skipping fenced-code links** = confirmed convention.
5. **Path confinement (E0010/E0011/E0012)** is a security layer *beyond* lychee/markdownlint — appropriate for a governed-tree engine, a genuine differentiator.

**Gaps worth follow-up issues (ranked):**
1. **Duplicate-heading `-N` disambiguation is deferred** → mdatron collects slugs into a `HashSet` with no suffixing, so a link to `#foo-1` (GitHub's id for the 2nd "Foo") is a **false-positive E0111**. lychee handles it statefully. *Highest-value, cheap.*
2. **Inline-code-span (and indented-code) links are a BUG, not just deferred coverage.** mdatron's line-based scanner skips *fenced* blocks but **not inline `` `code` ``** — so a line like `` use `[x](y)` `` has `[x](y)` matched and **resolved** → a **false-positive correctness bug**. (mdatron's own module doc lists inline-code-span as "deferred," but the code actively mis-resolves it.) lychee excludes it correctly via `Event::Code`. *This is the one item that's a bug rather than a coverage gap.*
3. **Reference-style links `[t][ref]` and images `![alt](src)` are silent false NEGATIVES.** Both are common in docs; mdatron's regex can't see them, so a reference link or image to a dead file is never caught. lychee checks both. *High value.* Adopting `pulldown-cmark` would fix items 2+3 together (at the cost of the current dependency-light regex approach).
4. **Setext headings + HTML anchors (`<a name>`/`id=`) not in the slug set** → false-positive E0111 on links to non-ATX / explicit anchors. *Medium; setext is cheap.*
5. **Optional `root:` mode** — legitimate root-relative links (`/docs/x.md`, common in static-site corpora) currently hard-refuse via E0010; lychee offers `--root-dir`. An opt-in in-tree root mode would resolve them while preserving confinement. *Low-medium.*

### takeaways

- mdatron's link family **chose the industry-standard model on every headline decision** (document-relative, GitHub-slug, defer-external, skip-fenced) and *adds* confinement the mature tools lack — the design is sound, validated against the reference implementations.
- **File follow-ups** for: (1) duplicate-heading `-N` slugs (cheap false-positive fix); (2) the **inline-code-span false-positive bug** (correctness); (3) reference-style + image links (false negatives) — the biggest coverage gap, and the case for eventually adopting `pulldown-cmark`; (4) setext/HTML anchors; (5) optional `root:` mode.
- The `pulldown-cmark` question is the strategic one: it would close items 2–4 and align with lychee/markdownlint's parser, trading mdatron's current regex simplicity + dependency-light posture for parser fidelity. Worth a deliberate decision, not an incidental one.

### sources

lychee repo/README; source `lychee-lib/src/extract/markdown.rs` & `extract/fragments.rs` (@master); lychee.cli.rs recipes (root-dir, base-url, local-folder, anchors); DeepWiki lychee; markdownlint MD051/MD042/MD059; mdatron `src/link.rs`, `src/markup.rs`. Flagged: full lychee `Status` enum not exhaustively sourced; lychee's precise leading-`/` behavior without `--root-dir` (docs describe the with-flag case).

