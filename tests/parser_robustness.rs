//! Parser-robustness harness (GH #52 major 1, crosslink #184): every input
//! parser is driven with hostile input under the NEVER-PANIC property — every
//! outcome must be `Ok` or a structured error, caught via the API, never a
//! panic (exit 101) or an abort (SIGABRT/134). DESIGN declares the parsers a
//! trust boundary; GH #52 blockers 2 and 3 (a char-boundary panic and an
//! unguarded recursion abort) reached a green RC precisely because nothing fed
//! them malformed input.
//!
//! Two layers:
//!
//! - **Revert-detector seeds** — deterministic `#[test]`s pinning the exact
//!   GH #52 / #48-era hostile classes (multibyte-adjacent keywords, nesting
//!   floods, non-2020-12 dialects, control-byte-smuggling YAML). Each would
//!   panic, abort, or silently pass on the pre-fix code, so a revert of any of
//!   those fixes turns this file red (an abort inside the test process fails
//!   the run loudly — deliberately no fork isolation).
//! - **Property tests** (proptest, dev-dependency, MSRV-clean) — bounded
//!   randomized exploration around those classes plus fully arbitrary input.
//!   Case count and RNG seed are env-driven (`PROPTEST_CASES`,
//!   `PROPTEST_RNG_SEED`): the ordinary test matrix runs the bounded default;
//!   CI's parser-robustness job pins a seed for determinism; the deep run
//!   raises the case count on demand.
//!
//! HONEST SCOPE — covered: the DSL expression parser (parse + evaluate,
//! including evaluation against generated frontmatter-shaped contexts), the
//! pattern-file YAML loader, frontmatter parsing, JSON-Schema compile (both
//! arbitrary JSON and keyword-shaped schemas), the five `.mdatron/` YAML
//! loaders (config/route/pin/vocab/codecat), and the schema-file JSON text
//! layer via the verify load path — all at the public API. NOT covered
//! (lane-C review C2/C3 scoped this list honestly): coverage-guided byte
//! mutation (no libFuzzer/cargo-fuzz — nightly-only, tracked as the on-demand
//! deep-fuzz follow-up); the markdown BODY surface — both pulldown-cmark
//! (hardened upstream) and mdatron's OWN first-party body scanners (marker
//! and citation checks, link resolution including percent-decoding and the
//! heading-slug algorithm, inline-code ranges, section scanning, the
//! vocabulary scan), whose never-panic properties are tracked as crosslink
//! #193; the snapshot/confinement IO paths; and semantic differential
//! properties (only panic-freedom is pinned here).

use mdatron::dsl::{evaluate, parse_expression, parse_pattern_file, EvalContext, Value};
use mdatron::schema::Schema;
use mdatron::{verify, VerifyConfig};
use proptest::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// The shipped expression-depth limit — every depth bound below derives from
/// this constant, never a bare literal (lane-C review C1).
const EXPR_DEPTH_LIMIT: usize = mdatron::limits::SHIPPED.expr_depth;

// ── Helpers ────────────────────────────────────────────────────────────────

/// A fresh scratch project root (unique per call; removed on drop).
struct ScratchRoot(PathBuf);

static SCRATCH_N: AtomicU64 = AtomicU64::new(0);

impl ScratchRoot {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = SCRATCH_N.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("mdatron-fuzz-{nanos}-{n}"));
        std::fs::create_dir_all(root.join(".mdatron")).unwrap();
        Self(root)
    }
}

impl Drop for ScratchRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Parse; on success, evaluate against a null context — neither may panic.
fn expr_never_panics(input: &str) {
    if let Ok(expr) = parse_expression(input) {
        let null = Value::Null;
        let ctx = EvalContext::new(&null, &null, &null);
        let _ = evaluate(&expr, &ctx); // Ok or EvalError — both fine.
    }
}

// ── Revert-detector seeds ──────────────────────────────────────────────────

