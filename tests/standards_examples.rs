// Test code: an unwrap IS the assertion — opt out of the [lints.clippy]
// panic-path restrictions production code is held to (#185).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The standards pack (#181) as self-testing documentation: every example
//! project under `examples/standards/` must verify clean; every seeded
//! violation a cookbook page shows must produce exactly the output the page
//! prints; every configuration file the page lists must equal the example's;
//! and every schema's `x-source` provenance must match the page's
//! "Evaluated against" table, so a rule cannot change without its source and
//! its documentation changing with it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mdatron"))
}

/// Normalize a platform artifact away: CRLF (a Windows autocrlf checkout),
/// the OS path separator in rendered locations, and Windows' wording of the
/// not-found OS error, which the pages show in its Unix form.
fn norm(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\\', "/").replace(
        "The system cannot find the file specified. (os error 2)",
        "No such file or directory (os error 2)",
    )
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            fs::copy(&src, &dst).unwrap();
        }
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn of(example: &str, case: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "mdatron-standards-{example}-{case}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        copy_dir(&repo().join("examples/standards").join(example), &dir);
        Scratch(dir)
    }
    fn edit(&self, rel: &str, f: impl FnOnce(String) -> String) {
        let p = self.0.join(rel);
        let s = fs::read_to_string(&p).unwrap().replace("\r\n", "\n");
        let edited = f(s.clone());
        assert_ne!(
            edited, s,
            "{rel}: the seeded edit changed nothing (fixture drift?)"
        );
        fs::write(&p, edited).unwrap();
    }
}
impl Scratch {
    fn rename(&self, from: &str, to: &str) {
        fs::rename(self.0.join(from), self.0.join(to)).unwrap();
    }
    fn create(&self, rel: &str, content: &str) {
        let p = self.0.join(rel);
        assert!(
            !p.exists(),
            "{rel}: already in the example (fixture drift?)"
        );
        fs::write(&p, content).unwrap();
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn verify(root: &Path, json: bool) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(bin());
    cmd.arg("verify").arg("--project-root").arg(root);
    if json {
        cmd.arg("--json");
    }
    let out = cmd.output().unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The fenced block that follows `<!-- {marker} -->` on a page.
fn block_after(page: &str, marker: &str) -> String {
    let tag = format!("<!-- {marker} -->");
    let at = page
        .find(&tag)
        .unwrap_or_else(|| panic!("the page lacks `{tag}`"));
    let rest = &page[at + tag.len()..];
    let open = rest.find("```").unwrap();
    let body_start = open + rest[open..].find('\n').unwrap() + 1;
    let close = body_start + rest[body_start..].find("\n```").unwrap();
    rest[body_start..=close].to_string()
}

fn page(name: &str) -> String {
    norm(&fs::read_to_string(repo().join("docs/cookbook").join(name)).unwrap())
}

#[test]
fn every_example_project_verifies_clean() {
    let mut seen = 0;
    for e in fs::read_dir(repo().join("examples/standards"))
        .unwrap()
        .flatten()
    {
        if !e.path().is_dir() {
            continue;
        }
        seen += 1;
        let (code, stdout, stderr) = verify(&e.path(), true);
        assert_eq!(code, Some(0), "{}: {stderr}", e.path().display());
        let env: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(
            env["findings"].as_array().unwrap().len(),
            0,
            "{}",
            e.path().display()
        );
    }
    assert!(seen > 0, "no example projects found");
}

/// The seeded violations the SKILL.md recipe shows: one edit each.
fn skill_md_case(case: &str) -> Scratch {
    let s = Scratch::of("skill-md", case);
    let spec = ".agents/skills/pdf-tools/SKILL.md";
    let cc = ".claude/skills/release-notes/SKILL.md";
    match case {
        "upload-trap" => s.edit(spec, |t| {
            t.replace(
                "license: Apache-2.0",
                "argument-hint: \"[file]\"\nlicense: Apache-2.0",
            )
        }),
        "name-dir" => s.edit(spec, |t| t.replace("name: pdf-tools", "name: pdf-tool")),
        "blank-first-line" => s.edit(cc, |t| format!("\n{t}")),
        "typo-key" => s.edit(cc, |t| t.replace("when_to_use:", "when-to-use:")),
        "agent-without-fork" => s.edit(cc, |t| t.replace("context: fork\n", "")),
        other => panic!("unknown case {other}"),
    }
    s
}

const SKILL_MD_CASES: &[&str] = &[
    "upload-trap",
    "name-dir",
    "blank-first-line",
    "typo-key",
    "agent-without-fork",
];

#[test]
fn skill_md_page_shows_the_real_output_of_each_case() {
    let page = page("skill-md.md");
    for case in SKILL_MD_CASES {
        let s = skill_md_case(case);
        let (code, _, stderr) = verify(&s.0, false);
        assert_eq!(code, Some(1), "{case}: a seeded violation fails the run");
        assert_eq!(
            norm(&stderr),
            block_after(&page, &format!("cookbook-case: {case}")),
            "{case}: the page's output block must be the real output"
        );
    }
    // The envelope view of the headline case, field for field.
    let s = skill_md_case("upload-trap");
    let (_, stdout, _) = verify(&s.0, true);
    let env: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let shown: serde_json::Value =
        serde_json::from_str(&block_after(&page, "cookbook-envelope: upload-trap")).unwrap();
    assert_eq!(env["findings"][0], shown);
    // The loop closes: reverting the edit is clean again.
    s.edit(".agents/skills/pdf-tools/SKILL.md", |t| {
        t.replace("argument-hint: \"[file]\"\n", "")
    });
    let (code, _, stderr) = verify(&s.0, false);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(norm(&stderr).contains("mdatron verify: clean"));
}

#[test]
fn skill_md_page_lists_the_example_configuration_verbatim() {
    let page = page("skill-md.md");
    let root = repo().join("examples/standards/skill-md");
    for rel in [
        ".mdatron/config.yaml",
        ".mdatron/routes.yaml",
        ".mdatron/patterns/claude-code-skill.yaml",
    ] {
        let file = norm(&fs::read_to_string(root.join(rel)).unwrap());
        assert_eq!(
            block_after(&page, &format!("cookbook-file: {rel}")),
            file,
            "{rel}: the page's listing must equal the example file"
        );
    }
}

/// Every example schema carries a complete `x-source` provenance block, and
/// each page's "Evaluated against" table lists exactly its example's sources.
#[test]
fn provenance_blocks_are_complete_and_match_the_pages() {
    for (example, page_name) in [
        ("skill-md", "skill-md.md"),
        ("subagents", "subagents.md"),
        ("copilot-instructions", "copilot-instructions.md"),
        ("llms-txt", "llms-txt.md"),
        ("agents-md", "agents-md.md"),
    ] {
        let dir = repo()
            .join("examples/standards")
            .join(example)
            .join(".mdatron/schemas");
        let mut sources: BTreeSet<Vec<String>> = BTreeSet::new();
        // Provenance carriers: every schema's `x-source` block, plus an
        // `x-source.json` beside the example for a recipe with no schema.
        let mut carriers: Vec<(PathBuf, serde_json::Value)> = Vec::new();
        for e in fs::read_dir(&dir).unwrap().flatten() {
            if e.path().extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let schema: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(e.path()).unwrap()).unwrap();
            carriers.push((e.path(), schema["x-source"].clone()));
        }
        let side = dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("x-source.json");
        if side.exists() {
            let v: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&side).unwrap()).unwrap();
            carriers.push((side, v));
        }
        assert!(!carriers.is_empty(), "{example}: no provenance carrier");
        for (path, value) in carriers {
            let blocks = value
                .as_array()
                .unwrap_or_else(|| panic!("{}: no x-source block", path.display()));
            assert!(!blocks.is_empty(), "{}: empty x-source", path.display());
            for b in blocks {
                let row: Vec<String> =
                    ["standard", "url", "revision", "revision_date", "retrieved"]
                        .iter()
                        .map(|k| {
                            let v = b[k].as_str().unwrap_or_else(|| {
                                panic!("{}: x-source lacks `{k}`", path.display())
                            });
                            assert!(!v.trim().is_empty(), "{}: empty `{k}`", path.display());
                            v.to_string()
                        })
                        .collect();
                sources.insert(row);
            }
        }
        let page = page(page_name);
        let start = page.find("<!-- cookbook-provenance-start -->").unwrap();
        let end = page.find("<!-- cookbook-provenance-end -->").unwrap();
        let rows: BTreeSet<Vec<String>> = page[start..end]
            .lines()
            .filter(|l| {
                l.starts_with("| ") && !l.starts_with("| Standard") && !l.starts_with("|---")
            })
            .map(|l| {
                l.trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().to_string())
                    .collect()
            })
            .collect();
        assert_eq!(
            rows, sources,
            "{page_name}: the provenance table must match the schemas' x-source blocks"
        );
    }
}

