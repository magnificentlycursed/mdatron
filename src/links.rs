//! External-link checks (#215): what the link family can say about an
//! absolute URL WITHOUT fetching it. mdatron never reaches the network — not
//! in `verify`, not behind a flag: a finding is a function of the tree alone
//! (offline, no clock), while liveness is a property of the world at a moment
//! and belongs to a scheduled liveness tool (lychee and its class). Four
//! offline pieces replace the fetch:
//!
//! 1. **Well-formedness** (`E0117`): a destination that is not a URL at all —
//!    whitespace, an invisible character, two `#`, an `https://` with no
//!    host, a `..` segment — is reported by every link-checked route with no
//!    data supplied, and is not exported (nothing could fetch it).
//! 2. **The register** (`.mdatron/links.yaml`): the closed set of URLs the
//!    corpus may point at, exactly or by prefix. An absolute link no entry
//!    matches is `E0115`; a `#fragment` an entry does not list, when it lists
//!    any, is `E0116`. The register is the one file a liveness tool checks on
//!    a schedule. Supplied-but-inert and never-used entries are announced
//!    (`W0056`, `W0057`), like every dead scope in this engine.
//! 3. **Policy** (`link_policy` on a route): allowed schemes, allowed hosts,
//!    forbidden query parameters (`E0118`).
//! 4. **The export** (`mdatron links --external`): every markdown link to an
//!    absolute URL the link-checked files hold, with file and line under
//!    `--json`, for whatever liveness tool runs it — fragment included, so a
//!    tool that checks anchors can.
//!
//! Activation of the register: the file exists; its scope is the link family's
//! (files a route opts in with `links: true`). Entries are compared with the
//! link's destination as the CommonMark parser yields it (entity and
//! character references and backslash escapes decoded, nothing more) — no case folding, no normalisation of a trailing
//! slash, no percent-decoding — so what the register says is what the corpus
//! must say. What the link family sees is what CommonMark calls a link:
//! inline, reference-style, image and `<autolink>` destinations; a bare URL
//! in prose and a raw HTML `<a href>` are not links and are neither checked
//! nor exported.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::Error;

/// The register's file name under `.mdatron/`.
pub(crate) const REGISTER_NAME: &str = "links.yaml";

/// The export format's own version (`mdatron links --json`), SemVer like the
/// verify envelope's `mdatron_output_version`: a consumer pins it.
pub const EXPORT_VERSION: &str = "1.0.0";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRegister {
    /// Input-format version (DEF5): links.yaml is new in 0.8.0, so it is born
    /// versioned. Read by the format-version probe; declared here only so
    /// `deny_unknown_fields` accepts the field.
    #[serde(default)]
    #[allow(dead_code)]
    mdatron_format_version: Option<u32>,
    #[serde(default)]
    links: Vec<RawEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    url: String,
    #[serde(default)]
    prefix: bool,
    #[serde(default)]
    fragments: Option<Vec<String>>,
}

/// One register entry: a URL the corpus may point at. With `prefix`, every
/// URL that starts with `url` is covered. `fragments`, when given, is the
/// closed set of anchors the corpus may use on the page(s); `None` accepts
/// any, `Some(empty)` accepts none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub url: String,
    pub prefix: bool,
    pub fragments: Option<Vec<String>>,
}

/// The loaded register plus the digest of the bytes read (#176 lineage).
pub(crate) struct LoadedRegister {
    pub entries: Vec<Entry>,
    pub digest: String,
}

/// Load `.mdatron/links.yaml`. `Ok(None)` when absent (register inactive);
/// `Err` when unreadable, malformed, or holding an entry that could never
/// match the way it reads (loud, strict parse).
pub(crate) fn load(project_root: &Path) -> Result<Option<LoadedRegister>, Error> {
    let path = project_root.join(".mdatron").join(REGISTER_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(Error::Config(format!(
                "cannot read '{}': {e}",
                path.display()
            )))
        }
    };
    crate::format_version::check_input_format_version(&content, REGISTER_NAME, true)?;
    let raw: RawRegister = crate::yaml::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    // Load-time refusals: every one is statically knowable and would make an
    // entry inert or ambiguous — the register must say what it means.
    let mut seen: HashSet<&str> = HashSet::new();
    for e in &raw.links {
        let url = e.url.as_str();
        let u = shown(url);
        if url.is_empty() {
            return Err(Error::Config(
                "a links.yaml entry has an empty url; an entry names the URL the corpus \
                 may point at"
                    .into(),
            ));
        }
        if let Some(what) = holds_forbidden_char(url) {
            return Err(Error::Config(format!(
                "links.yaml entry '{u}' holds {what}; a URL is written without them"
            )));
        }
        if url.contains('#') {
            return Err(Error::Config(format!(
                "links.yaml entry '{u}' holds a `#`; keep `url` to the page and list the \
                 anchors the corpus may use under `fragments`"
            )));
        }
        if !crate::link::is_external(url) {
            return Err(Error::Config(format!(
                "links.yaml entry '{u}' is not an absolute URL (a scheme such as `https:`, \
                 or a protocol-relative `//host`); a relative link is resolved against the \
                 tree, never the register"
            )));
        }
        // An entry no link could ever match is refused: an exact entry must
        // itself be a well-formed URL (a link equal to it is E0117 before the
        // register is consulted), and a prefix entry for a web URL must run
        // PAST its host — `https://acme.dev` as a prefix also covers
        // `https://acme.dev.evil.example/` and `https://acme.dev@evil.example/`,
        // the host escapes a text prefix cannot see. The same holds for every
        // `scheme://authority` form (`ftp://`, `wss://`, `git://`): RFC 3986
        // §3.2 makes `//` after any scheme an authority. A prefix for an opaque
        // scheme with no authority (`mailto:`) may stop at the scheme; a web
        // scheme is never opaque (`wss:acme.dev` is no URL a browser opens).
        let opaque_prefix = e.prefix && !has_authority(url) && !is_web(url);
        if !opaque_prefix {
            if let Err(reason) = parse(url) {
                return Err(Error::Config(format!(
                    "links.yaml entry '{u}' is not a well-formed URL ({reason}); no link \
                     could match it"
                )));
            }
        }
        if e.prefix && has_authority(url) && !authority_closed(url) {
            return Err(Error::Config(format!(
                "links.yaml prefix entry '{u}' stops inside its host; a prefix must run \
                 past the host (end it with `/`), or `{u}.evil.example/` and \
                 `{u}@evil.example/` would start with it too"
            )));
        }
        if !seen.insert(url) {
            return Err(Error::Config(format!(
                "duplicate links.yaml entry '{u}': two entries for one URL is ambiguous \
                 authority — merge them into one"
            )));
        }
        if let Some(frags) = &e.fragments {
            let mut seen_frags: HashSet<&str> = HashSet::new();
            for f in frags {
                let fs = shown(f);
                if f.is_empty() {
                    return Err(Error::Config(format!(
                        "links.yaml entry '{u}' declares an empty fragment; a bare `#` \
                         (top of page) is always accepted and needs no entry"
                    )));
                }
                if f.contains('#') {
                    return Err(Error::Config(format!(
                        "links.yaml entry '{u}' declares fragment '{fs}' holding `#`; a \
                         fragment is written without its `#`"
                    )));
                }
                if let Some(what) = holds_forbidden_char(f) {
                    return Err(Error::Config(format!(
                        "links.yaml entry '{u}' declares fragment '{fs}' holding {what}"
                    )));
                }
                if !seen_frags.insert(f) {
                    return Err(Error::Config(format!(
                        "links.yaml entry '{u}' declares fragment '{fs}' twice"
                    )));
                }
            }
        }
    }

    let entries = raw
        .links
        .into_iter()
        .map(|e| Entry {
            url: e.url,
            prefix: e.prefix,
            fragments: e.fragments,
        })
        .collect();
    Ok(Some(LoadedRegister {
        entries,
        digest: crate::init::sha256_hex(content.as_bytes()),
    }))
}

/// The loaded register, indexed for lookup: an exact map over every entry's
/// `url`, and the prefix entries by `url` with their distinct lengths
/// (longest first), so a link costs one hash probe per distinct prefix
/// length rather than a scan of the register.
pub(crate) struct Register {
    entries: Vec<Entry>,
    exact: HashMap<String, usize>,
    prefixes: HashMap<String, usize>,
    prefix_lens: Vec<usize>,
}

