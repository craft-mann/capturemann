//! Removing secrets from text that is stored or published (R2-12).
//!
//! Job error reasons and progress messages are copied to `jobs.sqlite`, to `get_job` /
//! `list_jobs` results and to the `job.failed` event, all readable by agents. Network handlers
//! (upload, publish) must never put a URL with a token, a signed query or credentials into an
//! error reason; [`scrub_secrets`] is the safety net the store applies to every reason and
//! message it writes, and handlers should call it too on anything that came from a server.
//!
//! What it does:
//!
//! * `http(s)://user:pass@host/path?query#frag` becomes `https://host/path?[redacted]`
//!   (credentials, query string and fragment removed; a URL without them is unchanged);
//! * `Bearer <token>` and `Basic <credentials>` become `Bearer [redacted]`;
//! * `key=value` / `key: value` pairs whose key looks secret (`token`, `secret`, `password`,
//!   `passwd`, `api_key`, `apikey`, `sig`, `signature`, `credential`, `authorization`) keep the key and lose the value.

const REDACTED: &str = "[redacted]";

const SECRET_KEYS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "credential",
    "authorization",
    "signature",
    "private_key",
    "client_secret",
    "access_key",
    "sig",
];

fn is_url_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | ']' | '}' | ',' | '`')
}

fn scrub_url(url: &str) -> String {
    // scheme://[userinfo@]host[:port]/path[?query][#frag]
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let (auth_and_path, tail) = match rest.find(['?', '#']) {
        Some(i) => (&rest[..i], Some(&rest[i..])),
        None => (rest, None),
    };
    let host_start = if let Some(slash) = auth_and_path.find('/') {
        match auth_and_path[..slash].rfind('@') {
            Some(at) => at + 1,
            None => 0,
        }
    } else {
        match auth_and_path.rfind('@') {
            Some(at) => at + 1,
            None => 0,
        }
    };
    let mut out = format!("{scheme}://{}", &auth_and_path[host_start..]);
    if tail.is_some_and(|t| t.len() > 1) {
        out.push_str("?[redacted]");
    }
    out
}

fn is_secret_key(k: &str) -> bool {
    let k = k.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-');
    let lower = k.to_ascii_lowercase().replace('-', "_");
    SECRET_KEYS.iter().any(|s| {
        lower == *s || lower.ends_with(&format!("_{s}")) || lower.starts_with(&format!("{s}_"))
    })
}

/// Returns `s` with URLs reduced to scheme, host and path, bearer tokens and secret-looking
/// `key=value` values replaced by `[redacted]`.
pub fn scrub_secrets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();
    let mut last_word = String::new();
    while let Some((i, c)) = chars.next() {
        let rest = &s[i..];
        let starts = |w: &str| {
            rest.get(..w.len())
                .is_some_and(|p| p.eq_ignore_ascii_case(w))
        };
        let is_url = starts("http://") || starts("https://");
        if is_url {
            let end = rest.find(is_url_end).unwrap_or(rest.len());
            out.push_str(&scrub_url(&rest[..end]));
            while chars.peek().is_some_and(|(j, _)| *j < i + end) {
                chars.next();
            }
            last_word.clear();
            continue;
        }
        // Bearer/Basic tokens.
        if (c == 'B' || c == 'b') && last_word.is_empty() {
            let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let word = &rest[..word_end];
            if word.eq_ignore_ascii_case("bearer") || word.eq_ignore_ascii_case("basic") {
                let after = &rest[word_end..];
                let ws = after.len() - after.trim_start().len();
                let tok_start = word_end + ws;
                if ws > 0 && tok_start < rest.len() {
                    let tok_end = rest[tok_start..]
                        .find(is_url_end)
                        .map_or(rest.len(), |e| tok_start + e);
                    out.push_str(word);
                    out.push(' ');
                    out.push_str(REDACTED);
                    while chars.peek().is_some_and(|(j, _)| *j < i + tok_end) {
                        chars.next();
                    }
                    continue;
                }
            }
        }
        // key=value / key: value / "key":"value"
        if c == '=' || c == ':' {
            // `://` was handled above; a lone `:` needs a secret-looking key before it.
            let is_scheme_value = {
                let v = rest[1..].trim_start();
                let starts = |w: &str| v.get(..w.len()).is_some_and(|p| p.eq_ignore_ascii_case(w));
                starts("bearer") || starts("basic")
            };
            if is_secret_key(&last_word) && !rest[1..].starts_with("//") && !is_scheme_value {
                let after = &rest[1..];
                let ws = after.len() - after.trim_start().len();
                let mut v_start = 1 + ws;
                let quoted = rest[v_start..].starts_with(['"', '\'']);
                if quoted {
                    v_start += 1;
                }
                let tail = &rest[v_start..];
                let v_len = if quoted {
                    tail.find(['"', '\'']).unwrap_or(tail.len())
                } else {
                    tail.find(|ch: char| is_url_end(ch) || ch == '&' || ch == ';')
                        .unwrap_or(tail.len())
                };
                if v_len > 0 {
                    out.push(c);
                    out.push_str(&rest[1..v_start]);
                    out.push_str(REDACTED);
                    let skip_to = i + v_start + v_len;
                    while chars.peek().is_some_and(|(j, _)| *j < skip_to) {
                        chars.next();
                    }
                    last_word.clear();
                    continue;
                }
            }
        }
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            last_word.push(c);
        } else if c == '"' || c == '\'' {
            // quotes around a JSON key do not end the word
        } else {
            last_word.clear();
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r2_12_urls_lose_query_fragment_and_credentials() {
        assert_eq!(
            scrub_secrets("upload to https://u:p@up.example.com/v1/x?token=abc&sig=zz#f failed"),
            "upload to https://up.example.com/v1/x?[redacted] failed"
        );
        assert_eq!(
            scrub_secrets("see (https://example.com/a/b)."),
            "see (https://example.com/a/b)."
        );
        assert_eq!(
            scrub_secrets("HTTP://h.example/p?X-Goog-Signature=1234"),
            "HTTP://h.example/p?[redacted]"
        );
    }

    #[test]
    fn r2_12_bearer_and_secret_pairs() {
        assert_eq!(
            scrub_secrets("401 with Authorization: Bearer abc.def.ghi, retry"),
            "401 with Authorization: Bearer [redacted], retry"
        );
        let s =
            scrub_secrets("bad request access_token=ya29.AAA&state=ok client_secret: hunter2 done");
        assert!(!s.contains("ya29"), "{s}");
        assert!(!s.contains("hunter2"), "{s}");
        assert!(s.contains("state=ok") || s.contains("state"), "{s}");
        let j = scrub_secrets(r#"{"refresh_token":"1//0abc","message":"nope"}"#);
        assert!(!j.contains("1//0abc"), "{j}");
        assert!(j.contains("nope"), "{j}");
    }

    #[test]
    fn plain_text_is_unchanged() {
        for t in [
            "Network error: connection reset",
            "gave up after 20 attempts: timeout",
            "dependency job_1 failed",
            "rate: 5 per second, key frames: 3, exit code: 1",
        ] {
            assert_eq!(scrub_secrets(t), t);
        }
    }
}