/// The seeded silent skips the subagents recipe shows: one edit each.
fn subagents_case(case: &str) -> Scratch {
    let s = Scratch::of("subagents", case);
    let p = ".claude/agents/code-reviewer.md";
    let g = "plugin/agents/review/security.md";
    match case {
        "no-name" => s.edit(p, |t| t.replace("name: code-reviewer\n", "")),
        "no-description" => s.edit(p, |t| {
            t.replace(
                "description: Reviews a diff for correctness, security and style. Use after a change is ready, before committing.\n",
                "",
            )
        }),
        "colon-name" => s.edit(p, |t| t.replace("name: code-reviewer", "name: team:code-reviewer")),
        "blank-first-line" => s.edit(p, |t| format!("\n{t}")),
        "unparseable-yaml" => s.edit(p, |t| t.replace("tools: Read, Grep, Glob, Bash", "tools: [Read, Grep")),
        "top-level-cachettl" => s.edit(p, |t| t.replace("experimental:\n  cacheTtl: 1h\n", "cacheTtl: 1h\n")),
        "snake-case-key" => s.edit(p, |t| t.replace("maxTurns: 20", "max_turns: 20")),
        "plugin-permission-mode" => s.edit(g, |t| {
            t.replace("effort: high\n", "effort: high\npermissionMode: acceptEdits\n")
        }),
        other => panic!("unknown case {other}"),
    }
    s
}