impl Register {
    pub(crate) fn new(entries: Vec<Entry>) -> Self {
        let mut exact = HashMap::new();
        let mut prefixes = HashMap::new();
        let mut lens = std::collections::BTreeSet::new();
        for (i, e) in entries.iter().enumerate() {
            exact.entry(e.url.clone()).or_insert(i);
            if e.prefix {
                prefixes.entry(e.url.clone()).or_insert(i);
                lens.insert(e.url.len());
            }
        }
        Register {
            entries,
            exact,
            prefixes,
            prefix_lens: lens.into_iter().rev().collect(),
        }
    }

    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry covering `url` (the destination without its fragment): an
    /// exact entry wins over a prefix entry, and among prefix entries the
    /// longest wins. Returns the entry's index for the used-set.
    fn resolve(&self, url: &str) -> Option<(usize, &Entry)> {
        if let Some(&i) = self.exact.get(url) {
            return Some((i, &self.entries[i]));
        }
        self.prefix_lens
            .iter()
            .filter(|&&n| n <= url.len() && url.is_char_boundary(n))
            .find_map(|&n| self.prefixes.get(&url[..n]))
            .map(|&i| (i, &self.entries[i]))
    }
}

/// A route's link policy as declared in `routes.yaml` (#215, piece 3).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawPolicy {
    /// The schemes an absolute link may use (`https`); a protocol-relative
    /// `//host` link has no scheme and violates any list.
    #[serde(default)]
    schemes: Option<Vec<String>>,
    /// The hosts a web link (`http`, `https`, `ws`, `wss`, `ftp`, `//host`)
    /// may name; `*.example.com` covers every subdomain and not the apex. A
    /// link of another scheme (`mailto:`, `ssh://`) is judged by `schemes`.
    #[serde(default)]
    hosts: Option<Vec<String>>,
    /// Patterns over query-parameter NAMES a link must not carry (`^utm_`).
    #[serde(default)]
    forbid_query: Option<Vec<String>>,
}

/// A compiled link policy. Lists are lowercased at load (schemes and hosts
/// compare case-insensitively, as the URL standard reads them); the
/// `forbid_query` patterns keep their source text for the finding.
#[derive(Debug, Clone)]
pub struct Policy {
    pub schemes: Option<Vec<String>>,
    pub hosts: Option<Vec<String>>,
    pub forbid_query: Option<Vec<(String, regex_lite::Regex)>>,
}

impl Policy {
    /// Compile a raw policy, refusing one that could never apply the way it
    /// reads: an empty block, an empty list, a scheme that is not a scheme
    /// token, a host holding a separator, a pattern that does not compile.
    pub(crate) fn compile(raw: RawPolicy) -> Result<Policy, Error> {
        if raw.schemes.is_none() && raw.hosts.is_none() && raw.forbid_query.is_none() {
            return Err(Error::Config(
                "route link_policy declares no clause; give it schemes, hosts or \
                 forbid_query, or remove it"
                    .into(),
            ));
        }
        let schemes = match raw.schemes {
            None => None,
            Some(list) => {
                if list.is_empty() {
                    return Err(Error::Config(
                        "route link_policy schemes is empty; it would allow no scheme at \
                         all — list the schemes links may use, or remove the key"
                            .into(),
                    ));
                }
                let mut out = Vec::with_capacity(list.len());
                for s in list {
                    let lower = s.to_ascii_lowercase();
                    let ok = lower
                        .bytes()
                        .next()
                        .is_some_and(|b| b.is_ascii_alphabetic())
                        && lower
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'));
                    if !ok {
                        return Err(Error::Config(format!(
                            "route link_policy scheme '{s}' is not a scheme token (a letter, \
                             then letters, digits, `+`, `.` or `-`; written without the `:`)"
                        )));
                    }
                    out.push(lower);
                }
                Some(out)
            }
        };
        let hosts = match raw.hosts {
            None => None,
            Some(list) => {
                if list.is_empty() {
                    return Err(Error::Config(
                        "route link_policy hosts is empty; it would allow no host at all — \
                         list the hosts links may name, or remove the key"
                            .into(),
                    ));
                }
                let mut out = Vec::with_capacity(list.len());
                for h in list {
                    let lower = h.to_ascii_lowercase();
                    let name = lower.strip_prefix("*.").unwrap_or(&lower);
                    let literal = lower
                        .strip_prefix('[')
                        .and_then(|r| r.strip_suffix(']'))
                        .is_some_and(is_ipv6_literal);
                    let ok = literal
                        || (!name.is_empty()
                            && !name.starts_with('.')
                            && !name.contains("..")
                            && name.chars().all(|c| {
                                !c.is_ascii()
                                    || c.is_ascii_alphanumeric()
                                    || matches!(c, '-' | '.' | '_' | '~')
                            }));
                    if !ok {
                        return Err(Error::Config(format!(
                            "route link_policy host '{h}' is not a host name (letters, digits, \
                             `-`, `.`, `_` and `~`, optionally led by `*.` for every \
                             subdomain, or a bracketed IPv6 literal; no scheme, port or path)"
                        )));
                    }
                    out.push(lower);
                }
                Some(out)
            }
        };
        let forbid_query = match raw.forbid_query {
            None => None,
            Some(list) => {
                if list.is_empty() {
                    return Err(Error::Config(
                        "route link_policy forbid_query is empty; list the query-parameter \
                         name patterns links must not carry, or remove the key"
                            .into(),
                    ));
                }
                let mut out = Vec::with_capacity(list.len());
                for p in list {
                    match regex_lite::Regex::new(&p) {
                        Ok(re) => out.push((p, re)),
                        Err(e) => {
                            return Err(Error::Config(format!(
                                "route link_policy forbid_query pattern '{p}' does not compile: {e}"
                            )))
                        }
                    }
                }
                Some(out)
            }
        };
        Ok(Policy {
            schemes,
            hosts,
            forbid_query,
        })
    }
}

/// The parts of an absolute destination the checks read. `scheme` is `None`
/// for a protocol-relative `//host` destination; `host` is `Some` only for
/// the web forms ([`WEB_SCHEMES`] and `//host`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Url<'a> {
    /// The destination before its `#`: the text the register compares.
    pub page: &'a str,
    pub scheme: Option<&'a str>,
    pub host: Option<&'a str>,
    pub query: Option<&'a str>,
    pub fragment: Option<&'a str>,
}

/// The WHATWG special schemes that carry a host: `http`, `https`, `ws`,
/// `wss`, `ftp`. (`file` is special too, but its host may be empty.)
const WEB_SCHEMES: &[&str] = &["http", "https", "ws", "wss", "ftp"];

/// Whether a destination is a WEB form: one of [`WEB_SCHEMES`], or
/// protocol-relative `//host` — the forms whose `//` is required, whose host
/// the parser validates and a `hosts` policy judges, and which a browser
/// reads with `\` as `/`.
fn is_web(dest: &str) -> bool {
    dest.starts_with("//")
        || dest
            .split_once(':')
            .is_some_and(|(s, _)| WEB_SCHEMES.iter().any(|w| s.eq_ignore_ascii_case(w)))
}

/// Whether a destination carries an authority: `//host`, or any
/// `scheme://…` (RFC 3986 §3.2 — `ftp://`, `wss://`, `git://` as much as
/// `https://`). Its host can be escaped by a text prefix, and its path's dot
/// segments are resolved away by every client.
fn has_authority(dest: &str) -> bool {
    dest.starts_with("//")
        || dest
            .split_once(':')
            .is_some_and(|(_, rest)| rest.starts_with("//"))
}

/// Whether an authority-form destination's authority is closed by a `/` or
/// a `?`, so a text prefix cannot be extended inside the host.
fn authority_closed(dest: &str) -> bool {
    let after = match dest.strip_prefix("//") {
        Some(rest) => rest,
        None => dest.split_once("://").map(|(_, rest)| rest).unwrap_or(""),
    };
    after.contains(['/', '?'])
}

