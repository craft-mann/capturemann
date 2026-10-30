//! Secrets and credentials: API keys and tokens, private-key blocks, `NAME=value` pairs and
//! URLs that carry credentials.
//!
//! Provider key shapes are data ([`KEY_SHAPES`]); the configuration-syntax rules use the
//! structural name table [`SECRET_NAME_PARTS`]. No rule here looks at one example: each is a
//! shape (prefix, length, alphabet) or a syntax (`NAME=value`, `scheme://user:password@host`,
//! `-----BEGIN ... PRIVATE KEY-----`).

use super::{Flat, Located, Span};
use crate::ocr::detect::RedactionKind;

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+' | '/')
}

/// The alphabet a key must use after its prefix.
#[derive(Clone, Copy)]
enum Charset {
    /// Any token character (letters, digits, `_ - + /`).
    Any,
    /// Letters and digits only.
    Alnum,
    /// Upper-case letters and digits only.
    UpperAlnum,
    /// Hexadecimal digits.
    Hex,
    /// Letters, digits, `_` and `-` (base64url, as in GitLab and Google tokens).
    Url,
}

impl Charset {
    fn accepts(self, ch: char) -> bool {
        match self {
            Charset::Any => is_token_char(ch),
            Charset::Alnum => ch.is_ascii_alphanumeric(),
            Charset::UpperAlnum => ch.is_ascii_uppercase() || ch.is_ascii_digit(),
            Charset::Hex => ch.is_ascii_hexdigit(),
            Charset::Url => ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'),
        }
    }
}

/// One public key format: the word starts with `prefix`, has `min_len..=max_len` characters in
/// total (`max_len` 0 means no upper bound) and every character after the prefix is in `charset`.
struct KeyShape {
    prefix: &'static str,
    min_len: usize,
    max_len: usize,
    charset: Charset,
    confidence: f32,
}

const fn shape(
    prefix: &'static str,
    min_len: usize,
    max_len: usize,
    charset: Charset,
    confidence: f32,
) -> KeyShape {
    KeyShape {
        prefix,
        min_len,
        max_len,
        charset,
        confidence,
    }
}

/// Version of [`KEY_SHAPES`]; bump it when an entry is added, removed or changed so a benchmark
/// report can say which table it measured.
pub const KEY_SHAPES_VERSION: u32 = 1;

/// Provider key shapes. Prefixes and lengths come from each provider's public documentation or
/// the open-source secret scanners that publish the same formats; a prefix alone is never
/// enough, the length and alphabet must fit too. The dotted shapes (`ya29.`, `SG.`) are in
/// [`dotted_shape`].
const KEY_SHAPES: &[KeyShape] = &[
    // OpenAI / Anthropic style: `sk-`, `sk-proj-`, `sk-ant-` + a long random part (provider API
    // reference, "API keys"). Kept as in version 0: any token characters, 20 or more in all.
    shape("sk-", 20, 0, Charset::Any, 0.95),
    // GitHub tokens: ghp_ ghu_ ghs_ gho_ ghr_ + 36 alphanumerics; fine-grained `github_pat_`
    // (GitHub docs, "About authentication to GitHub" / token formats blog post, 2021).
    shape("ghp_", 30, 0, Charset::Alnum, 0.95),
    shape("gho_", 30, 0, Charset::Alnum, 0.95),
    shape("ghu_", 30, 0, Charset::Alnum, 0.95),
    shape("ghs_", 30, 0, Charset::Alnum, 0.95),
    shape("ghr_", 30, 0, Charset::Alnum, 0.95),
    shape("github_pat_", 30, 0, Charset::Url, 0.95),
    // GitLab personal/project/group access tokens: `glpat-` + 20 (GitLab docs, "Token overview").
    shape("glpat-", 26, 0, Charset::Url, 0.95),
    // Stripe (docs, "API keys"): secret `sk_live_` / `sk_test_`, restricted `rk_live_` /
    // `rk_test_`, publishable `pk_live_`, webhook signing secret `whsec_` (Stripe, "Webhooks").
    shape("sk_live_", 20, 0, Charset::Alnum, 0.95),
    shape("pk_live_", 20, 0, Charset::Alnum, 0.95),
    shape("rk_live_", 20, 0, Charset::Alnum, 0.95),
    shape("sk_test_", 20, 0, Charset::Alnum, 0.9),
    shape("rk_test_", 20, 0, Charset::Alnum, 0.9),
    shape("whsec_", 30, 0, Charset::Alnum, 0.9),
    // Slack tokens `xoxb-` bot, `xoxp-` user, `xoxa-`, `xoxr-` refresh, `xoxs-`
    // (Slack docs, "Token types"); app-level tokens `xapp-` (same page).
    shape("xoxb", 15, 0, Charset::Any, 0.95),
    shape("xoxa", 15, 0, Charset::Any, 0.95),
    shape("xoxp", 15, 0, Charset::Any, 0.95),
    shape("xoxr", 15, 0, Charset::Any, 0.95),
    shape("xoxs", 15, 0, Charset::Any, 0.95),
    shape("xapp-", 20, 0, Charset::Url, 0.95),
    // npm automation/publish tokens: `npm_` + 36 alphanumerics (npm blog, "New access token
    // format", 2021). No underscore after the prefix, so `npm_config_*` names do not match.
    shape("npm_", 40, 40, Charset::Alnum, 0.95),
    // PyPI API tokens: a macaroon that starts `pypi-AgEIcHlwaS5vcmc` (PyPI help, "API tokens").
    shape("pypi-AgEIcHlwaS5vcmc", 40, 0, Charset::Url, 0.95),
    // Twilio: API key SID `SK` + 32 hex, account SID `AC` + 32 hex (Twilio docs, "API keys",
    // "Account SID").
    shape("SK", 34, 34, Charset::Hex, 0.9),
    shape("AC", 34, 34, Charset::Hex, 0.9),
    // Google API keys `AIza` + 35 (Google Cloud docs, "API keys"); OAuth tokens are `ya29.`.
    shape("AIza", 35, 0, Charset::Url, 0.95),
    // AWS access key IDs: 4-letter type prefix + 16 upper-case alphanumerics (AWS IAM docs,
    // "Understand unique ID prefixes"): AKIA long-term, ASIA temporary, AIDA user, AROA role.
    shape("AKIA", 20, 20, Charset::UpperAlnum, 0.95),
    shape("ASIA", 20, 20, Charset::UpperAlnum, 0.95),
    shape("AIDA", 20, 20, Charset::UpperAlnum, 0.95),
    shape("AROA", 20, 20, Charset::UpperAlnum, 0.95),
    // Hugging Face user access tokens: `hf_` + 34 letters (Hugging Face docs, "User access
    // tokens").
    shape("hf_", 37, 0, Charset::Alnum, 0.95),
    // Mailgun private API keys: `key-` + 32 hex (Mailgun docs, "Authentication").
    shape("key-", 36, 36, Charset::Hex, 0.9),
    // DigitalOcean personal access (`dop_v1_`), OAuth (`doo_v1_`) and refresh (`dor_v1_`) tokens:
    // prefix + 64 hex (DigitalOcean docs, "Token prefixes" / API changelog 2021).
    shape("dop_v1_", 71, 71, Charset::Hex, 0.95),
    shape("doo_v1_", 71, 71, Charset::Hex, 0.95),
    shape("dor_v1_", 71, 71, Charset::Hex, 0.95),
    // Shopify access tokens: `shpat_` Admin API, `shpca_` custom app, `shppa_` private app,
    // `shpss_` shared secret, each + 32 hex (Shopify dev docs, "Access tokens").
    shape("shpat_", 38, 38, Charset::Hex, 0.95),
    shape("shpca_", 38, 38, Charset::Hex, 0.95),
    shape("shppa_", 38, 38, Charset::Hex, 0.95),
    shape("shpss_", 38, 38, Charset::Hex, 0.95),
];

