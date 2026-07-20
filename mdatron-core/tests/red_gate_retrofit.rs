//! Red-gate retrofit for mdatron tracker issue #52 (L2 fix at 0b21986).
//!
//! This file is placed into the PRE-FIX worktree (0b21986^ = b30251b) at
//! mdatron-core/tests/red_gate_retrofit.rs and run there. Each test asserts
//! the ratified-contract expectation (DESIGN.md § Five check families:
//! parent-directory, absolute-path, and symlink escapes rejected, including
//! non-existent targets) that the corresponding post-fix test asserts.
//! A test that FAILS here is the red gate demonstrated: the pre-fix code did
//! not enforce the contract. A test that PASSES here is itself a finding:
//! the post-fix test never tested the defect (vsdd directive, 2026-07-19).
//!
//! Fixture roots are created under a CANONICAL temp path (std::env::temp_dir
//! canonicalized first) so the macOS /var -> /private/var divergence cannot
//! mask the predecessor's textual-fallback hole the way it masked the
//! original traversal test (see the L2 issue).

use mdatron_core::dsl::index::{IndexError, IndexRegistry};
use mdatron_core::dsl::types::KeyDecl;
use std::path::PathBuf;

fn canonical_temp_root(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Canonicalize the temp base FIRST so root itself has no symlinked
    // components; the divergence-masking is what we are removing.
    let base = std::env::temp_dir().canonicalize().unwrap();
    let path = base.join(format!("mdatron-redgate-{label}-{nanos}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn decl(name: &str, source: &str, select: &str, indexed_by: &str) -> KeyDecl {
    KeyDecl {
        name: name.into(),
        source: source.into(),
        select: select.into(),
        indexed_by: indexed_by.into(),
    }
}

fn is_confinement_class(err: &IndexError) -> bool {
    // Pre-fix taxonomy has only PathTraversal for this class; post-fix adds
    // AbsoluteSource/SymlinkRefused. Debug-string match keeps this file
    // compilable against both trees.
    let dbg = format!("{err:?}");
    dbg.contains("PathTraversal")
        || dbg.contains("AbsoluteSource")
        || dbg.contains("SymlinkRefused")
}

// ── Parent-segment class (retrofits: path_traversal_via_parent_dots_is_rejected,
//    deep_parent_traversal_with_nonexistent_target_is_rejected) ────────────────

#[test]
fn rg_parent_traversal_absent_target_rejected_as_confinement() {
    let root = canonical_temp_root("escape-absent");
    let d = decl("escape", "../mdatron-redgate-no-such-file.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!(
            "contract: absent escaping target must be a confinement rejection; got {other:?}"
        ),
    }
}

#[test]
fn rg_deep_parent_traversal_absent_target_rejected() {
    let root = canonical_temp_root("escape-deep");
    let d = decl("escape", "../../no-such-file-anywhere.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!(
            "contract: deep absent escaping target must be a confinement rejection; got {other:?}"
        ),
    }
}

#[test]
fn rg_parent_traversal_existent_target_rejected() {
    let root = canonical_temp_root("escape-existent");
    let sibling = root.parent().unwrap().join("mdatron-redgate-existent.yaml");
    std::fs::write(&sibling, "k: v\n").unwrap();
    let d = decl("escape", "../mdatron-redgate-existent.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_file(&sibling).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!(
            "contract: existing escaping target must be a confinement rejection \
             (and its content must not be read); got {other:?}"
        ),
    }
}

#[test]
fn rg_interior_parent_segment_rejected_even_when_non_escaping() {
    let root = canonical_temp_root("interior-dots");
    std::fs::write(root.join("matrix.yaml"), "matrix:\n  a: 1\n").unwrap();
    let d = decl("m", "sub/../matrix.yaml", "$.matrix", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!(
            "contract: interior parent segments are rejected outright \
             (BOUNDARY-PREAMBLE 7 carried); got {other:?}"
        ),
    }
}