/// The invisible characters: the invisible members of category Cf (the
/// visible Cf signs such as U+0600 are not listed) — a zero-width space, a
/// soft hyphen, a direction mark, a byte-order mark — and the blank fillers
/// that render as nothing (Hangul fillers, the braille blank, the combining
/// grapheme joiner, Mongolian free variation selectors), and the variation
/// selectors U+FE00-FE0F and U+E0100-E01EF. A link holding one can never
/// resolve as its author reads it. The joiners, selectors and flag tags of a
/// real emoji or ideograph are not hidden text: see [`in_emoji_sequence`].
fn is_format_char(c: char) -> bool {
    matches!(
        c,
        // Unicode's complete Default_Ignorable_Code_Point set (assigned and
        // reserved — a renderer draws an unassigned one as nothing too), plus
        // some Cf format controls outside it and the braille blank.
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{17B4}'
            | '\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{2800}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF0}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// The character class a URL is written without: it is the one check that
/// applies to every scheme.
fn holds_forbidden_char(s: &str) -> Option<&'static str> {
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_whitespace() {
            return Some("whitespace");
        } else if c.is_control() {
            return Some("a control character");
        } else if is_format_char(c) && !in_emoji_sequence(&chars, i) {
            return Some("an invisible character");
        }
    }
    None
}

/// An emoji presentation code point: the pictographic blocks a ZWJ sequence
/// joins (approximating Unicode's Extended_Pictographic).
fn is_pictographic(c: char) -> bool {
    matches!(
        c,
        '\u{00A9}'
            | '\u{00AE}'
            | '\u{203C}'
            | '\u{2049}'
            | '\u{2122}'
            | '\u{2139}'
            | '\u{2194}'..='\u{21FF}'
            | '\u{24C2}'
            | '\u{2300}'..='\u{23FF}'
            | '\u{25A0}'..='\u{27BF}'
            | '\u{2900}'..='\u{297F}'
            | '\u{2B00}'..='\u{2BFF}'
            | '\u{3030}'
            | '\u{303D}'
            | '\u{3297}'
            | '\u{3299}'
            | '\u{1F000}'..='\u{1FAFF}'
    )
}

/// Whether the invisible character at `i` is a legitimate part of an emoji
/// or ideograph: a ZWJ between two pictographs (one presentation selector or
/// skin-tone modifier may sit before it), one presentation selector
/// (U+FE0E/FE0F) after a pictograph or inside a keycap, one ideographic
/// variation selector after a CJK unified ideograph, or a tag inside a
/// RGI subdivision flag — U+1F3F4, the tags of `gbeng`, `gbsct` or `gbwls`,
/// then U+E007F CANCEL TAG. Anything else is hidden text (`é` followed by
/// tag-encoded ASCII is the "ASCII smuggling" payload, and is refused).
/// Residual, stated: an ideographic selector is accepted whatever its value
/// (Ideographic Variation Database sequences use the whole range), so a CJK
/// path can still carry about one hidden byte per ideograph.
fn in_emoji_sequence(chars: &[char], i: usize) -> bool {
    let c = chars[i];
    let at = |j: Option<usize>| j.and_then(|j| chars.get(j)).copied();
    let is_modifier = |p: char| p == '\u{FE0F}' || ('\u{1F3FB}'..='\u{1F3FF}').contains(&p);
    if c == '\u{200D}' {
        // A pictograph before it — directly, or under one presentation
        // selector or skin-tone modifier — and a pictograph after it.
        let before = match at(i.checked_sub(1)) {
            Some(p) if is_pictographic(p) => true,
            Some(p) if is_modifier(p) => at(i.checked_sub(2)).is_some_and(is_pictographic),
            _ => false,
        };
        return before && at(Some(i + 1)).is_some_and(is_pictographic);
    }
    if c == '\u{FE0E}' || c == '\u{FE0F}' {
        // One presentation selector after a pictograph, or inside a keycap
        // (`1` + selector + U+20E3 COMBINING ENCLOSING KEYCAP).
        let base = at(i.checked_sub(1));
        return base.is_some_and(is_pictographic)
            || (base.is_some_and(|p| p.is_ascii_digit() || p == '#' || p == '*')
                && at(Some(i + 1)) == Some('\u{20E3}'));
    }
    if ('\u{E0100}'..='\u{E01EF}').contains(&c) {
        // One ideographic variation selector after a CJK unified ideograph.
        return at(i.checked_sub(1)).is_some_and(|p| {
            matches!(p, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{3134F}')
        });
    }
    if ('\u{E0020}'..='\u{E007F}').contains(&c) {
        // A flag tag sequence: U+1F3F4, the tags of one of the three RGI
        // subdivision ids, then CANCEL TAG — never free text.
        let is_id_tag =
            |t: char| matches!(t, '\u{E0030}'..='\u{E0039}' | '\u{E0061}'..='\u{E007A}');
        let mut j = i;
        while j > 0 && is_id_tag(chars[j - 1]) {
            j -= 1;
        }
        if j == 0 || chars[j - 1] != '\u{1F3F4}' {
            return false;
        }
        let mut k = j;
        while k < chars.len() && is_id_tag(chars[k]) {
            k += 1;
        }
        // Only the three recommended (RGI) subdivision flags: any other id
        // renders as a plain black flag with its tags hidden, so a chain of
        // "flags" would carry free text five letters at a time.
        let id: String = chars[j..k]
            .iter()
            .map(|&t| char::from_u32(t as u32 - 0xE0000).unwrap_or('?'))
            .collect();
        return matches!(id.as_str(), "gbeng" | "gbsct" | "gbwls")
            && chars.get(k) == Some(&'\u{E007F}');
    }
    false
}

/// A string as a message may quote it: controls and invisible characters
/// shown as `\u{…}` escapes, so a bidi override or a tag character in
/// adopter data cannot rewrite the terminal line that reports it.
fn shown(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || is_format_char(c) {
                c.escape_unicode().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Refuse a path that a client resolves away from the text a register
/// compares: a `.` or `..` segment (percent-encoded forms included), and on
/// a web form a `\`, which a browser reads as `/`.
fn check_path(path: &str, web: bool) -> Result<(), String> {
    if web && path.contains('\\') {
        return Err("its path holds `\\`, which a browser reads as `/`".into());
    }
    for segment in path.split('/') {
        let decoded = crate::link::percent_decode(segment);
        if decoded == "." || decoded == ".." {
            return Err(
                "its path holds a `.` or `..` segment, which a client resolves away".into(),
            );
        }
    }
    Ok(())
}

/// Parse an absolute destination (one `link::is_external` accepted), or say
/// why it is not a well-formed URL. Conservative: only what is wrong
/// everywhere — the structural shape of RFC 3986 — is refused, never a
/// stylistic choice. The web forms ([`WEB_SCHEMES`] and `//host`) require
/// `//` and a host; any other scheme requires something after its `:`.
pub(crate) fn parse(dest: &str) -> Result<Url<'_>, String> {
    if let Some(what) = holds_forbidden_char(dest) {
        return Err(format!("it holds {what}"));
    }
    let (page, fragment) = crate::link::split_fragment(dest);
    if fragment.is_some_and(|f| f.contains('#')) {
        return Err("it holds more than one `#`".into());
    }
    let query = page.split_once('?').map(|(_, q)| q);
    let (scheme, after_scheme) = match page.strip_prefix("//") {
        Some(rest) => (None, rest),
        None => match page.split_once(':') {
            Some((s, rest)) => (Some(s), rest),
            None => return Err("it has no scheme".into()),
        },
    };
    if !is_web(page) {
        if after_scheme.is_empty() {
            return Err("nothing follows its scheme".into());
        }
        // `file:` is a WHATWG special scheme: a browser reads `\` as `/`
        // anywhere in it (`file://a\b`, `file:C:\x`), so it is refused there.
        if scheme.is_some_and(|s| s.eq_ignore_ascii_case("file")) && page.contains('\\') {
            return Err("it holds `\\`, which a browser reads as `/` in a `file:` URL".into());
        }
        // `ftp://host/a/../b`-style forms: the authority is not validated as
        // a web host, but the path is resolved like any hierarchical URI's.
        if let Some(after_slashes) = after_scheme.strip_prefix("//") {
            let rest = after_slashes
                .find(['/', '?'])
                .map_or("", |i| &after_slashes[i..]);
            check_path(rest.split_once('?').map_or(rest, |(p, _)| p), false)?;
        }
        return Ok(Url {
            page,
            scheme,
            host: None,
            query,
            fragment,
        });
    }
    let after_slashes = match scheme {
        None => after_scheme,
        Some(_) => match after_scheme.strip_prefix("//") {
            Some(rest) => rest,
            None => return Err("its scheme is not followed by `//`".into()),
        },
    };
    // A browser ends a web authority at `\` as at `/` (WHATWG authority
    // state), so `https://evil.example\@acme.dev/` is the host `evil.example`;
    // ending it there too puts the `\` in the path, where it is refused.
    let authority_end = after_slashes
        .find(['/', '?', '\\'])
        .unwrap_or(after_slashes.len());
    let authority = &after_slashes[..authority_end];
    let host = host_of(authority)?;
    // A dot segment resolves away in every client (RFC 3986 §5.2.4), so the
    // text a register compares is not the page a reader lands on; refuse it
    // (percent-encoded forms included) rather than normalise silently.
    let rest = &after_slashes[authority_end..];
    check_path(rest.split_once('?').map_or(rest, |(p, _)| p), true)?;
    Ok(Url {
        page,
        scheme,
        host: Some(host),
        query,
        fragment,
    })
}

/// The host of an authority component: userinfo stripped, port validated and
/// stripped, an IPv6 literal kept with its brackets.
fn host_of(authority: &str) -> Result<&str, String> {
    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, authority),
    };
    // RFC 3986 userinfo: unreserved, percent-encoded, sub-delims and `:`.
    if let Some(c) = userinfo.and_then(|u| {
        u.chars()
            .find(|c| c.is_ascii() && !(is_reg_name_char(*c) || *c == ':'))
    }) {
        return Err(format!("its userinfo holds `{c}`"));
    }
    if let Some(inner) = hostport.strip_prefix('[') {
        let Some(close) = inner.find(']') else {
            return Err("its IPv6 host literal is not closed with `]`".into());
        };
        check_port(&inner[close + 1..])?;
        if !is_ipv6_literal(&inner[..close]) {
            return Err("its IPv6 host literal is not an address".into());
        }
        return Ok(&hostport[..close + 2]);
    }
    let host = match hostport.rsplit_once(':') {
        Some((h, _)) => {
            check_port(&hostport[h.len()..])?;
            h
        }
        None => hostport,
    };
    if host.is_empty() {
        return Err("it has no host".into());
    }
    if host.starts_with('.') || host.contains("..") {
        return Err("its host has an empty label".into());
    }
    // RFC 3986 reg-name: unreserved, percent-encoded, sub-delims — plus any
    // non-ASCII char (an internationalised name, written as it is). Nothing
    // stylistic: `_` and `~` are legal here and browsers open them.
    if let Some(c) = host.chars().find(|c| c.is_ascii() && !is_reg_name_char(*c)) {
        return Err(format!("its host holds `{c}`"));
    }
    // No emoji-sequence exemption in a host: a joiner there fails IDNA
    // (UTS 46 CheckJoiners) and hides a lookalike label.
    if host.chars().any(|c| is_format_char(c) || c == '\u{200D}') {
        return Err("its host holds an invisible character".into());
    }
    // A `%` must start a percent-encoding, and what it encodes must itself be
    // a host character: `evil.example%23.acme.dev` decodes to a `#` and is no
    // host at all, though as text it ends with `.acme.dev`.
    let bytes = host.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'%' {
            continue;
        }
        let Some(hex) = bytes
            .get(i + 1..i + 3)
            .filter(|h| h.iter().all(u8::is_ascii_hexdigit))
        else {
            return Err("its host holds a `%` that is not a percent-encoding".into());
        };
        let hex = std::str::from_utf8(hex).unwrap_or("00");
        let decoded = u8::from_str_radix(hex, 16).unwrap_or(0);
        if decoded == b'%' || (decoded.is_ascii() && !is_reg_name_char(decoded as char)) {
            return Err(format!(
                "its host holds `%{hex}`, an encoded `{}` no host may contain",
                (decoded as char).escape_default()
            ));
        }
    }
    Ok(host)
}

