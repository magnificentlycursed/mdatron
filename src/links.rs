//! External-link checks (#215): what the link family can say about an
//! absolute URL WITHOUT fetching it. mdatron never reaches the network — not
//! in `verify`, not behind a flag: a finding is a function of the tree alone
//! (offline, no clock), while liveness is a property of the world at a moment
//! and belongs to a scheduled liveness tool (lychee and its class). Four
//! offline pieces replace the fetch:
//!
//! 1. **Well-formedness** (`E0117`): a destination that is not a URL at all —
//!    whitespace, a control character, two `#`, an `https://` with no host —
//!    is dead everywhere, forever, and is reported by every link-checked
//!    route with no data supplied.
//! 2. **The register** (`.mdatron/links.yaml`): the closed set of URLs the
//!    corpus may point at, exactly or by prefix. An absolute link no entry
//!    matches is `E0115`; a `#fragment` an entry does not list, when it lists
//!    any, is `E0116`. The register is the one file a liveness tool checks on
//!    a schedule. Supplied-but-inert and never-used entries are announced
//!    (`W0056`, `W0057`), like every dead scope in this engine.
//! 3. **Policy** (`link_policy` on a route): allowed schemes, allowed hosts,
//!    forbidden query parameters (`E0118`).
//! 4. **The export** (`mdatron links --external`): every absolute URL the
//!    link-checked files hold, with file and line, for whatever liveness tool
//!    runs it — fragment included, so a tool that checks anchors can.
//!
//! Activation of the register: the file exists; its scope is the link family's
//! (files a route opts in with `links: true`). Entries and links are compared
//! AS WRITTEN — no case folding, no normalisation of a trailing slash — so
//! what the register says is what the corpus must say.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::Error;

/// The register's file name under `.mdatron/`.
pub const REGISTER_NAME: &str = "links.yaml";

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
pub struct Entry {
    pub url: String,
    pub prefix: bool,
    pub fragments: Option<Vec<String>>,
}

/// The loaded register plus the digest of the bytes read (#176 lineage).
pub struct LoadedRegister {
    pub entries: Vec<Entry>,
    pub digest: String,
}

