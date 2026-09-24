---
title: "ontology-review-2026-09-24"
tags: ["reference"]
sources:
  - url: "cold-session ontological review of mdatron 0.6.0 / envelope 3.0.0, 2026-09-24 (crosslink #204)"
    title: ""
    accessed_at: "2026-09-24"
contributors: ["79Ig"]
created: 2026-09-24
updated: 2026-09-24
---

# mdatron ontological review — 2026-09-24 (crate 0.6.0 / envelope 3.0.0; cold-session, worktree)
# mdatron ontological review — cold session, worktree `agent-a91d76cf1c48adb8a`, crate 0.6.0 / envelope 3.0.0

Surfaces read: DESIGN.md (all), README.md, docs/{dsl-reference,faq,limits,methodology,methodology-enforcement,field-rename-ledger,cost-ledger}.md, schema/{mdatron-output.schema,code-catalog}.json, all 48 explain H1s (23 pages in full), src/{lib,config,route,pin,vocab,codecat,section,marker,link,cite,init,output,diagnostic,verify,codes,format_version,confine,snapshot,memo,dep,limits,markup,main}.rs, src/dsl/{types,parser,index,mod}.rs, .mdatron/*, live `mdatron --help` for every subcommand and a live self-verify envelope (built binary, no repo mutation). Rename-cost classes used below: **KEY-strict** (routes/pins/vocabulary/code-catalogs: `#[serde(alias)]` + ledger row, format version unchanged; alias drop = MAJOR `mdatron_format_version`), **KEY-lenient** (config.yaml: alias mandatory or the old key silently drops to default), **ENV-MAJOR/MINOR** (envelope shape), **FP** (a `summary` or quoted `label` — both fingerprint inputs, schema:215 — so a re-spelling shifts fingerprints; needs `explain::migration_note`, src/explain/mod.rs:274-298), **REASON** (free-text `families[].reason`, schema:157 — no bump), **CLI** (clap `visible_alias`, free), **INT** (internal, free), **DOC**.

## A. Concept inventory (canonical = DESIGN name where one exists)

| # | Canonical | DESIGN | Code identifier | Adopter input | Envelope | Code/explain | CLI/README/docs aliases |
|---|---|---|---|---|---|---|---|
| K1 | conformance engine | DESIGN:9 "conformance engine for typed markdown" | crate `mdatron` | — | — | — | "Typed-markdown validator" Cargo.toml:9, `--help`; "validator" README:3,21 |
| K2 | check family | DESIGN:68 "check families", :70 "generic engine" | `Families` output.rs:116 | — | `families` schema:86 | — | "Conformance families (Layer 2 data)" README:270; "engines" README:273 |
| K3 | schema family | DESIGN:72 "Schema conformance" | schema.rs | `.mdatron/schemas/*.json`, `schema_class` | `families.schema` | E0040/E0050/W0047 | "Layer 1" README:9,195; verify.rs:1277 reason; E0002.md:10 |
| K4 | route family | DESIGN:73 "Route conformance"; "route table" :70 | route.rs, `Route` :130 | `routes.yaml` → `routes[]` | `families.route` | E0030-32, W0041 | "Routes" README:277; "closed-world allowlist" |
| K5 | pin family | DESIGN:74 | pin.rs, `Pin` :76 | `pins.yaml` → `pins[]`, `unpinned[]` | `families.pin` | E0061-63, L0001, W0042 | "Pins" README:294; "pin record" main.rs:132, DESIGN:62; "pin manifest" DESIGN:17,58 |
| K6 | vocabulary family | DESIGN:75; "vocabulary registry" :58 | vocab.rs, `LoadedVocab` :90 | `vocabulary.yaml` + config `vocabulary_globs` | `families.vocabulary` | E0090-94, W0043/44 | "naming register" config.yaml:16, schema:95, methodology.md:63; "Vocabulary" README:325 |
| K7 | citation family | DESIGN:76 | cite.rs | route `citations: true` | `families.citation` | E0100/01 | "Citations" README:367 |
| K8 | link family | DESIGN:77 | link.rs | route `links`, `link_root` | `families.link` | E0110/11 | "Links" README:373; "fragment" DESIGN:77 vs "anchor" E0111 |
| K9 | marker family | DESIGN:78 "Marker-line reference conformance" | marker.rs, `MarkerRule` route.rs:142 | route `marker_rules[]` | `families.marker` | E0112/14 | "Markers" README:391 |
| K10 | code-catalog family | DESIGN:79 "Adopter-namespace code-catalog integrity" | codecat.rs, `CodeCatalog` :66 | `code-catalogs.yaml` → `catalogs[]` | `families.code_catalog` | E0113 | "Code catalogs" README:421; "code-catalog registry" codecat.rs:34 |
| K11 | section family | DESIGN:80 "Section-structural conformance" | section.rs, `Rule` :98 | route `section_rules[]` | `families.section` | E0120-22 | "Section rules" README:449 |
| K12 | rule DSL (one lane) | DESIGN:92 | src/dsl, `PatternFile` types.rs:16 | `patterns/*.yaml` `pattern:` | none (no `families` member) | E0021/22, W0050, W0045 | "Layer 2" README:13,239; lib.rs:13; "pattern rule" W0045.md:10 |
| K13 | jurisdiction | DESIGN:84 (once) | `VerifyConfig.file_globs` verify.rs:47 | config `file_globs` config.rs:30 | `summary.files_checked` (schema:82 "empty jurisdiction") | W0046 `jurisdiction-glob-matches-nothing` | `--files` main.rs:52-56; README:72; "walked files" W0043.md:11; "checked corpus" W0046.md:10 |
| K14 | governed tree | DESIGN:70 (confinement universe), :118 | `--project-root` | — | — | E0031.md:10, E0062.md:10 (root sense); W0046.md:13, E0030.md:11 (jurisdiction sense) | README:334-336 sample registry sense = "file set inside the declared jurisdiction" |
| K15 | governed file | DESIGN:73-74 (route/pin sense) | `GovernedFile` dep.rs:43 (walked sense) | — | — | E0003 `governed-file-unreadable`, W0040 (walked sense); E0061 message pin.rs:242 (pin sense) | — |
| K16 | governing document | DESIGN:73, :158-159 | `Route.governed_by` route.rs:132; `Pin.governing` pin.rs:77 | routes `governed_by` route.rs:49; pins `governing` pin.rs:53,68 | quoted label `governed_by` route.rs:232,255 / `governing` pin.rs:262,475 | E0031 `governing-document-absent` | "governing doc" README ×4 |
| K17 | route (entry) | DESIGN:73 | `Route` | `routes[]{files, governed_by, naming?, citations?, links?, link_root?, marker_rules?, section_rules?}` route.rs:45-85 | — | E0030 "no route claims it" | "claims" |
| K18 | naming grammar | DESIGN:73 | `Route.naming` | `naming` route.rs:52 | label `naming grammar` route.rs:452 | W0041 `name-underivable` | — |
| K19 | pin | DESIGN:74 | `Pin{governing,file,section,sha256}` | `pins[]` pin.rs:52-62 | labels `file`,`governing`,`recorded`,`found` | E0061 `pin-stale`, E0062 `pin-target-missing`, E0063 `pin-section-not-found` | `mdatron pin`, "re-pin" |
| K20 | unpin tombstone | DESIGN:159 "`unpinned:` tombstone" | `RawUnpinned{file,governing,reason,owner}` pin.rs:66-73 | `unpinned[]` | labels `file`,`reason`,`owner` pin.rs:172-182 | L0001 `governance-weakening-standing`, W0042 | README:313 "justified `unpinned:` tombstone" |
| K21 | demotion tombstone | DESIGN:62 | `DemotionTombstone{path,reason,owner}` init.rs:134-137 | manifest `demoted[]` manifest.yaml:6-9 | — | E0060.md:34 (adopter hand-writes it) | "tombstoned demotion" DESIGN:151 |
| K22 | governance weakening | DESIGN:62 | — | `reason`/`owner` | — | L0001 / W0042 | "justification annotation" |
| K23 | init manifest / managed partition | DESIGN:62 "init manifest", :132 | `Manifest{version,managed[]{path,sha256},demoted[]}` init.rs:112-137 | `.mdatron/manifest.yaml` | — | E0060 `managed-manifest-drift` | "managed-partition manifest" README:65, manifest.yaml:1; "managed manifest" main.rs:152; "init manifest" E0080.md:42 |
| K24 | seeded (adopter-owned) file | — | config.rs:3-4 | `config.yaml` | — | E0060.md:30-33 | README:64 |
| K25 | term / term status | DESIGN:75 "registered", "draft" | `TermStatus{Registered,Draft,Reserved}` vocab.rs:63-67 | `terms[]{term,status,sense}` vocab.rs:54-59 | label `term` | E0090 `unregistered-coinage`, E0092 `reserved-word-misuse`, W0044 | "coinage" |
| K26 | label scheme / letter cluster | DESIGN:75 "letter-plus-number clusters" | `label_allow`, `cluster` vocab.rs:96,99 | `label_schemes.allow` vocab.rs:71-74 | label `cluster` | E0091 `invented-label-scheme` | "letter-clusters" methodology.md:65; "letter-cluster" vocab.rs:13 |
| K27 | register anti-pattern | DESIGN:75 "register anti-patterns" | `anti` vocab.rs:97 | `anti_patterns[]{pattern, register}` vocab.rs:78-81 | labels `matched`,`register` vocab.rs:375,380 | E0093 `register-anti-pattern` | README:346-348 |
| K28 | numeric claim | DESIGN:75 | `numeric` | `numeric_claims[]{field}` vocab.rs:85-87 | labels `claim`,`field` | E0094 `numeric-claim-drift` | — |
| K29 | citation | DESIGN:76 | `Citation` cite.rs:22 | prose `path:line` | label `citation` | E0100 `dead-citation`, E0101 | — |
| K30 | body link / anchor | DESIGN:77 "#fragment", "heading-slug" | `BodyLink` markup.rs:167; `heading_slugs` :77 | prose links | label `link` | E0110 `dead-link-target`, E0111 `dead-anchor` | "anchor"/"fragment"/"slug" |
| K31 | marker rule | DESIGN:78 | `MarkerRule{pattern,element,target_doc,target_section}` route.rs:142-148 | `marker_rules[]` route.rs:90-104 | label param marker.rs:367 | E0112 `dead-marker-reference`, E0114 | "marker line" |
| K32 | element class | DESIGN:78 `heading` \| `list-item-bold-name`; :80 "`- **bold**` bullet lead" | `ElementClass{Heading,ListItemBoldName}` route.rs:111-117; `HeadingLevel{H1..H6}` section.rs:72-80; `IdSource{H3Heading,BulletLead}` section.rs:89-95; `markup::list_item_bold_name` :301 | marker `element:` ; section count `element:` ; disjoint `id_from:` | — | E0112.md:17-18; E0121.md:11-12 | README:402,463,469,472; "bold lead" README:482, E0121.md:19 |
| K33 | target document | DESIGN:78 "`target_doc`… not derived from `governed_by`" | `MarkerRule.target_doc` | `target_doc` route.rs:99 | label `target_doc` route.rs:318 | E0112.md:19 "target document" | "target doc" README:393 |
| K34 | section (heading spec → span) | DESIGN:74 "heading-delimited section" | `markup::section_span` :325 / `section_spans` :356 | pin `section` pin.rs:60; rule `section` section.rs:52,66; `target_section` route.rs:103 | label `section` pin.rs:509, section.rs:307 | E0063, E0114, E0122 "section spec" | "span" |
| K35 | section rule | DESIGN:80 count / disjoint | `RawRule` section.rs:50-61, `RawOperand` :65-69 | `{section, element, match, count}` / `{disjoint:[{section,id_from,id_pattern}]}` | labels `section a/b`, `shared ids` section.rs:409-419 | E0120 `section-count-violation`, E0121 `section-ids-not-disjoint`, E0122 `section-not-found` | — |
| K36 | adopter code catalog | DESIGN:79 | `RawCatalog{namespace,comprehensive,codes}` codecat.rs:51-63; `CodeCatalog.tokens` :70 | `catalogs[]` | label `code` codecat.rs:268 | E0113 `orphaned-adopter-code`; "code token" E0113.md:9 | "code body" codecat.rs:60 |
| K37 | explain catalog (engine) | DESIGN:45 "the catalog" | `explain::catalog()` mod.rs:259; schema/code-catalog.json | — | — | `explain --list` | "mdatron's explain catalog" main.rs:111; "golden code catalog" faq.md:38; "its own explain catalog" E0113.md:13 |
| K38 | diagnostic code | DESIGN:44 | `Finding.code` diagnostic.rs:283; codes.rs | DSL rule `code` types.rs:65 | `findings[].code`, `pipeline_error.code` | `MDATRON-[EWL]nnnn` | "error code" main.rs:102-104; "diagnostic code" README:158 |
| K39 | finding | DESIGN:44 | `Finding` diagnostic.rs:282; vs `Error` error.rs:4 | — | `findings[]` schema:161 | — | "diagnostic" README:22 |
| K40 | severity | DESIGN:62 "informational lint" | `Severity{Error,Warning,Lint}` diagnostic.rs:15-18; `label()` → "info" :26 | — | `error`/`warning`/`lint` schema:171; `lint_count` | explain "**Severity:** lint"; `explain --compact` "lint" | TTY header `info[MDATRON-L0001]` |
| K41 | summary (headline) | — | `Finding.summary` diagnostic.rs:285 | — | `findings[].summary` schema:174 (kebab); NB top-level `summary` = counts object schema:58 | explain H1 after em dash (mod.rs:154); code-catalog.json values | — |
| K42 | quoted region / marking | DESIGN:30 "prefix marking" | `QuotedRegion{label,content,platform_variant}` diagnostic.rs:214-237 | — | `quoted[]{label,content,origin,trusted}` schema:217-243 | — | "> " prefix |
| K43 | pipeline failure | DESIGN:44 | `VerifyError` verify.rs:127-173; `kind()` :212-223 | — | `pipeline_status`, `pipeline_error{code,kind,message}` output.rs:165-184 | E0080 `pipeline-orchestration-failure` | exit 2; README:114-118; "engine defect" README:118 (for the E0080 *finding* sense) |
| K44 | family activity tri-state | DESIGN:44 | `FamilyActivity{Active,Inert,Inactive}` output.rs:81-88 | — | `state` + `reason` schema:146-159 | — | — |
| K45 | snapshot / capture / seam | DESIGN:102-107 | snapshot.rs, `Captured` :66, `seal` :123 | — | `timings.capture_ms` | W0048 degrade | "working tree" |
| K46 | path confinement | DESIGN:70 | confine.rs: `ConfinedPath` :38, `LexicalViolation{Absolute,ParentSegment}` :50, `OpenViolation{Symlink,NotRegular,Io}` :61, `ListViolation` :303 | — | — | E0010/E0011/E0012 (see R8 for spellings) | "no-follow", "escapes the governed tree" |
| K47 | declared limits | DESIGN:110 | `Limits`/`SHIPPED` limits.rs:23,47; `BoundExceeded` verify.rs:172 | — | `pipeline_error.kind: bound_exceeded` | W0048; `MAX_EXPR_DEPTH` E0080.md:25 | kebab names limits.md:17-23 |
| K48 | version axes | DESIGN:47 "three versioned axes" | `OUTPUT_VERSION` output.rs:53; format_version.rs:27,73; `MANIFEST_VERSION` init.rs:62 | `mdatron_format_version` (4 files), `mdatron_dsl_version` (patterns), manifest `version: 2` | `mdatron_output_version`, `envelope_schema`, `mdatron_version` | — | README:433-439 |
| K49 | scope glob | DESIGN:84 | `FileScope` (CHANGELOG:56) | `require_frontmatter`, `vocabulary_globs` config.rs:36,46 | — | W0040, W0043, W0051 | "scope list" README:360 |
| K50 | incremental / dependents | DESIGN:104-108 | dep.rs `DepGraph` :50; `IncrementalReport.visited` verify.rs:302 | — | — | — | `--changed <FILE>` main.rs:74-78; "visited-file trace" |
| K51 | roles: adopter / consumer / operator / agent | DESIGN:15,30,58 | — | — | `consumer` schema:27,36,89; `adopter` schema:168,180,235 | "operator-fixable" E0080.md:37; E0022.md:13 | "consumer" = adopter in vocab.rs:93,116, DESIGN:75, main.rs:82 |
| K52 | fingerprint | DESIGN:44 | `output::fingerprints` :351 | — | `fingerprint` schema:212-215 | — | — |
| K53 | input lineage | DESIGN:44 | `digest` fields | — | `inputs` schema:103-109 | — | "governance-input lineage" |
| K54 | schema_class / artifact class | DESIGN:72 "artifact class" | `ContextSelector` types.rs:111-118 | frontmatter `schema_class`; rule `context` | — | E0002 `schema-class-unknown`, W0045 | "class" E0002.md:17-22 |

## B/C. Drift table (severity · surfaces · rename cost)

### SYNONYMS (one concept, many names)

| ID | Concept | Names and citations | Sev | Cost |
|---|---|---|---|---|
| S1 | K32 "leading bold name of a `- ` item" | `list-item-bold-name` (route.rs:116; README:402,407; DESIGN:78,147; E0112.md:18; CHANGELOG:23) ≡ `bullet-lead` (section.rs:94; README:472; E0121.md:12) ≡ `markup::list_item_bold_name` (markup.rs:301) ≡ "bullet lead" (DESIGN:80; README:482) ≡ "bold lead" (E0121.md:19). Companion: `heading` (route.rs:113, level-agnostic) vs `h3-heading` (section.rs:92, H3-locked) vs `h3` (section.rs:75, `element:` on count rules). The adopter's report. | **major** | KEY-strict: alias on `IdSource` + ledger; see D2 |
| S2 | K16 governing document | routes `governed_by` (route.rs:49) vs pins/unpinned `governing` (pin.rs:53,68); DESIGN:158-159 ratified both. Quoted labels mirror the split (`governed_by` route.rs:232,255 vs `governing` pin.rs:262,475 — FP-bearing). | **major** | KEY-strict alias + ledger; `pin --update` re-serializes (pin.rs:36 `Serialize`), precedent pin.rs:39-41; labels are FP |
| S3 | K20/K21 tombstone slot | `unpinned[].file` (pin.rs:67) vs `demoted[].path` (init.rs:135, manifest.yaml:7) — same slot, and both records are "the tombstone" (DESIGN:62 vs :159; E0060.md:34; L0001.md:9). E0060.md:34 tells adopters to hand-author `demoted[]`, so `path` is adopter-facing. | minor | INT alias on `DemotionTombstone` (`#[serde(alias="file")]`), DOC |
| S4 | K19/K23 "manifest" | pins.yaml = "pin record" (main.rs:132; pins.yaml:1; DESIGN:62 ×3) and "pin manifest" (DESIGN:17,58; Project declarations). manifest.yaml = "init manifest" (DESIGN:62,132,153; E0080.md:42), "managed manifest" (main.rs:152; E0060 summary), "managed-partition manifest" (README:65; manifest.yaml:1). | minor | DOC (pick "pin record" / "init manifest") |
| S5 | K3-K11 family names per surface | envelope singular (`citation`, `code_catalog`, `section` schema:90) vs README plural headers ("Citations" :367, "Code catalogs" :421, "Section rules" :449) vs DESIGN "X conformance"/"…integrity" (:76-80) vs modules `cite`/`vocab`/`codecat` (lib.rs:27,28,51) vs reason strings naming keys ("a route opts in with links: true" verify.rs:1308). Root word survives everywhere except `codecat`. | minor | INT (modules free), DOC |
| S6 | K10/K37 "code catalog" | adopter `code-catalogs.yaml`/`catalogs:` (codecat.rs:35,46) vs family `code_catalog` (output.rs:136) vs the ENGINE's `schema/code-catalog.json` ("golden code catalog" faq.md:38; "explain catalog" main.rs:111; E0113.md:11-13 uses both in one paragraph). Two artifacts, one name; plus plural-kebab / singular-snake / singular-kebab spread. | minor→major | rename the engine artifact's prose name (DOC/INT); adopter file name stays |
| S7 | K3/K12 "Layer 1 / Layer 2" | README:9,13 (Layer 2 = the DSL) vs README:270-273 (Layer 2 = eight families, "Layer 2 data"); lib.rs:12-14; reason "schemas supplied; Layer 1 ran" verify.rs:1277; W0047 message verify.rs:1184; E0002.md:10,25; W0045.md:11,20-21. DESIGN has no "Layer" vocabulary at all and inverts the emphasis (DESIGN:11 "the check families are the center… the DSL is a kept component serving one lane"). | **major** (two framings of the product) | DOC + REASON; explain pages DOC |
| S8 | K6 registry/register | "vocabulary registry" (DESIGN:58-59; vocab.rs:2; W0043.md:9) ≡ "naming register" (config.yaml:16; schema:95; W0043.md:10; DESIGN:121) ≡ "the register" (methodology.md:63). See O9 for the collision with `anti_patterns[].register`. | minor | DOC |
| S9 | K13 jurisdiction | "jurisdiction" (README:72; main.rs:52-55; W0046; E0030.md:10; verify.rs:96,110) ≡ "walked files"/"the walk" (W0043.md:11) ≡ "checked corpus" (W0046.md:10) ≡ `file_globs`; DESIGN uses the word once (:84). | minor | DOC (define once in DESIGN) |
| S10 | K40 lint severity | envelope `lint` (schema:171; `lint_count`), explain "**Severity:** lint", `explain --compact` "lint" (live) vs TTY finding header `info` (diagnostic.rs:22-27) vs "informational lint" (DESIGN:62,159; W0042.md:20). | minor | TTY only (INT); three-form test may pin "info" |
| S11 | K13 `--files` / `file_globs` / route `files` | CLI `--files <GLOB>...` (main.rs:52-56) = config `file_globs` (jurisdiction); route `files` (route.rs:47) = claim glob. Same word, two roles. | minor | CLI alias `--file-globs` |
| S12 | K1 product noun | "conformance engine" (DESIGN:9) vs "validator" (Cargo.toml:9; `--help`; README:3,21) vs "linter" (Cargo.toml keywords). | nit | DOC |

### OVERLOADS (one name, many concepts)

| ID | Name | Senses and citations | Sev | Cost |
|---|---|---|---|---|
| O1 | "governed" | (a) *inside the jurisdiction*: E0003 `governed-file-unreadable` (verify.rs:1397), W0040 `governed-file-has-no-frontmatter`, `GovernedFile` dep.rs:43, `governed-path-*` verify.rs:682,685 — all fire with **no routes at all**; (b) *claimed by a route / attested by a pin*: DESIGN:73-74; E0031.md:10 "governed by nothing"; pin.rs:242 "the governed file changed". | **major** | summaries are FP; DOC otherwise |
| O2 | "governed tree" | (a) project root as confinement universe: DESIGN:70; route.rs:2,305; pin.rs:287,457; E0031.md:10; E0062.md:10; README:385; verify.rs:694,749,1536; (b) the walked set: **README:334-336 sample registry entry** ("the file set inside the declared jurisdiction"), W0046.md:13, E0030.md:11. The README teaches adopters to register sense (b) while every engine message uses sense (a). | **major** | DOC (fix README sample + define in DESIGN) |
| O3 | `element` | marker `element` ∈ {heading, list-item-bold-name} (route.rs:95,111-117) vs section count `element` ∈ {h1…h6} (section.rs:54,72-80) vs disjoint operand `id_from` ∈ {h3-heading, bullet-lead} (section.rs:67,89-95). One key, two value sets; a third value set for the same idea under another key. | **major** | KEY-strict (see D2) |
| O4 | "section" | heading-spec fields (pin `section` pin.rs:60; rule `section` section.rs:52,66; `target_section` route.rs:103) — one sense; but the *family* is also `section` (output.rs:140; README:449 "Section rules"), so "section not found" (E0122) vs "section family inactive" read alike. | minor | DOC (family prose name "section-structural") |
| O5 | "pattern" | DSL pattern file (`pattern:` types.rs:19; `patterns/`; `--patterns` main.rs:48; `pattern_load` kind schema:49); marker `pattern` regex (route.rs:93); `anti_patterns[].pattern` (vocab.rs:79); `id_pattern` (section.rs:68); JSON-Schema `pattern` (DESIGN:17); `glob::Pattern` (route.rs:131; W0046.md:18 "dead pattern" = a glob); **DESIGN:9 "adopter-supplied pattern data (routes, content-hash pins, vocabularies, citations)"** = family data. | major in prose, nit in keys | DOC (DESIGN:9 fix; say "regex" / "pattern file") |
| O6 | "code" | diagnostic code (`findings[].code`), pipeline code (`pipeline_error.code`), DSL rule `code` (types.rs:65), catalog `codes` = **bodies without namespace** (codecat.rs:60-62) vs `tokens` (:70) vs "code token" (E0113.md:9,19); `explain <CODE>` "error code" (main.rs:102,104) though W/L exist. | minor | DOC (define code / code body / code token) |
| O7 | "schema" | frontmatter JSON Schema family (`--schemas`, `families.schema`, `schema_class`, E0040/E0050, W0047) vs `mdatron schema` = the **output-envelope** schema (main.rs:164-167) vs `envelope_schema` field vs DESIGN:59,133 "engine-shipped schemas" for route/pin data (realized as `deny_unknown_fields` serde structs; routes.yaml:2 "engine-shipped strict schema", no JSON Schema exists). | minor→major (`mdatron schema` is the odd one) | CLI alias; DOC |
| O8 | "route"/"unrouted" | route family (E0030 `unrouted-file`) vs W0045 `schema-class-unrouted` = "no schema and no rule `context` serves this class" (W0045.md:9-15,19-21; verify.rs:2640) — nothing to do with routes.yaml. | **major** (code summary) | FP + catalog + explain H1 + migration_note |
| O9 | "register" | (1) the naming register = K6; (2) `anti_patterns[].register` = the corrective guidance text (vocab.rs:80; README:348; label `register` vocab.rs:380); (3) E0093 `register-anti-pattern` = linguistic register; (4) `registered` term status. | major for a vocabulary adopter | KEY-strict alias for (2); FP for (3) |
| O10 | `MDATRON-E0080` | (a) pipeline failure, exit 2, `pipeline_error` (main.rs:707,886; E0080.md:9-12 "no finding-level diagnostics were emitted"); (b) an emitted **finding** for never-captured targets, exit 1, `explain_ref` E0080 (pin.rs:307-317; cite.rs:250; link.rs:395; marker.rs:250; memo.rs:9; CHANGELOG:63 "E0080 engine defect"); (c) raw stderr for CLI-level failures — stdout EPIPE, pin/init failures, `explain` lookup miss, wrong namespace (main.rs:292,357,399,509,957,1002,1014). The explain page documents only (a). | **major** (code-meaning contract, DESIGN:44) | ENV-MINOR: new code in the reserved E0080-89 range (codes.rs:34) for (b); (c) is CLI/DOC |
| O11 | "tombstone" | K20 (pins `unpinned[]`) and K21 (manifest `demoted[]`) — DESIGN:62 defines the manifest one, DESIGN:159 the pins one; L0001.md:9 and E0060.md:34 each use "tombstone" for a different record. | minor | DOC (one concept, two carriers — say so) |
| O12 | "manifest" | init manifest vs "pin manifest" (S4). | minor | DOC |
| O13 | "naming" | route `naming` = filename grammar (route.rs:50-52; W0041) vs "naming register" = prose-term registry (K6). | minor | DOC |
| O14 | "reason" | engine-authored `families[].reason` (schema:157) vs adopter justification `unpinned[].reason`/`demoted[].reason` (pin.rs:70; init.rs:136) vs quoted label `reason` for an index-degrade cause (verify.rs:1472) and for the adopter's text (pin.rs:177). | minor | DOC |
| O15 | `summary` | envelope top-level counts object (schema:58) vs `findings[].summary` headline (schema:174) vs "explain catalog summary" (main.rs:111). Both are in the 3.0.0 contract. | minor | ENV-MAJOR to fix → do not; define |
| O16 | "consumer" | envelope reader (schema:27,36,109; faq.md:71-87) vs the adopter who writes config (vocab.rs:93,116 "consumer's `label_schemes.allow`"; DESIGN:75; main.rs:82 "a consumer wiring `verify`"; DESIGN:29 "first consumer" = agents; faq.md:85 "first downstream consumer" = vsdd). | minor | DOC (define adopter / consumer / operator / agent) |
| O17 | "context" | DSL rule `context` selector (types.rs:55) vs "agent-context form" (main.rs:64) vs "cold context" (methodology.md:29). | nit | — |
| O18 | "refused" | exit-2 load refusal ("refused at load" README:417-419; `Error::Config`) vs exit-1 findings named `…-refused` (E0012 `symlinked-component-refused`; `absolute-path-refused`). | minor | DOC (verb register table) |
| O19 | "version" | six version-bearing things: `mdatron_output_version`, `envelope_schema` (URL-embedded), `mdatron_version` (crate), `mdatron_format_version`, `mdatron_dsl_version`, manifest `version: 2` (init.rs:62). DESIGN:47 says "three axes". | minor | DOC |

### ORPHANS (concept without referent, or referent without name)

| ID | Finding | Citations | Sev | Cost |
|---|---|---|---|---|
| R1 | `phases:` on a pattern is parsed, documented ("runtime-selectable subsets"), and never read — no selector on CLI or in verify. Contradicts dsl-reference.md:7-9's exactness claim. | types.rs:30; dsl-reference.md:24; grep verify.rs/main.rs: no consumer | major (documented adopter key, no behavior) | DOC (remove) or KEY-strict deprecation |
| R2 | `location: {field, expression}` on a rule is in the AST and parsed (strict) but absent from the DSL reference and unconsumed in verify. | types.rs:69,196-201; parser.rs:144,162-165; dsl-reference.md has no "location" | major (undocumented adopter key) | DOC or remove |
| R3 | DESIGN's "a tombstoned demotion emits the standing informational finding" (manifest `demoted[]`) has no code referent: L0001 fires only from pins `unpinned[]`; live self-verify on this repo (manifest.yaml:6-9 carries a demoted entry) reports `lint_count: 0`. | DESIGN:62,151 vs pin.rs:160; init.rs has no L0001; verify.rs reads no `demoted` | major (DESIGN acceptance criterion unmet or mis-worded) | ENV-MINOR if implemented; DOC if the criterion meant pins |
| R4 | DESIGN:62 "additions that permit (a new registry term, a new route, a relaxed schema) carry an annotated justification… additions carry theirs inline" — no `reason`/`owner` field exists on route entries or terms (both `deny_unknown_fields`), no lint. | route.rs:45-85; vocab.rs:54-59 | major (DESIGN term, no referent) | DOC (narrow the claim) or feature |
| R5 | `inert` is defined for every family (schema:152) but produced only by vocabulary (verify.rs:1291-1301); a route with `links: true` claiming no file reports link `active` (verify.rs:1308). | output.rs:81-88; verify.rs:1277-1326 | minor→major (the tri-state's audit claim is one-family deep) | REASON/INT |
| R6 | The rule-DSL lane has no `families` member — nothing in the envelope says whether patterns were supplied or rules ran; README calls it "Layer 2", W0045 depends on it, the schema reason says "Layer 1 ran" with no Layer-2 counterpart. | output.rs:116-141; schema:90 | major (envelope cannot express "checked nothing" for the DSL) | ENV-MINOR (additive member, schema:89 permits new keys) |
| R7 | Codes with no family: E0001-03, E0010-12, E0021/22/W0050 (DSL), E0060 (init), E0070, E0080, W0040/46/47/49/51. DESIGN:161 reserves "config/governance warnings" outside any family; the "nine families" ontology omits the config layer, the DSL, init and confinement. | codes.rs:23-43 | minor | DOC (name the non-family bucket) |
| R8 | Catalog headline never emitted: `key-source-absolute-path`/`key-source-parent-traversal` (code-catalog.json:6-7; explain E0010/E0011 H1) appear in **no** emission site. Emitted `summary` spellings: `absolute-path-refused`/`parent-segment-refused` (route.rs:298-299,533-534; pin.rs:523-524; cite.rs:113-114; link.rs:213,226; marker.rs:187-188) and `governed-path-absolute`/`governed-path-parent-traversal` (verify.rs:682,685). The `key()` case surfaces as a pipeline error (index.rs:264-271), not a finding. index.rs:273 names a fourth phantom `key-source-symlink-refused`. One code → three names, none the catalog's; `explain --list` shows a headline no finding carries. | as cited | **major** | FP (collapse to one spelling) + catalog + explain H1 + migration_note |
| R9 | `frontmatter-key` reserved element class with no referent. | route.rs:107; DESIGN:78; CHANGELOG:23 | nit | — |
| R10 | Dead DESIGN §-citations on shipped surfaces: "§ Five check families" (route.rs:1,182; pin.rs:1; vocab.rs:1; cite.rs:1; confine.rs:55,274,325; init.rs:175; verify.rs:324; index.rs:21,499,1207; docs/dependencies/serde_json.md:9, jsonschema.md:17) — heading is "Nine check families" (DESIGN:68); "§ Machine output is a public interface" (lib.rs:6); "DESIGN §Output" (main.rs:65, in `--help`); "§ Governance data is governed" (pin.rs:10; E0060.md:11,47; L0001.md:13; W0042.md:11) is a bullet in DESIGN:62, not a heading. DESIGN:44 itself says "five families"; DESIGN:58 lists inputs for six. | as cited | minor | DOC/INT |
| R11 | "SARIF output" as a shipped path in docs/dependencies/serde_json.md:9,27 and serde.md:9; DESIGN:176 puts SARIF out of scope. | as cited | minor | DOC |
| R12 | The project's own register has **no `terms:`** — `.mdatron/vocabulary.yaml:11-13` is `label_schemes` only, scoped to `docs/methodology*.md` (config.yaml:19-20; DESIGN.md and README are outside `vocabulary_globs`). The ontology this review reconstructs is registered nowhere machine-checkable; methodology-enforcement.md:63 admits it. Root cause of the drift. | as cited | major (doctrine gap) | DOC/config (see D1) |
| R13 | "L17" — a coined line-number label for a DESIGN sentence, used as a citation in DESIGN:84 and route.rs:17, vocab.rs:20, section.rs:34, marker/link docs. A letter-plus-number cluster the register would flag if DESIGN were in scope. | as cited | minor | DOC |

### LEAKS (internal/historical names on adopter-facing surfaces)

| ID | Leak | Citations (confirmed in live `--help` output) | Sev | Cost |
|---|---|---|---|---|
| L1 | Tracker numbers and review jargon rendered verbatim by clap from `///` doc comments: "(#125/#126)", "(#126)", "DESIGN §Output; #80 D4", "(#102)", "(#121, requested by vsdd-cli)", "(#175)… cold-review R4… R6", "crosslink #13 SEC/F1 + RT/F2 convergence", "(#117, vsdd W4)", "crosslink #13 AIE/F7", "(#180 — previously…)", "crosslink #13 AIE/F2", "(#84)", "(#127)", "(#180 discoverability)" ×2. Also violates methodology §5's own letter-cluster rule (SEC/F1, RT/F2, AIE/F7, W4, D4, R4, R6). | main.rs:19,52-56,59-61,64-66,74,81,88-98,104-107,111-113,117-121,125-128,132,164-166,169 | **major** | CLI (`#[arg(help=…)]` or first-paragraph-only comments) |
| L2 | Envelope schema descriptions carry "#145/#147" and "(DEF4)". | schema:89,198 | minor | DOC (description edit, PATCH) |
| L3 | Methodology vocabulary and internal history in engine-shipped explain pages (DESIGN:61 forbids methodology vocabulary in engine-shipped documentation): example `"## Decomposition (phase 1c)"` (E0063.md:25; E0114.md:36); `phase-1a`/`phase-2a` enum example (W0050.md:10-12); "the coinage-log discipline of the bootstrap period" (E0090.md:9); "SEC-F3, AIE-F2, M4" (E0091.md:9); "the review-log slug incident" (W0041.md:12); "a review round" (E0100.md:12); "one review cycle" (E0094.md:9); "The motivating incident: an engine code-range table…" (E0061.md:12-14); "Live case from the first adopter" (E0120.md:18; E0121.md:14); "nine orphaned codes across six live files after doc sunsets" (E0113.md:18). | as cited | minor (phase-1c/phase-1a: major under DESIGN:61) | DOC |
| L4 | Rust identifiers / internal framing in adopter text: `MAX_EXPR_DEPTH` (E0080.md:25; the catalog name is "DSL expression depth", limits.md:20); "the schema family (Layer 1)" in a finding message (verify.rs:1184); reason "schemas supplied; Layer 1 ran" (verify.rs:1277). | as cited | minor | DOC/REASON |
| L5 | DESIGN.md ships in the crate (Cargo.toml:21-27) carrying DEF-n, "roast", lane letters, tracker numbers, "L17" (DESIGN:29,46-47,77-80,84). By the project's own lifecycle ruling DESIGN is a standing adopter-visible document. | as cited | minor (accepted by ruling) | DOC |

### CONVENTION DRIFT

| ID | Axis | Observation | Sev | Cost |
|---|---|---|---|---|
| C1 | Activation grammar | Five loci, one principle ("presence of supplied data", DESIGN:133): directory presence (`schemas/` → schema; `patterns/` → DSL, *no family*); file presence (routes/pins/vocabulary/code-catalogs.yaml); per-route boolean (`citations`, `links`, modifier `link_root`); per-route rule list (`marker_rules`, `section_rules`); config scope that gates but "alone does not activate" (`vocabulary_globs`, verify.rs:1293) and one that activates a family-less check (`require_frontmatter` → W0040). Key shapes: plural-noun booleans vs `<noun>_rules` lists. | minor→major (one principle, five spellings) | DOC (activation table) — not a rename |
| C2 | Casing | Keys snake everywhere; adopter enum values kebab (`list-item-bold-name`, `h3-heading`, `bullet-lead`) or bare (`h3`, `registered`); envelope enum values bare (`active`, `ok`, `lint`) **except** `pipeline_error.kind` snake (`schema_load`, `pattern_load`, `index_build`, `expr_parse`, `bound_exceeded`, schema:49); summaries kebab; limit names kebab; file names single-word except `code-catalogs.yaml`; `families.code_catalog` snake. | minor | ENV-MAJOR for `kind` → do not; DOC |
| C3 | "Absent target" idiom in summaries | `dead-` (E0100/E0110/E0111/E0112), `-missing` (E0062, W0047), `-absent` (E0031), `-not-found` (E0063/E0114/E0122), `-unresolved` (E0070), `unrouted-` (E0030), `orphaned-` (E0113), `-matches-nothing` (W0043/46/51), `-unverified` (W0048), `-unreadable` (E0003/W0049). | minor | FP → do not churn; fix the idiom for new codes |
| C4 | Scope-warning naming | W0046 `jurisdiction-glob-` (key `file_globs`, role name), W0043 `vocabulary-scope-` (key `vocabulary_globs`, family+role), W0051 `require-frontmatter-scope-` (key literal). Three key→summary strategies. | minor | FP → leave; DOC |
| C5 | Reason strings | ".mdatron/routes.yaml supplied" vs "vocabulary.yaml supplied" vs "no schemas in .mdatron/schemas/" — path prefix inconsistent; "Layer 1 ran" jargon (verify.rs:1277-1326). | nit | REASON (free) |
| C6 | Quoted-region labels | `governed_by` vs `governing` (S2); `escaping-path` the only kebab label (verify.rs:712) vs `naming grammar`, `undeclared property`, `section a` (space-separated) vs `schema_class` (snake, verify.rs:2657). Labels are FP inputs. | nit | FP |
| C7 | Family ↔ module ↔ file ↔ key | schema/schema.rs/schemas/·schema_class; route/route.rs/routes.yaml/routes; pin/pin.rs/pins.yaml/pins+unpinned; vocabulary/**vocab.rs**/vocabulary.yaml/terms…; citation/**cite.rs**/—/citations; link/link.rs/—/links+link_root; marker/marker.rs/—/marker_rules; code_catalog/**codecat.rs**/code-catalogs.yaml/catalogs; section/section.rs/—/section_rules. | nit | INT |
| C8 | Outcome verbs | error = "blocks"/"fails"/"rejected"; warning = "flagged"/"warns"; exit-2 = "refused"; but O18. | nit | DOC |
| C9 | "error code" vs "diagnostic code" | main.rs:102-104 vs schema:168, README:158. | nit | CLI |

