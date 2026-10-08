# mdatron inputs reference

Every file mdatron reads from `.mdatron/`, in one place: its shape, the keys it
accepts, what activates it, what it scans, and the codes it can emit. This is
the page `mdatron docs inputs` prints. `mdatron init` deploys the skeleton and
an inert `*.example` template for each family file below (copy a template to
its real name to activate that family; a tree initialized before the templates
existed gains them on its next `init`, and a template you have not edited is
refreshed to the running version's content — forward only from 0.8.0 on: a
template whose content the running version does not know (a newer mdatron's)
is kept as it is and reported — while an edited one is
refused as drift and left alone). Two tests hold this page current: one
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
The five family files (`routes.yaml`, `pins.yaml`, `vocabulary.yaml`,
`code-catalogs.yaml`, `links.yaml`) carry `mdatron_format_version`, the input
contract's own version axis (absent reads as `1`; required on
`code-catalogs.yaml` and `links.yaml`, optional on the other three). `manifest.yaml` carries its own `version`, and a pattern
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

One glob dialect applies to every glob mdatron reads — `file_globs`, the
scope globs, a route's `files`, a pattern rule's path `context`: `*`, `?` and
`[…]` match within one path segment; `**` crosses any number of them; a
leading `.` is not special, so `**/CLAUDE.md` reaches `.claude/CLAUDE.md`.
(Through 0.6.0 the matcher let `*` cross `/` in a route's `files`, the scope
globs and a rule's `context` — the walk never did; a glob that relied on it
needs `**`. A route glob that now claims fewer files fails loud, `E0030`; a
scope glob that now matches fewer files is silent unless it matches nothing.)

Activation: `verify` refuses without it. Codes: `W0040`, `W0043`, `W0046`,
`W0051`, `W0055`.

## routes.yaml

The route family — the closed-world allowlist over the walked files, and the
gateway to the citation, link, marker, and section families.

Keys: `mdatron_format_version`, `routes`, `files`, `governed_by`, `naming`, `citations`, `links`, `link_root`, `marker_rules`, `pattern`, `element`, `target_doc`, `target_section`, `section_rules`, `section`, `match`, `match_on`, `count`, `disjoint`, `id_pattern`, `schema`, `name_equals_dir`, `max_bytes`, `every`, `order`, `imports`, `requires_sibling`, `link_policy`, `schemes`, `hosts`, `forbid_query`.

- Per route — required: `files` (root-relative glob; `*` matches within one
  path segment and `**` crosses any number of them — the one glob dialect
  every adopter glob uses, see `config.yaml`),
  `governed_by` (a document that must open inside the governed tree, `E0031`).
  Optional: `naming` (a filename grammar, `W0041`), `citations: true`,
  `links: true`, `link_root: true` (resolve `/root-relative` links; needs
  `links`), `marker_rules`, `section_rules`, `schema` (bind every claimed file
  to `.mdatron/schemas/<class>.json` and to rules with `context: <class>`,
  without the file carrying `schema_class` — for formats whose frontmatter you
  do not own; a disagreeing `schema_class` is `E0033`, a class nothing serves
  `E0034`, and a claimed file with no frontmatter is validated as an empty
  mapping), `name_equals_dir` (a frontmatter field that must equal the file's
  parent directory name, `E0035`), `max_bytes` (the most bytes a claimed file
  may hold, `E0036`; per file, so a budget shared by several files is split
  across their routes; the count is the file's bytes as checked out,
  frontmatter and any CRLF line endings included, so keep bounded files on
  LF; a bound of 0, or one at or above mdatron's own per-file input limit, is
  refused), `imports: true` (resolve Claude Code's `@path` imports in the
  claimed files like relative links — an `@` at the start of a line or after
  whitespace, the path running to the next whitespace, outside code spans and
  fences, a `\ ` being an escaped space; the path is opaque — `#` and `%`
  are path characters, not a fragment or an encoding; a missing target, or a
  directory, is `E0110` with the label `import`; an absolute, `~` or URL-shaped
  import is not resolved; needs `links`), `requires_sibling` (a file name whose
  regular file must exist in the same directory as every claimed file,
  `E0037`; a directory or a symlink there does not count), `link_policy`
  (what an absolute URL in the claimed files may be, `E0118` per clause
  broken: `schemes`, the schemes a link may use — a `//host` link has none
  and violates any list; `hosts`, the hosts an `http`, `https`, `ws`, `wss`, `ftp` or `//host`
  link may name, `*.example.com` covering every subdomain and not the apex,
  a bracketed IPv6 literal listed as one, a trailing-dot name being its
  own name; a link of another scheme (`mailto:`, `ssh://`, `git://`) is
  judged by `schemes` alone, so a `hosts` list without `schemes` says
  nothing about it; `forbid_query`, regexes over the query-parameter names a link
  must not carry, `^utm_` for the tracking parameters, one `E0118` per
  link naming its forbidden parameters (distinct, percent-decoded first,
  the first ten listed); schemes and hosts
  compare without regard to ASCII case; each list given must be non-empty
  and an empty block is refused; needs `links`). Every route with
  `links: true` also reports an absolute URL that is not well-formed
  (`E0117`) — including a destination a browser trims into one
  (`< https://…>`; a decoded `\/host`, written `\\/host` in the source),
  judged as a URL and never resolved as a path — and, when `links.yaml` exists, one the register does not
  declare (`E0115`, `E0116`); mdatron never fetches a URL.
