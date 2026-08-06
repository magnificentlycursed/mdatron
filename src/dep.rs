//! Dependency resolver (#100, sub-lane B of #92: incremental verify).
//!
//! Computes the transitive dependent set of a changed file over three
//! undirected edge kinds (`DESIGN.md` § Verification is fast where it is
//! invoked):
//!
//! - **governance** — a governed file and its governing document, per the route
//!   table (either direction).
//! - **rule-reference** — a rule's context files and the source files of any
//!   index the rule's `key()` calls name (either direction).
//! - **shared-key** — two files that contribute the same key to one index,
//!   naming neither each other nor a rule.
//!
//! The graph is built once per invocation; `dependents` is a breadth-first
//! closure over it. This module is pure graph logic — snapshotting, mode
//! selection, and the `.mdatron/`-change → whole-tree rule live in later
//! sub-lanes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::confine;
use crate::dsl::expr::Expr;
use crate::dsl::expr_parser::parse_expression;
use crate::dsl::index::IndexRegistry;
use crate::dsl::PatternFile;
use crate::route::Route;

/// Normalize an adopter-supplied path string to the same lexical form the
/// confined walk produces for governed files and index sources (#100, F3): a
/// leading `./` on `governed_by` must not split it into a distinct graph node
/// from the file it names. Falls back to the raw string if it does not confine
/// (routes are confined at load, so this is a defensive default).
fn normalize_node(raw: &str) -> PathBuf {
    confine::confine_lexically(Path::new(raw))
        .map(|c| c.as_path().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(raw))
}

/// A governed file the resolver reasons about: its root-relative path and its
/// `schema_class` (`None` when the file carries no frontmatter).
#[derive(Debug, Clone)]
pub struct GovernedFile {
    pub path: PathBuf,
    pub schema_class: Option<String>,
}

/// Undirected dependency graph over governed files and their governing docs.
#[derive(Debug, Default)]
pub struct DepGraph {
    adj: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
}

impl DepGraph {
    fn connect(&mut self, a: &Path, b: &Path) {
        if a == b {
            return;
        }
        self.adj
            .entry(a.to_path_buf())
            .or_default()
            .insert(b.to_path_buf());
        self.adj
            .entry(b.to_path_buf())
            .or_default()
            .insert(a.to_path_buf());
    }

    /// Build the graph from the walked governed files, the route table, the
    /// pattern rules, and the (provenance-bearing) index registry.
    pub fn build(
        files: &[GovernedFile],
        routes: &[Route],
        patterns: &[PatternFile],
        registry: &IndexRegistry,
    ) -> Self {
        let mut g = DepGraph::default();

        // 1. Governance: each governed file to its governing document. The
        //    governing-doc node is normalized so a `./`-prefixed governed_by
        //    coincides with the file it names (F3).
        for f in files {
            for r in routes {
                if r.files.matches_path(&f.path) {
                    g.connect(&f.path, &normalize_node(&r.governed_by));
                }
            }
        }

        // 2. Shared-key: files contributing the same key to one index are
        //    coupled even though neither names the other. ("keyed rule" in the
        //    DESIGN definition is the same registry-index coupling — key() is
        //    the sole cross-file mechanism.)
        for index in registry.indices.values() {
            for contributors in index.provenance.values() {
                let list: Vec<&PathBuf> = contributors.iter().collect();
                for i in 0..list.len() {
                    for j in (i + 1)..list.len() {
                        g.connect(list[i], list[j]);
                    }
                }
            }
        }

        // 3. Rule-reference: a rule's context files to the source files of every
        //    index its key() calls name (assert + let-binding expressions).
        for pf in patterns {
            for rule in &pf.pattern.rules {
                let mut referenced: BTreeSet<String> = BTreeSet::new();
                let mut dynamic = false;
                collect_key_refs(&rule.assert, &mut referenced, &mut dynamic);
                for (_binding, expr_src) in &rule.let_bindings {
                    collect_key_refs(expr_src, &mut referenced, &mut dynamic);
                }
                // A key() can also live in the message: interpolation evaluates
                // every {{...}} slot, so it is a real cross-file dependency (F1).
                collect_message_key_refs(&rule.message, &mut referenced, &mut dynamic);
                if referenced.is_empty() && !dynamic {
                    continue;
                }
                let ctx_files: Vec<&PathBuf> = files
                    .iter()
                    .filter(|f| {
                        crate::verify::context_matches(
                            &rule.context,
                            f.schema_class.as_deref(),
                            &f.path,
                        )
                    })
                    .map(|f| &f.path)
                    .collect();
                // A dynamic (runtime-computed) index name could resolve to any
                // index, so connect to every index's sources — conservative, the
                // no-missing-edge choice (F2). Otherwise, only the named indices.
                let targets: Vec<&crate::dsl::index::Index> = if dynamic {
                    registry.indices.values().collect()
                } else {
                    referenced
                        .iter()
                        .filter_map(|n| registry.indices.get(n))
                        .collect()
                };
                for index in targets {
                    for src in &index.sources {
                        for cf in &ctx_files {
                            g.connect(cf, src);
                        }
                    }
                }
            }
        }

        g
    }