## D. Recommendations

### D1. Canonical lexicon (register as `terms:` in `.mdatron/vocabulary.yaml`; widen `vocabulary_globs` to `README.md`, `DESIGN.md`, `docs/**`)

Mechanism note (honest): the vocabulary family has no synonym concept — E0090 fires only on **bold-introduced** unregistered terms (vocab.rs:6-7) and E0092 on any use of a `reserved` term (E0092.md:9). The dogfood lever is therefore: register the canonical spelling as `registered`, and register each deprecated alias as **`reserved`** with `sense: "retired alias of X"`, so its reappearance in governed prose fires E0092. Untested for hyphenated aliases (see E).

| Term (status) | Sense |
|---|---|
| conformance engine (registered) | the product; "validator" is the crates.io blurb only |
| check family (registered) | one of the nine generic engines; names = envelope `families` keys |
| schema / route / pin / vocabulary / citation / link / marker / code-catalog / section family (registered) | the nine, spelled as the envelope keys; README headers become "Route family (`routes.yaml`)" etc. |
| rule DSL (registered) | the pattern-file lane; not a family; "Layer 2" (reserved) |
| Layer 1 (reserved) / Layer 2 (reserved) | retired README pedagogy; if kept, Layer 1 = schema family, Layer 2 = rule DSL only |
| jurisdiction (registered) | the walked file set declared by `file_globs` / `--files` |
| governed tree (registered) | the project-root directory every adopter path must resolve inside (confinement) — never the jurisdiction |
| walked file (registered) | a file in the jurisdiction; replaces sense (a) of "governed file" |
| governed file (registered) | a walked file claimed by a route or attested by a pin |
| governing document (registered) | the document a route's `governed_by` / a pin's `governed_by` names |
| target document (registered) | a marker rule's `target_doc`; not a governing document |
| section (registered) | a heading spec (`## …`) and the span it delimits; used identically by pins, section rules, `target_section` |
| element class (registered) | the by-name element a marker or section rule resolves against: `heading`, `h1`…`h6`, `list-item-bold-name` |
| bullet-lead (reserved) / bold lead (reserved) / h3-heading (reserved) | retired aliases of `list-item-bold-name` / `h3` |
| pin record (registered) / pin manifest (reserved) | `pins.yaml` |
| init manifest (registered) / managed manifest, managed-partition manifest (reserved) | `manifest.yaml` |
| tombstone (registered) | a standing removal record with `reason` + `owner`; carried in `pins.yaml` (`unpinned[]`) or the init manifest (`demoted[]`) |
| governance weakening (registered) | a removal that unguards (tombstoned) — additions are not annotated (R4) |
| naming register (registered) / registry (reserved as bare noun) | the vocabulary family and its `vocabulary.yaml` |
| register (reserved outside "naming register", "register anti-pattern") | — |
| guidance (registered) | the corrective text on an anti-pattern (today `register:`) |
| explain catalog (registered) / golden code catalog (reserved) | `schema/code-catalog.json`, the engine's own code registry |
| code catalog (registered) | the adopter's `code-catalogs.yaml` |
| diagnostic code / code body / code token (registered) | `MDATRON-E0050` / `E0050` (catalog `codes[]`) / a prefixed occurrence in prose |
| finding (registered) / diagnostic (registered) | an emitted validation outcome / its rendered form |
| summary (registered, two senses documented) | headline (kebab) on a finding; the counts object at envelope top level |
| severity: error, warning, lint (registered) / info (reserved) | TTY must render `lint` |
| pipeline failure (registered) | orchestration did not complete; exit 2; `pipeline_error` |
| engine defect (registered) | a never-captured target reported as a finding — gets its own code (D2) |
| activity: active / inert / inactive (registered) | with "inert" implemented per family (R5) |
| adopter / consumer / operator / agent (registered) | writes `.mdatron/` data / reads the envelope / the human at the terminal / the first consumer |
| pattern file, rule, regex, glob (registered) / "pattern data" (reserved) | disambiguate O5; DESIGN:9 to read "adopter-supplied family data" |
| scope glob (registered) | `require_frontmatter`, `vocabulary_globs`; `file_globs` is the walk driver (DESIGN:84) |
| snapshot, capture, capture-complete seam (registered) | as DESIGN:102-107 |