- An `element` is one of `heading`, `h1`…`h6`, `list-item-bold-name`,
  `list-item`, `blockquote` or `line`. Elements are lines, recognised by the
  line's own prefix. A line inside a fenced code block is never an element —
  where the fence is indented at most three spaces; a fence nested deeper
  (inside a list item) is not recognised, and its lines are elements:
  - `list-item` — a line opening with `-`, `*`, `+` or an ordinal (`1.`,
    `1)`) followed by a space or a tab, after any indentation; named by its
    text after the marker. A nested item is an item, and so is an item-shaped
    line of indented code. A continuation line is not part of the element, a
    marker with no space or tab after it is not an item (a marker and a
    space with nothing more is an item with empty text), and a thematic
    break (`* * *`, `- - -`) is not an item.
  - `blockquote` — a line opening with `>` after at most three spaces; named
    by its text after the marker. A quote indented further (inside a list
    item, say) and a continuation line without `>` are not seen.
  - `line` — any non-blank line, named by the whole line; a heading is a
    `line` too, named with its `#` marker.
- A marker rule — required: `pattern` (a regex whose first capture is the
  referenced name), `element`, `target_doc`. Optional: `target_section` (a
  full ATX heading line).
- A section rule is one of four shapes:
  - a count rule (`element`, `match`, `count` with one of `>=`, `<=`, `==`,
    `!=`, `>`, `<` and an integer; optional `match_on`: `line`, the default,
    or `name`): how many elements match, `E0120`;
  - an every rule (`element`, `every`; optional `match_on`): each element of
    the class must match the `every` pattern, `E0123` per element that does
    not;
  - an order rule (`order`: two or more items of `element`, `match`, optional
    `match_on`): an element matching an earlier item must not follow one
    matching a later item, `E0124`; an item nothing matches is not a
    violation;
  - a `disjoint` rule (exactly two operands of `section`, `element`,
    `id_pattern`), `E0121`.

  On a count, every or order rule `section` is optional: given, the rule
  covers that heading's span — from the heading through just before the next
  heading of the same or a higher level; the section's own heading line is
  the container, never one of its elements — and an absent heading is
  `E0122`; absent, it
  covers the whole document body, so "the file is not empty" is
  `element: line`, `match: "."`, `count: ">= 1"`. The body starts after the
  frontmatter (a frontmatter line is never an element, so a rule cannot
  require one) and after a leading byte-order mark; with fenced code
  excluded too, a file holding only frontmatter or only a fenced block
  counts as having no lines. In the whole-document form the document's own
  H1 is an element like any other heading.

  In an order rule an element belongs to the FIRST item it matches, so an
  early item that matches broadly (`element: line`, `match: "."`) takes
  every element and the later items are never reached; a repeated item
  and an item with an empty `match` are refused at load, as are an empty
  `every` pattern, a whole-document count of `>= 0` and any count of `< 0`
  (rules that could never report, or never pass). A finding quotes the
  rule's pattern, and a `match_on` region when the pattern was tested
  against the name. When the section's heading occurs more than once, each
  occurrence is ordered on its own, an every rule checks the elements of all
  of them, and counts sum.
- What each pattern is tested against — the three differ, so read this before
  writing one:
  - a marker rule's `pattern` runs against the whole LINE, and its first
    capture group is the referenced name;
  - a count rule's `match`, an every rule's `every` and an order item's
    `match` run against the whole LINE by default (`### REQ-1`,
    so `'^REQ-[0-9]+$'` never matches) or against the element's NAME with
    `match_on: name` (the heading text, the list item's bold name or its
    text, the quoted text: `REQ-1`); they need no capture group;
  - a disjoint operand's `id_pattern` runs against the element's NAME, and its
    first capture group is the id.

