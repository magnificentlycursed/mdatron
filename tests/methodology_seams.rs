//! Methodology-enforcement seams: regression seeds proving each bound seam
//! catches its escape-corpus dodge (#96; `docs/methodology-enforcement.md`).
//! "If your seams don't catch each, they're insufficient."
//!
//! The amendment-citation seam: the `commit-msg` hook refuses a governing
//! DESIGN.md change whose message does not cite its authorizing issue and a
//! ratified review/decision — catching the forged-record dodge (a
//! self-authored gate record). Driven in a self-contained temp git repo so the
//! seed does not depend on ambient git config.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs")
}

/// A throwaway git repo wired to the real `.githooks/commit-msg`.
fn seeded_repo(label: &str) -> PathBuf {
    let root = repo().join("target").join(format!(
        "seam-{label}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join(".githooks")).unwrap();
    std::fs::copy(
        repo().join(".githooks/commit-msg"),
        root.join(".githooks/commit-msg"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            root.join(".githooks/commit-msg"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    git(&root, &["-c", "init.defaultBranch=main", "init", "-q"]);
    git(&root, &["config", "user.email", "seed@test"]);
    git(&root, &["config", "user.name", "seed"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(&root, &["config", "core.hooksPath", ".githooks"]);
    // A committed baseline so later commits are diffs, not the initial commit.
    std::fs::write(root.join("README.md"), "seed\n").unwrap();
    git(&root, &["add", "README.md"]);
    git(&root, &["commit", "-q", "-m", "chore: baseline"]);
    root
}

fn commit(root: &Path, file: &str, contents: &str, msg: &str) -> bool {
    std::fs::write(root.join(file), contents).unwrap();
    git(root, &["add", file]);
    git(root, &["commit", "-q", "-m", msg]).status.success()
}

// Forged-record dodge: a DESIGN change with a self-asserting (uncited) message
// is refused; the same change with an issue + ratification citation is accepted.
#[test]
fn amendment_citation_requires_ratification() {
    let root = seeded_repo("s1");

    // Forged: bare message, no authorizing citation -> refused, no commit.
    let forged = commit(
        &root,
        "DESIGN.md",
        "# design\nedit one\n",
        "docs: tweak design",
    );
    assert!(
        !forged,
        "a DESIGN change with no ratification citation must be refused"
    );

    // Valid: cites the issue and a ratified decision -> accepted.
    let valid = commit(
        &root,
        "DESIGN.md",
        "# design\nedit two\n",
        "docs(#96): record the ratified methodology decision (operator ruling)",
    );
    assert!(
        valid,
        "a cited DESIGN change is accepted: {:?}",
        git(&root, &["log", "--oneline"])
    );

    let _ = std::fs::remove_dir_all(&root);
}

// Seam scope (routed finding, #96): an integration merge of a DESIGN-touching
// branch is NOT gated — the amendment discipline gates the authoring commit
// (still refused above), not its integration. Pins the corrected scope so a
// future weakening that also un-gated authoring would fail the seed above.
#[test]
fn amendment_citation_ignores_integration_merges() {
    let root = seeded_repo("s1-merge");

    // Author the DESIGN change on a branch with a properly cited message.
    git(&root, &["checkout", "-q", "-b", "feat"]);
    assert!(
        commit(
            &root,
            "DESIGN.md",
            "# design\nlane edit\n",
            "docs(#96): lane DESIGN change (operator ruling)",
        ),
        "the in-lane authoring commit is cited and accepted"
    );

    // Integrate with a bare --no-ff merge message (no citation): must succeed,
    // because a merge integrates already-gated authoring, it does not amend.
    git(&root, &["checkout", "-q", "main"]);
    let merged = git(&root, &["merge", "--no-ff", "-m", "Merge feat", "feat"])
        .status
        .success();
    assert!(
        merged,
        "an integration merge touching DESIGN with a bare message is not gated: {:?}",
        git(&root, &["log", "--oneline"])
    );

    let _ = std::fs::remove_dir_all(&root);
}

// Seam boundary: a non-DESIGN change is not gated (the amendment discipline is
// specific to the governing document).
#[test]
fn amendment_citation_ignores_non_design_commits() {
    let root = seeded_repo("s1-boundary");
    let ok = commit(
        &root,
        "src.rs",
        "pub const VERSION: u8 = 1;\n",
        "feat: add a thing",
    );
    assert!(ok, "a non-DESIGN commit with a bare message is not gated");
    let _ = std::fs::remove_dir_all(&root);
}

// Governed-estate register (#97, phase-3 F1 repo guard): a zero-match
// vocabulary_globs silently disables the whole register (verify stays clean
// while scanning nothing). This pins that this repo's scope still names the
// methodology docs, so a dropped or mistyped glob fails loudly in CI rather
// than quietly un-governing our own governance prose.
#[test]
fn repo_register_stays_scoped_to_the_methodology_docs() {
    let cfg = std::fs::read_to_string(repo().join(".mdatron/config.yaml")).unwrap();
    assert!(
        cfg.contains("vocabulary_globs"),
        "config must scope the vocabulary family:\n{cfg}"
    );
    assert!(
        cfg.lines()
            .any(|l| l.contains("docs/methodology") && l.contains("*.md")),
        "vocabulary_globs must still cover docs/methodology*.md; got:\n{cfg}"
    );
}

// Checker-edit dodge: the seam script is version-controlled and its behavior is
// pinned by these tests, so a change that weakened it would have to also defeat
// these seeds. Guard: the hook exists and is the gate.
#[test]
fn amendment_citation_checker_is_version_controlled() {
    let hook = repo().join(".githooks/commit-msg");
    assert!(hook.is_file(), "the commit-msg seam must be repo-tracked");
    let body = std::fs::read_to_string(&hook).unwrap();
    assert!(
        body.contains("DESIGN.md") && body.contains("amendment discipline"),
        "the seam still gates DESIGN amendments"
    );
}