### D2. Renames worth doing in the next minor (0.7.0), with ledger rows (docs/field-rename-ledger.md:40-42 format)

1. **Section disjoint operand: `id_from` → `element`; values `h3-heading` → `h3`, `bullet-lead` → `list-item-bold-name`.** Unifies K32 into one enum shared by marker `element`, count `element`, and disjoint `element` (`heading | h1..h6 | list-item-bold-name`; marker resolution level-agnostic on `heading`, level-specific on `hN`). Also lifts the H3 hard-lock (section.rs:91). Ledger: `id_from → element | routes.yaml | 0.7.0 | format v2`; `h3-heading → h3 | routes.yaml | 0.7.0 | format v2`; `bullet-lead → list-item-bold-name | routes.yaml | 0.7.0 | format v2`. Cost: KEY-strict aliases; README:469,472; E0121.md:11-12; DESIGN:80.
2. **Pins: `governing` → `governed_by` on `pins[]` and `unpinned[]`; `pin --update` writes the new spelling** (precedent pin.rs:39-41). Quoted label `governing` → `governed_by` (FP shift on E0061/E0062 only). Ledger: `governing → governed_by | pins.yaml | 0.7.0 | format v2`. Amend DESIGN:158-159.
3. **Split E0080:** new `MDATRON-E0081 reference-target-not-captured` (error) for the pin/cite/link/marker never-captured finding (pin.rs:307-317; cite.rs:250; link.rs:395; marker.rs:250), explain page shipped; E0080 stays pipeline-only; CLI-level stderr failures (main.rs:1002,1014 `explain` misses; :292 EPIPE) stop borrowing E0080. ENV-MINOR (additive code in the reserved range; schema:168 unchanged). Rewrite E0080.md:9-12.
4. **Confinement summaries: one spelling.** Adopt `absolute-path-refused` / `parent-segment-refused` (already six sites); change verify.rs:682,685; set code-catalog.json:6-7 and explain E0010/E0011 H1 to match; drop `key-source-` wording (index.rs:264-275 comments); add `migration_note` entries. FP shifts only at the walk site.
5. **W0045 `schema-class-unrouted` → `schema-class-unvalidated`** (summary + H1 + migration_note). FP.
6. **TTY severity label `info` → `lint`** (diagnostic.rs:26); update the three-form tests.
7. **Strip tracker/review references from clap help** (main.rs:19-172): keep the first sentence as `help`, move rationale to `//` comments. CLI, free.
8. **Reason strings** (verify.rs:1277-1326): uniform `.mdatron/<file> supplied` / `no .mdatron/<file>`; drop "Layer 1 ran"; add an `inert` branch per route-attached family when its opt-in matches no walked file (R5).
9. **Add a `families.rule_dsl` member** (`active` when `patterns/` has files and rules ran; `inactive` otherwise) — ENV-MINOR, permitted by schema:89's forward-extensibility note. Closes R6.
10. **Retire `phases:`** from the DSL reference (R1) or refuse it at load; **document or remove `location:`** (R2) — dsl-reference.md:7-9 makes both a contract violation today.
11. **README:334-336 sample:** change the "governed tree" sense to the confinement sense, or switch the sample term to "jurisdiction".

