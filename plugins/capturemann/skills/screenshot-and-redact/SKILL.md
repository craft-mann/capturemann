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
3. Apply the accepted ones in one `annotate` call with `blur` ops (`{ "kind": "blur", "rect": [x, y, w, h] }`; `pixelate` is the alternative). One call is one undo step.
4. Say plainly: **a blurred screenshot is not proof that nothing sensitive remains.** The user must look at the result before sharing it. Do not claim a screenshot is "clean".

On a computer without a text recognizer `suggest_redactions` fails with `not_implemented`: then ask the user where the sensitive text is and blur it with `annotate`.

## Annotate

`annotate` ops: `arrow`, `rect`, `ellipse`, `line`, `text`, `highlight`, `step` (numbered markers), `blur`, `pixelate`, `crop`. Pass `copy_to_clipboard: true` to copy the rendered image (annotations and blurs applied, never the original). `replace: true` replaces the op list in one undo step. `get_capture` returns the current revision and image.

## Do not

- Do not take screenshots the user did not ask for, and do not take one "to see the screen" without saying so.
- Do not read the screenshot's text back to the user or into a file when it may contain secrets; describe what kind of text it is.
- Do not use `set_settings` to change capture behaviour the user did not request.