    /// Every file reachable from `changed` through the edges, excluding
    /// `changed` itself. A file with no edges has an empty dependent set.
    pub fn dependents(&self, changed: &Path) -> BTreeSet<PathBuf> {
        let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
        seen.insert(changed.to_path_buf());
        let mut stack: Vec<PathBuf> = vec![changed.to_path_buf()];
        while let Some(cur) = stack.pop() {
            if let Some(neighbors) = self.adj.get(&cur) {
                for n in neighbors {
                    if seen.insert(n.clone()) {
                        stack.push(n.clone());
                    }
                }
            }
        }
        seen.remove(changed);
        seen
    }
}

/// Parse an expression string and collect the index names named by `key()`
/// calls. A string-literal first argument is a named index; a non-literal
/// (runtime-computed) first argument sets `dynamic`. A parse failure contributes
/// nothing — the rule surfaces its own parse diagnostic in the normal pipeline;
/// the resolver stays conservative rather than erroring.
fn collect_key_refs(expr_src: &str, out: &mut BTreeSet<String>, dynamic: &mut bool) {
    if let Ok(expr) = parse_expression(expr_src) {
        walk_expr(&expr, out, dynamic);
    }
}

/// Scan a message template's `{{...}}` slots for `key()` references — the
/// interpolator evaluates every slot, so they are real cross-file dependencies
/// (#100, F1). Mirrors the slot-splitting in `verify::interpolate_message`.
fn collect_message_key_refs(message: &str, out: &mut BTreeSet<String>, dynamic: &mut bool) {
    let bytes = message.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(close_rel) = message[i + 2..].find("}}") {
                let expr_str = message[i + 2..i + 2 + close_rel].trim();
                collect_key_refs(expr_str, out, dynamic);
                i = i + 2 + close_rel + 2;
                continue;
            }
        }
        i += 1;
    }
}