const SUBAGENTS_CASES: &[&str] = &[
    "no-name",
    "no-description",
    "colon-name",
    "unparseable-yaml",
    "blank-first-line",
    "top-level-cachettl",
    "snake-case-key",
    "plugin-permission-mode",
];

#[test]
fn subagents_page_shows_the_real_output_of_each_case() {
    let page = page("subagents.md");
    for case in SUBAGENTS_CASES {
        let s = subagents_case(case);
        let (code, _, stderr) = verify(&s.0, false);
        assert_eq!(
            code,
            Some(1),
            "{case}: a seeded defect fails the run: {stderr}"
        );
        assert_eq!(
            norm(&stderr),
            block_after(&page, &format!("cookbook-case: {case}")),
            "{case}: the page's output block must be the real output"
        );
    }
}

#[test]
fn subagents_page_lists_the_example_configuration_verbatim() {
    let page = page("subagents.md");
    let root = repo().join("examples/standards/subagents");
    for rel in [".mdatron/config.yaml", ".mdatron/routes.yaml"] {
        let file = norm(&fs::read_to_string(root.join(rel)).unwrap());
        assert_eq!(
            block_after(&page, &format!("cookbook-file: {rel}")),
            file,
            "{rel}: the page's listing must equal the example file"
        );
    }
}

