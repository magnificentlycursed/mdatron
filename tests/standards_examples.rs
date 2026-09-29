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

/// Normalize a checkout artifact away: CRLF (a Windows autocrlf checkout)
/// and the OS path separator in rendered locations.
fn norm(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\\', "/")
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
        fs::write(&p, f(s)).unwrap();
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
    for (example, page_name) in [("skill-md", "skill-md.md")] {
        let dir = repo()
            .join("examples/standards")
            .join(example)
            .join(".mdatron/schemas");
        let mut sources: BTreeSet<Vec<String>> = BTreeSet::new();
        for e in fs::read_dir(&dir).unwrap().flatten() {
            let schema: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(e.path()).unwrap()).unwrap();
            let blocks = schema["x-source"]
                .as_array()
                .unwrap_or_else(|| panic!("{}: no x-source block", e.path().display()));
            assert!(!blocks.is_empty(), "{}: empty x-source", e.path().display());
            for b in blocks {
                let row: Vec<String> =
                    ["standard", "url", "revision", "revision_date", "retrieved"]
                        .iter()
                        .map(|k| {
                            let v = b[k].as_str().unwrap_or_else(|| {
                                panic!("{}: x-source lacks `{k}`", e.path().display())
                            });
                            assert!(!v.trim().is_empty(), "{}: empty `{k}`", e.path().display());
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