fn walk_expr(expr: &Expr, out: &mut BTreeSet<String>, dynamic: &mut bool) {
    match expr {
        Expr::Call(name, args) => {
            if name == "key" {
                match args.first() {
                    // A string literal names an index directly.
                    Some(Expr::Lit(v)) => {
                        if let Some(s) = v.as_str() {
                            out.insert(s.to_string());
                        }
                        // A non-string literal is a broken rule (eval errors on
                        // it), forming no real edge — ignore.
                    }
                    // A computed first argument resolves at runtime to some
                    // index name; treat the rule as referencing all of them.
                    Some(_) => *dynamic = true,
                    None => {}
                }
            }
            for a in args {
                walk_expr(a, out, dynamic);
            }
        }
        Expr::Field(inner, _) | Expr::Not(inner) => walk_expr(inner, out, dynamic),
        Expr::Eq(a, b)
        | Expr::Ne(a, b)
        | Expr::And(a, b)
        | Expr::Or(a, b)
        | Expr::In(a, b)
        | Expr::NotIn(a, b)
        | Expr::Every(_, a, b)
        | Expr::Some_(_, a, b)
        | Expr::Filter(_, a, b) => {
            walk_expr(a, out, dynamic);
            walk_expr(b, out, dynamic);
        }
        Expr::Lit(_) | Expr::Var(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::index::Index;
    use crate::dsl::{ContextSelector, Pattern, PatternFile, Rule};

    fn pb(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn gf(path: &str, schema: Option<&str>) -> GovernedFile {
        GovernedFile {
            path: pb(path),
            schema_class: schema.map(String::from),
        }
    }

    fn route(files_glob: &str, governed_by: &str) -> Route {
        Route {
            files: glob::Pattern::new(files_glob).unwrap(),
            governed_by: governed_by.to_string(),
            naming: None,
            citations: false,
            links: false,
            link_root: false,
            marker_rules: Vec::new(),
            section_rules: Vec::new(),
        }
    }

    /// An index with hand-specified provenance (`key -> files`) and source set.
    fn index(name: &str, prov: &[(&str, &[&str])], sources: &[&str]) -> Index {
        let mut provenance = BTreeMap::new();
        for (k, files) in prov {
            provenance.insert(k.to_string(), files.iter().map(|f| pb(f)).collect());
        }
        Index {
            name: name.to_string(),
            entries: BTreeMap::new(),
            provenance,
            sources: sources.iter().map(|f| pb(f)).collect(),
        }
    }

    fn registry(indices: Vec<Index>) -> IndexRegistry {
        let mut r = IndexRegistry::new();
        for i in indices {
            r.insert(i);
        }
        r
    }

    /// A one-rule pattern whose `assert` carries the `key()` calls.
    fn rule_pattern(context: ContextSelector, assert: &str) -> PatternFile {
        PatternFile {
            mdatron_dsl_version: 1,
            pattern: Pattern {
                id: "p".into(),
                description: None,
                phases: vec![],
                keys: vec![],
                rules: vec![Rule {
                    id: "r".into(),
                    context,
                    let_bindings: vec![],
                    assert: assert.into(),
                    // Adopter-supplied code (the resolver ignores it); kept out
                    // of the MDATRON-* namespace the reserved-range scan checks.
                    code: "ADOPTER-E0001".into(),
                    message: "m".into(),
                    location: None,
                }],
            },
        }
    }

    fn set(items: &[&str]) -> BTreeSet<PathBuf> {
        items.iter().map(|s| pb(s)).collect()
    }

    fn mk_rule(
        context: ContextSelector,
        lets: &[(&str, &str)],
        assert: &str,
        message: &str,
    ) -> Rule {
        Rule {
            id: "r".into(),
            context,
            let_bindings: lets
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            assert: assert.into(),
            code: "ADOPTER-E0001".into(),
            message: message.into(),
            location: None,
        }
    }

    fn pat(rules: Vec<Rule>) -> PatternFile {
        PatternFile {
            mdatron_dsl_version: 1,
            pattern: Pattern {
                id: "p".into(),
                description: None,
                phases: vec![],
                keys: vec![],
                rules,
            },
        }
    }

    // FIXTURE (a): a three-file transitive chain via rule-reference edges.
    // A's rule references index i1 (source B); B's rule references i2 (source C).
    // The hand-authored dependent set of A is {B, C}.
    #[test]
    fn transitive_chain_of_three() {
        let files = [
            gf("a.md", Some("a")),
            gf("b.md", Some("b")),
            gf("c.md", Some("c")),
        ];
        let patterns = [
            rule_pattern(
                ContextSelector::Bare("a".into()),
                "key(\"i1\", \"x\") == \"\"",
            ),
            rule_pattern(
                ContextSelector::Bare("b".into()),
                "key(\"i2\", \"x\") == \"\"",
            ),
        ];
        let reg = registry(vec![
            index("i1", &[], &["b.md"]),
            index("i2", &[], &["c.md"]),
        ]);
        let g = DepGraph::build(&files, &[], &patterns, &reg);
        assert_eq!(g.dependents(&pb("a.md")), set(&["b.md", "c.md"]));
        assert_eq!(g.dependents(&pb("c.md")), set(&["a.md", "b.md"]));
    }

    // FIXTURE (b): shared-key — two files contribute the same key to one index,
    // naming neither each other nor a rule. Their hand-authored coupling is direct.
    #[test]
    fn shared_key_couples_files_naming_neither() {
        let reg = registry(vec![index(
            "people",
            &[("alice", &["x.md", "y.md"])],
            &["x.md", "y.md"],
        )]);
        let g = DepGraph::build(&[], &[], &[], &reg);
        assert_eq!(g.dependents(&pb("x.md")), set(&["y.md"]));
        assert_eq!(g.dependents(&pb("y.md")), set(&["x.md"]));
        // a key with a single contributor forms no edge
        let reg2 = registry(vec![index("solo", &[("k", &["only.md"])], &["only.md"])]);
        let g2 = DepGraph::build(&[], &[], &[], &reg2);
        assert!(g2.dependents(&pb("only.md")).is_empty());
    }

    // FIXTURE (c): governance — governed file <-> governing document, both ways.
    #[test]
    fn governance_edge_both_directions() {
        let files = [gf("review-log/a.md", None), gf("review-log/b.md", None)];
        let routes = [route("review-log/**/*.md", "DESIGN.md")];
        let g = DepGraph::build(&files, &routes, &[], &registry(vec![]));
        // governed -> governing
        assert_eq!(
            g.dependents(&pb("review-log/a.md")),
            set(&["DESIGN.md", "review-log/b.md"])
        );
        // governing -> all its governed files
        assert_eq!(
            g.dependents(&pb("DESIGN.md")),
            set(&["review-log/a.md", "review-log/b.md"])
        );
    }

    // FIXTURE (d): rule-reference — context files <-> index source files, both ways.
    #[test]
    fn rule_reference_edge_both_directions() {
        let files = [gf("a.md", Some("post")), gf("registry.md", None)];
        let patterns = [rule_pattern(
            ContextSelector::Bare("post".into()),
            "key(\"reg\", $self.id) == $self.id",
        )];
        let reg = registry(vec![index("reg", &[], &["registry.md"])]);
        let g = DepGraph::build(&files, &[], &patterns, &reg);
        assert_eq!(g.dependents(&pb("a.md")), set(&["registry.md"])); // context -> source
        assert_eq!(g.dependents(&pb("registry.md")), set(&["a.md"])); // source -> context
    }

    // The `let`-binding scan is live: a key() reached only through a binding
    // forms the rule-reference edge (guards the let loop, F4).
    #[test]
    fn key_in_let_binding_forms_edge() {
        let files = [gf("a.md", Some("a"))];
        let patterns = [pat(vec![mk_rule(
            ContextSelector::Bare("a".into()),
            &[("idx", "key(\"i\", \"x\")")],
            "$idx == \"\"",
            "m",
        )])];
        let reg = registry(vec![index("i", &[], &["b.md"])]);
        let g = DepGraph::build(&files, &[], &patterns, &reg);
        assert_eq!(g.dependents(&pb("a.md")), set(&["b.md"]));
    }

    // F1: a key() reached only through the message's {{...}} forms the edge —
    // interpolation evaluates it, so it is a real dependency.
    #[test]
    fn key_in_message_forms_edge() {
        let files = [gf("a.md", Some("a"))];
        let patterns = [pat(vec![mk_rule(
            ContextSelector::Bare("a".into()),
            &[],
            "$self.x == \"\"",
            "see {{key(\"i\", $self.x)}}",
        )])];
        let reg = registry(vec![index("i", &[], &["b.md"])]);
        let g = DepGraph::build(&files, &[], &patterns, &reg);
        assert_eq!(g.dependents(&pb("a.md")), set(&["b.md"]));
    }

    // F2: a runtime-computed index name connects to every index's sources
    // (conservative, no missing edge).
    #[test]
    fn dynamic_index_name_connects_all_indices() {
        let files = [gf("a.md", Some("a"))];
        let patterns = [pat(vec![mk_rule(
            ContextSelector::Bare("a".into()),
            &[],
            "key($self.idx, \"x\") == \"\"",
            "m",
        )])];
        let reg = registry(vec![
            index("i1", &[], &["b.md"]),
            index("i2", &[], &["c.md"]),
        ]);
        let g = DepGraph::build(&files, &[], &patterns, &reg);
        assert_eq!(g.dependents(&pb("a.md")), set(&["b.md", "c.md"]));
    }

    // The three edge kinds compose into one transitive closure:
    // a --rule-ref--> x --shared-key--> y --governance--> gov.md
    #[test]
    fn edge_kinds_compose_transitively() {
        let files = [gf("a.md", Some("a")), gf("y.md", None)];
        let patterns = [pat(vec![mk_rule(
            ContextSelector::Bare("a".into()),
            &[],
            "key(\"i\", \"k\") == \"\"",
            "m",
        )])];
        let routes = [route("y.md", "gov.md")];
        let reg = registry(vec![
            index("i", &[], &["x.md"]),
            index("j", &[("k", &["x.md", "y.md"])], &["x.md", "y.md"]),
        ]);
        let g = DepGraph::build(&files, &routes, &patterns, &reg);
        assert_eq!(g.dependents(&pb("a.md")), set(&["x.md", "y.md", "gov.md"]));
    }

    // A rule on a file that is also its index's source forms no self-edge (the
    // a==b guard), but still couples to the index's other sources.
    #[test]
    fn self_referential_rule_forms_no_self_edge() {
        let files = [gf("x.md", Some("a"))];
        let patterns = [pat(vec![mk_rule(
            ContextSelector::Bare("a".into()),
            &[],
            "key(\"i\", \"k\") == \"\"",
            "m",
        )])];
        let reg = registry(vec![index("i", &[], &["x.md", "y.md"])]);
        let g = DepGraph::build(&files, &[], &patterns, &reg);
        let deps = g.dependents(&pb("x.md"));
        assert!(!deps.contains(&pb("x.md")), "no self-edge");
        assert_eq!(deps, set(&["y.md"]));
    }

    // F3: a `./`-prefixed governed_by normalizes to the same node as the file it
    // names, so the governance edge connects in both directions.
    #[test]
    fn governed_by_with_leading_dot_slash_still_connects() {
        let files = [gf("a.md", None)];
        let routes = [route("a.md", "./DESIGN.md")];
        let g = DepGraph::build(&files, &routes, &[], &registry(vec![]));
        assert_eq!(g.dependents(&pb("a.md")), set(&["DESIGN.md"]));
        assert_eq!(g.dependents(&pb("DESIGN.md")), set(&["a.md"]));
    }

    // A file with no edges has an empty dependent set (no spurious coupling).
    #[test]
    fn unconnected_file_has_no_dependents() {
        let reg = registry(vec![index(
            "people",
            &[("alice", &["x.md", "y.md"])],
            &["x.md", "y.md"],
        )]);
        let g = DepGraph::build(&[], &[], &[], &reg);
        assert!(g.dependents(&pb("unrelated.md")).is_empty());
    }

    // B1 end-to-end: real files feeding one index produce a shared-key edge via
    // IndexRegistry::build's provenance (not a hand-built Index).
    #[test]
    fn provenance_from_real_files_drives_shared_key() {
        use crate::dsl::KeyDecl;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("dep-prov-{nanos}"));
        std::fs::create_dir_all(root.join("people")).unwrap();
        // Two files that both declare id "alice" — the shared key.
        std::fs::write(root.join("people/one.md"), "---\nid: alice\n---\n# one\n").unwrap();
        std::fs::write(root.join("people/two.md"), "---\nid: alice\n---\n# two\n").unwrap();
        let decl = KeyDecl {
            name: "people".into(),
            source: "people/*.md".into(),
            select: "$.frontmatter".into(),
            indexed_by: "$.id".into(),
        };
        let reg = IndexRegistry::build(&root, &[decl]).unwrap();
        let g = DepGraph::build(&[], &[], &[], &reg);
        assert_eq!(g.dependents(&pb("people/one.md")), set(&["people/two.md"]));
        let _ = std::fs::remove_dir_all(&root);
    }
}