/// Shapes that contain dots, so they are not one run of token characters. `run` has no trailing
/// dot.
fn dotted_shape(run: &str) -> Option<f32> {
    let url_part =
        |p: &str, len: usize| p.len() == len && p.chars().all(|c| Charset::Url.accepts(c));
    // SendGrid API keys: `SG.` + 22 base64url + `.` + 43 base64url (SendGrid docs, "API keys").
    let parts: Vec<&str> = run.split('.').collect();
    if parts.len() == 3 && parts[0] == "SG" && url_part(parts[1], 22) && url_part(parts[2], 43) {
        return Some(0.95);
    }
    // Google OAuth 2.0 access tokens start `ya29.` (Google Identity docs, "OAuth 2.0 for
    // server-side apps": token examples).
    if let Some(rest) = run.strip_prefix("ya29.") {
        if rest.chars().count() >= 20 && rest.chars().all(|c| Charset::Url.accepts(c)) {
            return Some(0.9);
        }
    }
    None
}

pub fn find_tokens(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    // JWTs first: three base64url segments with dots between them. Dotted provider shapes too.
    let mut i = 0;
    while i < c.len() {
        let run_end = (i..c.len())
            .find(|&j| !(is_token_char(c[j]) || c[j] == '.'))
            .unwrap_or(c.len());
        let run: String = c[i..run_end].iter().collect();
        let trimmed = run.trim_end_matches('.');
        let found = if run.starts_with("eyJ") {
            let parts: Vec<&str> = trimmed.split('.').collect();
            (parts.len() == 3 && parts.iter().all(|p| p.len() >= 8)).then_some(0.9)
        } else {
            dotted_shape(trimmed)
        };
        if let Some(confidence) = found {
            out.push(Span {
                start: i,
                end: i + trimmed.chars().count(),
                kind: RedactionKind::Token,
                confidence,
            });
        }
        i = run_end.max(i + 1);
    }
    // Everything else: runs of token characters (a '.' or '=' ends one; trailing '=' padding
    // belongs to it).
    let mut i = 0;
    while i < c.len() {
        if !is_token_char(c[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < c.len() && is_token_char(c[j]) {
            j += 1;
        }
        let mut end = j;
        while end < c.len() && end < j + 2 && c[end] == '=' {
            end += 1;
        }
        let word: String = c[i..j].iter().collect();
        if let Some(conf) = classify_token(&word) {
            if !out.iter().any(|s: &Span| i < s.end && s.start < end) {
                out.push(Span {
                    start: i,
                    end,
                    kind: RedactionKind::Token,
                    confidence: conf,
                });
            }
        }
        i = j.max(i + 1);
    }
    find_bearer(c, &mut out);
    out
}

/// The credential after `Bearer ` in an HTTP Authorization header (RFC 6750, section 2.1: the
/// token is `1*( ALPHA / DIGIT / "-" / "." / "_" / "~" / "+" / "/" ) *"="`). A span that
/// overlaps a shape found already is replaced by the whole credential, keeping the higher
/// confidence.
fn find_bearer(c: &[char], out: &mut Vec<Span>) {
    const WORD: [char; 6] = ['b', 'e', 'a', 'r', 'e', 'r'];
    let mut i = 0;
    while i + WORD.len() < c.len() {
        let at_word = (i == 0 || !c[i - 1].is_ascii_alphanumeric())
            && c[i..i + 6]
                .iter()
                .zip(WORD)
                .all(|(a, b)| a.to_ascii_lowercase() == b);
        if !at_word || c[i + 6] != ' ' {
            i += 1;
            continue;
        }
        let mut s = i + 6;
        while s < c.len() && c[s] == ' ' {
            s += 1;
        }
        let mut e = s;
        while e < c.len() && (c[e].is_ascii_alphanumeric() || "-._~+/=".contains(c[e])) {
            e += 1;
        }
        while e > s && c[e - 1] == '.' {
            e -= 1;
        }
        let digits = c[s..e].iter().any(char::is_ascii_digit);
        if e - s >= 16 && digits {
            let mut conf = 0.85f32;
            out.retain(|t| {
                let overlap = s < t.end && t.start < e;
                if overlap {
                    conf = conf.max(t.confidence);
                }
                !overlap
            });
            out.push(Span {
                start: s,
                end: e,
                kind: RedactionKind::Token,
                confidence: conf,
            });
        }
        i = e.max(i + 1);
    }
}

fn classify_token(w: &str) -> Option<f32> {
    let n = w.chars().count();
    let best = KEY_SHAPES
        .iter()
        .filter(|k| {
            w.starts_with(k.prefix)
                && n >= k.min_len
                && (k.max_len == 0 || n <= k.max_len)
                && w[k.prefix.len()..].chars().all(|ch| k.charset.accepts(ch))
        })
        .map(|k| k.confidence)
        .fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b))));
    if best.is_some() {
        return best;
    }
    // Long, random-looking: letters and digits mixed (hex, base64). A UUID (RFC 4122 text form,
    // 8-4-4-4-12 hex) is an identifier, not a secret.
    if n >= 24 && !is_uuid(w) {
        let digits = w.chars().filter(char::is_ascii_digit).count();
        let letters = w.chars().filter(char::is_ascii_alphabetic).count();
        if digits >= 3 && letters >= 3 && digits * 100 >= n * 7 && !w.starts_with('/') {
            return Some(0.6);
        }
    }
    None
}