/// Load `.mdatron/links.yaml`. `Ok(None)` when absent (register inactive);
/// `Err` when unreadable, malformed, or holding an entry that could never
/// match the way it reads (loud, strict parse).
pub fn load(project_root: &Path) -> Result<Option<LoadedRegister>, Error> {
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
    let raw: RawRegister = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    // Load-time refusals: every one is statically knowable and would make an
    // entry inert or ambiguous — the register must say what it means.
    let mut seen: HashSet<&str> = HashSet::new();
    for e in &raw.links {
        let url = e.url.as_str();
        if url.is_empty() {
            return Err(Error::Config(
                "a links.yaml entry has an empty url; an entry names the URL the corpus \
                 may point at"
                    .into(),
            ));
        }
        if let Some(what) = holds_forbidden_char(url) {
            return Err(Error::Config(format!(
                "links.yaml entry '{url}' holds {what}; a URL is written without them"
            )));
        }
        if url.contains('#') {
            return Err(Error::Config(format!(
                "links.yaml entry '{url}' holds a `#`; keep `url` to the page and list the \
                 anchors the corpus may use under `fragments`"
            )));
        }
        if !crate::link::is_external(url) {
            return Err(Error::Config(format!(
                "links.yaml entry '{url}' is not an absolute URL (a scheme such as `https:`, \
                 or a protocol-relative `//host`); a relative link is resolved against the \
                 tree, never the register"
            )));
        }
        if !seen.insert(url) {
            return Err(Error::Config(format!(
                "duplicate links.yaml entry '{url}': two entries for one URL is ambiguous \
                 authority — merge them into one"
            )));
        }
        if let Some(frags) = &e.fragments {
            let mut seen_frags: HashSet<&str> = HashSet::new();
            for f in frags {
                if f.is_empty() {
                    return Err(Error::Config(format!(
                        "links.yaml entry '{url}' declares an empty fragment; a bare `#` \
                         (top of page) is always accepted and needs no entry"
                    )));
                }
                if f.contains('#') || holds_forbidden_char(f).is_some() {
                    return Err(Error::Config(format!(
                        "links.yaml entry '{url}' declares fragment '{f}' holding `#`, \
                         whitespace or a control character; a fragment is written without \
                         its `#`"
                    )));
                }
                if !seen_frags.insert(f) {
                    return Err(Error::Config(format!(
                        "links.yaml entry '{url}' declares fragment '{f}' twice"
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

/// The register entry covering `url` (the destination without its fragment):
/// an exact entry wins over a prefix entry, and among prefix entries the
/// longest wins. Returns the entry's index for the used-set.
fn resolve<'a>(entries: &'a [Entry], url: &str) -> Option<(usize, &'a Entry)> {
    let mut best: Option<(usize, &Entry)> = None;
    for (i, e) in entries.iter().enumerate() {
        if e.url == url {
            return Some((i, e));
        }
        if e.prefix && url.starts_with(e.url.as_str()) {
            match best {
                Some((_, b)) if b.url.len() >= e.url.len() => {}
                _ => best = Some((i, e)),
            }
        }
    }
    best
}

/// A route's link policy as declared in `routes.yaml` (#215, piece 3).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPolicy {
    /// The schemes an absolute link may use (`https`); a protocol-relative
    /// `//host` link has no scheme and violates any list.
    #[serde(default)]
    schemes: Option<Vec<String>>,
    /// The hosts an `http`/`https` (or `//host`) link may name; `*.example.com`
    /// covers every subdomain and not the apex. A link with no host (`mailto:`)
    /// is judged by `schemes` alone.
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
    pub fn compile(raw: RawPolicy) -> Result<Policy, Error> {
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
                    let ok = !name.is_empty()
                        && !name.starts_with('.')
                        && !name.contains("..")
                        && name.chars().all(|c| {
                            !c.is_ascii() || c.is_ascii_alphanumeric() || matches!(c, '-' | '.')
                        });
                    if !ok {
                        return Err(Error::Config(format!(
                            "route link_policy host '{h}' is not a host name (letters, digits, \
                             `-` and `.`, optionally led by `*.` for every subdomain; no \
                             scheme, port or path)"
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
/// the hierarchical forms (`http`, `https`, `//host`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Url<'a> {
    pub scheme: Option<&'a str>,
    pub host: Option<&'a str>,
    pub query: Option<&'a str>,
    pub fragment: Option<&'a str>,
}

/// The character class a URL is written without: it is the one check that
/// applies to every scheme.
fn holds_forbidden_char(s: &str) -> Option<&'static str> {
    s.chars().find_map(|c| {
        if c.is_whitespace() {
            Some("whitespace")
        } else if c.is_control() {
            Some("a control character")
        } else {
            None
        }
    })
}

/// Parse an absolute destination (one `link::is_external` accepted), or say
/// why it is not a well-formed URL. Conservative: only what is wrong
/// everywhere — the structural shape of RFC 3986 — is refused, never a
/// stylistic choice. `http`/`https` and `//host` require a host; any other
/// scheme requires something after its `:`.
pub(crate) fn parse(dest: &str) -> Result<Url<'_>, String> {
    if let Some(what) = holds_forbidden_char(dest) {
        return Err(format!("it holds {what}"));
    }
    let (before_fragment, fragment) = match dest.find('#') {
        Some(i) => (&dest[..i], Some(&dest[i + 1..])),
        None => (dest, None),
    };
    if fragment.is_some_and(|f| f.contains('#')) {
        return Err("it holds more than one `#`".into());
    }
    let query = before_fragment.split_once('?').map(|(_, q)| q);
    let (scheme, after_scheme) = match before_fragment.strip_prefix("//") {
        Some(rest) => (None, rest),
        None => match before_fragment.split_once(':') {
            Some((s, rest)) => (Some(s), rest),
            None => return Err("it has no scheme".into()),
        },
    };
    let hierarchical = match scheme {
        None => true,
        Some(s) => s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https"),
    };
    if !hierarchical {
        if after_scheme.is_empty() {
            return Err("nothing follows its scheme".into());
        }
        return Ok(Url {
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
    let authority_end = after_slashes
        .find(['/', '?'])
        .unwrap_or(after_slashes.len());
    let authority = &after_slashes[..authority_end];
    let host = host_of(authority)?;
    Ok(Url {
        scheme,
        host: Some(host),
        query,
        fragment,
    })
}

/// The host of an authority component: userinfo stripped, port validated and
/// stripped, an IPv6 literal kept with its brackets.
fn host_of(authority: &str) -> Result<&str, String> {
    let hostport = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    if let Some(inner) = hostport.strip_prefix('[') {
        let Some(close) = inner.find(']') else {
            return Err("its IPv6 host literal is not closed with `]`".into());
        };
        check_port(&inner[close + 1..])?;
        let literal = &inner[..close];
        if literal.is_empty()
            || !literal
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || matches!(b, b':' | b'.'))
        {
            return Err("its IPv6 host literal is not an address".into());
        }
        return Ok(&hostport[..close + 2]);
    }
    let host = match hostport.rsplit_once(':') {
        Some((h, port)) => {
            check_port(&hostport[h.len()..])?;
            let _ = port;
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
    if let Some(c) = host
        .chars()
        .find(|c| c.is_ascii() && !(c.is_ascii_alphanumeric() || matches!(c, '-' | '.')))
    {
        return Err(format!("its host holds `{c}`"));
    }
    Ok(host)
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
        Some(_) => Ok(()),
    }
}

/// The names of a query string's parameters, as written.
fn query_parameter_names(query: &str) -> impl Iterator<Item = &str> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| p.split_once('=').map(|(k, _)| k).unwrap_or(p))
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

/// Per-run state the external checks accumulate: which register entries a
/// link used (for `W0057`) and the export list. Lives in the run memo.
#[derive(Default)]
pub struct RunState {
    pub used: HashSet<usize>,
    pub export: Vec<ExternalLink>,
}

/// What the checks read for one file: the register (when supplied) and the
/// claiming route's policy (when declared).
#[derive(Default, Clone, Copy)]
pub struct Context<'a> {
    pub register: Option<&'a [Entry]>,
    pub policy: Option<&'a Policy>,
}

/// Check one absolute destination at byte `at` of `content` (whole file) and
/// record it for the export. `rel` is the file's root-relative path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    ctx: &Context<'_>,
    state: &mut RunState,
    rel: &Path,
    path: &Path,
    content: &str,
    at: usize,
    dest: &str,
    findings: &mut Vec<Finding>,
) {
    let line = line_at(content, at);
    state.export.push(ExternalLink {
        file: rel.to_string_lossy().replace('\\', "/"),
        line,
        url: dest.to_string(),
    });
    let url = match parse(dest) {
        Ok(u) => u,
        Err(reason) => {
            findings.push(finding(
                path,
                line,
                "MDATRON-E0117",
                "malformed-url",
                &format!(
                    "this link's destination is not a well-formed URL: {reason}; it is dead \
                     everywhere, before any liveness check"
                ),
                Some("correct the destination, or remove the link"),
                vec![quoted("link", dest)],
            ));
            return;
        }
    };
    if let Some(policy) = ctx.policy {
        check_policy(policy, path, line, dest, &url, findings);
    }
    if let Some(entries) = ctx.register {
        let page = dest.split_once('#').map(|(p, _)| p).unwrap_or(dest);
        match resolve(entries, page) {
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
        for name in query_parameter_names(query) {
            if let Some((source, _)) = patterns.iter().find(|(_, re)| re.is_match(name)) {
                findings.push(violation(
                    &format!("its query parameter `{name}` matches a forbidden pattern"),
                    &format!("forbid_query: {source}"),
                ));
            }
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

fn line_at(content: &str, at: usize) -> u32 {
    1 + content[..at.min(content.len())].matches('\n').count() as u32
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
        assert_eq!(err("https://a_b.c"), "its host holds `_`");
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
            resolve(&entries, "https://a.dev/docs/exact").map(|(i, _)| i),
            Some(2)
        );
        assert_eq!(
            resolve(&entries, "https://a.dev/docs/other").map(|(i, _)| i),
            Some(1)
        );
        assert_eq!(
            resolve(&entries, "https://a.dev/blog").map(|(i, _)| i),
            Some(0)
        );
        assert_eq!(resolve(&entries, "https://b.dev/").map(|(i, _)| i), None);
        // As written: no case folding, no slash normalisation.
        assert_eq!(
            resolve(&entries, "https://A.dev/docs/exact").map(|(i, _)| i),
            None
        );
        assert_eq!(resolve(&entries, "https://a.dev").map(|(i, _)| i), None);
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
    fn query_parameter_names_are_split_as_written() {
        let names: Vec<&str> = query_parameter_names("a=1&utm_source=x&&flag&b=2=3").collect();
        assert_eq!(names, vec!["a", "utm_source", "flag", "b"]);
    }

    fn raw_policy(yaml: &str) -> RawPolicy {
        serde_yaml_ng::from_str(yaml).unwrap()
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
            "schemes: [HTTPS]\nhosts: ['*.Example.com', acme.dev]\nforbid_query: ['^utm_']",
        ))
        .unwrap();
        assert_eq!(p.schemes, Some(vec!["https".to_string()]));
        assert_eq!(
            p.hosts,
            Some(vec!["*.example.com".to_string(), "acme.dev".to_string()])
        );
        assert_eq!(p.forbid_query.as_ref().map(|v| v.len()), Some(1));
    }

    fn run_check(ctx: &Context<'_>, state: &mut RunState, body: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        let path = Path::new("/root/docs/a.md");
        let rel = Path::new("docs/a.md");
        for link in crate::markup::body_links(body) {
            if crate::link::is_external(&link.dest) {
                check(
                    ctx,
                    state,
                    rel,
                    path,
                    body,
                    link.offset,
                    &link.dest,
                    &mut findings,
                );
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
            "schemes: [https]\nhosts: [a.dev, '*.a.dev']\nforbid_query: ['^utm_']",
        ))
        .unwrap();
        let ctx = Context {
            register: Some(&entries),
            policy: Some(&policy),
        };
        let mut state = RunState::default();
        let body = "\
[ok](https://a.dev/page#intro)
[bad anchor](https://a.dev/page#outro)
[undeclared](https://b.dev/)
[http](http://a.dev/page)
[tracking](https://cdn.a.dev/x?utm_source=y)
[malformed](<https://a.dev/a b>)
[relative](../x.md)
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
            ],
            "{findings:?}"
        );
        let lines: Vec<u32> = findings.iter().map(|f| f.location.line).collect();
        // The http link violates the scheme clause AND is undeclared: the
        // register compares the page as written, and `http://a.dev/page` is
        // not the declared `https://a.dev/page`.
        assert_eq!(lines, vec![2, 3, 3, 4, 4, 5, 6]);
        assert_eq!(state.used, HashSet::from([0, 1]));
        let urls: Vec<&str> = state.export.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://a.dev/page#intro",
                "https://a.dev/page#outro",
                "https://b.dev/",
                "http://a.dev/page",
                "https://cdn.a.dev/x?utm_source=y",
                "https://a.dev/a b",
            ]
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
        let ctx = Context {
            register: Some(&entries),
            policy: None,
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
        assert_eq!(state.export.len(), 3);
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
        for (yaml, needle) in [
            ("links:\n- url: https://a.dev/\n", "must declare `mdatron_format_version"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://a.dev/\n  bogus: 1\n", "bogus"),
            ("mdatron_format_version: 1\nlinks:\n- url: ''\n", "empty url"),
            ("mdatron_format_version: 1\nlinks:\n- url: 'https://a.dev/a b'\n", "holds whitespace"),
            ("mdatron_format_version: 1\nlinks:\n- url: https://a.dev/#x\n", "holds a `#`"),
            ("mdatron_format_version: 1\nlinks:\n- url: docs/x.md\n", "not an absolute URL"),
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