### D3. Accept-but-deprecate (alias now, retire at format v2)

`id_from`, `h3-heading`, `bullet-lead`, `governing` (D2 items 1-2); `anti_patterns[].register` → alias `guidance`; `mdatron schema` → `mdatron envelope-schema` with `schema` as `visible_alias`; `--files` → `visible_alias` `--file-globs`; manifest `demoted[].path` → accept `file` (alias only; engine keeps writing `path`); prose aliases "pin manifest", "managed manifest", "golden code catalog", "Layer 1/Layer 2" (reserve in the register, keep in CHANGELOG history).

### D4. Do NOT rename — define instead

- route `files` (claim glob) vs pin `file` (exact hash target): different kinds, ruled at DESIGN:84; add one sentence to README:294.
- `target_doc` vs `governed_by`: different edges (route.rs:96-98); define "target document".
- `mdatron_dsl_version` vs `mdatron_format_version`: ratified separate axes (DESIGN:47); document the count as five version-bearing fields, three adopter-authored.
- `section` across pins/section rules/`target_section`: one sense; define once; rename only the *family's prose name* to "section-structural".
- envelope `summary` object vs `findings[].summary`, and `pipeline_error.kind` snake values: ENV-MAJOR to touch; define in the schema descriptions.
- "tombstone" with two carriers: one concept; align only the slot name by alias (D3).
- "code" senses: define code / code body / code token (D1).
- "consumer/adopter/operator/agent": define the four roles once (DESIGN Project declarations); fix the "consumer = adopter" sites (vocab.rs:93,116; DESIGN:75; main.rs:82) by wording, not by key.
- The non-family codes (R7): name the bucket ("engine and configuration diagnostics") in DESIGN and `explain --list`; do not invent a tenth family.