// GH #52 blocker 2: multibyte chars adjacent to keyword probe positions
// panicked on a char-boundary slice (exit 101, empty envelope).
#[test]
fn seed_multibyte_adjacent_keywords_error_cleanly() {
    for input in [
        "$self.count — required",
        "$a o€",
        "$a — $b",
        "not\u{2014}x",
        "true —",
        "$self.f in\u{fe0f} $x",
    ] {
        expr_never_panics(input);
        assert!(
            parse_expression(input).is_err(),
            "hostile input parses to a structured error: {input:?}"
        );
    }
}

// GH #52 blocker 3 (+ the #124 classes): nesting floods must be the bounded
// depth ParseError — pre-fix the bracket flood overflowed the stack, an
// uncatchable SIGABRT that would abort THIS test process (red by crash).
#[test]
fn seed_nesting_floods_are_bounded_errors() {
    for flood in [
        "[".repeat(80_000),
        "(".repeat(80_000),
        format!("{}true", "not ".repeat(20_000)),
        format!("{}1", "some(x in ".repeat(20_000)),
    ] {
        let err = parse_expression(&flood).expect_err("floods never parse");
        assert!(
            err.message.contains("maximum depth"),
            "flood is the bounded depth error; got {err}"
        );
    }
}

// GH #52 blocker 1: non-2020-12 dialects compiled clean and enforced NOTHING;
// they must refuse with MDATRON-E0040, while the supported spellings compile.
#[test]
fn seed_non_2020_12_dialects_are_refused() {
    let body =
        r#""type":"object","properties":{"name":{"enum":["a","b"]}},"additionalProperties":false"#;
    for uri in [
        "http://json-schema.org/draft-07/schema#",
        "http://json-schema.org/draft-06/schema#",
        "http://json-schema.org/draft-04/schema#",
        "https://json-schema.org/draft/2019-09/schema",
        "https://example.com/my-own-dialect/schema",
    ] {
        let json: serde_json::Value =
            serde_json::from_str(&format!(r#"{{"$schema":"{uri}",{body}}}"#)).unwrap();
        let err = Schema::compile(&json).err().map(|e| e.to_string());
        assert!(
            err.as_deref().is_some_and(|e| e.contains("MDATRON-E0040")),
            "{uri}: unsupported dialect refuses with E0040; got {err:?}"
        );
    }
    for accepted in [
        format!(r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema",{body}}}"#),
        format!(r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema#",{body}}}"#),
        format!("{{{body}}}"),
    ] {
        let json: serde_json::Value = serde_json::from_str(&accepted).unwrap();
        assert!(
            Schema::compile(&json).is_ok(),
            "the supported dialect spellings compile: {accepted}"
        );
    }
}

// #48-era hostile inputs on the pattern-file loader: control-byte smuggling,
// binary tags, small alias fans, and moderate flow nesting — each must be a
// structured Ok/Err, never a panic.
#[test]
fn seed_hostile_pattern_yaml_never_panics() {
    let hostile = [
        // ESC-smuggling ids (YAML decodes \u001b to a raw ESC).
        "mdatron_dsl_version: 1\npattern:\n  id: \"p\\u001b[31m\"\n  rules:\n    - id: \"r\\u001b[2J\"\n      context: doc\n      assert: \"true\"\n      code: T-E0001\n      message: \"m\\u0000null\"\n".to_string(),
        // Binary tag where a string is expected.
        "mdatron_dsl_version: 1\npattern: !!binary R0lGODlh\n".to_string(),
        // A small alias fan (well under any repetition guard; must not hang).
        "a: &a [1, 2]\nb: [*a, *a, *a, *a]\n".to_string(),
        // Unterminated flow collection.
        "pattern: { id: [\n".to_string(),
        // Moderate flow nesting.
        format!("pattern: {}1{}", "[".repeat(200), "]".repeat(200)),
        // U+2028 and raw tabs in odd places.
        "pattern:\u{2028}\t- x\n".to_string(),
    ];
    for yaml in hostile {
        let _ = parse_pattern_file(&yaml); // Ok or Err — both fine.
    }
}

// Hostile frontmatter: unterminated blocks, binary tags, separators, and
// control bytes — parse must return Ok/Err, never panic.
#[test]
fn seed_hostile_frontmatter_never_panics() {
    for content in [
        "---\nkey: value\n--",
        "---\nkey: !!binary AAAA\n---\nbody\n",
        "---\n\u{2028}: 1\n---\n",
        "---\na: [\n",
        "---\n\"\\x1b\": 1\n---\nbody",
        "--\nnot frontmatter\n",
        "---\n---\n",
    ] {
        let _ = mdatron::frontmatter::parse(content);
    }
}

// ── Property tests ─────────────────────────────────────────────────────────

/// Hostile expression strings: mixes of the grammar's own tokens, multibyte
/// chars at boundary-hostile positions, control bytes, and arbitrary chars.
fn hostile_expr_strategy() -> impl Strategy<Value = String> {
    let token = prop_oneof![
        Just("$self.".to_string()),
        Just("$".to_string()),
        Just("(".to_string()),
        Just(")".to_string()),
        Just("[".to_string()),
        Just("]".to_string()),
        Just("not ".to_string()),
        Just("or".to_string()),
        Just("and".to_string()),
        Just(" in ".to_string()),
        Just("not_in".to_string()),
        Just("\"".to_string()),
        Just("\\".to_string()),
        Just("—".to_string()),
        Just("€".to_string()),
        Just("\u{0}".to_string()),
        Just("\u{1b}".to_string()),
        Just("\u{2028}".to_string()),
        Just(",".to_string()),
        Just(".".to_string()),
        Just("==".to_string()),
        Just("!=".to_string()),
        Just("every(".to_string()),
        Just("count(".to_string()),
        Just("42".to_string()),
        Just("-".to_string()),
        Just("x".to_string()),
        Just("null".to_string()),
        any::<char>().prop_map(|c| c.to_string()),
    ];
    prop::collection::vec(token, 0..64).prop_map(|v| v.concat())
}

/// A bounded arbitrary JSON value for schema-compile hostility.
fn json_strategy() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::from),
        any::<i64>().prop_map(serde_json::Value::from),
        prop::collection::vec(any::<char>(), 0..12)
            .prop_map(|v| serde_json::Value::from(v.into_iter().collect::<String>())),
    ];
    leaf.prop_recursive(4, 32, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..5).prop_map(serde_json::Value::from),
            prop::collection::btree_map(
                prop::collection::vec(any::<char>(), 0..8)
                    .prop_map(|v| v.into_iter().collect::<String>()),
                inner,
                0..5
            )
            .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
        ]
    })
}

