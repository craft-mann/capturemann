# Bundled fonts

All fonts are embedded with `include_bytes!` and registered in `Renderer::new()` (see
`src/text.rs`), so annotation text never depends on system fonts. Every font is SIL Open Font
License 1.1; the license text sits next to each font. Fonts are data, not linked code, so
`deny.toml` (which only audits crate licenses) is unaffected; OFL permits bundling and
redistribution as long as the license ships with the font and the font is not sold on its own.

| File | Font | License file | Size |
|------|------|--------------|------|
| `Inter-Regular.ttf` | Inter Regular (primary UI font) | `Inter-LICENSE.txt` | 310 KB |
| `NotoSansSC-Subset.ttf` | Noto Sans SC Regular 2.004, subset (CJK + kana fallback) | `NotoSansSC-LICENSE.txt` | 3.4 MB |
| `NotoEmoji-Regular.ttf` | Noto Emoji 3.006 Regular, monochrome (emoji fallback) | `NotoEmoji-LICENSE.txt` | 887 KB |

## Sources

Direct GitHub downloads were unavailable, so the unmodified upstream TTFs were taken from the
`@expo-google-fonts/noto-sans-sc@0.4.4` and `@expo-google-fonts/noto-emoji@0.4.8` npm packages
(static `400Regular` instances of the Google Fonts releases; the packages' `LICENSE_FONT` files
are the OFL texts copied here). Upstream: https://fonts.google.com/noto and
https://github.com/googlefonts/noto-emoji. The OFL Reserved Font Name of Noto Sans SC is
"Source"; the subset keeps the name "Noto Sans SC" and does not use it.

## Noto Sans SC subset

The full font is 10.5 MB, so it is subset to keep the repo small:

```sh
python3 subset-chars.py > uni.txt   # codepoint list
pyftsubset NotoSansSC_400Regular.ttf --unicodes-file=uni.txt \
  --layout-features='' --no-hinting --desubroutinize --output-file=NotoSansSC-Subset.ttf
```

`subset-chars.py` emits: every character in GB2312 (about 6,700 simplified-Chinese hanzi plus
symbols) and JIS X 0208 (about 6,350 kanji plus kana), CJK punctuation, hiragana, katakana,
small kana extensions, fullwidth forms (U+FF00-FFEF), general punctuation, arrows, geometric
shapes and misc symbols, and Latin-1. Coverage tradeoff: traditional-Chinese-only hanzi outside
JIS (e.g. many Big5 characters), Hangul, rare/extension-B hanzi and Bopomofo render as the
missing-glyph box. Kanji use Simplified-Chinese glyph forms (Noto SC), which differ slightly
from Japanese forms for some characters. Layout features were dropped (no vertical or
locale-specific forms), which is fine for horizontal annotation text.

## Emoji

Monochrome Noto Emoji is used, not Noto Color Emoji: emoji render as single-color glyphs in the
annotation's text color. Color emoji (COLR/CBDT/SVG) are not used for determinism and size; users
of `Renderer::with_system_fonts()` may get system color emoji instead, on machines that have them.

## Caption weight

Burned-in video captions (AI-1) are specified as Inter 600 (SemiBold). Only `Inter-Regular.ttf` is
bundled (direct GitHub downloads were not available when the fonts were added), so
`compose/caption.rs` thickens the Regular glyphs by 1.8% of the font size per side (at least one
pixel from 28 px up) to read as weight 600. To use the real SemiBold: add `Inter-SemiBold.ttf` here
(unmodified upstream, OFL, same license file), register it in `text.rs` (`Fonts::build`) and ask for
`Weight::SEMIBOLD` in the caption layout; then drop the thickening (`EMBOLDEN_EM`) and regenerate
the `compose_caption_*` goldens.