fn is_uuid(w: &str) -> bool {
    let groups: Vec<&str> = w.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// True when `v` is a credential with a known provider shape or a JWT; the token rule reports
/// those with a higher confidence, so the structural rules leave them alone.
fn known_shape(v: &[char]) -> bool {
    let s: String = v.iter().collect();
    if s.starts_with("eyJ") && s.split('.').count() == 3 {
        return true;
    }
    dotted_shape(&s).is_some() || classify_token(&s).is_some_and(|c| c >= 0.9)
}

/// Words that, as a part of a name, mark a configuration key whose value is a secret. This is a
/// structural pattern for configuration syntax (`NAME=value`, `"name": "value"`, `?name=value`),
/// matched on whole parts after splitting the name at `_ - .` and camelCase boundaries (and on two
/// adjacent parts joined, so `api_key` and `apiKey` both give `apikey`). It is a short list of
/// the generic words that credentials are called in code, not phrases from any one document.
const SECRET_NAME_PARTS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "passphrase",
    "secret",
    "token",
    "apikey",
    "accesskey",
    "privatekey",
    "secretkey",
    "signingkey",
    "auth",
    "authorization",
    "credential",
    "credentials",
];

/// Last parts of a name that say the value describes a secret instead of being one
/// (`token_type`, `password_length`, `auth_url`).
const NOT_A_VALUE_SUFFIXES: &[&str] = &[
    "count", "length", "len", "size", "limit", "type", "expires", "expiry", "ttl", "max", "min",
    "url", "uri", "path", "file", "name", "id", "field", "label", "hint", "prompt", "required",
    "enabled", "header", "method", "mode",
];

/// HTTP authentication scheme words (RFC 7235) that come before the credential.
const AUTH_SCHEMES: &[&str] = &["bearer", "basic", "digest", "token"];

/// Splits a name at `_ - .` and at camelCase boundaries (`apiKey`, `APIKey`), lower-cased.
fn name_parts(name: &[char]) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &ch) in name.iter().enumerate() {
        if matches!(ch, '_' | '-' | '.') {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let prev = i.checked_sub(1).map(|p| name[p]);
        let next = name.get(i + 1).copied();
        let boundary = ch.is_ascii_uppercase()
            && prev.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase()))
            });
        if boundary && !cur.is_empty() {
            parts.push(std::mem::take(&mut cur));
        }
        cur.push(ch.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

fn is_secret_name(name: &[char]) -> bool {
    let parts = name_parts(name);
    if parts
        .last()
        .is_some_and(|l| NOT_A_VALUE_SUFFIXES.contains(&l.as_str()))
    {
        return false;
    }
    parts
        .iter()
        .any(|p| SECRET_NAME_PARTS.contains(&p.as_str()))
        || parts
            .windows(2)
            .any(|w| SECRET_NAME_PARTS.contains(&format!("{}{}", w[0], w[1]).as_str()))
}

fn is_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')
}

/// A value that is only a mask, a template or a literal, not a secret.
fn is_placeholder(v: &[char]) -> bool {
    let s: String = v.iter().collect();
    s.chars().all(|c| "*xX.•·#_-?".contains(c))
        || (s.starts_with('<') && s.ends_with('>'))
        || s.starts_with("${")
        || s.starts_with("{{")
        || s.starts_with('$')
        || (s.starts_with('%') && s.ends_with('%'))
        || ["null", "none", "nil", "true", "false", "undefined"]
            .contains(&s.to_ascii_lowercase().as_str())
}

