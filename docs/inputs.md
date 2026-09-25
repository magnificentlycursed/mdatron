# mdatron inputs reference

Every file mdatron reads from `.mdatron/`, in one place: its shape, the keys it
accepts, what activates it, what it scans, and the codes it can emit. This is
the page `mdatron docs inputs` prints. `mdatron init` deploys the skeleton and
an inert `*.example` template for each family file below (copy a template to
its real name to activate that family; a tree initialized before the templates
existed gains them on its next `init`). Two tests hold this page current: one
loads every template through the real parser and cross-checks the keys listed
here against the keys the template exercises; the other asks each strict
parser directly — by feeding it an unknown key at every nesting level and
reading the accepted-key list its refusal names — and asserts that list equals
the keys listed here. `config.yaml` is the one file that cannot be asked (it
tolerates unknown keys), so its list is held by review.

Conventions. Paths are project-root-relative and confined: an absolute path or
a `..` segment is refused (`E0010`, `E0011`), a symlinked component is refused
(`E0012`). Every family file is parsed strictly — an unknown key is a load
refusal (exit 2), never ignored — except `config.yaml`, which tolerates unknown
keys. "Supplied" means the file exists; delete a file to deactivate its family.
The four family files (`routes.yaml`, `pins.yaml`, `vocabulary.yaml`,
`code-catalogs.yaml`) carry `mdatron_format_version`, the input contract's own
version axis (absent reads as `1`; required on `code-catalogs.yaml`, optional
on the other three). `manifest.yaml` carries its own `version`, and a pattern
file carries `mdatron_dsl_version`; neither accepts `mdatron_format_version`.

## config.yaml

Seeded by `mdatron init`, adopter-owned from then on (never hashed, never
overwritten). The jurisdiction and the scope globs.

Keys: `file_globs`, `require_frontmatter`, `vocabulary_globs`, `code_catalog_globs`.

- `file_globs` (required) — the jurisdiction: the globs of files mdatron walks.
  Never guessed: an absent or empty list refuses (exit 2); pass `--file-globs`
  (`--files`) for an ad-hoc run instead. A glob matching nothing warns
  (`W0046`).
- `require_frontmatter` (optional) — files that must carry a frontmatter block
  (`W0040` otherwise); a dead glob warns (`W0051`).
- `vocabulary_globs` (optional) — the files the vocabulary family scans; empty =
  every walked file; a dead scope warns (`W0043`).
- `code_catalog_globs` (optional) — the files the code-catalog family scans;
  empty = every walked file; a dead scope warns (`W0055`).

Activation: `verify` refuses without it. Codes: `W0040`, `W0043`, `W0046`,
`W0051`, `W0055`.

## routes.yaml

The route family — the closed-world allowlist over the walked files, and the
gateway to the citation, link, marker, and section families.

Keys: `mdatron_format_version`, `routes`, `files`, `governed_by`, `naming`, `citations`, `links`, `link_root`, `marker_rules`, `pattern`, `element`, `target_doc`, `target_section`, `section_rules`, `section`, `match`, `count`, `disjoint`, `id_pattern`.

- Per route — required: `files` (root-relative glob; `*` crosses `/`),
  `governed_by` (a document that must open inside the governed tree, `E0031`).
  Optional: `naming` (a filename grammar, `W0041`), `citations: true`,
  `links: true`, `link_root: true` (resolve `/root-relative` links; needs
  `links`), `marker_rules`, `section_rules`.
- A marker rule — required: `pattern` (a regex whose first capture is the
  referenced name), `element` (`heading`, `h1`…`h6`, or `list-item-bold-name`),
  `target_doc`. Optional: `target_section` (a full ATX heading line).
- A section rule — a count rule (`section`, `element`, `match`, `count` with
  one of `>=`, `<=`, `==`, `!=`, `>`, `<` and an integer) or a `disjoint` rule
  (exactly two operands of `section`, `element`, `id_pattern`).

Activation: the file exists — even `routes: []` (announced as `W0053`); from
then on every walked file must be claimed by exactly one route. Scope: every
walked file. Codes: `E0030`, `E0031`, `E0032`, `W0041`, `W0053`, `W0054`; per
opt-in `E0100`/`E0101`/`W0048`/`E0081` (citations), `E0110`/`E0111`/`W0048`/
`E0081` (links), `E0112`/`E0114`/`W0048`/`E0081` (markers),
`E0120`/`E0121`/`E0122` (section rules).

## pins.yaml

The pin family — a governing document attests the sha256 of a governed file,
or of one heading's span. `mdatron pin` checks, `mdatron pin --update` re-pins.

Keys: `mdatron_format_version`, `pins`, `governed_by`, `file`, `section`, `sha256`, `unpinned`, `reason`, `owner`.

