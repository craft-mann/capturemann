//! Command search and ranking (LNC-1 "Search"): case-insensitive subsequence match on the
//! title with word-start matches first, then keywords.

/// Lower is better. Tiers: 0 title starts with the query, 1 a title word starts with it,
/// 2 title subsequence, 3 a keyword word starts with it, 4 keyword substring.
pub type Rank = u8;

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

fn word_starts<'a>(s: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    s.char_indices()
        .filter(|(i, c)| {
            c.is_alphanumeric()
                && (*i == 0
                    || s[..*i]
                        .chars()
                        .next_back()
                        .is_some_and(|p| !p.is_alphanumeric()))
        })
        .map(|(i, _)| &s[i..])
}

/// Rank of one query token against a command, or `None` when it does not match.
fn token_rank(token: &str, title: &str, keywords: &[&str]) -> Option<Rank> {
    let title = title.to_lowercase();
    if title.starts_with(token) {
        return Some(0);
    }
    if word_starts(&title).any(|w| w.starts_with(token)) {
        return Some(1);
    }
    if is_subsequence(token, &title) {
        return Some(2);
    }
    let mut best: Option<Rank> = None;
    for k in keywords {
        let k = k.to_lowercase();
        let r = if word_starts(&k).any(|w| w.starts_with(token)) {
            3
        } else if k.contains(token) {
            4
        } else {
            continue;
        };
        best = Some(best.map_or(r, |b| b.min(r)));
    }
    best
}

/// Rank of a whole query: every whitespace-separated token must match; the rank is the worst
/// token's. An empty query matches everything at rank 0.
pub fn rank(query: &str, title: &str, keywords: &[&str]) -> Option<Rank> {
    let q = query.to_lowercase();
    let mut worst: Rank = 0;
    for token in q.split_whitespace() {
        worst = worst.max(token_rank(token, title, keywords)?);
    }
    Some(worst)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LNC-1 search: "rec" finds the record commands first by word start.
    #[test]
    fn rec_matches_title_prefix() {
        assert_eq!(rank("rec", "Record screen + webcam", &[]), Some(0));
        assert_eq!(rank("REC", "Record region", &[]), Some(0));
        assert_eq!(rank("rec", "Screenshot region", &[]), None);
    }

    #[test]
    fn word_start_beats_subsequence_beats_keyword() {
        assert_eq!(rank("reg", "Screenshot region", &[]), Some(1));
        assert_eq!(rank("sw", "Screenshot window", &[]), Some(2));
        assert_eq!(rank("shot", "Open library", &["shot"]), Some(3));
        assert_eq!(rank("ibr", "Open lib", &["xibrx"]), Some(4));
        assert_eq!(rank("zzz", "Open library", &["mcp"]), None);
    }

    #[test]
    fn keywords_are_searched_after_the_title() {
        assert_eq!(
            rank("claude", "Copy MCP server config", &["claude"]),
            Some(3)
        );
        assert_eq!(rank("mcp", "Copy MCP server config", &[]), Some(1));
    }

    #[test]
    fn every_token_must_match() {
        assert_eq!(rank("shot win", "Screenshot window", &[]), Some(2));
        assert_eq!(rank("shot xyz", "Screenshot window", &[]), None);
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(rank("", "Anything", &[]), Some(0));
        assert_eq!(rank("   ", "Anything", &[]), Some(0));
    }
}