const MIN_VALUE: usize = 4;

/// `NAME=value`, `NAME: value`, `"name": "value"`, `export NAME=value`, `NAME = "value"` where
/// the name has a secret-looking part (see [`SECRET_NAME_PARTS`]). The span is the value only,
/// without quotes. Confidence 0.7. Syntax from the dotenv, shell, YAML and JSON conventions; the
/// name list follows the OWASP Secrets Management Cheat Sheet's list of what counts as a secret
/// (passwords, API keys, tokens, private keys, credentials).
pub fn find_secret_assignments(c: &[char]) -> Vec<Span> {
    let n = c.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if !is_name_char(c[i]) {
            i += 1;
            continue;
        }
        let s = i;
        while i < n && is_name_char(c[i]) {
            i += 1;
        }
        let name = &c[s..i];
        let mut p = i;
        if p < n && matches!(c[p], '"' | '\'') {
            p += 1;
        }
        while p < n && c[p] == ' ' {
            p += 1;
        }
        let Some(&sep) = c.get(p) else { continue };
        let next = c.get(p + 1).copied();
        match sep {
            '=' if matches!(next, Some('=' | '>')) => continue,
            ':' if matches!(next, Some('/' | ':' | '=')) => continue,
            '=' | ':' => {}
            _ => continue,
        }
        if !is_secret_name(name) {
            continue;
        }
        p += 1;
        while p < n && c[p] == ' ' {
            p += 1;
        }
        let Some((vs, ve, quoted)) = value_range(c, p) else {
            continue;
        };
        let v = &c[vs..ve];
        if v.len() < MIN_VALUE || is_placeholder(v) || known_shape(v) {
            continue;
        }
        if !quoted {
            let lowercase_word = v.iter().all(|&ch| ch.is_ascii_lowercase() || ch == '_');
            // `auth: required`, `token: string`: a plain lower-case word after a colon is prose
            // or a type, not a secret.
            if (sep == ':' && lowercase_word) || v.iter().any(|&ch| matches!(ch, '(' | ')')) {
                continue;
            }
        }
        out.push(Span {
            start: vs,
            end: ve,
            kind: RedactionKind::SecretAssignment,
            confidence: 0.7,
        });
        i = i.max(ve);
    }
    out
}

/// Where the value that starts at `p` is: `(start, end, quoted)`.
fn value_range(c: &[char], p: usize) -> Option<(usize, usize, bool)> {
    let n = c.len();
    let first = *c.get(p)?;
    if matches!(first, '"' | '\'') {
        let start = p + 1;
        let mut end = (start..n).find(|&j| c[j] == first).unwrap_or(n);
        if end == n {
            while end > start && matches!(c[end - 1], ' ' | ',' | ';') {
                end -= 1;
            }
        }
        return (end > start).then_some((start, end, true));
    }
    if matches!(first, '[' | '{') && c.get(p + 1) != Some(&'{') {
        return None;
    }
    let word_end = |from: usize| (from..n).find(|&j| c[j] == ' ').unwrap_or(n);
    let (mut start, mut end) = (p, word_end(p));
    let word: String = c[start..end]
        .iter()
        .collect::<String>()
        .to_ascii_lowercase();
    if AUTH_SCHEMES.contains(&word.as_str()) && end < n {
        // `Basic dXNlcjpwYXNz`: the credential is the next word. A bearer token is the token
        // rule's job.
        if word == "bearer" {
            return None;
        }
        start = end;
        while start < n && c[start] == ' ' {
            start += 1;
        }
        end = word_end(start);
    }
    while end > start && matches!(c[end - 1], ',' | ';' | '"' | '\'') {
        end -= 1;
    }
    (end > start).then_some((start, end, false))
}

/// URLs that carry a secret: `scheme://user:password@host` (the span is the password; RFC 3986,
/// section 3.2.1 userinfo, deprecated but common) and `?api_key=...&token=...` query parameters
/// whose name has a secret-looking part (the span is the value). Confidence 0.8.
pub fn find_url_secrets(c: &[char]) -> Vec<Span> {
    let n = c.len();
    let mut out = Vec::new();
    let url_end = |from: usize| {
        (from..n)
            .find(|&j| c[j].is_whitespace() || matches!(c[j], '"' | '\'' | '<' | '>'))
            .unwrap_or(n)
    };
    // Credentials in the authority.
    let mut k = 1;
    while k + 2 < n {
        if c[k] == ':' && c[k + 1] == '/' && c[k + 2] == '/' && c[k - 1].is_ascii_alphanumeric() {
            let a_start = k + 3;
            let a_end = (a_start..url_end(a_start))
                .find(|&j| matches!(c[j], '/' | '?' | '#'))
                .unwrap_or_else(|| url_end(a_start));
            if let Some(at) = (a_start..a_end).rev().find(|&j| c[j] == '@') {
                if let Some(colon) = (a_start..at).find(|&j| c[j] == ':') {
                    let pw = &c[colon + 1..at];
                    if pw.len() >= 3 && at + 1 < a_end && !is_placeholder(pw) {
                        out.push(Span {
                            start: colon + 1,
                            end: at,
                            kind: RedactionKind::UrlSecret,
                            confidence: 0.8,
                        });
                    }
                }
            }
            k = a_end.max(k + 1);
        } else {
            k += 1;
        }
    }
    // Secret-named query parameters.
    let mut i = 0;
    while i < n {
        if !matches!(c[i], '?' | '&' | ';') {
            i += 1;
            continue;
        }
        let s = i + 1;
        let mut e = s;
        while e < n && is_name_char(c[e]) {
            e += 1;
        }
        i = e.max(i + 1);
        if e == s || c.get(e) != Some(&'=') || !is_secret_name(&c[s..e]) {
            continue;
        }
        let vs = e + 1;
        let ve = (vs..url_end(vs))
            .find(|&j| matches!(c[j], '&' | '#'))
            .unwrap_or_else(|| url_end(vs));
        let v = &c[vs..ve];
        if v.len() >= MIN_VALUE && !is_placeholder(v) {
            out.push(Span {
                start: vs,
                end: ve,
                kind: RedactionKind::UrlSecret,
                confidence: 0.8,
            });
        }
        i = ve.max(i);
    }
    out
}