/// Recursive conversion for evaluator-context generation (C5).
fn json_to_dsl_value(j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n.as_i64().map(Value::Int).unwrap_or(Value::Null),
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Array(a) => Value::Array(a.iter().map(json_to_dsl_value).collect()),
        serde_json::Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_dsl_value(v)))
                .collect(),
        ),
    }
}

/// Objects whose keys are drawn from real JSON-Schema keywords with generated
/// values, so compile internals beyond the dialect gate are exercised (C5).
fn schema_shaped_strategy() -> impl Strategy<Value = serde_json::Value> {
    let keyword = prop_oneof![
        Just("type"),
        Just("properties"),
        Just("items"),
        Just("prefixItems"),
        Just("required"),
        Just("enum"),
        Just("const"),
        Just("pattern"),
        Just("patternProperties"),
        Just("$ref"),
        Just("$defs"),
        Just("additionalProperties"),
        Just("minItems"),
        Just("minLength"),
        Just("format"),
        Just("allOf"),
        Just("anyOf"),
        Just("not"),
    ];
    prop::collection::btree_map(keyword.prop_map(str::to_string), json_strategy(), 1..8)
        .prop_map(|m| serde_json::Value::Object(m.into_iter().collect()))
}

proptest! {
    // Expr parser + evaluator: hostile token mixes never panic; a parse
    // success must also evaluate without panicking (Ok or EvalError).
    #[test]
    fn prop_expr_parser_never_panics_on_token_mixes(input in hostile_expr_strategy()) {
        expr_never_panics(&input);
    }

    // Fully arbitrary strings (any chars, any length up to 256).
    #[test]
    fn prop_expr_parser_never_panics_on_arbitrary_strings(
        input in prop::collection::vec(any::<char>(), 0..256)
    ) {
        expr_never_panics(&input.into_iter().collect::<String>());
    }

    // Per-recursion-root floods hit the SPECIFIC depth error (lane-C review
    // C1: the previous mixed-token form was empirically inert — it PASSED on
    // the pre-fix aborting code, because incoherent mixes die shallow on
    // ordinary parse errors long before reaching guard depth, and is_err()
    // was satisfied by those. Asserting the depth message means a regressed
    // guard reds by wrong-error at small depths and by crash at abort scale;
    // abort scale itself is pinned by seed_nesting_floods_are_bounded_errors).
    #[test]
    fn prop_each_recursion_root_hits_the_depth_guard(
        root in 0u8..4,
        depth in (EXPR_DEPTH_LIMIT + 1)..(EXPR_DEPTH_LIMIT * 6)
    ) {
        let flood = match root {
            0 => "(".repeat(depth),
            1 => "[".repeat(depth),
            2 => "not ".repeat(depth),
            _ => format!("{}1", "some(x in ".repeat(depth)),
        };
        let err = parse_expression(&flood).expect_err("past-limit floods never parse");
        prop_assert!(
            err.message.contains("maximum depth"),
            "root {root} at depth {depth} must be the bounded depth error; got {err}"
        );
    }

    // Syntactically COHERENT mixed nesting past the limit hits the depth
    // guard: every opener in the sequence is legal at its position (after
    // '[', elements go through parse_primary, where 'not' is refused — so
    // the generator never places 'not ' there), which keeps the descent
    // alive until the guard fires instead of dying shallow (lane-C C1(b)).
    #[test]
    fn prop_coherent_nesting_mixes_hit_the_depth_guard(
        choices in prop::collection::vec(
            any::<u8>(),
            (EXPR_DEPTH_LIMIT + 1)..(EXPR_DEPTH_LIMIT * 4)
        )
    ) {
        let mut flood = String::new();
        let mut inside_array = false;
        for c in &choices {
            let opener = if inside_array {
                match c % 3 {
                    0 => "(",
                    1 => "[",
                    _ => "some(x in ",
                }
            } else {
                match c % 4 {
                    0 => "(",
                    1 => "[",
                    2 => "not ",
                    _ => "some(x in ",
                }
            };
            inside_array = opener == "[";
            flood.push_str(opener);
        }
        let err = parse_expression(&flood).expect_err("past-limit mixes never parse");
        prop_assert!(
            err.message.contains("maximum depth"),
            "coherent mix of {} openers must be the bounded depth error; got {err}",
            choices.len()
        );
    }

    // Arbitrary (incoherent) opener mixes: a pure NEVER-PANIC probe. This
    // deliberately claims only panic-freedom — most such mixes die shallow on
    // ordinary parse errors, so it has no depth-guard detection power (the
    // two properties above carry that; lane-C review C1(c)).
    #[test]
    fn prop_arbitrary_opener_mixes_never_panic(
        depth in 300usize..1500,
        mix in prop::collection::vec(0u8..3, 300..1500)
    ) {
        let flood: String = mix
            .iter()
            .take(depth)
            .map(|k| match k {
                0 => "(",
                1 => "[",
                _ => "not ",
            })
            .collect();
        prop_assert!(parse_expression(&flood).is_err());
    }

    // The evaluator against generated frontmatter-shaped contexts (lane-C
    // review C5: evaluating only against an all-Null context left the
    // evaluator's hostile surfaces unexplored). Realistic parseable
    // expressions are guaranteed to reach evaluate(); generated hostile
    // strings join in when they happen to parse.
    #[test]
    fn prop_eval_never_panics_with_hostile_context(
        generated in hostile_expr_strategy(),
        ctx_json in json_strategy()
    ) {
        let ctx_value = json_to_dsl_value(&ctx_json);
        let null = Value::Null;
        for input in [
            "defined($self.count)",
            "$self.count == 3",
            "\"x\" in $self.tags",
            "count(filter(m in $self.members, $m.kind == \"lane\")) == 1",
            "every(t in $self.tags, defined($t))",
            "not ($self.status == \"draft\") or $self.owner != null",
            generated.as_str(),
        ] {
            if let Ok(expr) = parse_expression(input) {
                let ctx = EvalContext::new(&ctx_value, &null, &null);
                let _ = evaluate(&expr, &ctx); // Ok or EvalError — both fine.
            }
        }
    }

    // Schema compile with KEYWORD-SHAPED objects (lane-C review C5: pure
    // json_strategy rarely emits schema keywords, so compile internals beyond
    // the dialect gate went unexercised).
    #[test]
    fn prop_schema_compile_never_panics_on_keyword_shapes(
        schema in schema_shaped_strategy()
    ) {
        let _ = Schema::compile(&schema);
    }

    // Pattern-file YAML loader: arbitrary strings never panic.
    #[test]
    fn prop_pattern_file_never_panics(
        input in prop::collection::vec(any::<char>(), 0..512)
    ) {
        let _ = parse_pattern_file(&input.into_iter().collect::<String>());
    }

    // Frontmatter parser: arbitrary bodies behind a frontmatter opener, and
    // fully arbitrary content, never panic.
    #[test]
    fn prop_frontmatter_never_panics(
        body in prop::collection::vec(any::<char>(), 0..512),
        opener in prop::bool::ANY
    ) {
        let body: String = body.into_iter().collect();
        let content = if opener { format!("---\n{body}") } else { body };
        let _ = mdatron::frontmatter::parse(&content);
    }

    // Schema compile: arbitrary JSON (with an arbitrary `$schema` injected in
    // half the cases) compiles or errors, never panics.
    #[test]
    fn prop_schema_compile_never_panics(
        mut json in json_strategy(),
        dialect in prop::option::of(prop::collection::vec(any::<char>(), 0..24))
    ) {
        if let (Some(d), Some(obj)) = (dialect, json.as_object_mut()) {
            obj.insert(
                "$schema".into(),
                serde_json::Value::from(d.into_iter().collect::<String>()),
            );
        }
        let _ = Schema::compile(&json);
    }
}