- Per pin — required: `governed_by`, `file`, `sha256`. Optional: `section` (a
  full ATX heading line; the pin covers that span, `E0063` if absent).
- Per `unpinned` tombstone — required: `file`, `governed_by`, `reason`, `owner`
  (a tombstone without its justification is `W0042`; a justified one lints
  `L0001` on every whole-tree run).

Activation: the file exists. Scope: the pinned files themselves — any file
inside the project root, walked or not. Codes: `E0061`, `E0062`, `E0063`,
`E0081`, `L0001`, `W0042`.

## vocabulary.yaml

The vocabulary family — the naming register over governed prose.

Keys: `mdatron_format_version`, `terms`, `term`, `status`, `sense`, `coinage_globs`, `label_schemes`, `allow`, `anti_patterns`, `pattern`, `guidance`, `numeric_claims`, `field`.

- `terms` (optional) — entries of `term`, `status` (`registered`, `draft`, or
  `reserved`), `sense`. A bold-introduced term not registered is `E0090`; a
  `reserved` term used in prose is `E0092`; a term declared both registered
  and draft is `W0044`.
- `coinage_globs` (optional) — where bold means coinage (`E0090`); empty =
  wherever the register applies; a dead scope warns (`W0043`).
- `label_schemes.allow` (optional) — regexes for sanctioned letter-plus-number
  label schemes; a cluster outside them is `E0091` (the scan is active only
  when the list is non-empty).
- `anti_patterns` (optional) — entries of `pattern` (regex) and `guidance`
  (the corrective wording); a match is `E0093`.
- `numeric_claims` (optional) — entries of `field`; a prose numeral disagreeing
  with that frontmatter field's count is `E0094`.

Activation: the file exists. Scope: every walked file, or `vocabulary_globs`.
Codes: `E0090`, `E0091`, `E0092`, `E0093`, `E0094`, `W0043`, `W0044`.

## code-catalogs.yaml

The code-catalog family — every adopter code token cited in the corpus must
resolve to a declared entry.

Keys: `mdatron_format_version`, `catalogs`, `namespace`, `comprehensive`, `codes`.

- Per catalog — required: `namespace` (the ownership prefix, e.g. `ADOPTER-`),
  `codes` (the legal set of code bodies: class letter plus digits). Optional:
  `comprehensive` (default `false`; only a comprehensive catalog can call an
  unlisted token an orphan, `E0113`).
- `mdatron_format_version` is required on this file.

Activation: the file exists. Scope: every walked file, or `code_catalog_globs`;
inline code spans are scanned, fenced blocks are not. Codes: `E0113`, `W0055`.

## manifest.yaml

The init manifest: the engine-written record of the managed partition. Do not
edit by hand, except to add a demotion tombstone in a reviewed commit.

Keys: `version`, `managed`, `path`, `sha256`, `demoted`, `reason`, `owner`.

- `managed` — entries of `path` (relative to `.mdatron/`) and `sha256`; a
  managed file whose content drifted is refused by `mdatron init` (`E0060`).
- `demoted` — tombstones of `path`, `reason`, `owner` for entries removed from
  the managed partition; a justified tombstone lints `L0001` on every
  whole-tree run, an unjustified one is `W0042`.

Activation: written by `mdatron init`; `verify` reads it when present. Codes:
`E0060`, `L0001`, `W0042`.

## schemas/

The schema family: one JSON Schema (draft 2020-12 only, `E0040` otherwise) per
`schema_class`, at `.mdatron/schemas/<class>.json`, validating the frontmatter
of every walked file that declares that class.

Activation: the directory holds at least one schema. Scope: every walked file
with a `schema_class`. Codes: `E0002`, `E0040`, `E0050`, `W0045`, `W0047`.

## patterns/

The rule DSL: pattern files at `.mdatron/patterns/<name>.yaml` — see
`mdatron docs dsl` for the complete construct inventory.

Keys: `mdatron_dsl_version`, `pattern`, `id`, `description`, `keys`, `name`, `source`, `select`, `indexed_by`, `rules`, `context`, `let`, `assert`, `code`, `message`, `phases`, `location`, `field`, `expression`.

- `pattern` (required) — `id` (required), `description` (optional), `keys`
  (optional cross-file indices of `name`, `source`, `select`, `indexed_by`),
  `rules` (required; each with `id`, `context`, `assert`, `code`, `message`, and
  an optional `let` map). `phases` and a rule's `location` (`field`,
  `expression`) are accepted and inert (`W0052`).

Activation: the directory holds at least one pattern file. Scope: every walked
file whose `schema_class` a rule's `context` selects. Codes: `E0021`, `E0022`,
`W0050`, `W0052`, and the adopter's own rule codes.