/// How a line relates to a PEM block.
#[derive(PartialEq)]
enum Boundary {
    Begin,
    End,
}

/// Is this line a `-----BEGIN ... PRIVATE KEY-----` / `-----END ...-----` line? RFC 7468
/// (PEM textual encoding) names the labels (`RSA PRIVATE KEY`, `EC PRIVATE KEY`,
/// `ENCRYPTED PRIVATE KEY`, `PRIVATE KEY`); OpenSSH and OpenPGP (RFC 4880, section 6.2,
/// `PGP PRIVATE KEY BLOCK`) use the same frame. Recognized text may have mangled the dashes, so
/// three or more of them, with spaces between, are accepted, and case is ignored. The line must
/// start with the dashes, so a sentence that quotes a header is not a block.
fn boundary(c: &[char]) -> Option<Boundary> {
    let dash = |ch: char| matches!(ch, '-' | '—' | '–');
    let lead = c.iter().take_while(|&&ch| dash(ch) || ch == ' ').count();
    if c[..lead].iter().filter(|&&ch| dash(ch)).count() < 3 {
        return None;
    }
    let squeezed: String = c[lead..]
        .iter()
        .filter(|ch| ch.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    if !squeezed.contains("PRIVATEKEY") {
        return None;
    }
    if squeezed.starts_with("BEGIN") {
        Some(Boundary::Begin)
    } else if squeezed.starts_with("END") {
        Some(Boundary::End)
    } else {
        None
    }
}

/// Is the line plausibly a base64 body line: only the base64 alphabet, and not word-like
/// (several short words)? Armor headers (`Proc-Type: 4,ENCRYPTED`, `Version: ...`) are accepted
/// before the body starts.
fn body_line(c: &[char], in_headers: &mut bool) -> bool {
    let text: String = c.iter().collect();
    let t = text.trim();
    if t.is_empty() {
        return *in_headers;
    }
    if *in_headers {
        if let Some((name, _)) = t.split_once(':') {
            if name.len() <= 24 && name.chars().all(|ch| ch.is_ascii_alphabetic() || ch == '-') {
                return true;
            }
        }
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    let n: usize = words.iter().map(|w| w.chars().count()).sum();
    let alphabet = t
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | ' '));
    let ok = alphabet && (words.len() <= 3 || n / words.len() >= 8);
    if ok {
        *in_headers = false;
    }
    ok
}

/// A block with no END line is followed for at most this many lines.
const MAX_BLOCK_LINES: usize = 60;