/// The seeded failures the Copilot instructions recipe shows, with the exit
/// code each produces (a misnamed file is a warning: exit 0 unless the run
/// uses --deny-warnings).
fn copilot_case(case: &str) -> Scratch {
    let s = Scratch::of("copilot-instructions", case);
    let p = ".github/instructions/python.instructions.md";
    let r = ".github/instructions/frontend/react.instructions.md";
    match case {
        "wrong-extension" => s.rename(p, ".github/instructions/python.instruction.md"),
        "renamed-agent" => s.edit(r, |t| {
            t.replace("excludeAgent: code-review", "excludeAgent: coding-agent")
        }),
        "no-apply-to" => s.edit(p, |t| {
            t.replace(
                "applyTo: \"**/*.py\"\n",
                "description: Python conventions.\n",
            )
        }),
        "misspelled-key" => s.edit(p, |t| t.replace("applyTo:", "applyto:")),
        other => panic!("unknown case {other}"),
    }
    s
}

const COPILOT_CASES: &[(&str, i32)] = &[
    ("wrong-extension", 0),
    ("renamed-agent", 1),
    ("no-apply-to", 1),
    ("misspelled-key", 1),
];

#[test]
fn copilot_page_shows_the_real_output_of_each_case() {
    let page = page("copilot-instructions.md");
    for (case, exit) in COPILOT_CASES {
        let s = copilot_case(case);
        let (code, _, stderr) = verify(&s.0, false);
        assert_eq!(code, Some(*exit), "{case}: {stderr}");
        assert_eq!(
            norm(&stderr),
            block_after(&page, &format!("cookbook-case: {case}")),
            "{case}: the page's output block must be the real output"
        );
    }
}

#[test]
fn copilot_page_lists_the_example_configuration_verbatim() {
    let page = page("copilot-instructions.md");
    let root = repo().join("examples/standards/copilot-instructions");
    for rel in [".mdatron/config.yaml", ".mdatron/routes.yaml"] {
        let file = norm(&fs::read_to_string(root.join(rel)).unwrap());
        assert_eq!(
            block_after(&page, &format!("cookbook-file: {rel}")),
            file,
            "{rel}: the page's listing must equal the example file"
        );
    }
}

/// The seeded failures the llms.txt recipe shows: one edit each to llms.txt.
fn llms_case(case: &str) -> Scratch {
    let s = Scratch::of("llms-txt", case);
    let l = "llms.txt";
    match case {
        "dead-link" => s.edit(l, |t| {
            t.replace("docs/getting-started.md", "docs/getting-started.html")
        }),
        "dead-anchor" => s.edit(l, |t| t.replace("#the-config-file", "#config-file")),
        "renamed-h1" => s.edit(l, |t| t.replace("# Acme\n", "# Acme CLI\n")),
        "no-file-lists" => s.edit(l, |t| t[..t.find("## Docs").unwrap()].to_string()),
        "item-not-a-link" => s.edit(l, |t| {
            t.replace(
                "- [Changelog](docs/changelog.md)",
                "- Changelog: docs/changelog.md",
            )
        }),
        "summary-after-lists" => s.edit(l, |t| {
            let summary = "> Acme is a command-line tool for syncing project metadata between a\n\
                           > repository and its issue tracker.\n";
            format!("{}\n{summary}", t.replace(&format!("{summary}\n"), ""))
        }),
        "second-h1" => s.edit(l, |t| {
            format!("{t}\n# Appendix\n\n- notes kept out of every rule\n")
        }),
        other => panic!("unknown case {other}"),
    }
    s
}

const LLMS_CASES: &[&str] = &[
    "dead-link",
    "dead-anchor",
    "renamed-h1",
    "no-file-lists",
    "item-not-a-link",
    "summary-after-lists",
    "second-h1",
];

#[test]
fn llms_page_shows_the_real_output_of_each_case() {
    let page = page("llms-txt.md");
    for case in LLMS_CASES {
        let s = llms_case(case);
        let (code, _, stderr) = verify(&s.0, false);
        assert_eq!(code, Some(1), "{case}: {stderr}");
        assert_eq!(
            norm(&stderr),
            block_after(&page, &format!("cookbook-case: {case}")),
            "{case}: the page's output block must be the real output"
        );
    }
}