#[test]
fn rg_glob_pattern_with_parent_segment_rejected() {
    let root = canonical_temp_root("glob-dots");
    let d = decl("g", "../*.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!("contract: glob with parent segment rejected; got {other:?}"),
    }
}

// ── Absolute class (retrofits: absolute_source_rejected_even_when_inside_root,
//    absolute_source_with_nonexistent_target_rejected) ─────────────────────────

#[test]
fn rg_absolute_source_inside_root_rejected() {
    let root = canonical_temp_root("absolute-inside");
    let inside = root.join("inside.yaml");
    std::fs::write(&inside, "k: v\n").unwrap();
    let d = decl("abs", &inside.to_string_lossy(), "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!("contract: absolute spelling rejected even in-root; got {other:?}"),
    }
}

#[test]
fn rg_absolute_source_absent_target_rejected() {
    let root = canonical_temp_root("absolute-absent");
    let d = decl("abs", "/no-such-mdatron-redgate-target.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!("contract: absent absolute target rejected; got {other:?}"),
    }
}

// ── Symlink class (retrofits: symlink_source_refused_even_when_target_is_inside_root,
//    symlink_source_pointing_outside_root_refused,
//    symlinked_intermediate_component_refused, glob_matched_symlink_refused) ───

#[cfg(unix)]
#[test]
fn rg_symlink_source_inside_root_refused() {
    let root = canonical_temp_root("symlink-inside");
    std::fs::write(root.join("real.yaml"), "k: v\n").unwrap();
    std::os::unix::fs::symlink(root.join("real.yaml"), root.join("alias.yaml")).unwrap();
    let d = decl("s", "alias.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!(
            "contract: symlink refused whatever its target (no-follow is \
             unconditional); got {other:?}"
        ),
    }
}

#[cfg(unix)]
#[test]
fn rg_symlink_source_outside_root_refused() {
    let root = canonical_temp_root("symlink-escape");
    let outside = canonical_temp_root("symlink-escape-target");
    std::fs::write(outside.join("target.yaml"), "k: v\n").unwrap();
    std::os::unix::fs::symlink(outside.join("target.yaml"), root.join("link.yaml")).unwrap();
    let d = decl("s", "link.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&outside).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!(
            "contract: symlink escaping the root refused, content never read; got {other:?}"
        ),
    }
}

#[cfg(unix)]
#[test]
fn rg_symlinked_intermediate_component_refused() {
    let root = canonical_temp_root("symlink-mid");
    let outside = canonical_temp_root("symlink-mid-target");
    std::fs::write(outside.join("data.yaml"), "k: v\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("sub")).unwrap();
    let d = decl("s", "sub/data.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&outside).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!(
            "contract: symlinked intermediate as confined as symlinked leaf; got {other:?}"
        ),
    }
}

#[cfg(unix)]
#[test]
fn rg_glob_matched_symlink_refused() {
    let root = canonical_temp_root("glob-symlink");
    let outside = canonical_temp_root("glob-symlink-target");
    std::fs::write(outside.join("target.yaml"), "k: v\n").unwrap();
    std::os::unix::fs::symlink(outside.join("target.yaml"), root.join("linked.yaml")).unwrap();
    let d = decl("g", "*.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&outside).unwrap();
    match result {
        Err(ref e) if is_confinement_class(e) => {}
        other => panic!("contract: glob-matched symlink still refused; got {other:?}"),
    }
}

// ── Control (must pass on BOTH trees; validates the harness itself) ────────────

#[test]
fn rg_control_confined_missing_source_errors_without_traversal_claim() {
    let root = canonical_temp_root("missing-confined");
    let d = decl("m", "missing.yaml", "$", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        result.is_err(),
        "control: absent in-root target must error (any class)"
    );
}

#[test]
fn rg_control_plain_read_succeeds() {
    let root = canonical_temp_root("control-read");
    std::fs::write(root.join("matrix.yaml"), "matrix:\n  a: 1\n").unwrap();
    let d = decl("m", "matrix.yaml", "$.matrix", "$key");
    let result = IndexRegistry::build(&root, &[d]);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(result.is_ok(), "control: ordinary confined read must work");
}