Activation: the file exists — even `routes: []` (announced as `W0053`); from
then on every walked file must be claimed by exactly one route. Scope: every
walked file. Codes: `E0030`, `E0031`, `E0032`, `E0036`, `E0037`, `W0041`, `W0053`, `W0054`; per
opt-in `E0100`/`E0101`/`W0048`/`E0081` (citations), `E0110`/`E0111`/`E0115`/
`E0116`/`E0117`/`W0048`/`E0081` (links), `E0118` (link_policy),
`E0112`/`E0114`/`W0048`/`E0081` (markers),
`E0120`/`E0121`/`E0122`/`E0123`/`E0124` (section rules).

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

## links.yaml

The external-link register — the closed set of absolute URLs the corpus may
point at, read by the link family. mdatron never fetches a URL: the register
is the list a liveness tool checks on a schedule, and `mdatron links
--external` exports every markdown link to an absolute URL (`--json` adds
file and line) for that tool. What the link family sees is what CommonMark
calls a link — inline, reference-style, image and `<autolink>` destinations
(an email autolink is a `mailto:` link); a bare URL in prose and a raw HTML
`<a href>` are not links and are neither checked nor exported.

Keys: `mdatron_format_version`, `links`, `url`, `prefix`, `fragments`.

- Per entry — required: `url` (an absolute URL: a scheme such as `https:`, or
  a protocol-relative `//host`; written without a `#`; a well-formed URL
  itself, except a prefix for an opaque scheme such as `mailto:`). Optional:
  `prefix` (default `false`; `true` covers every URL that starts with `url`;
  a prefix for any `scheme://` or `//host` URL must run past its host —
  `https://acme.dev/`, never `https://acme.dev`, which would also cover
  `https://acme.dev.evil.example/`), `fragments` (the anchors the corpus may
  use on the page — or on every page a prefix entry covers — written without
  their `#`; absent accepts any fragment, an empty list accepts none, and a
  bare `#` is always accepted). An exact entry wins over a prefix entry and
  the longest prefix over a shorter one; a URL and a fragment compare as the
  link's destination reads after CommonMark decoding (`&amp;` is `&`, `\_`
  is `_`) — no case folding, no slash normalising, no percent-decoding. The
  invisible characters inside a real emoji or ideograph are not
  "invisible characters" (a zero-width joiner between pictographs, one
  presentation or ideographic variation selector, the tags of the
  England, Scotland and Wales flags; an ideographic selector is accepted
  whatever its value, so a CJK path can carry about one hidden byte per
  ideograph); a joiner, selector or tag run anywhere else, and
  any invisible character in a host, are.
- `mdatron_format_version` is required on this file. Refused at load: an
  empty `url`, one that is not absolute or not well-formed, one holding
  `#`, whitespace, a control or an invisible character, a prefix for any
  `scheme://` or `//host` URL that stops inside
  its host, a duplicate `url`, and an empty or repeated fragment, or one
  holding `#`, whitespace, a control or an invisible character.

Activation: the file exists. Scope: every markdown link to an absolute URL
in every file on a route with `links: true` (the link family's scope; on a
whole-tree run whose jurisdiction came from `config.yaml`, `W0056` when no
such file is walked and `W0057` for each entry no link uses). Codes: `E0115`, `E0116`, `W0056`,
`W0057`.

## manifest.yaml

The init manifest: the engine-written record of the managed partition. Do not
edit by hand, except to add a demotion tombstone in a reviewed commit.

Keys: `version`, `managed`, `path`, `sha256`, `demoted`, `reason`, `owner`.

- `managed` — entries of `path` (relative to `.mdatron/`) and `sha256`; a
  managed file whose content drifted is `E0060` — refused by `mdatron init` and
  reported by `mdatron verify` (one shared check, so the two always agree).
- `demoted` — tombstones of `path`, `reason`, `owner` for entries removed from
  the managed partition; a justified tombstone lints `L0001` on every
  whole-tree run, an unjustified one is `W0042`.

Activation: written by `mdatron init`; `verify` reads it when present. Codes:
`E0060`, `L0001`, `W0042`.

## schemas/

The schema family: one JSON Schema (draft 2020-12 only, `E0040` otherwise) per
`schema_class`, at `.mdatron/schemas/<class>.json`, validating the frontmatter
of every walked file that declares that class or whose route binds it
(`schema`).

Activation: the directory holds at least one schema. Scope: every walked file
with a `schema_class` or a route-bound class. Codes: `E0002`, `E0040`, `E0050`, `W0045`, `W0047`.

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
file a rule's `context` selects — by the `schema_class` it declares or a route
binds with `schema:`, or by path glob; a rule whose context selects no walked
file warns (`W0058`, whole-tree runs only). Codes: `E0021`, `E0022`, `E0023`,
`W0050`, `W0052`, `W0058`, and the adopter's own rule codes.