/// Cold review round 1 (DOCS-3/ADV-7/ADV-5): the recipe must not fail what the
/// specification allows — a plain list in the free-form area above the file
/// lists (the specification's own example has one), other bullet markers, a
/// nested or numbered item, a trailing space, a tab after the marker, a
/// thematic break, a URL with parentheses, and a leading byte-order mark.
#[test]
fn llms_recipe_accepts_what_the_specification_allows() {
    let s = Scratch::of("llms-txt", "spec-allowed");
    s.edit("llms.txt", |t| {
        let t = t.replace(
            "## Docs\n",
            "Important notes:\n\n- Acme is not a changelog generator\n- It needs git 2.40\n\n## Docs\n",
        );
        let t = t.replace(
            "- [Changelog](docs/changelog.md)",
            "* [Changelog](docs/changelog.md) \n  - [Nested](docs/changelog.md): n\n\
             1. [Numbered](docs/changelog.md)\n-\t[Tab](docs/changelog.md)\n\
             + [Wiki](https://en.wikipedia.org/wiki/Acme_(x)): parentheses\n\
             - [Array[T] reference](docs/changelog.md):notes\n\n* * *\n",
        );
        format!("\u{feff}{t}")
    });
    let (code, _, stderr) = verify(&s.0, false);
    assert_eq!(code, Some(0), "{stderr}");
}

