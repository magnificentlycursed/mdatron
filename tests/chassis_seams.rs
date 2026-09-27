// Test code: an unwrap IS the assertion — opt out of the [lints.clippy]
// panic-path restrictions production code is held to (#185).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Chassis seams (#200): the TRACKED crosslink policy, `.crosslink/hook-config.json`,
//! against the hook that consumes it.
//!
//! The behavioral-guard hook (`.claude/hooks/work-check.py`, deployed into the
//! checkout by `crosslink init` and gitignored) carries default lists
//! (`DEFAULT_ALLOWED_BASH`, `DEFAULT_BLOCKED_GIT`, `DEFAULT_GATED_GIT`,
//! `DEFAULT_AGENT_BLOCKED_GIT`), and a config key that is PRESENT replaces the
//! matching default list wholesale. The tracked config is therefore a frozen
//! snapshot of one hook version's defaults: upgrade the hook and a default it
//! grew is silently absent from the snapshot — the #196 drift class, where the
//! guard's own suggested remedy had become blockable. The seam asserted here:
//! every hook default is present in the tracked list that replaces it (the
//! config may ADD entries; it must never silently DROP a default). A key the
//! config does not carry replaces nothing — the hook's default applies — so it
//! is not a seam. A growth tripwire pins the seam table to the hook's list
//! count, so a fifth list cannot arrive unnoticed.
//!
//! The hook is machine-local. This repository develops under the chassis, so a
//! checkout without the hook is a defect UNLESS the absence is declared:
//! `MDATRON_NO_CHASSIS=1` (CI sets it; a clone that will never commit may). A
//! silent pass was the alternative — and a control that passes silently where
//! it cannot look is no control (L3 cold review MINOR-4).

use std::fs;
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Locate the line-anchored Python assignment `NAME = [` (an optional type
/// annotation `NAME: list[str] = [` is accepted) and return the byte offset
/// just past its `[`.
fn list_body_start(src: &str, name: &str) -> Option<usize> {
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        let Some(after_name) = line.strip_prefix(name) else {
            continue;
        };
        let after_annotation = after_name
            .strip_prefix(':')
            .and_then(|a| a.split_once(" = [").map(|(_, r)| r))
            .or_else(|| after_name.strip_prefix(" = ["));
        if let Some(rest) = after_annotation {
            return Some(at + line.len() - rest.len());
        }
    }
    None
}