/// RFC 3986 `reg-name` characters: unreserved, the `%` of a percent-encoding,
/// and the sub-delims.
fn is_reg_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '-' | '.'
                | '_'
                | '~'
                | '%'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
        )
}

/// The inside of a bracketed IPv6 literal: an IPv6 address (`std::net`'s
/// grammar, an IPv4 tail allowed), optionally a `%25`-led zone id (RFC 6874).
fn is_ipv6_literal(literal: &str) -> bool {
    let (address, zone) = match literal.split_once("%25") {
        Some((a, z)) => (a, Some(z)),
        None => (literal, None),
    };
    let address_ok = address.parse::<std::net::Ipv6Addr>().is_ok();
    let zone_ok = zone.is_none_or(|z| {
        !z.is_empty()
            && z.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
    });
    address_ok && zone_ok
}

/// `rest` is what follows the host: empty, or `:` and a port.
fn check_port(rest: &str) -> Result<(), String> {
    match rest.strip_prefix(':') {
        None if rest.is_empty() => Ok(()),
        None => Err("its host is followed by something that is not a port".into()),
        Some("") => Err("its port is empty".into()),
        Some(port) if !port.bytes().all(|b| b.is_ascii_digit()) => {
            Err("its port is not a number".into())
        }
        Some(port) if port.parse::<u16>().is_err() => Err("its port is above 65535".into()),
        Some(_) => Ok(()),
    }
}

/// The names of a query string's parameters, percent-decoded the way a server
/// reads them (`utm%5Fsource` is `utm_source`); a name whose encoding is
/// malformed is kept as written.
fn query_parameter_names(query: &str) -> impl Iterator<Item = std::borrow::Cow<'_, str>> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| crate::link::percent_decode(p.split_once('=').map(|(k, _)| k).unwrap_or(p)))
}

/// Whether `host` (lowercased) is one of the policy's hosts: an exact name,
/// or any subdomain of a `*.` name (never the apex).
fn host_allowed(host: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|a| match a.strip_prefix("*.") {
        Some(apex) => {
            host.len() > apex.len() + 1
                && host.ends_with(apex)
                && host.as_bytes()[host.len() - apex.len() - 1] == b'.'
        }
        None => a == host,
    })
}

/// One exported outbound link (#215, piece 4): root-relative file with
/// forward slashes, the 1-based line, the destination as written (fragment
/// included, so a liveness tool that checks anchors can).
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExternalLink {
    pub file: String,
    pub line: u32,
    pub url: String,
}

/// The `mdatron links --external --json` document.
#[derive(Serialize)]
pub struct Export<'a> {
    pub mdatron_links_version: &'static str,
    pub links: &'a [ExternalLink],
}

/// Per-run OUTPUT of the external checks — not a cache: which register
/// entries a link used (for `W0057`) and the export list, accumulated across
/// every link-checked file of the run and read once the walk ends. Carried in
/// the run memo for its lifetime (one per `run_inner`), never dropped early.
#[derive(Default)]
pub(crate) struct RunState {
    pub used: HashSet<usize>,
    pub export: Vec<ExternalLink>,
}

/// The link family's per-file options: whether the file is link-checked at
/// all, how relative links resolve, whether `@path` imports are resolved,
/// and — for absolute destinations — the register (when supplied) and the
/// claiming route's policy (when declared). Built once per file at the walk.
#[derive(Default, Clone, Copy)]
pub(crate) struct Context<'a> {
    pub enabled: bool,
    pub root_relative: bool,
    pub imports: bool,
    pub register: Option<&'a Register>,
    pub policy: Option<&'a Policy>,
}

