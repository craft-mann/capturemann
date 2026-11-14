# QA 0.3.4 BRD-1 Boards on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine / Build / Data:** as in `QA-0.3.4-ann-9-retype-windows-2026-11-15-claude.md`; sources: a CMYK JPEG (400x200), the retyped window render (1332x951), the text-page capture (1332x951)
- **Method:** `capturemann board <ids> --layout ...`; sizes from the JSON result

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| Pair with `--before-after`, captions Before/After, title (AC1) | PASS | new capture `QA pair board`, 768x365; both sources scaled to one height (each source listed with its own size in the result) |
| Strip, stacked, gap 12 | PASS | 464x860 (the width follows the widest scaled frame) |
| Grid 2 columns, gap 20, margin 30, radius 12, shadow, background `#EEEEEE` | PASS | 880x480 |
| Free with two `--slot x,y,w,h` | PASS | 852x282 (slots quoted: PowerShell splits an unquoted `0,0,400,250`) |
| The board is an ordinary screenshot capture (storage rule) | PASS | `library list` shows kind `screenshot`, titles as named |
| Deleting a source after the board exists leaves it unchanged (AC3) | PASS | a pair board of two imported pictures; `capture get` of the board before and after deleting a source with `library rm` (confirmed on screen): SHA-256 identical (`244C50F9...3B0894`), the source no longer listed |
| Source with a blur shows the blur (AC2) | not run | |
| MCP `create_board` (AC4) | not run through an MCP client; `board` CLI uses the same action | |
| The Make a board sheet, the Free drag canvas | not run | UI not driven |

## Verdict

PARTIAL
