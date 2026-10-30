//! Network identifiers: IPv4 and IPv6 addresses and MAC addresses.

use super::Span;
use crate::ocr::detect::RedactionKind;

fn is_hex(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

/// IPv4 and IPv6 addresses. Loopback and unspecified addresses are not proposed.
pub fn find_ips(c: &[char]) -> Vec<Span> {
    let mut v6 = Vec::new();
    find_ipv6(c, &mut v6);
    // An IPv4 tail inside an IPv6 address is part of that address.
    let mut out: Vec<Span> = find_ipv4(c)
        .into_iter()
        .filter(|a| !v6.iter().any(|b| a.start < b.end && b.start < a.end))
        .collect();
    out.extend(v6);
    out.sort_by_key(|s| s.start);
    out
}

fn find_ipv4(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if !c[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < c.len() && (c[j].is_ascii_digit() || c[j] == '.') {
            j += 1;
        }
        let run: String = c[i..j].iter().collect();
        let run = run.trim_end_matches('.');
        let parts: Vec<&str> = run.split('.').collect();
        let ok = parts.len() == 4
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u16>().is_ok_and(|v| v <= 255));
        // Loopback (127.x) and the unspecified address say nothing about the user.
        let unwanted = ok && (parts[0] == "127" || parts.iter().all(|p| p.parse::<u16>() == Ok(0)));
        let before_ok = i == 0 || !c[i - 1].is_ascii_alphanumeric();
        if ok && before_ok && !unwanted {
            out.push(Span {
                start: i,
                end: i + run.chars().count(),
                kind: RedactionKind::IpAddress,
                confidence: 0.5,
            });
        }
        i = j.max(i + 1);
    }
    out
}

/// An IPv6 address worth proposing: it parses (RFC 4291 text forms, including `::` and an IPv4
/// tail), has at least three groups (an IPv4 tail counts as two), and is neither unspecified
/// nor loopback (also not as an IPv4-mapped address).
fn proposable_ipv6(text: &str) -> bool {
    let Ok(addr) = text.parse::<std::net::Ipv6Addr>() else {
        return false;
    };
    let groups: usize = text
        .split(':')
        .filter(|g| !g.is_empty())
        .map(|g| if g.contains('.') { 2 } else { 1 })
        .sum();
    if groups < 3 || addr.is_unspecified() || addr.is_loopback() {
        return false;
    }
    if let Some(v4) = addr.to_ipv4_mapped() {
        if v4.is_unspecified() || v4.is_loopback() {
            return false;
        }
    }
    true
}