/// Check one absolute destination on `line` of the file at `path` (`rel` is
/// its root-relative form) and, when it is a URL at all, record it for the
/// export — a destination that is not one is `E0117` and nothing a liveness
/// tool could fetch.
pub(crate) fn check(
    ctx: &Context<'_>,
    state: &mut RunState,
    rel: &Path,
    path: &Path,
    line: u32,
    dest: &str,
    findings: &mut Vec<Finding>,
) {
    let url = match parse(dest) {
        Ok(u) => u,
        Err(reason) => {
            findings.push(finding(
                path,
                line,
                "MDATRON-E0117",
                "malformed-url",
                &if crate::link::is_external(dest) {
                    format!("this link's destination is not a well-formed URL: {reason}")
                } else if dest.trim_start().starts_with('\\')
                    || dest.trim_start().starts_with("/\\")
                {
                    {
                        let lead = if dest.trim_start().starts_with('\\') {
                            "`\\`"
                        } else {
                            "`/\\`"
                        };
                        format!(
                            "this link's destination opens with {lead}, and a browser reads \
                             `\\` as `/`, so it is the protocol-relative `//host` URL to every \
                             reader; write `//host` or `https://host` if it is meant as one, or \
                             a path with a single `/` if it is not"
                        )
                    }
                } else {
                    format!(
                        "this link's destination is read by a browser as an absolute URL \
                         (it trims spaces and controls and reads `\\` as `/`), and as one \
                         it is not well-formed: {reason}"
                    )
                },
                Some("correct the destination, or remove the link"),
                vec![quoted("link", dest)],
            ));
            return;
        }
    };
    state.export.push(ExternalLink {
        file: crate::diagnostic::to_forward_slash(rel),
        line,
        url: dest.to_string(),
    });
    if let Some(policy) = ctx.policy {
        check_policy(policy, path, line, dest, &url, findings);
    }
    if let Some(register) = ctx.register {
        match register.resolve(url.page) {
            None => findings.push(finding(
                path,
                line,
                "MDATRON-E0115",
                "external-link-undeclared",
                "this link's absolute URL matches no entry in .mdatron/links.yaml, the \
                 register of the URLs this corpus may point at (an entry matches its URL \
                 exactly, or every URL starting with it when it sets prefix: true)",
                Some(
                    "add the URL to links.yaml — or a prefix entry that covers it — or \
                     correct the link",
                ),
                vec![quoted("link", dest)],
            )),
            Some((i, entry)) => {
                state.used.insert(i);
                if let (Some(declared), Some(frag)) = (&entry.fragments, url.fragment) {
                    if !frag.is_empty() && !declared.iter().any(|d| d == frag) {
                        let list = if declared.is_empty() {
                            "(none)".to_string()
                        } else {
                            declared.join(", ")
                        };
                        findings.push(finding(
                            path,
                            line,
                            "MDATRON-E0116",
                            "external-anchor-undeclared",
                            "this link's `#fragment` is not among the fragments \
                             .mdatron/links.yaml declares for its URL (the entry's \
                             `fragments` is the closed set of anchors the corpus may use on \
                             that page; an entry without one accepts any); mdatron does \
                             not fetch the page to read its headings, so the list is what \
                             it can check against",
                            Some(
                                "add the fragment to the entry's `fragments` after confirming \
                                 the anchor exists on the live page, or correct the link",
                            ),
                            vec![
                                quoted("link", dest),
                                quoted("url", &entry.url),
                                quoted("fragments", &list),
                            ],
                        ));
                    }
                }
            }
        }
    }
}

fn check_policy(
    policy: &Policy,
    path: &Path,
    line: u32,
    dest: &str,
    url: &Url<'_>,
    findings: &mut Vec<Finding>,
) {
    let violation = |reason: &str, clause: &str| {
        finding(
            path,
            line,
            "MDATRON-E0118",
            "link-policy-violation",
            &format!("this link violates the claiming route's link_policy: {reason}"),
            Some("change the link to one the policy allows, or widen the policy"),
            vec![quoted("link", dest), quoted("policy", clause)],
        )
    };
    if let Some(schemes) = &policy.schemes {
        let clause = format!("schemes: {}", schemes.join(", "));
        match url.scheme {
            None => findings.push(violation(
                "it is protocol-relative (no scheme), and the policy lists the schemes a \
                 link may use",
                &clause,
            )),
            Some(s) => {
                let lower = s.to_ascii_lowercase();
                if !schemes.contains(&lower) {
                    findings.push(violation(
                        &format!("its scheme `{s}` is not one the policy allows"),
                        &clause,
                    ));
                }
            }
        }
    }
    if let (Some(hosts), Some(host)) = (&policy.hosts, url.host) {
        let lower = host.to_ascii_lowercase();
        if !host_allowed(&lower, hosts) {
            findings.push(violation(
                &format!("its host `{host}` is not one the policy allows"),
                &format!("hosts: {}", hosts.join(", ")),
            ));
        }
    }
    if let (Some(patterns), Some(query)) = (&policy.forbid_query, url.query) {
        // One finding per link for the clause, naming the forbidden names
        // (distinct, decoded, the first ten): a finding per parameter made a
        // link with thousands of them thousands of findings, each quoting the
        // whole destination.
        const SHOWN: usize = 10;
        let mut names: Vec<String> = Vec::new();
        let mut sources: Vec<&str> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for name in query_parameter_names(query) {
            if !seen.insert(name.to_string()) {
                continue;
            }
            if let Some((source, _)) = patterns.iter().find(|(_, re)| re.is_match(&name)) {
                names.push(name.into_owned());
                if !sources.contains(&source.as_str()) {
                    sources.push(source);
                }
            }
        }
        if !names.is_empty() {
            let mut list = names
                .iter()
                .take(SHOWN)
                .map(|n| format!("`{}`", shown(n)))
                .collect::<Vec<_>>()
                .join(", ");
            if names.len() > SHOWN {
                list.push_str(&format!(" and {} more", names.len() - SHOWN));
            }
            let noun = if names.len() == 1 {
                "parameter"
            } else {
                "parameters"
            };
            findings.push(violation(
                &format!(
                    "its query {noun} {list} {} a forbidden pattern",
                    if names.len() == 1 { "matches" } else { "match" }
                ),
                &format!("forbid_query: {}", sources.join(", ")),
            ));
        }
    }
}

/// `W0056`: the register is supplied but no link-checked file was walked, so
/// every entry is inert — the fail-open class this engine always announces.
pub(crate) fn inert_finding(project_root: &Path) -> Finding {
    Finding {
        code: "MDATRON-W0056".into(),
        severity: Severity::Warning,
        summary: "link-register-inert".into(),
        message: "a `links.yaml` register is present but no walked file is on a route \
                  with links: true, so no absolute URL is checked against it — a \
                  register nothing consults passes silently as if every link were \
                  declared"
            .into(),
        help: Some(
            "opt the files whose links the register should govern into the link family \
             (links: true on their route), or delete links.yaml"
                .into(),
        ),
        location: Location {
            file: project_root.join(".mdatron").join(REGISTER_NAME),
            line: 1,
            column: 0,
        },
        explain_ref: Some("MDATRON-W0056".into()),
        quoted: Vec::new(),
    }
}

/// `W0057`: one per register entry no link used this whole-tree run — a stale
/// entry a liveness tool would keep checking, or a link that went away while
/// the register did not.
pub(crate) fn unused_findings(
    project_root: &Path,
    entries: &[Entry],
    used: &HashSet<usize>,
) -> Vec<Finding> {
    let path = project_root.join(".mdatron").join(REGISTER_NAME);
    entries
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .map(|(_, e)| Finding {
            code: "MDATRON-W0057".into(),
            severity: Severity::Warning,
            summary: "link-register-entry-unused".into(),
            message: if e.prefix {
                "no absolute URL in any link-checked file starts with this links.yaml \
                 prefix entry, so it declares nothing the corpus says — a stale entry a \
                 liveness tool would keep checking, or a link that went away while the \
                 register did not"
            } else {
                "no absolute URL in any link-checked file matches this links.yaml entry, \
                 so it declares nothing the corpus says — a stale entry a liveness tool \
                 would keep checking, or a link that went away while the register did not"
            }
            .into(),
            help: Some("remove the entry, or restore the link it declared".into()),
            location: Location {
                file: path.clone(),
                line: 1,
                column: 0,
            },
            explain_ref: Some("MDATRON-W0057".into()),
            quoted: vec![quoted("url", &e.url)],
        })
        .collect()
}

fn quoted(label: &str, content: &str) -> QuotedRegion {
    QuotedRegion {
        platform_variant: false,
        label: label.into(),
        content: content.into(),
    }
}

