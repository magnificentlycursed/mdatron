//! The one glob dialect every adopter glob is matched with (#220).
//!
//! Through 0.6.0 `*` crossed `/` — the `glob` crate's default — so a route
//! glob `packages/*/AGENTS.md` also claimed `packages/api/sub/AGENTS.md`, a
//! recipe could not say "one directory level", and the documentation warned
//! about it in three places. Every other dialect adopters know (the shell,
//! `.gitignore`, `.gitattributes`, editors) gives `*` one path segment and
//! `**` any depth. From 0.7.0 so does mdatron: `*`, `?` and `[…]` stop at a
//! separator, `**` alone crosses one. The same options apply to the walk
//! (`file_globs`), to route `files`, to the scope globs (`require_frontmatter`,
//! `vocabulary_globs`, `code_catalog_globs`) and to a pattern rule's path
//! `context`, so one glob means one thing everywhere. (The DSL's indexed key
//! sources already matched component by component.)
//!
//! A leading `.` is not literal: `*` and `**` match dot-files and dot-dirs as
//! before, so `**/CLAUDE.md` still reaches `.claude/CLAUDE.md`.

use std::path::Path;

/// The dialect: a separator is matched only by a literal `/` or by `**`.
pub(crate) const OPTIONS: glob::MatchOptions = glob::MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

/// Match a compiled pattern against a root-relative path.
pub(crate) fn matches_path(pattern: &glob::Pattern, path: &Path) -> bool {
    pattern.matches_path_with(path, OPTIONS)
}

/// Walk the filesystem for an absolute pattern, in the dialect.
pub(crate) fn walk(absolute_pattern: &str) -> Result<glob::Paths, glob::PatternError> {
    glob::glob_with(absolute_pattern, OPTIONS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, path: &str) -> bool {
        matches_path(&glob::Pattern::new(pattern).unwrap(), Path::new(path))
    }

    // #220: `*` is one segment; `**` is any depth; dot-dirs are not special.
    #[test]
    fn star_stays_within_a_segment_and_double_star_crosses() {
        assert!(m("packages/*/AGENTS.md", "packages/api/AGENTS.md"));
        assert!(!m("packages/*/AGENTS.md", "packages/api/sub/AGENTS.md"));
        assert!(!m("packages/*/CLAUDE.md", "packages/api/.claude/CLAUDE.md"));
        assert!(m("packages/**/AGENTS.md", "packages/api/sub/AGENTS.md"));
        assert!(m("**/CLAUDE.md", "CLAUDE.md"));
        assert!(m("**/CLAUDE.md", ".claude/CLAUDE.md"));
        assert!(m("**/*.md", "docs/a/b.md"));
        assert!(!m("*.md", "docs/b.md"));
        assert!(m("*.md", "b.md"));
        assert!(!m("docs/?.md", "docs/a/.md"));
        assert!(m(
            ".github/instructions/*.md",
            ".github/instructions/python.instructions.md"
        ));
        assert!(!m(
            ".github/instructions/*.md",
            ".github/instructions/frontend/react.instructions.md"
        ));
    }
}