proptest! {
    // The five `.mdatron/` YAML loaders + the schema-file JSON text layer:
    // arbitrary bytes on disk never panic. File-backed, so bounded tighter
    // than the pure parsers (this explicit `cases` also means the deep
    // workflow_dispatch profile does NOT deepen this property — deliberate).
    #![proptest_config(ProptestConfig {
        cases: 24,
        ..ProptestConfig::default()
    })]
    #[test]
    fn prop_loaders_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let scratch = ScratchRoot::new();
        let root = &scratch.0;
        for name in [
            "config.yaml",
            "routes.yaml",
            "pins.yaml",
            "vocabulary.yaml",
            "code-catalogs.yaml",
            "manifest.yaml",
        ] {
            std::fs::write(root.join(".mdatron").join(name), &bytes).unwrap();
        }
        let _ = mdatron::config::load(root);
        let _ = mdatron::init::load_tombstones(root);
        let _ = mdatron::route::load(root);
        let _ = mdatron::pin::load(root);
        let _ = mdatron::vocab::load(root);
        let _ = mdatron::codecat::load(root);

        // The schema-file JSON TEXT layer (lane-C review C2): a separate
        // scratch where ONLY a schema file is hostile, driven through the
        // real verify load path so serde_json::from_str sees the raw bytes
        // (the pure-compile properties above feed already-parsed Values).
        let schema_scratch = ScratchRoot::new();
        let sroot = &schema_scratch.0;
        std::fs::create_dir_all(sroot.join(".mdatron/schemas")).unwrap();
        std::fs::write(sroot.join(".mdatron/schemas/h.json"), &bytes).unwrap();
        let _ = verify(&VerifyConfig::new(sroot.clone()));
    }
}
