---
name: screenshot-and-redact
description: Take a screenshot of a display, window or region with CaptureMann, find and blur emails, keys, cards and other sensitive text, annotate it. Use when the user asks for a screenshot or to redact one.
---

# Screenshot and redact

Read `capturemann-basics` first if you have not. All tools here are Free, except that the user may have set `capture_screenshot` or `annotate` to Ask first.

## Take the screenshot

1. `list_targets` lists displays and windows (id, title, app, bounds). Pick the window or display the user means; ask when several match.
2. `capture_screenshot` with `target: { display_id }`, `{ window_id }` or `{ display_id, rect: [x, y, w, h] }` (physical px). Optional `delay_s` (0 to 30) and `copy_to_clipboard`. On a Wayland desktop where `list_targets` reports `capabilities.pick`, use `target: { pick: true }`: the user chooses on screen.
3. The result has the `capture_id`, the file path and the image. The original is kept read-only; every edit is a revision.

## Redact before sharing

A screenshot can show passwords, tokens, emails, card numbers or private messages. Before a screenshot leaves the computer (an issue, a chat, an upload), redact it:

1. `suggest_redactions` with the `capture_id` (optional `kinds`, from the 21 names in its schema, for example `email`, `token`, `card`, `phone`). It uses on-device text recognition and returns `rects` with `kind`, `confidence` and a masked `text_preview`. **It applies nothing and it is a proposal, not a guarantee**: it can miss text (handwriting, unusual fonts, text inside images) and it can flag harmless text.
2. Show the user the list (kind, position, preview) and ask what to blur. Add anything they point at that the detector missed.
3. Apply the accepted ones in one `annotate` call with `blur` ops (`{ "op": "blur", "rect": [x, y, w, h] }`; `{ "op": "pixelate", "rect": [...] }` is the alternative). One call is one undo step. Only `blur` and `pixelate` hide text: a `spotlight` with `mode: "blur"`, a `rect` or a stamp laid over it is a look, not a redaction.
4. Say plainly: **a blurred screenshot is not proof that nothing sensitive remains.** The user must look at the result before sharing it. Do not claim a screenshot is "clean".

On a computer without a text recognizer `suggest_redactions` fails with `not_implemented`: then ask the user where the sensitive text is and blur it with `annotate`.

## Copy the text out of a screenshot

`recognize_text` with the `capture_id` returns the text in the picture in reading order, using on-device recognition (nothing is sent anywhere, nothing is stored, no revision). Optional `region: [x, y, w, h]` (original pixels), `format: "lines"` for each line's box and `copy_to_clipboard: true` to put the text on the user's clipboard. It reads the **rendered** image, so text the user blurred is not returned. What it reads depends on the system: macOS reads what Apple Vision supports, Windows only the OCR languages installed in Windows, Linux `tesseract` (English unless more language data is installed); it can misread, so do not treat the result as exact. Do this only when the user asks for the text, and do not paste text that may contain secrets elsewhere. On a computer without a text recognizer it fails with `not_implemented`.

## Annotate

`annotate` ops: `arrow`, `rect`, `ellipse`, `line`, `text`, `highlight`, `step` (numbered markers), `blur`, `pixelate`, `crop` and, from CaptureMann 0.3.3, `callout`, `spotlight` and `stamp`. Each op is an object with an `"op"` field; coordinates are pixels of the original image. On a build older than 0.3.3 the new ops fail with `invalid_input`: then use the older ones.
- `callout`: `rect` (the smallest box), `tail` (the tip of the pointer), `text`, optional `size`, `color` (text), `bg` (fill), `radius`, `style` (`bubble` or `box`). The box grows to fit the text.
- `spotlight`: `shapes` (1 to 8 of `{ "kind": "rect" | "ellipse", "rect": [x, y, w, h], "radius" }`), optional `dim` (0 to 1, default 0.6) and `mode` (`dim` or `blur`, with `strength`). **Only one `spotlight` per image**: two in one call give `invalid_input` naming the second (`ops[1]`). Put it before the arrows and callouts in the list so they stay bright. `mode: "blur"` is only a look; it is not a redaction, so use `blur` or `pixelate` to hide anything.
- `step` also takes `style` (`number`, `upper` or `lower`), `start` (the first label, at least 1) and `text` (a short label, drawn beside the badge). Steps number in the order they are created.
- `stamp`: `at`, `kind` (`check`, `cross`, `exclaim`, `question` or `label`; `label` needs `text`), `size`, `color`.

 Pass `copy_to_clipboard: true` to copy the rendered image (annotations and blurs applied, never the original). `replace: true` replaces the op list in one undo step. `get_capture` returns the current revision and image.

## Do not

- Do not take screenshots the user did not ask for, and do not take one "to see the screen" without saying so.
- Do not read the screenshot's text back to the user or into a file when it may contain secrets; describe what kind of text it is. `recognize_text` is for when the user asks for the text.
- Do not use `set_settings` to change capture behaviour the user did not request.