/// PEM private-key blocks, from the BEGIN line to the END line (or up to [`MAX_BLOCK_LINES`]
/// lines, or the first line that is clearly not base64). One span per line of the block, over
/// the whole line; where a line rule already found something on a line, the spans cover the
/// rest of the line so the block has no gap (the page keeps the first span on overlapping
/// characters). Confidence 0.95.
pub fn find_private_keys(lines: &[Flat], found: &[Located]) -> Vec<Located> {
    let mut out: Vec<(usize, usize, usize)> = Vec::new();
    let emit = |out: &mut Vec<(usize, usize, usize)>, line: usize, len: usize| {
        let mut covered: Vec<(usize, usize)> = found
            .iter()
            .filter(|l| l.line == line)
            .map(|l| (l.span.start, l.span.end.min(len)))
            .collect();
        covered.sort_unstable();
        let mut at = 0;
        for (s, e) in covered {
            if s > at {
                out.push((line, at, s));
            }
            at = at.max(e);
        }
        if at < len {
            out.push((line, at, len));
        }
    };
    let mut i = 0;
    while i < lines.len() {
        if boundary(&lines[i].chars) != Some(Boundary::Begin) {
            i += 1;
            continue;
        }
        emit(&mut out, i, lines[i].chars.len());
        let mut in_headers = true;
        let mut k = i + 1;
        while k < lines.len() && k <= i + MAX_BLOCK_LINES {
            let chars = &lines[k].chars;
            match boundary(chars) {
                Some(Boundary::End) => {
                    emit(&mut out, k, chars.len());
                    k += 1;
                    break;
                }
                Some(Boundary::Begin) => break,
                None => {}
            }
            if !body_line(chars, &mut in_headers) {
                break;
            }
            if !chars.is_empty() {
                emit(&mut out, k, chars.len());
            }
            k += 1;
        }
        i = k.max(i + 1);
    }
    out.into_iter()
        .map(|(line, start, end)| Located {
            line,
            span: Span {
                start,
                end,
                kind: RedactionKind::PrivateKey,
                confidence: 0.95,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::{suggest_redactions, SuggestOptions, TextLine};

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn texts(spans: &[Span], s: &str) -> Vec<String> {
        let c = chars(s);
        spans
            .iter()
            .map(|sp| c[sp.start..sp.end].iter().collect())
            .collect()
    }

    fn token_conf(s: &str) -> Option<f32> {
        find_tokens(&chars(s)).first().map(|sp| sp.confidence)
    }

    /// Invented, random-looking filler (never a real key).
    fn rnd(n: usize, alphabet: &str) -> String {
        let a: Vec<char> = alphabet.chars().collect();
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                a[(x % a.len() as u64) as usize]
            })
            .collect()
    }

    const ALNUM: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const HEX: &str = "0123456789abcdef";
    const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

    #[test]
    fn provider_shapes_are_found() {
        let keys = [
            format!("{}{}", "sk_live_", rnd(24, ALNUM)),
            format!("{}{}", "sk_test_", rnd(24, ALNUM)),
            format!("{}{}", "rk_live_", rnd(24, ALNUM)),
            format!("{}{}", "whsec_", rnd(32, ALNUM)),
            format!("xoxb-{}-{}", rnd(11, "0123456789"), rnd(24, ALNUM)),
            format!("xapp-1-{}-{}", rnd(10, UPPER), rnd(30, ALNUM)),
            format!("ghp_{}", rnd(36, ALNUM)),
            format!("github_pat_{}", rnd(40, ALNUM)),
            format!("glpat-{}", rnd(20, ALNUM)),
            format!("npm_{}", rnd(36, ALNUM)),
            format!("pypi-AgEIcHlwaS5vcmc{}", rnd(60, ALNUM)),
            format!("SG.{}.{}", rnd(22, ALNUM), rnd(43, ALNUM)),
            format!("SK{}", rnd(32, HEX)),
            format!("AC{}", rnd(32, HEX)),
            format!("AIza{}", rnd(35, ALNUM)),
            format!("ya29.{}", rnd(40, ALNUM)),
            format!("AKIA{}", rnd(16, UPPER)),
            format!("ASIA{}", rnd(16, UPPER)),
            format!("AIDA{}", rnd(16, UPPER)),
            format!("AROA{}", rnd(16, UPPER)),
            format!("sk-ant-{}", rnd(40, ALNUM)),
            format!("sk-proj-{}", rnd(40, ALNUM)),
            format!("hf_{}", rnd(34, ALNUM)),
            format!("key-{}", rnd(32, HEX)),
            format!("dop_v1_{}", rnd(64, HEX)),
            format!("shpat_{}", rnd(32, HEX)),
        ];
        for k in keys {
            let line = format!("value {k} end");
            let spans = find_tokens(&chars(&line));
            assert_eq!(texts(&spans, &line), std::slice::from_ref(&k), "{k}");
            assert!(spans[0].confidence >= 0.9, "{k}");
        }
    }

    #[test]
    fn shapes_with_the_wrong_length_or_alphabet_are_not_keys() {
        for k in [
            format!("npm_{}", rnd(20, ALNUM)),
            "npm_config_registry_cache_dir_value".to_string(),
            format!("AKIA{}", rnd(10, UPPER)),
            format!("AKIA{}", rnd(16, "abcdefgh")),
            format!("SK{}", rnd(32, "ghijklmnopqrstuvwxyz")),
            format!("SG.{}.{}", rnd(5, ALNUM), rnd(5, ALNUM)),
            format!("hf_{}", rnd(10, ALNUM)),
            format!("shpat_{}", rnd(10, HEX)),
            "ya29.short".to_string(),
        ] {
            assert!(token_conf(&k).is_none_or(|c| c < 0.9), "{k}");
        }
    }

    #[test]
    fn bearer_credentials_are_found() {
        let cred = rnd(30, ALNUM) + "7";
        let line = format!("Authorization: Bearer {cred}");
        let spans = find_tokens(&chars(&line));
        assert_eq!(texts(&spans, &line), std::slice::from_ref(&cred));
        assert!((spans[0].confidence - 0.85).abs() < 1e-6);
        // A provider shape inside keeps its higher confidence.
        let gh = format!("ghp_{}", rnd(36, ALNUM));
        let spans = find_tokens(&chars(&format!("bearer {gh}")));
        assert_eq!(spans.len(), 1);
        assert!(spans[0].confidence >= 0.95);
        // Not credentials.
        for t in ["Bearer authentication", "the bearer of this note", "Bearer"] {
            assert!(find_tokens(&chars(t)).is_empty(), "{t}");
        }
    }

    #[test]
    fn tokens_ignore_ordinary_text() {
        for t in [
            "The quick brown fox jumps over the lazy dog",
            "/Users/someone/Documents/Projects/report.pdf",
            "550e8400-e29b-41d4-a716-446655440000",
            "commit 3f2a9c1 fixed the build",
            "key-value store",
            "see https://example.com/docs/getting-started",
        ] {
            assert!(find_tokens(&chars(t)).is_empty(), "{t}");
        }
        // A 40 hex git hash is only the generic low-confidence shape.
        assert!(token_conf(&rnd(40, HEX)).is_none_or(|c| c <= 0.6));
    }

    #[test]
    fn generic_mixed_token_stays_at_point_six() {
        assert_eq!(token_conf("Zm9vYmFyMTIzNDU2Nzg5MGFiY2RlZg=="), Some(0.6));
    }

    #[test]
    fn assignments_in_common_syntaxes() {
        let cases: [(&str, &str); 10] = [
            ("DB_PASSWORD=hunter42x", "hunter42x"),
            ("export API_KEY=abcd1234efgh", "abcd1234efgh"),
            ("password: Winter2026", "Winter2026"),
            (r#""client_secret": "s3cr3t-value""#, "s3cr3t-value"),
            (r#"apiKey = "abc 123 def""#, "abc 123 def"),
            ("config.auth.token = 'zz99yy88'", "zz99yy88"),
            ("secretKey: 'qwe123rty'", "qwe123rty"),
            ("MY_APP_PASSWD=letmein", "letmein"),
            (
                "Authorization: Basic dXNlcjpwYXNzd29yZA==",
                "dXNlcjpwYXNzd29yZA==",
            ),
            ("PRIVATE_KEY=\"MIIEvQIBADANBgkq", "MIIEvQIBADANBgkq"),
        ];
        for (line, want) in cases {
            let spans = find_secret_assignments(&chars(line));
            assert_eq!(texts(&spans, line), [want], "{line}");
            assert_eq!(spans[0].kind, RedactionKind::SecretAssignment);
            assert!((spans[0].confidence - 0.7).abs() < 1e-6);
        }
    }

    #[test]
    fn assignments_reject_non_values() {
        for line in [
            "password",
            "Password:",
            "password: ",
            "password = ********",
            "PASSWORD=xxxxxxxx",
            "token: ....",
            "secret = <your-secret>",
            "API_KEY=${API_KEY}",
            "password: {{ vault_password }}",
            "password = $DB_PASS",
            "password=abc",
            "token: string",
            "auth: required",
            "token_type: bearerx9",
            "password_length = 12345",
            "max_tokens: 2048",
            "password == other",
            "password = getPassword()",
            "Authorization: Bearer abcdefghijklmnop12",
            "name: jane",
            "path=/usr/local/bin",
            "password = null",
        ] {
            assert!(
                find_secret_assignments(&chars(line)).is_empty(),
                "{line}: {:?}",
                find_secret_assignments(&chars(line))
            );
        }
    }

    #[test]
    fn assignments_leave_provider_keys_to_the_token_rule() {
        let gh = format!("ghp_{}", rnd(36, ALNUM));
        assert!(find_secret_assignments(&chars(&format!("token = {gh}"))).is_empty());
    }

    #[test]
    fn name_parts_split_on_separators_and_camel_case() {
        let p = |s: &str| name_parts(&chars(s));
        assert_eq!(p("DB_PASSWORD"), ["db", "password"]);
        assert_eq!(p("apiKey"), ["api", "key"]);
        assert_eq!(p("APIKey"), ["api", "key"]);
        assert_eq!(
            p("aws.secret-access.key"),
            ["aws", "secret", "access", "key"]
        );
        assert!(is_secret_name(&chars("accessKey")));
        assert!(!is_secret_name(&chars("keyboard")));
        assert!(!is_secret_name(&chars("tokenizer")));
    }

    #[test]
    fn url_userinfo_password_is_the_span() {
        for (line, want) in [
            (
                "postgres://admin:Tr0ub4dor@db.internal:5432/app",
                "Tr0ub4dor",
            ),
            (
                "clone https://ci:abc123xyz@git.example.com/org/repo.git now",
                "abc123xyz",
            ),
            ("redis://:p4ssw0rd@cache.local", "p4ssw0rd"),
        ] {
            let spans = find_url_secrets(&chars(line));
            assert_eq!(texts(&spans, line), [want], "{line}");
            assert_eq!(spans[0].kind, RedactionKind::UrlSecret);
            assert!((spans[0].confidence - 0.8).abs() < 1e-6);
        }
    }

    #[test]
    fn url_query_secrets_are_the_values() {
        let line = "GET /v1/items?page=2&api_key=k3y9v4lue&token=t0k3n88&sort=asc HTTP/1.1";
        let spans = find_url_secrets(&chars(line));
        assert_eq!(texts(&spans, line), ["k3y9v4lue", "t0k3n88"]);
    }

    #[test]
    fn urls_without_secrets_are_left_alone() {
        for line in [
            "https://example.com/docs/getting-started",
            "ssh://git@github.com/org/repo.git",
            "https://user@example.com/path",
            "https://example.com/?page=2&sort=asc",
            "https://example.com/?token=****",
            "https://u:****@example.com/",
            "https://example.com/?token_type=refresh1",
            "mailto:someone@example.com",
            "git@github.com:org/repo.git",
        ] {
            assert!(
                find_url_secrets(&chars(line)).is_empty(),
                "{line}: {:?}",
                find_url_secrets(&chars(line))
            );
        }
    }

    fn flats(lines: &[&str]) -> Vec<Flat> {
        lines
            .iter()
            .map(|t| super::super::flatten(&TextLine::laid_out(t, 0.0, 0.0, 10.0, 20.0)))
            .collect()
    }

    fn pem_body(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| rnd(64, ALNUM) + &"+/"[i % 2..i % 2 + 1])
            .collect()
    }

    #[test]
    fn private_key_block_covers_every_line() {
        let body = pem_body(4);
        let mut text = vec!["-----BEGIN RSA PRIVATE KEY-----".to_string()];
        text.extend(body);
        text.push("-----END RSA PRIVATE KEY-----".into());
        text.push("after the key".into());
        let refs: Vec<&str> = text.iter().map(String::as_str).collect();
        let f = flats(&refs);
        let found = find_private_keys(&f, &[]);
        assert_eq!(found.len(), 6);
        for (i, l) in found.iter().enumerate() {
            assert_eq!(l.line, i);
            assert_eq!((l.span.start, l.span.end), (0, f[i].chars.len()));
            assert_eq!(l.span.kind, RedactionKind::PrivateKey);
            assert!((l.span.confidence - 0.95).abs() < 1e-6);
        }
    }

    #[test]
    fn private_key_labels_and_mangled_dashes() {
        for begin in [
            "-----BEGIN PRIVATE KEY-----",
            "-----BEGIN EC PRIVATE KEY-----",
            "-----BEGIN DSA PRIVATE KEY-----",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "-----BEGIN ENCRYPTED PRIVATE KEY-----",
            "-----BEGIN PGP PRIVATE KEY BLOCK-----",
            "--- BEGIN RSA PRIVATE KEY ---",
            "— — — BEGIN PRIVATE KEY — — —",
            "-----begin private key-----",
        ] {
            let body = pem_body(1);
            let f = flats(&[begin, &body[0]]);
            assert_eq!(find_private_keys(&f, &[]).len(), 2, "{begin}");
        }
    }

    #[test]
    fn private_key_without_end_stops_at_prose_or_the_cap() {
        let body = pem_body(3);
        let f = flats(&[
            "-----BEGIN PRIVATE KEY-----",
            &body[0],
            &body[1],
            "Next we configure the server with this file",
            &body[2],
        ]);
        assert_eq!(find_private_keys(&f, &[]).len(), 3);
        // The cap.
        let many = pem_body(80);
        let mut text = vec!["-----BEGIN PRIVATE KEY-----"];
        text.extend(many.iter().map(String::as_str));
        let f = flats(&text);
        assert_eq!(find_private_keys(&f, &[]).len(), 1 + MAX_BLOCK_LINES);
    }

    #[test]
    fn encrypted_pem_headers_belong_to_the_block() {
        let body = pem_body(2);
        let f = flats(&[
            "-----BEGIN RSA PRIVATE KEY-----",
            "Proc-Type: 4,ENCRYPTED",
            "DEK-Info: AES-128-CBC,0123456789ABCDEF0123456789ABCDEF",
            &body[0],
            &body[1],
            "-----END RSA PRIVATE KEY-----",
        ]);
        assert_eq!(find_private_keys(&f, &[]).len(), 6);
    }

    #[test]
    fn not_private_keys() {
        for t in [
            "-----BEGIN CERTIFICATE-----",
            "-----BEGIN PUBLIC KEY-----",
            "-----END RSA PRIVATE KEY-----",
            "A private key is a secret",
            "Generate with: openssl genrsa -----BEGIN PRIVATE KEY----- header",
            "BEGIN PRIVATE KEY",
        ] {
            let body = pem_body(1);
            let f = flats(&[t, &body[0]]);
            assert!(find_private_keys(&f, &[]).is_empty(), "{t}");
        }
    }

    #[test]
    fn private_key_spans_fill_around_line_rule_hits() {
        let body = rnd(64, ALNUM);
        let f = flats(&["-----BEGIN PRIVATE KEY-----", &body]);
        let hit = Located {
            line: 1,
            span: Span {
                start: 10,
                end: 30,
                kind: RedactionKind::Token,
                confidence: 0.6,
            },
        };
        let found = find_private_keys(&f, &[hit]);
        let ranges: Vec<(usize, usize, usize)> = found
            .iter()
            .map(|l| (l.line, l.span.start, l.span.end))
            .collect();
        assert_eq!(ranges, [(0, 0, 27), (1, 0, 10), (1, 30, 64)]);
    }

    #[test]
    fn suggest_redactions_finds_secrets_on_a_page() {
        let body = pem_body(3);
        let gh = format!("ghp_{}", rnd(36, ALNUM));
        let mut text: Vec<String> = vec![
            "Deploy notes for the staging server".into(),
            "DB_PASSWORD=Hunter2xyz".into(),
            "curl https://ci:abc123xyz@git.example.com/org/repo.git".into(),
            format!("GITHUB_TOKEN={gh}"),
            "-----BEGIN RSA PRIVATE KEY-----".into(),
        ];
        text.extend(body);
        text.push("-----END RSA PRIVATE KEY-----".into());
        text.push("Thanks for reading".into());
        let lines: Vec<TextLine> = text
            .iter()
            .enumerate()
            .map(|(i, t)| TextLine::laid_out(t, 100.0, 40.0 + 40.0 * i as f64, 10.0, 20.0))
            .collect();
        let opts = SuggestOptions {
            kinds: vec![
                RedactionKind::Token,
                RedactionKind::SecretAssignment,
                RedactionKind::UrlSecret,
                RedactionKind::PrivateKey,
            ],
        };
        let r = suggest_redactions(&lines, (1600, 900), &opts);
        let count = |k| r.iter().filter(|s| s.kind == k).count();
        assert_eq!(count(RedactionKind::SecretAssignment), 1, "{r:?}");
        assert_eq!(count(RedactionKind::UrlSecret), 1, "{r:?}");
        // The GitHub token is a Token; the PEM lines (BEGIN, 3 body, END) are covered by
        // private-key rectangles, possibly merged with the 0.6 generic-token hits on body lines.
        assert!(count(RedactionKind::Token) >= 1, "{r:?}");
        let pem_ys: Vec<f64> = r
            .iter()
            .filter(|s| matches!(s.kind, RedactionKind::PrivateKey | RedactionKind::Token))
            .map(|s| s.rect.y)
            .collect();
        for line in 4..9 {
            let y = 40.0 + 40.0 * line as f64;
            assert!(
                pem_ys.iter().any(|&py| (py - (y - 3.0)).abs() < 0.5),
                "line {line}: {r:?}"
            );
        }
        // Nothing on the prose lines.
        assert!(r.iter().all(|s| s.rect.y > 40.0 && s.rect.y < 40.0 * 10.0));
    }
}