## E. Honest uncertainties

- Whether the walk-site spellings `governed-path-absolute`/`-parent-traversal` (verify.rs:682,685) are a deliberate fingerprint-identity split from the family sites; I found no rationale comment. D2-4 assumes not.
- R3: DESIGN:151's "tombstoned demotion emits the standing informational finding" may have been intended to be satisfied by the pins tombstone after the DESIGN:159 resolution; the live run (`lint_count: 0` with manifest.yaml:6-9 present) shows the manifest tombstone emits nothing either way — the ambiguity is in DESIGN's own wording.
- R1/R2: `phases:` and `location:` may be pre-narrowing residue kept for forward-compat rather than intended surface; either way they contradict dsl-reference.md:7-9.
- L1 was confirmed for `--help` (long help). `-h` (short help) may truncate to the first sentence and hide most of the leak; I did not diff the two.
- D1's "reserve the alias" lever assumes `find_word` (vocab.rs:466) matches hyphenated terms such as `bullet-lead` on word boundaries; not exercised.
- I read 23 of 48 explain pages in full (the rest by H1 and targeted grep), CHANGELOG and docs/dependencies/ by grep only, and no test files beyond identifier grep; a leak or alias confined to those bodies could be missed.
- Severity calls are mine; the project's own vocabulary-check doctrine (methodology.md:63-73) may weigh the register overloads (O9) higher than I did.
