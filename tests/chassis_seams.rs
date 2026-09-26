// Test code: an unwrap IS the assertion — opt out of the [lints.clippy]
// panic-path restrictions production code is held to (#185).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Chassis seams (#200): the TRACKED crosslink policy, `.crosslink/hook-config.json`,
//! against the hook that consumes it.
//!
//! The behavioral-guard hook (`.claude/hooks/work-check.py`, deployed into the
//! checkout by `crosslink init` and gitignored) carries its own default lists
//! (`DEFAULT_ALLOWED_BASH`, `DEFAULT_BLOCKED_GIT`, `DEFAULT_AGENT_BLOCKED_GIT`),
//! and a config key that is PRESENT replaces the matching default list
//! wholesale. The tracked config is therefore a frozen snapshot of one hook
//! version's defaults: upgrade the hook and a default it grew is silently
//! absent from the snapshot — the #196 drift class, where the guard's own
//! suggested remedy had become blockable. The seam asserted here: every hook
//! default is present in the tracked list (the config may ADD entries; it
//! must never silently DROP a default).
//!
//! The hook is machine-local. Where it is not deployed (CI, a clone without
//! crosslink) there is no seam to compare, and the test says so and passes;
//! the drift arises on developer machines, which is where `cargo test` runs
//! before every PR. The list extractor is exercised on a fixture regardless,
//! so the test has teeth on every machine.

use std::fs;
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The string literals of the Python list assignment `NAME = [ ... ]` in
/// `src`, in order. Comments (`#` to end of line, outside a literal) are
/// dropped; both quote styles are accepted; the list must not nest.
fn python_string_list(src: &str, name: &str) -> Vec<String> {
    let assignment = format!("{name} = [");
    let start = src
        .find(&assignment)
        .unwrap_or_else(|| panic!("hook source lacks `{assignment}`"));
    let body = &src[start + assignment.len()..];
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut quote = '"';
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match current.as_mut() {
            Some(lit) => {
                if c == quote {
                    out.push(current.take().unwrap_or_default());
                } else {
                    lit.push(c);
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = c;
                    current = Some(String::new());
                }
                '#' => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                ']' => return out,
                '[' => panic!("`{name}` nests a list; this extractor reads flat string lists"),
                _ => {}
            },
        }
    }
    panic!("`{name}` list never closes")
}

#[test]
fn python_string_list_reads_literals_and_skips_comments() {
    let src = r#"
OTHER = ["not", "this"]
DEFAULT_ALLOWED_BASH = [
    "crosslink ",
    "git status", "git diff",  # trailing comment, "quoted" inside it
    # a full-line comment with a ] bracket
    'single quoted', "with # hash inside",
]
def after(): pass
"#;
    assert_eq!(
        python_string_list(src, "DEFAULT_ALLOWED_BASH"),
        [
            "crosslink ",
            "git status",
            "git diff",
            "single quoted",
            "with # hash inside"
        ]
    );
    assert_eq!(python_string_list(src, "OTHER"), ["not", "this"]);
}

#[test]
fn tracked_hook_config_carries_every_hook_default() {
    let hook_path = repo().join(".claude/hooks/work-check.py");
    let Ok(hook) = fs::read_to_string(&hook_path) else {
        eprintln!(
            "chassis seam absent: {} is not deployed here (CI, or a clone without crosslink); \
             nothing to compare against the tracked hook-config.json",
            hook_path.display()
        );
        return;
    };
    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo().join(".crosslink/hook-config.json")).unwrap(),
    )
    .unwrap();
    let tracked = |path: &[&str]| -> Vec<String> {
        let mut node = &config;
        for key in path {
            node = node
                .get(key)
                .unwrap_or_else(|| panic!("hook-config.json lacks `{}`", path.join(".")));
        }
        node.as_array()
            .unwrap_or_else(|| panic!("hook-config.json `{}` is not a list", path.join(".")))
            .iter()
            .map(|v| v.as_str().expect("list entries are strings").to_string())
            .collect()
    };

    // (hook default list, tracked key path that REPLACES it when present)
    let seams: [(&str, &[&str]); 3] = [
        ("DEFAULT_ALLOWED_BASH", &["allowed_bash_prefixes"]),
        ("DEFAULT_BLOCKED_GIT", &["blocked_git_commands"]),
        (
            "DEFAULT_AGENT_BLOCKED_GIT",
            &["agent_overrides", "blocked_git_commands"],
        ),
    ];
    let mut missing: Vec<String> = Vec::new();
    for (hook_list, path) in seams {
        let defaults = python_string_list(&hook, hook_list);
        assert!(
            !defaults.is_empty(),
            "parsed no entries from the hook's {hook_list}"
        );
        let snapshot = tracked(path);
        for default in defaults {
            if !snapshot.contains(&default) {
                missing.push(format!(
                    "`{}` lacks the hook default {default:?} (from {hook_list})",
                    path.join(".")
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "the tracked .crosslink/hook-config.json has drifted behind the deployed hook's defaults \
         (a present key replaces the default list wholesale, so a default the snapshot lacks is \
         silently gone — the #196 class):\n  {}\nAdd the entries to hook-config.json.",
        missing.join("\n  ")
    );
}