fn finding(
    path: &Path,
    line: u32,
    code: &str,
    summary: &str,
    message: &str,
    help: Option<&str>,
    quoted: Vec<QuotedRegion>,
) -> Finding {
    Finding {
        code: code.into(),
        severity: Severity::Error,
        summary: summary.into(),
        message: message.into(),
        help: help.map(str::to_string),
        location: Location {
            file: path.to_path_buf(),
            line,
            column: 0,
        },
        explain_ref: Some(code.to_string()),
        quoted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(dest: &str) -> Url<'_> {
        parse(dest).unwrap_or_else(|e| panic!("{dest}: {e}"))
    }

    fn err(dest: &str) -> String {
        parse(dest)
            .err()
            .unwrap_or_else(|| panic!("{dest}: parsed"))
    }

    #[test]
    fn well_formed_urls_parse_into_their_parts() {
        let u = ok("https://docs.acme.dev/guide?x=1&utm_source=a#install");
        assert_eq!(u.page, "https://docs.acme.dev/guide?x=1&utm_source=a");
        assert_eq!(u.scheme, Some("https"));
        assert_eq!(u.host, Some("docs.acme.dev"));
        assert_eq!(u.query, Some("x=1&utm_source=a"));
        assert_eq!(u.fragment, Some("install"));
        assert_eq!(ok("//cdn.acme.dev/x.png").host, Some("cdn.acme.dev"));
        assert_eq!(
            ok("http://user:pw@host.example:8080/p").host,
            Some("host.example")
        );
        assert_eq!(ok("https://[::1]:3000/").host, Some("[::1]"));
        assert_eq!(ok("https://localhost").host, Some("localhost"));
        assert_eq!(ok("https://例え.jp/").host, Some("例え.jp"));
        assert_eq!(ok("HTTPS://Acme.DEV").host, Some("Acme.DEV"));
        let m = ok("mailto:x@y.z?subject=hi");
        assert_eq!(m.scheme, Some("mailto"));
        assert_eq!(m.host, None);
        assert_eq!(m.query, Some("subject=hi"));
        assert_eq!(ok("https://a.b/#").fragment, Some(""));
        // RFC 3986 reg-name and RFC 6874 zone ids are legal, not stylistic.
        assert_eq!(
            ok("https://my_host.acme.dev/").host,
            Some("my_host.acme.dev")
        );
        assert_eq!(ok("https://a~b.acme.dev/").host, Some("a~b.acme.dev"));
        assert_eq!(ok("https://acme%2Edev/").host, Some("acme%2Edev"));
        assert_eq!(
            ok("https://[fe80::1%25eth0]:80/").host,
            Some("[fe80::1%25eth0]")
        );
        assert_eq!(ok("https://acme.dev/a/b.c/").host, Some("acme.dev"));
        // Emoji sequences join with U+200D and tag characters; legal there.
        assert_eq!(
            ok("https://acme.dev/👨\u{200D}👩\u{200D}👧/").host,
            Some("acme.dev")
        );
        assert_eq!(
            ok("https://acme.dev/🏴\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}/").host,
            Some("acme.dev")
        );
        assert!(parse("https://acme.dev/a\u{200D}b").is_err());
        // `..` in a query or fragment is not a path segment; `%2F` is no separator.
        assert!(parse("https://acme.dev/x?y=../z#../w").is_ok());
        assert!(parse("https://acme.dev/..%2Fx").is_ok());
        assert!(parse("ftp://acme.dev/a/b").is_ok());
        assert!(parse("file:///etc/x").is_ok());
    }

    #[test]
    fn malformed_urls_say_why() {
        assert_eq!(err("https://a b"), "it holds whitespace");
        assert_eq!(err("https://a\u{7}b"), "it holds a control character");
        assert_eq!(err("https://a/#x#y"), "it holds more than one `#`");
        assert_eq!(err("https://"), "it has no host");
        assert_eq!(err("https:///path"), "it has no host");
        assert_eq!(err("https:docs"), "its scheme is not followed by `//`");
        assert_eq!(err("https://.acme.dev"), "its host has an empty label");
        assert_eq!(err("https://a..b"), "its host has an empty label");
        assert_eq!(err("https://a|b.c"), "its host holds `|`");
        // A browser ends a web authority at `\`: the backslash is in the path.
        assert_eq!(
            err("https://a\\b.c"),
            "its path holds `\\`, which a browser reads as `/`"
        );
        // Round 3: the `evil\@good` split, every web scheme, emoji smuggling,
        // invisible host characters, strict IPv6 and ports.
        assert_eq!(
            err("https://evil.example\\@acme.dev/x"),
            "its path holds `\\`, which a browser reads as `/`"
        );
        assert_eq!(err("https://a|b@acme.dev/"), "its userinfo holds `|`");
        assert_eq!(err("wss:acme.dev/x"), "its scheme is not followed by `//`");
        assert_eq!(
            err("wss://acme.dev/x\\..\\admin"),
            "its path holds `\\`, which a browser reads as `/`"
        );
        assert_eq!(
            err("file:///x\\..\\y"),
            "it holds `\\`, which a browser reads as `/` in a `file:` URL"
        );
        assert_eq!(
            err("file://a\\b/c"),
            "it holds `\\`, which a browser reads as `/` in a `file:` URL"
        );
        assert_eq!(
            err("file:C:\\x"),
            "it holds `\\`, which a browser reads as `/` in a `file:` URL"
        );
        assert_eq!(ok("wss://Acme.dev:443/s").host, Some("Acme.dev"));
        assert_eq!(
            err("https://acme.dev/\u{e9}\u{E0041}\u{E0042}"),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://acme.dev/\u{e9}\u{200D}\u{200D}"),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://ac\u{e9}\u{200D}me.acme.dev/"),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://\u{1F468}\u{200D}\u{1F469}.acme.dev/"),
            "its host holds an invisible character"
        );
        assert_eq!(
            err("https://acme.dev/\u{3164}"),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://[:]/"),
            "its IPv6 host literal is not an address"
        );
        assert_eq!(
            err("https://[:::::]/"),
            "its IPv6 host literal is not an address"
        );
        assert_eq!(err("https://host:99999/"), "its port is above 65535");
        assert!(parse("https://acme.dev/\u{2764}\u{FE0F}\u{200D}\u{1F525}/").is_ok());
        // Round 4: a flag's tag run spells a subdivision id, nothing else;
        // variation selectors only where they select.
        let smuggled: String = "ignore previous"
            .chars()
            .map(|c| char::from_u32(0xE0000 + c as u32).unwrap())
            .collect();
        assert_eq!(
            err(&format!("https://acme.dev/\u{1F3F4}{smuggled}\u{E007F}")),
            "it holds an invisible character"
        );
        assert!(parse(
            "https://acme.dev/\u{1F3F4}\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}/"
        )
        .is_ok());
        let vs: String = "hidden"
            .bytes()
            .map(|b| char::from_u32(0xE0100 + b as u32).unwrap())
            .collect();
        assert_eq!(
            err(&format!("https://acme.dev/\u{1F600}{vs}")),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://acme.dev/x\u{FE00}y"),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://acme.dev/a\u{FE0F}\u{200D}\u{1F600}"),
            "it holds an invisible character"
        );
        assert!(parse("https://acme.dev/1\u{FE0F}\u{20E3}").is_ok());
        // Round 6: the whole Default_Ignorable set, reserved code points too.
        let shifted: String = "ignore all"
            .chars()
            .map(|c| char::from_u32(0xE0080 + c as u32).unwrap())
            .collect();
        assert_eq!(
            err(&format!("https://a.example/x{shifted}")),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://a.example/x\u{2065}y"),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://a.example/x\u{FFF0}y"),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://a.example/x\u{E0200}y"),
            "it holds an invisible character"
        );
        assert!(parse("https://a.example/\u{24C2}\u{FE0F}").is_ok());
        // Only RGI subdivision flags: a chain of made-up "flags" is text.
        let flag = |id: &str| -> String {
            let tags: String = id
                .chars()
                .map(|c| char::from_u32(0xE0000 + c as u32).unwrap())
                .collect();
            format!("\u{1F3F4}{tags}\u{E007F}")
        };
        let chained: String = ["ignor", "eallp", "revio"]
            .iter()
            .map(|id| flag(id))
            .collect();
        assert_eq!(
            err(&format!("https://acme.dev/{chained}")),
            "it holds an invisible character"
        );
        assert!(parse(&format!(
            "https://acme.dev/{}{}",
            flag("gbwls"),
            flag("gbeng")
        ))
        .is_ok());
        assert_eq!(
            err("https://a.example/1\u{FE0E}2\u{FE0F}3"),
            "it holds an invisible character"
        );
        assert!(parse("https://acme.dev/\u{8FBB}\u{E0100}").is_ok());
        assert!(parse("https://acme.dev/\u{1F3F3}\u{FE0F}\u{200D}\u{26A7}\u{FE0F}").is_ok());
        assert!(parse("https://[2001:db8::7]:8080/").is_ok());
        assert_eq!(
            err("https://acme.dev/public/../private/x"),
            "its path holds a `.` or `..` segment, which a client resolves away"
        );
        assert_eq!(
            err("https://acme.dev/%2e%2e/x"),
            "its path holds a `.` or `..` segment, which a client resolves away"
        );
        assert_eq!(
            err("https://acme.dev/./x?y=../z"),
            "its path holds a `.` or `..` segment, which a client resolves away"
        );
        assert_eq!(
            err("https://acme\u{200B}.dev/"),
            "it holds an invisible character"
        );
        assert_eq!(
            err("https://[::1%25]/"),
            "its IPv6 host literal is not an address"
        );
        assert_eq!(
            err("https://[abc]/"),
            "its IPv6 host literal is not an address"
        );
        // Round 2: authority forms beyond the web schemes, backslashes,
        // encoded host characters.
        assert_eq!(
            err("ftp://acme.dev/public/../private"),
            "its path holds a `.` or `..` segment, which a client resolves away"
        );
        assert_eq!(
            err("https://acme.dev/docs/x\\..\\..\\admin"),
            "its path holds `\\`, which a browser reads as `/`"
        );
        assert_eq!(
            err("https://evil.example%23.acme.dev/"),
            "its host holds `%23`, an encoded `#` no host may contain"
        );
        assert_eq!(
            err("https://acme%zz.dev/"),
            "its host holds a `%` that is not a percent-encoding"
        );
        assert_eq!(err("https://host:"), "its port is empty");
        assert_eq!(err("https://host:8o80"), "its port is not a number");
        assert_eq!(
            err("https://[::1"),
            "its IPv6 host literal is not closed with `]`"
        );
        assert_eq!(
            err("https://[::1]x"),
            "its host is followed by something that is not a port"
        );
        assert_eq!(err("mailto:"), "nothing follows its scheme");
        assert_eq!(err("//"), "it has no host");
    }

    #[test]
    fn register_resolution_prefers_exact_then_longest_prefix() {
        let entries = vec![
            Entry {
                url: "https://a.dev/".into(),
                prefix: true,
                fragments: None,
            },
            Entry {
                url: "https://a.dev/docs/".into(),
                prefix: true,
                fragments: Some(vec!["x".into()]),
            },
            Entry {
                url: "https://a.dev/docs/exact".into(),
                prefix: false,
                fragments: None,
            },
        ];
        assert_eq!(
            Register::new(entries.clone())
                .resolve("https://a.dev/docs/exact")
                .map(|(i, _)| i),
            Some(2)
        );
        assert_eq!(
            Register::new(entries.clone())
                .resolve("https://a.dev/docs/other")
                .map(|(i, _)| i),
            Some(1)
        );
        assert_eq!(
            Register::new(entries.clone())
                .resolve("https://a.dev/blog")
                .map(|(i, _)| i),
            Some(0)
        );
        assert_eq!(
            Register::new(entries.clone())
                .resolve("https://b.dev/")
                .map(|(i, _)| i),
            None
        );
        // As written: no case folding, no slash normalisation.
        assert_eq!(
            Register::new(entries.clone())
                .resolve("https://A.dev/docs/exact")
                .map(|(i, _)| i),
            None
        );
        assert_eq!(
            Register::new(entries.clone())
                .resolve("https://a.dev")
                .map(|(i, _)| i),
            None
        );
    }

    #[test]
    fn hosts_match_exactly_or_as_subdomains() {
        let allowed = vec!["acme.dev".to_string(), "*.example.com".to_string()];
        assert!(host_allowed("acme.dev", &allowed));
        assert!(!host_allowed("www.acme.dev", &allowed));
        assert!(host_allowed("docs.example.com", &allowed));
        assert!(host_allowed("a.b.example.com", &allowed));
        assert!(!host_allowed("example.com", &allowed));
        assert!(!host_allowed("notexample.com", &allowed));
    }

    #[test]
    fn query_parameter_names_are_split_and_decoded() {
        let names: Vec<String> = query_parameter_names("a=1&utm%5Fsource=x&&flag&b=2=3&%zz=1")
            .map(|c| c.into_owned())
            .collect();
        assert_eq!(names, vec!["a", "utm_source", "flag", "b", "%zz"]);
    }

    fn raw_policy(yaml: &str) -> RawPolicy {
        crate::yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn policy_compile_refuses_what_could_never_apply() {
        for (yaml, needle) in [
            ("{}", "declares no clause"),
            ("schemes: []", "schemes is empty"),
            ("schemes: ['https:']", "not a scheme token"),
            ("schemes: ['1x']", "not a scheme token"),
            ("hosts: []", "hosts is empty"),
            ("hosts: ['https://a.dev']", "not a host name"),
            ("hosts: ['a.dev:443']", "not a host name"),
            ("hosts: ['*.']", "not a host name"),
            ("forbid_query: []", "forbid_query is empty"),
            ("forbid_query: ['(']", "does not compile"),
        ] {
            let e = Policy::compile(raw_policy(yaml))
                .err()
                .unwrap_or_else(|| panic!("{yaml}: compiled"));
            assert!(e.to_string().contains(needle), "{yaml}: {e}");
        }
        let p = Policy::compile(raw_policy(
            "schemes: [HTTPS]\nhosts: ['*.Example.com', acme.dev, '[::1]', my_host.dev]\nforbid_query: ['^utm_']",
        ))
        .unwrap();
        assert_eq!(p.schemes, Some(vec!["https".to_string()]));
        assert_eq!(
            p.hosts,
            Some(vec![
                "*.example.com".to_string(),
                "acme.dev".to_string(),
                "[::1]".to_string(),
                "my_host.dev".to_string()
            ])
        );
        assert_eq!(p.forbid_query.as_ref().map(|v| v.len()), Some(1));
    }

    fn run_check(ctx: &Context<'_>, state: &mut RunState, body: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        let path = Path::new("/root/docs/a.md");
        let rel = Path::new("docs/a.md");
        for link in crate::markup::body_links(body) {
            if crate::link::reads_as_external(&link.dest) {
                let line = 1 + body[..link.offset].matches('\n').count() as u32;
                check(ctx, state, rel, path, line, &link.dest, &mut findings);
            }
        }
        findings
    }

    fn codes(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.code.as_str()).collect()
    }

    #[test]
    fn check_reports_each_offline_defect_and_records_the_export() {
        let entries = vec![
            Entry {
                url: "https://a.dev/page".into(),
                prefix: false,
                fragments: Some(vec!["intro".into()]),
            },
            Entry {
                url: "https://cdn.a.dev/".into(),
                prefix: true,
                fragments: None,
            },
        ];
        let policy = Policy::compile(raw_policy(
            "schemes: [https]\nhosts: [a.dev, '*.a.dev', '[::1]']\nforbid_query: ['^utm_']",
        ))
        .unwrap();
        let reg = Register::new(entries.clone());
        let ctx = Context {
            register: Some(&reg),
            policy: Some(&policy),
            ..Default::default()
        };
        let mut state = RunState::default();
        let body = "\
[ok](https://a.dev/page#intro)
[bad anchor](https://a.dev/page#outro)
[undeclared](https://b.dev/)
[http](http://a.dev/page)
[tracking](https://cdn.a.dev/x?utm%5Fsource=y)
[malformed](<https://a.dev/a b>)
[relative](../x.md)
[v6](https://[::1]/x) [v6bad](https://[::2]/x)
";
        let findings = run_check(&ctx, &mut state, body);
        assert_eq!(
            codes(&findings),
            vec![
                "MDATRON-E0116",
                "MDATRON-E0118",
                "MDATRON-E0115",
                "MDATRON-E0118",
                "MDATRON-E0115",
                "MDATRON-E0118",
                "MDATRON-E0117",
                "MDATRON-E0115",
                "MDATRON-E0118",
                "MDATRON-E0115",
            ],
            "{findings:?}"
        );
        let lines: Vec<u32> = findings.iter().map(|f| f.location.line).collect();
        // The http link violates the scheme clause AND is undeclared: the
        // register compares the page as written, and `http://a.dev/page` is
        // not the declared `https://a.dev/page`. The tracking parameter is
        // caught through its percent-encoding. The listed IPv6 literal passes
        // the hosts clause (and is undeclared); the other fails it too.
        assert_eq!(lines, vec![2, 3, 3, 4, 4, 5, 6, 8, 8, 8]);
        assert_eq!(state.used, HashSet::from([0, 1]));
        let urls: Vec<&str> = state.export.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://a.dev/page#intro",
                "https://a.dev/page#outro",
                "https://b.dev/",
                "http://a.dev/page",
                "https://cdn.a.dev/x?utm%5Fsource=y",
                "https://[::1]/x",
                "https://[::2]/x",
            ],
            "a destination that is not a URL is reported, not exported"
        );
        assert!(state.export.iter().all(|l| l.file == "docs/a.md"));
    }

    #[test]
    fn a_bare_fragment_and_an_entry_without_fragments_accept_any_anchor() {
        let entries = vec![
            Entry {
                url: "https://a.dev/listed".into(),
                prefix: false,
                fragments: Some(vec![]),
            },
            Entry {
                url: "https://a.dev/open".into(),
                prefix: false,
                fragments: None,
            },
        ];
        let reg = Register::new(entries.clone());
        let ctx = Context {
            register: Some(&reg),
            ..Default::default()
        };
        let mut state = RunState::default();
        let body = "[top](https://a.dev/listed#) [any](https://a.dev/open#whatever) [closed](https://a.dev/listed#x)\n";
        let findings = run_check(&ctx, &mut state, body);
        assert_eq!(codes(&findings), vec!["MDATRON-E0116"], "{findings:?}");
        assert_eq!(findings[0].quoted[2].content, "(none)");
    }

    #[test]
    fn without_register_or_policy_only_well_formedness_is_checked() {
        let ctx = Context::default();
        let mut state = RunState::default();
        let body = "[a](https://anything.example/#x) [b](mailto:x@y) [c](<https://a b>)\n";
        let findings = run_check(&ctx, &mut state, body);
        assert_eq!(codes(&findings), vec!["MDATRON-E0117"]);
        assert_eq!(state.export.len(), 2, "the malformed one is not exported");
    }

    #[test]
    fn one_finding_per_link_for_forbidden_query_names() {
        let policy = Policy::compile(raw_policy("forbid_query: ['^utm_']")).unwrap();
        let ctx = Context {
            policy: Some(&policy),
            ..Default::default()
        };
        let mut state = RunState::default();
        let q: Vec<String> = (0..12).map(|i| format!("utm_{i}=1")).collect();
        let body = format!("[x](https://a.dev/?{}&utm_0=2&ok=1)\n", q.join("&"));
        let findings = run_check(&ctx, &mut state, &body);
        assert_eq!(codes(&findings), vec!["MDATRON-E0118"], "{findings:?}");
        assert!(
            findings[0].message.contains("`utm_9` and 2 more"),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn a_prefix_that_runs_past_its_host_cannot_be_extended_into_another() {
        let entries = vec![Entry {
            url: "https://acme.dev/".into(),
            prefix: true,
            fragments: None,
        }];
        let reg = Register::new(entries.clone());
        let ctx = Context {
            register: Some(&reg),
            ..Default::default()
        };
        let mut state = RunState::default();
        let body = "[ok](https://acme.dev/x) [a](https://acme.dev.evil.example/) \
                    [b](https://acme.dev@evil.example/) [c](https://acme.devious.example/)\n";
        let findings = run_check(&ctx, &mut state, body);
        assert_eq!(codes(&findings), vec!["MDATRON-E0115"; 3], "{findings:?}");
    }

    #[test]
    fn unused_entries_and_the_inert_register_are_announced() {
        let entries = vec![
            Entry {
                url: "https://a.dev/".into(),
                prefix: true,
                fragments: None,
            },
            Entry {
                url: "https://b.dev/".into(),
                prefix: false,
                fragments: None,
            },
        ];
        let used = HashSet::from([1]);
        let f = unused_findings(Path::new("/root"), &entries, &used);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "MDATRON-W0057");
        assert_eq!(f[0].quoted[0].content, "https://a.dev/");
        assert!(f[0].message.contains("prefix entry"));
        assert_eq!(inert_finding(Path::new("/root")).code, "MDATRON-W0056");
    }

    struct Dir(std::path::PathBuf);
    impl Dir {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir().join(format!("mdatron-links-{label}-{nanos}"));
            std::fs::create_dir_all(p.join(".mdatron")).unwrap();
            Self(p)
        }
        fn register(&self, yaml: &str) -> Result<Option<LoadedRegister>, Error> {
            std::fs::write(self.0.join(".mdatron").join(REGISTER_NAME), yaml).unwrap();
            load(&self.0)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn load_reads_the_register_and_refuses_what_could_never_match() {
        let d = Dir::new("load");
        assert!(load(&d.0).unwrap().is_none(), "absent = inactive");
        let ok = d
            .register(
                "mdatron_format_version: 1\nlinks:\n- url: https://a.dev/\n  prefix: true\n- url: https://b.dev/p\n  fragments: [x, y]\n",
            )
            .unwrap()
            .unwrap();
        assert_eq!(ok.entries.len(), 2);
        assert!(ok.entries[0].prefix);
        assert_eq!(ok.entries[1].fragments, Some(vec!["x".into(), "y".into()]));
        assert_eq!(ok.digest.len(), 64);
        assert!(d
            .register("mdatron_format_version: 1\nlinks: []\n")
            .unwrap()
            .unwrap()
            .entries
            .is_empty());
        // An opaque-scheme prefix may stop at the scheme; a web prefix must
        // run past its host.
        assert_eq!(
            d.register("mdatron_format_version: 1\nlinks:\n- url: 'mailto:'\n  prefix: true\n- url: //cdn.acme.dev/\n  prefix: true\n")
                .unwrap()
                .unwrap()
                .entries
                .len(),
            2
        );
        for (yaml, needle) in [
            ("links:\n- url: https://a.dev/\n", "must declare `mdatron_format_version"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://a.dev/\n  bogus: 1\n", "bogus"),
            ("mdatron_format_version: 1\nlinks:\n- url: ''\n", "empty url"),
            ("mdatron_format_version: 1\nlinks:\n- url: 'https://a.dev/a b'\n", "holds whitespace"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://a.dev/#x\n", "holds a `#`"),
            ("mdatron_format_version: 1\nlinks:\n- url: docs/x.md\n", "not an absolute URL"),
            ("mdatron_format_version: 1\nlinks:\n- url: 'https://host:'\n", "not a well-formed URL"),
            ("mdatron_format_version: 1\nlinks:\n- url: 'https://'\n  prefix: true\n", "not a well-formed URL"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://acme.dev\n  prefix: true\n", "stops inside its host"),
            ("mdatron_format_version: 1\nlinks:\n- url: //acme.dev\n  prefix: true\n", "stops inside its host"),
            ("mdatron_format_version: 1\nlinks:\n- url: ftp://acme.dev\n  prefix: true\n", "stops inside its host"),
            ("mdatron_format_version: 1\nlinks:\n- url: wss://acme.dev\n  prefix: true\n", "stops inside its host"),
            ("mdatron_format_version: 1\nlinks:\n- url: 'wss:acme.dev'\n  prefix: true\n", "not a well-formed URL"),
            ("mdatron_format_version: 1\nlinks:\n- url: 'https:acme.dev'\n  prefix: true\n", "not a well-formed URL"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://a.dev/\n- url: https://a.dev/\n  prefix: true\n", "duplicate"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://a.dev/\n  fragments: ['']\n", "empty fragment"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://a.dev/\n  fragments: ['a#b']\n", "holding `#`"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://a.dev/\n  fragments: [a, a]\n", "twice"),
        ] {
            let e = d
                .register(yaml)
                .err()
                .unwrap_or_else(|| panic!("{yaml}: loaded"));
            assert!(e.to_string().contains(needle), "{yaml}: {e}");
        }
    }
}
