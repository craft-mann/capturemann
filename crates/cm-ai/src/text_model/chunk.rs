//! Map-reduce over long text (AI-7): when a prompt would use more than 60% of the model's context,
//! callers split the transcript at speaker turns or sentence ends into chunks of about 40% of the
//! context (with 5% overlap), process each chunk and merge the answers. This is the one shared
//! implementation for AI-2, AI-5 and AI-9.

use unicode_segmentation::UnicodeSegmentation;

/// Characters per token the budget assumes. Real tokenizers give 3.5 to 4.5 for English and about
/// 2 to 3 for Vietnamese and CJK text; 3 errs on the safe side for the languages that matter.
pub const CHARS_PER_TOKEN: f64 = 3.0;
/// A prompt above this share of the context is chunked.
pub const CHUNK_TRIGGER: f64 = 0.60;
/// Size of one chunk as a share of the context.
pub const CHUNK_SHARE: f64 = 0.40;
/// Overlap carried from one chunk into the next, as a share of the context.
pub const OVERLAP_SHARE: f64 = 0.05;

/// A rough token count (never zero for non-empty text).
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    ((text.chars().count() as f64) / CHARS_PER_TOKEN).ceil() as usize
}

/// Whether `prompt_tokens` is over 60% of `context_tokens`.
pub fn needs_chunking(prompt_tokens: usize, context_tokens: usize) -> bool {
    prompt_tokens as f64 > CHUNK_TRIGGER * context_tokens as f64
}

/// The parts of a text that are never cut: one per line (a speaker turn or caption line), then
/// one per sentence when a line is longer than `max_chars`, then one per run of words, then
/// character runs for text without any break.
fn units(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            continue;
        }
        if line.chars().count() <= max_chars {
            out.push(line.to_owned());
            continue;
        }
        for sentence in line.split_sentence_bounds() {
            if sentence.chars().count() <= max_chars {
                out.push(sentence.to_owned());
                continue;
            }
            let mut cur = String::new();
            for word in sentence.split_word_bounds() {
                if word.chars().count() > max_chars {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                    let chars: Vec<char> = word.chars().collect();
                    for part in chars.chunks(max_chars) {
                        out.push(part.iter().collect());
                    }
                    continue;
                }
                if cur.chars().count() + word.chars().count() > max_chars && !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                cur.push_str(word);
            }
            if !cur.is_empty() {
                out.push(cur);
            }
        }
    }
    out
}

/// Splits `text` into chunks for a model with `context_tokens`. Text that fits under 60% of the
/// context stays one chunk. Otherwise each chunk is about 40% of the context, cut at line ends
/// (speaker turns) and sentence ends, and starts with the last ~5% of the context of the chunk
/// before it, so a sentence at a boundary is seen whole at least once.
pub fn split_chunks(text: &str, context_tokens: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    if !needs_chunking(estimate_tokens(text), context_tokens) {
        return vec![text.to_owned()];
    }
    let max_chars = ((context_tokens as f64 * CHUNK_SHARE * CHARS_PER_TOKEN) as usize).max(2);
    let overlap_chars = (context_tokens as f64 * OVERLAP_SHARE * CHARS_PER_TOKEN) as usize;
    // A unit is at most the chunk size minus the overlap, so overlap plus a new unit still fits.
    let parts = units(text, max_chars.saturating_sub(overlap_chars).max(1));
    let len = |s: &str| s.chars().count();

    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Vec<&String> = Vec::new();
    let mut cur_len = 0usize;
    for p in &parts {
        if cur_len + len(p) > max_chars && !cur.is_empty() {
            chunks.push(cur.iter().map(|s| s.as_str()).collect());
            // Keep whole trailing units that fit in the overlap.
            let mut keep: Vec<&String> = Vec::new();
            let mut kept = 0usize;
            for u in cur.iter().rev() {
                if kept + len(u) > overlap_chars {
                    break;
                }
                kept += len(u);
                keep.push(u);
            }
            keep.reverse();
            cur = keep;
            cur_len = kept;
        }
        cur.push(p);
        cur_len += len(p);
    }
    if !cur.is_empty() {
        chunks.push(cur.iter().map(|s| s.as_str()).collect());
    }
    chunks
}

/// Map-reduce: runs `map` on every chunk of `text` and hands the answers, in order, to `reduce`.
/// A text that fits is one `map` call and `reduce` still receives its single answer.
pub fn map_reduce<T, R, E>(
    text: &str,
    context_tokens: usize,
    mut map: impl FnMut(usize, &str) -> Result<T, E>,
    reduce: impl FnOnce(Vec<T>) -> Result<R, E>,
) -> Result<R, E> {
    let chunks = split_chunks(text, context_tokens);
    let mut parts = Vec::with_capacity(chunks.len());
    for (i, c) in chunks.iter().enumerate() {
        parts.push(map(i, c)?);
    }
    reduce(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// About two hours of speech: 150 words a minute, speaker turns of four sentences.
    fn two_hour_transcript() -> String {
        let mut s = String::new();
        let mut words = 0;
        let mut n = 0;
        while words < 120 * 150 {
            let who = if n % 2 == 0 { "Anna" } else { "Ben" };
            s.push_str(&format!("{who}: "));
            for k in 0..4 {
                s.push_str(&format!("This is sentence {n} part {k} here. "));
                words += 7;
            }
            s.push('\n');
            n += 1;
        }
        s
    }

    #[test]
    fn short_text_is_one_chunk() {
        let c = split_chunks("Hello there.", 8192);
        assert_eq!(c, vec!["Hello there.".to_owned()]);
        assert!(split_chunks("  \n", 8192).is_empty());
    }

    #[test]
    fn ai7_ac5_two_hour_transcript_never_exceeds_the_context() {
        let text = two_hour_transcript();
        let ctx = 8192;
        assert!(needs_chunking(estimate_tokens(&text), ctx));
        let chunks = split_chunks(&text, ctx);
        assert!(chunks.len() > 5);
        for c in &chunks {
            // Chunk plus the instructions and the reply budget stays inside the context.
            assert!(
                estimate_tokens(c) as f64 <= 0.40 * ctx as f64 + 1.0,
                "chunk of {} tokens",
                estimate_tokens(c)
            );
            // Cut at a turn end, never in the middle of a sentence.
            assert!(c.trim_end().ends_with('.'), "{c:?}");
        }
        // Every line is in some chunk.
        for line in text.lines().step_by(37) {
            assert!(chunks.iter().any(|c| c.contains(line)), "lost {line}");
        }
        // Consecutive chunks overlap by their last line.
        assert!(chunks.windows(2).all(|w| {
            let tail = w[0].lines().last().unwrap_or("");
            w[1].contains(tail)
        }));
    }

    #[test]
    fn a_long_line_without_breaks_is_cut_by_characters() {
        let text = "x".repeat(30_000);
        let chunks = split_chunks(&text, 2048);
        assert!(chunks.len() > 1);
        assert!(chunks
            .iter()
            .all(|c| estimate_tokens(c) as f64 <= 2048.0 * 0.40 + 1.0));
        assert!(chunks.iter().map(|c| c.len()).sum::<usize>() >= 30_000);
    }

    #[test]
    fn map_reduce_merges_in_order() {
        let text = two_hour_transcript();
        let r: Result<String, ()> = map_reduce(
            &text,
            8192,
            |i, c| Ok(format!("{i}:{}", !c.is_empty())),
            |parts| Ok(parts.join(",")),
        );
        assert!(r.unwrap().starts_with("0:true,1:true"));
    }
}