/// IPv6 addresses; an optional zone (`%eth0`) is included, brackets and ports are not.
fn find_ipv6(c: &[char], out: &mut Vec<Span>) {
    let mut i = 0;
    while i < c.len() {
        if !(is_hex(c[i]) || c[i] == ':') {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < c.len() && (is_hex(c[j]) || c[j] == ':' || c[j] == '.') {
            j += 1;
        }
        let mut start = i;
        // A single leading colon is a label separator ("ip:2001:db8::1").
        let single_colon = c[start] == ':' && c.get(start + 1) != Some(&':');
        if single_colon {
            start += 1;
        }
        let mut end = j;
        while end > start + 1 && (c[end - 1] == '.' || (c[end - 1] == ':' && c[end - 2] != ':')) {
            end -= 1;
        }
        let before_ok = single_colon
            || start == 0
            || !(c[start - 1].is_ascii_alphanumeric() || matches!(c[start - 1], '.' | ':'));
        let after_ok = c.get(end).is_none_or(|ch| !ch.is_ascii_alphanumeric());
        if end > start && before_ok && after_ok {
            let text: String = c[start..end].iter().collect();
            if text.matches(':').count() >= 2 && proposable_ipv6(&text) {
                let mut span_end = end;
                if c.get(end) == Some(&'%') {
                    let mut z = end + 1;
                    while z < c.len() && (c[z].is_ascii_alphanumeric() || c[z] == '_') {
                        z += 1;
                    }
                    if z > end + 1 {
                        span_end = z;
                    }
                }
                out.push(Span {
                    start,
                    end: span_end,
                    kind: RedactionKind::IpAddress,
                    confidence: 0.5,
                });
            }
        }
        i = j.max(i + 1);
    }
}

/// MAC addresses: six hex pairs with one separator (`aa:bb:cc:dd:ee:ff`, `aa-bb-...`) or the
/// dotted form `aabb.ccdd.eeff`. A longer hex token (an IPv6 address, a seventh pair) is not a
/// MAC address.
pub fn find_macs(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let hex_at = |k: usize| c.get(k).is_some_and(|x| is_hex(*x));
    let pair = |k: usize| hex_at(k) && hex_at(k + 1);
    let mut i = 0;
    while i < c.len() {
        if !is_hex(c[i]) {
            i += 1;
            continue;
        }
        let prev_ok = match i.checked_sub(1).map(|k| c[k]) {
            None => true,
            Some(p) if p.is_ascii_alphanumeric() => false,
            Some(':' | '-' | '.') => {
                // Reject a seventh pair or an IPv6 context; accept a label such as "mac:"
                // (the hex run before the separator is the tail of a longer word).
                let mut k = i - 1;
                while k > 0 && is_hex(c[k - 1]) {
                    k -= 1;
                }
                let label = k < i - 1 && k > 0 && c[k - 1].is_ascii_alphanumeric();
                let prev_hex = i >= 2 && is_hex(c[i - 2]);
                !(prev_hex && !label) && !(i >= 2 && c[i - 2] == ':')
            }
            Some(_) => true,
        };
        let mut len = 0;
        if prev_ok && pair(i) {
            match c.get(i + 2).copied() {
                Some(s @ (':' | '-')) => {
                    if (0..6).all(|n| pair(i + 3 * n)) && (1..6).all(|n| c[i + 3 * n - 1] == s) {
                        len = 17;
                    }
                }
                _ => {
                    if (0..3).all(|n| (0..4).all(|m| hex_at(i + 5 * n + m)))
                        && c.get(i + 4) == Some(&'.')
                        && c.get(i + 9) == Some(&'.')
                    {
                        len = 14;
                    }
                }
            }
        }
        if len > 0 {
            let more = c.get(i + len).is_some_and(|n| {
                n.is_ascii_alphanumeric() || (matches!(n, ':' | '-' | '.') && hex_at(i + len + 1))
            });
            // All pairs the same (ff:ff:ff:ff:ff:ff broadcast, 00:00:... null) names no device.
            let mut digits = c[i..i + len].iter().filter(|x| x.is_ascii_hexdigit());
            let first = digits.next().map(char::to_ascii_lowercase);
            let uniform = digits.all(|x| Some(x.to_ascii_lowercase()) == first);
            if !more && !uniform {
                out.push(Span {
                    start: i,
                    end: i + len,
                    kind: RedactionKind::MacAddress,
                    confidence: 0.6,
                });
                i += len;
                continue;
            }
        }
        // Skip the whole hex token so a later pair inside it is not taken for a start.
        while i < c.len() && is_hex(c[i]) {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(f: fn(&[char]) -> Vec<Span>, s: &str) -> Vec<String> {
        let c: Vec<char> = s.chars().collect();
        f(&c)
            .into_iter()
            .map(|p| c[p.start..p.end].iter().collect())
            .collect()
    }

    #[test]
    fn ipv4_documentation_ranges_are_found_loopback_is_not() {
        assert_eq!(
            found(find_ips, "a 192.0.2.1, 198.51.100.7 203.0.113.250"),
            ["192.0.2.1", "198.51.100.7", "203.0.113.250"]
        );
        assert!(found(find_ips, "127.0.0.1 127.3.4.5 0.0.0.0 300.1.1.1 1.2.3").is_empty());
    }

    #[test]
    fn ipv6_forms_are_found() {
        for t in [
            "2001:db8:0:0:0:0:2:1",
            "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
            "2001:db8::1",
            "2001:db8:1::ab:2",
            "::ffff:192.0.2.1",
            "fe80::1:2%eth0",
            "2001:DB8::FF00:42:8329",
        ] {
            assert_eq!(found(find_ips, &format!("host {t} up")), [t], "{t}");
        }
    }

    #[test]
    fn ipv6_brackets_ports_and_labels_are_not_in_the_span() {
        assert_eq!(found(find_ips, "[2001:db8::1]:8080"), ["2001:db8::1"]);
        assert_eq!(found(find_ips, "(2001:db8::1)."), ["2001:db8::1"]);
        assert_eq!(found(find_ips, "ip:2001:db8::1, next"), ["2001:db8::1"]);
    }

    #[test]
    fn things_that_only_look_like_ipv6_are_left_alone() {
        for t in [
            "12:30:45",
            "at 12:30:45:10",
            "aa:bb:cc:dd:ee:ff",
            "a::b",
            "2001:db8::",
            "::1",
            "::",
            "::ffff:127.0.0.1",
            "1:2:3:4:5:6:7:8:9",
            "2001:db8::1::2",
            "std::vector::iterator",
            "xx2001:db8::1",
        ] {
            assert!(
                found(find_ips, t).is_empty(),
                "{t}: {:?}",
                found(find_ips, t)
            );
        }
    }

    #[test]
    fn mac_forms_are_found() {
        assert_eq!(
            found(find_macs, "mac aa:bb:cc:dd:ee:ff."),
            ["aa:bb:cc:dd:ee:ff"]
        );
        assert_eq!(found(find_macs, "02-1A-2b-3C-4d-5E"), ["02-1A-2b-3C-4d-5E"]);
        assert_eq!(found(find_macs, "[0a1b.2c3d.4e5f]"), ["0a1b.2c3d.4e5f"]);
        assert_eq!(
            found(find_macs, "mac:aa:bb:cc:dd:ee:ff"),
            ["aa:bb:cc:dd:ee:ff"]
        );
    }

    #[test]
    fn longer_tokens_mixed_separators_and_ipv6_are_not_macs() {
        for t in [
            "aa:bb:cc:dd:ee",
            "aa:bb:cc:dd:ee:ff:00",
            "aa:bb-cc:dd-ee:ff",
            "aa:bb:cc:dd:ee:fff",
            "xaa:bb:cc:dd:ee:ff",
            "2001:db8:aa:bb:cc:dd:ee:ff",
            "fe80::aa:bb:cc:dd:ee:ff",
            "aabb.ccdd.eeff.0011",
            "gg:bb:cc:dd:ee:ff",
        ] {
            assert!(
                found(find_macs, t).is_empty(),
                "{t}: {:?}",
                found(find_macs, t)
            );
        }
    }

    #[test]
    fn network_and_phone_rules_through_suggest_redactions() {
        use crate::ocr::{suggest_redactions, SuggestOptions, TextLine};
        let lines = [
            TextLine::laid_out(
                "gw [2001:db8::1]:8080 nic aa:bb:cc:dd:ee:ff",
                100.0,
                100.0,
                10.0,
                20.0,
            ),
            TextLine::laid_out("call (415) 555-0132 ext. 12", 100.0, 160.0, 10.0, 20.0),
        ];
        let r = suggest_redactions(&lines, (1200, 800), &SuggestOptions::default());
        let mut kinds: Vec<_> = r.iter().map(|s| s.kind).collect();
        kinds.sort();
        assert_eq!(
            kinds,
            [
                RedactionKind::IpAddress,
                RedactionKind::MacAddress,
                RedactionKind::Phone
            ],
            "{r:?}"
        );
    }
}