/// The string literals of the Python list assignment `NAME = [ ... ]` in
/// `src`, in order. The assignment must start a line; comments (`#` to end of
/// line, outside a literal) are dropped; both quote styles are accepted; `\"`,
/// `\'` and `\\` unescape and any other backslash pair is kept verbatim; the
/// list must not nest.
fn python_string_list(src: &str, name: &str) -> Vec<String> {
    let start = list_body_start(src, name)
        .unwrap_or_else(|| panic!("hook source lacks a line opening `{name} = [`"));
    let body = &src[start..];
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut quote = '"';
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match current.as_mut() {
            Some(lit) => match c {
                '\\' => match chars.next() {
                    Some(e @ ('"' | '\'' | '\\')) => lit.push(e),
                    Some(e) => {
                        lit.push('\\');
                        lit.push(e);
                    }
                    None => panic!("`{name}`: dangling escape at end of source"),
                },
                c if c == quote => out.push(current.take().unwrap_or_default()),
                c => lit.push(c),
            },
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

/// Every `DEFAULT_…` list the hook declares at line start, in order — empty
/// lists and type-annotated names included (round 2, MINOR-B / NIT-C: a list
/// that empties or gains an annotation must stay visible to the tripwire).
fn default_list_names(src: &str) -> Vec<String> {
    src.lines()
        .filter_map(|l| {
            let (lhs, _) = l.split_once(" = [")?;
            let name = lhs.split(':').next().unwrap_or(lhs).trim_end();
            (name.starts_with("DEFAULT_")
                && name.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
            .then(|| name.to_string())
        })
        .collect()
}

#[test]
fn python_string_list_reads_literals_skips_comments_and_unescapes() {
    let src = r#"
OTHER = ["not", "this"]
  # DEFAULT_ALLOWED_BASH = ["an indented decoy in a comment"]
DEFAULT_ALLOWED_BASH = [
    "crosslink ",
    "git status", "git diff",  # trailing comment, "quoted" inside it
    # a full-line comment with a ] bracket
    'single quoted', "with # hash inside", "say \"hi\"", "back\\slash", 'it\'s',
]
DEFAULT_EMPTY = []
DEFAULT_TYPED: list[str] = ["t"]
def after(): pass
"#;
    assert_eq!(
        python_string_list(src, "DEFAULT_ALLOWED_BASH"),
        [
            "crosslink ",
            "git status",
            "git diff",
            "single quoted",
            "with # hash inside",
            "say \"hi\"",
            "back\\slash",
            "it's"
        ]
    );
    assert_eq!(python_string_list(src, "OTHER"), ["not", "this"]);
    assert_eq!(python_string_list(src, "DEFAULT_TYPED"), ["t"]);
    assert!(python_string_list(src, "DEFAULT_EMPTY").is_empty());
    assert_eq!(
        default_list_names(src),
        ["DEFAULT_ALLOWED_BASH", "DEFAULT_EMPTY", "DEFAULT_TYPED"]
    );
}

/// Hook default lists that are deliberately NOT seams — declared here so the
/// growth tripwire can tell "known and not a seam" from "unnoticed". Empty
/// today; a future list the config cannot replace goes here with its reason.
const NON_SEAMS: &[&str] = &[];

/// (hook default list, the tracked key path that REPLACES it when present)
const SEAMS: [(&str, &[&str]); 4] = [
    ("DEFAULT_ALLOWED_BASH", &["allowed_bash_prefixes"]),
    ("DEFAULT_BLOCKED_GIT", &["blocked_git_commands"]),
    ("DEFAULT_GATED_GIT", &["gated_git_commands"]),
    (
        "DEFAULT_AGENT_BLOCKED_GIT",
        &["agent_overrides", "blocked_git_commands"],
    ),
];

#[test]
fn tracked_hook_config_carries_every_hook_default() {
    let hook_path = repo().join(".claude/hooks/work-check.py");
    let hook = match fs::read_to_string(&hook_path) {
        Ok(hook) => hook,
        Err(_) if std::env::var_os("MDATRON_NO_CHASSIS").is_some() => {
            eprintln!(
                "chassis declared absent (MDATRON_NO_CHASSIS): {} is not deployed here, so there \
                 is no hook to compare the tracked hook-config.json against",
                hook_path.display()
            );
            return;
        }
        Err(e) => panic!(
            "the crosslink hook is not deployed at {} ({e}). This repository develops under the \
             crosslink chassis (`crosslink init` deploys it), and the tracked hook-config.json can \
             only be checked against a deployed hook. Where the chassis is deliberately absent — CI, \
             a clone that will never commit — declare it: MDATRON_NO_CHASSIS=1.",
            hook_path.display()
        ),
    };
    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo().join(".crosslink/hook-config.json")).unwrap(),
    )
    .unwrap();
    let tracked = |path: &[&str]| -> Option<Vec<String>> {
        let mut node = &config;
        for key in path {
            node = node.get(key)?;
        }
        Some(
            node.as_array()
                .unwrap_or_else(|| panic!("hook-config.json `{}` is not a list", path.join(".")))
                .iter()
                .map(|v| v.as_str().expect("list entries are strings").to_string())
                .collect(),
        )
    };

    // Growth tripwire: a list the hook declares that this table does not know
    // is a seam nobody is watching (or a deliberate non-seam that must be
    // recorded here).
    let declared: std::collections::BTreeSet<String> =
        default_list_names(&hook).into_iter().collect();
    let known: std::collections::BTreeSet<String> = SEAMS
        .iter()
        .map(|(name, _)| name.to_string())
        .chain(NON_SEAMS.iter().map(|n| n.to_string()))
        .collect();
    assert_eq!(
        declared, known,
        "the hook declares {declared:?}; this test knows {known:?} — add the new list's seam \
         (which config key replaces it?), or list it in NON_SEAMS with its reason"
    );

    let mut missing: Vec<String> = Vec::new();
    for (hook_list, path) in SEAMS {
        let defaults = python_string_list(&hook, hook_list);
        if defaults.is_empty() {
            // An empty default has nothing a snapshot could drop (MINOR-B).
            eprintln!(
                "the hook's {hook_list} is empty: nothing for `{}` to drift from",
                path.join(".")
            );
            continue;
        }
        let Some(snapshot) = tracked(path) else {
            eprintln!(
                "`{}` is not in hook-config.json: the hook's {hook_list} applies unreplaced — no \
                 snapshot, so nothing to drift",
                path.join(".")
            );
            continue;
        };
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