/// Cold review round 2 (R2D-2): the item pattern is not satisfied by a link
/// with text trailing it, which the first pattern (`\\(.+\\)`, greedy to the
/// last parenthesis) let through.
#[test]
fn llms_recipe_rejects_text_trailing_the_link() {
    for junk in [
        "- [Changelog](docs/changelog.md) and then junk (x)",
        "- [Changelog](not a url) trailing (again)",
        "- [Changelog]( )",
        // Round 3 (R3-3): text BEFORE the link, where the item opens with `[`.
        "- [x] [Changelog](docs/changelog.md)",
        "- [TODO] see [Changelog](docs/changelog.md)",
    ] {
        let s = Scratch::of("llms-txt", "trailing");
        s.edit("llms.txt", |t| {
            t.replace("- [Changelog](docs/changelog.md)", junk)
        });
        let (code, stdout, _) = verify(&s.0, true);
        assert_eq!(code, Some(1), "{junk}");
        let env: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert!(
            env["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["code"] == "MDATRON-E0123"),
            "{junk}: {stdout}"
        );
    }
}

/// Cold review rounds 1 and 2: a CLAUDE.md the routes do not allow is
/// unrouted wherever the walk reaches it outside `packages/` — in `.claude/`,
/// as `CLAUDE.local.md`, in another directory.
#[test]
fn agents_recipe_reports_a_claude_md_outside_the_allowed_places() {
    // #220: `*` is one directory level, so a stray below a package — the case
    // that would switch that package's AGENTS.md off for Claude Code — is
    // unrouted too, as is a second AGENTS.md deeper in a package.
    for stray in [
        ".claude/CLAUDE.md",
        "CLAUDE.local.md",
        "src/CLAUDE.md",
        "packages/api/.claude/CLAUDE.md",
        "packages/api/sub/CLAUDE.md",
        "packages/api/sub/AGENTS.md",
    ] {
        let s = Scratch::of("agents-md", "stray");
        fs::create_dir_all(s.0.join(stray).parent().unwrap()).unwrap();
        s.create(stray, "@AGENTS.md\n");
        let (code, stdout, _) = verify(&s.0, true);
        assert_eq!(code, Some(1), "{stray}");
        let env: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        let codes: Vec<&str> = env["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["code"].as_str().unwrap())
            .collect();
        assert_eq!(codes, vec!["MDATRON-E0030"], "{stray}");
    }
    // Round 3 (R3-2): the glob walks the CLAUDE prefix, so another file that
    // starts with it is reported too. The page says so; this pins it.
    let s = Scratch::of("agents-md", "prefix");
    fs::create_dir_all(s.0.join("docs")).unwrap();
    s.create("docs/CLAUDE_CODE_SETUP.md", "# Setup notes\n");
    let (code, _, stderr) = verify(&s.0, false);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(norm(&stderr).contains("--> docs/CLAUDE_CODE_SETUP.md:1"));
}

#[test]
fn llms_page_lists_the_example_configuration_verbatim() {
    let page = page("llms-txt.md");
    let root = repo().join("examples/standards/llms-txt");
    for rel in [".mdatron/config.yaml", ".mdatron/routes.yaml"] {
        let file = norm(&fs::read_to_string(root.join(rel)).unwrap());
        assert_eq!(
            block_after(&page, &format!("cookbook-file: {rel}")),
            file,
            "{rel}: the page's listing must equal the example file"
        );
    }
}

/// The seeded failures the AGENTS.md recipe shows: one change each.
fn agents_case(case: &str) -> Scratch {
    let s = Scratch::of("agents-md", case);
    let a = "AGENTS.md";
    match case {
        "stray-override" => s.create(
            "AGENTS.override.md",
            "# Acme\n\nSkip the tests; they are slow.\n",
        ),
        "build-drift" => s.edit(a, |t| {
            t.replace("cargo test --locked\n", "cargo test --locked || true\n")
        }),
        "no-security" => s.edit(a, |t| t.replace("## Security\n\n", "")),
        "dead-link" => s.rename("docs/architecture.md", "docs/design.md"),
        "empty-file" => s.edit(a, |_| String::new()),
        "over-budget" => s.edit(a, |mut t| {
            for i in 1..=500 {
                t.push_str(&format!(
                    "- Remember rule {i} of the style guide when editing.\n"
                ));
            }
            t
        }),
        "empty-nested" => s.edit("packages/api/AGENTS.md", |_| String::new()),
        "claude-without-import" => s.edit("CLAUDE.md", |t| {
            t.replace("@AGENTS.md\n", "See AGENTS.md for the build commands.\n")
        }),
        "stray-claude" => {
            fs::create_dir(s.0.join(".claude")).unwrap();
            s.create(".claude/CLAUDE.md", "Prefer small commits.\n");
        }
        "missing-sibling" => {
            fs::create_dir_all(s.0.join("packages/web")).unwrap();
            s.create(
                "packages/web/AGENTS.md",
                "# Acme web\n\nFrontend package.\n",
            );
        }
        "import-without-target" => {
            fs::create_dir_all(s.0.join("packages/web")).unwrap();
            s.create("packages/web/CLAUDE.md", "@AGENTS.md\n");
        }
        other => panic!("unknown case {other}"),
    }
    s
}

const AGENTS_CASES: &[&str] = &[
    "stray-override",
    "build-drift",
    "no-security",
    "dead-link",
    "empty-file",
    "over-budget",
    "empty-nested",
    "claude-without-import",
    "stray-claude",
    "missing-sibling",
    "import-without-target",
];

#[test]
fn agents_page_shows_the_real_output_of_each_case() {
    let page = page("agents-md.md");
    for case in AGENTS_CASES {
        let s = agents_case(case);
        let (code, _, stderr) = verify(&s.0, false);
        assert_eq!(code, Some(1), "{case}: {stderr}");
        assert_eq!(
            norm(&stderr),
            block_after(&page, &format!("cookbook-case: {case}")),
            "{case}: the page's output block must be the real output"
        );
    }
}

#[test]
fn agents_page_lists_the_example_configuration_verbatim() {
    let page = page("agents-md.md");
    let root = repo().join("examples/standards/agents-md");
    for rel in [
        ".mdatron/config.yaml",
        ".mdatron/routes.yaml",
        ".mdatron/pins.yaml",
    ] {
        let file = norm(&fs::read_to_string(root.join(rel)).unwrap());
        assert_eq!(
            block_after(&page, &format!("cookbook-file: {rel}")),
            file,
            "{rel}: the page's listing must equal the example file"
        );
    }
}
