# mdatron DSL reference

The complete Layer-2 construct inventory (`DESIGN.md` § Cross-file semantics
stay narrowed). The DSL serves one lane: **cross-file and registry integrity
over frontmatter**. Body-content extraction is excluded by design; there are no
markdown-AST helpers, no regex functions, and no string-extraction functions.
This document and the implementation match exactly — a documented construct
absent from the engine, or an engine construct absent here, is a contract
violation.

## Pattern files

Patterns live at `.mdatron/patterns/<name>.yaml`:

```yaml
mdatron_dsl_version: 1
pattern:
  id: my-pattern            # required
  description: optional prose
  phases: []                # optional; runtime-selectable subsets
  keys: []                  # optional; cross-file indices, see below
  rules:                    # one or more
    - id: my-rule           # required
      context: my-class     # required; see Context selectors
      let:                  # optional; name -> expression string
        n: count($self.items)
      assert: $n == 3       # required; fires the finding when FALSE
      code: MYPROJ-E0001    # required; the finding's code (use your own
                            # namespace — MDATRON-* is reserved for the engine)
      message: "expected three items, found {{$n}}"   # required
```

Every rule whose `context` matches a walked file has its `let:` bindings
evaluated top to bottom, then `assert:`. A `false` assertion emits one finding
with the rule's `code` and `message` at that file.

## Context selectors

- `context: blog` — bare string without glob metacharacters or `/`: matches
  files whose frontmatter `schema_class` equals it.
- `context: "docs/**/*.md"` — bare string with `*`, `?`, or `/`: a path glob
  relative to the project root.
- Object form, both constraints optional and ANDed:

  ```yaml
  context:
    schema_class: review-entry
    path: "review-log/**/*.md"
  ```

## Variables

- `$self` — the current file's parsed frontmatter (an object).
- `$file` — reserved per-file namespace (object; `Null` fields when unset).
- `$project` — reserved project namespace.
- `$<name>` — a `let:` binding declared in the same rule.

Field access is `.`-chained: `$self.meta.owner`. Accessing a missing field
yields `Null`; field access **on** `Null` yields `Null` (no error) — pair with
`defined()` to distinguish.

### Field-reference validation (`MDATRON-E0021`)

Because a missing field reads as `Null` rather than erroring, a **typo** in a
`$self` path (`$self.ownr` for `$self.owner`) would silently make an assertion
mis-fire or pass vacuously against every governed document. To catch this before
it ships, mdatron validates `$self` field references against the frontmatter
schema **at load** (adopting Cedar's validate-before-deploy posture) and
hard-gates a bad reference as `MDATRON-E0021` (error, exit 1) — the run stops
before any document is checked.

The check is **deliberately conservative** — it flags only a provable typo:

- Only rules whose `context` resolves to a **known `schema_class`** are examined.
  A path-glob context (whose `$self` schema is not statically known) is skipped.
- Only `$self`-rooted paths are walked. `let:` bindings, `$file`, `$project`, and
  quantifier variables (`$m` in `every(m in …)`) are not `$self` and never flag —
  but a `$self` path *inside* a quantifier collection or `let:` value is checked.
- A path is flagged **only** when a segment is absent from a level's `properties`
  **and** that level is a closed object (`additionalProperties: false`). Every
  undecidable shape — an open object (the JSON-Schema default), an array, a
  `$ref`, or a combinator (`allOf`/`anyOf`/…) — passes unflagged.

To bring a field into scope, declare it under the schema's `properties` (or relax
`additionalProperties` if the object is meant to carry open-ended keys). See
`mdatron explain MDATRON-E0021`.

## Expressions

Precedence, loosest first: `or`, `and`, `not`, `in`/`not_in`, `==`/`!=`,
postfix `.field`, primaries. Parentheses group. Literals: `"strings"` (with
`\"`, `\\`, `\n` escapes), integers (optionally negative), `true`, `false`,
`null`, and arrays of literals only: `["a", "b", 3]` (expression-typed array
elements are not supported).

- `x in coll` — membership: true when `x` equals an element of the array
  `coll`. `not_in` negates.
- `==` / `!=` — deep equality over strings, ints, bools, arrays, objects,
  `Null`.

## Quantifiers

```text
every(x in <array-expr>, <predicate>)
some(x in <array-expr>, <predicate>)
filter(x in <array-expr>, <predicate>)
```

Each binds `x` (referenced as `$x`) over every element. `every` is true when
the predicate holds for all elements (true over an empty array); `some` when it
holds for at least one (false over an empty array). `filter` returns the
**sub-array** of elements for which the predicate holds. A `Null` collection is
treated as empty (empty array for `filter`).

`filter` composes with `count`/`len` for **arity rules** — exactly-N, at-least-N —
that the boolean quantifiers cannot express directly:

```yaml
# exactly one member carries kind "lane"
assert: count(filter(m in $self.members, $m.kind == "lane")) == 1
# at least two do
assert: count(filter(m in $self.members, $m.kind == "lane")) != 1
        and some(m in $self.members, $m.kind == "lane")
```

`count` over `intersect`/`difference` expresses **disjointness and subset rules**
between two frontmatter collections:

```yaml
# the phase ids of two sections are disjoint (share no element)
assert: count(intersect($self.phases_a, $self.phases_b)) == 0
# every id in `required` also appears in `declared` (required is a subset)
assert: count(difference($self.required, $self.declared)) == 0
```

Together, arity-with-predicate (`count(filter(...))`) and disjointness
(`count(intersect(...))`) are the frontmatter form of the "exactly one X" and
"ids disjoint between two sections" acceptance checks (tracker #149). They apply
to collections declared **in frontmatter**; the same shapes over *body-content*
structure (e.g. counting H3 headings inside a `##` section) are excluded from the
DSL pending the body-content falsifiability gate (see `DESIGN.md`
§ Cross-file semantics stay narrowed) and are tracked separately.

## Functions

| Signature | Semantics |
|---|---|
| `count(arr)` | Number of elements. Errors on non-array. |
| `len(v)` | Characters of a string or elements of an array. |
| `defined(v)` | `true` iff `v` is not `Null`. Strict: the empty string IS defined. |
| `union(a, b)` | Array: `a` followed by elements of `b` not already present (dedup, order preserved). |
| `intersect(a, b)` | Elements of `a` also present in `b`. |
| `difference(a, b)` | Elements of `a` NOT present in `b`. |
| `concat(s1, s2)` | String concatenation. |
| `join(arr, sep)` | Join array elements into one string with separator. |
| `key(index, k)` | Cross-file index lookup, see below. Miss returns `Null`. |

All functions check arity and types at evaluation; a mismatch is a loud
pipeline error naming the pattern and rule.

## Cross-file indices (`keys:`)

A pattern declares named indices built once per verification run:

```yaml
keys:
  - name: entries               # referenced as key("entries", ...)
    source: "registry/*.md"     # one file or a glob, relative to the
                                # project root; path-confined (no .., no
                                # absolute paths, symlinks refused)
    select: "$.frontmatter.items"   # path into the source file's parsed
                                    # structure yielding the entries
    indexed_by: "$.id"          # JSONPath into each entry yielding its key,
                                # or the literal "$key" when the selection is
                                # an object whose keys index its values
```

**Source parsing.** A `.md` source parses to the wrapper object
`{frontmatter: <parsed frontmatter>}` — so selections into markdown sources
start `$.frontmatter.`; `$` alone selects the whole wrapper. `.yaml`/`.yml`
and `.json` sources parse to their document root directly. The select path is
`$` or `$.field.subfield` (field walks only; no array indexing or wildcards).

If `select` resolves to an array, each element becomes an entry; a single
object becomes one entry. This applies uniformly to single-file and glob
sources — a glob source may contribute many entries per file. Entries from the
file currently being verified are included. When two entries produce the same
index key, the later one replaces the earlier (last wins).

`key("entries", $self.parent)` returns the entry whose index key equals the
string argument, or `Null` on a miss — so referential integrity is:

```yaml
assert: every(id in $self.pairs_with, defined(key("entries", $id)))
```

## Evaluation semantics

- `and` / `or` **short-circuit**: the right operand is not evaluated when the
  left decides the result. `defined($x) and <uses $x>` is therefore a safe
  guard, as is `$self.f == "" or <uses $self.f>`.
- `x in coll` requires `coll` to evaluate to an array; a `Null` or non-array
  right side is an evaluation error — guard with `defined()` first.
- Quantifier collections (`every`/`some`) must likewise be arrays; iterating a
  possibly-absent field needs a `defined()` guard.
- `key(index, k)` requires `k` to be a string; a `Null` key is an evaluation
  error — guard first. A lookup MISS (string key, no entry) returns `Null`.
- An evaluation error (type mismatch, arity, non-boolean assert) is a loud
  pipeline failure naming the pattern and rule — not a finding and not a pass.

## Messages

`message:` is engine-rendered with `{{expr}}` interpolation. The evaluated
value is NOT inlined into the message text: the rendered diagnostic carries an
engine-authored `{expr}` placeholder in the message line and the value beneath
it as a quoted, prefix-marked block (adopter-derived text never rides inline in
an engine line).

## What is deliberately absent

No body-content access (headings, links, tables, comments), no regex or
string-extraction functions, no arithmetic beyond integer literals and
equality, no floats, no built-in patterns. Expansion beyond this inventory is
gated (`DESIGN.md` § Cross-file semantics stay narrowed).
