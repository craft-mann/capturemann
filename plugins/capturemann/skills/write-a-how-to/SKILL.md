---
name: write-a-how-to
description: Turn a task the user clicks through once into a step-by-step how-to guide with a screenshot of each click, using CaptureMann's Click Guide. Use when the user asks for a how-to, a tutorial, a walkthrough or documentation of clicks in an app, or to "write up these steps".
---

# Write a how-to with Click Guide

Read `capturemann-basics` first if you have not. Click Guide needs CaptureMann 0.3.4 or later; on an older build the tools are missing.

The flow: **start, the user clicks, stop, rewrite the titles, check for secrets, export.**

## 1. Start (Confirm tier)

`start_guide` listens to the user's mouse clicks, so the user must approve it on screen. Tell them first what will happen: "I will ask you to approve a Click Guide. It takes a screenshot of each click, never records what you type, and you can stop it any time."

1. `list_targets` and pick what to follow: a window (`{ window_id }`), a display (`{ display_id }`) or a region (`{ display_id, rect }`). Ask when several windows match. A window is best for a how-to about one app.
2. `start_guide { target, title? }`. The call waits for the approval (up to 5 minutes), then counts down (`countdown_s`, default 3) and records. If it fails:
   - `user_declined`: the user said no. Do not retry unasked.
   - `permission_missing` with `data.permission: "input_monitoring"` (macOS): the user must allow CaptureMann in System Settings > Privacy & Security > Input Monitoring. Say so and stop; nothing recorded.
   - `not_implemented` with a reason (a Wayland session that gives no click events): Click Guide is not available there. Offer screenshots one by one instead (`screenshot-and-redact`).
   - `busy`: a guide is already recording; `stop_guide` it or ask the user.

## 2. The user clicks

Tell the user to do the task once, slowly, and to say when they are done. You cannot click for them, and you must not try. Clicks on CaptureMann's own bar and outside the target are not steps. While it records you may call `set_guide { paused: true }` and `{ paused: false }` when the user asks, `set_guide { note_step: { title: "Type your password" } }` for something a click cannot show (typing is never recorded), and `set_guide { remove_last: true }` to drop a mistaken last step. `get_guide` shows the progress (`recorded_steps`).

## 3. Stop

`stop_guide` when the user says they are done. It returns the guide: every step has `title` (a draft such as `Click "Save"` or `Click here`), `note`, the `click` point, `has_proposals` and `image_id`. A guide with no step is not kept.

## 4. Rewrite the titles

The drafts come from the text nearest the click. Make them proper instructions ("Open Settings", "Choose Billing"):

1. For each step, `get_capture { capture_id: image_id }` returns the step's picture, so you can see what was clicked. `recognize_text { capture_id: image_id }` reads the text on it (on-device, nothing is sent anywhere) when you only need words.
2. Write short imperative titles and, where it helps, a one-line `note` (why, or what to expect). Keep the user's words for labels that appear on screen.
3. Apply everything in ONE `set_guide { guide_id, steps: [{ id, title, note }, ...] }` call: one call is one undo step, and the user can `undo` it. Do not renumber: the order and the badges follow the list. Set `title` and `intro` in the same call or another.

Do not invent steps, button names or outcomes that you cannot see in the pictures. When a picture is unclear, say so and keep the draft title.

## 5. Check for secrets before it leaves the computer

A guide can show emails, tokens, names and other private text. Steps with `has_proposals: true` have redaction proposals nobody accepted.

1. For each such step (and for any step that may hold private text), `suggest_redactions { capture_id: image_id }` returns proposals; show the user the kinds and positions (the previews are masked).
2. Apply the ones the user accepts with `annotate { capture_id: image_id, ops: [{ "op": "blur", "rect": [...] }] }`. `blur` and `pixelate` are the only ops that hide text.
3. Say plainly: **blurred does not mean certain**. The user must look at the pictures before sharing. `export_guide` lists the steps that still have proposals in `steps_to_review` and warns; it exports them without blurs.

## 6. Export

`export_guide { guide_id, format, dest, ... }`:

- `markdown`: `dest` is a folder; it gets `guide.md` and `images/step-NN.png` with relative links, ready for a wiki or a docs repo.
- `html`: one self-contained file (pictures embedded, light and dark, table of contents, print styles; print it for a PDF).
- `rich_text`: puts formatted text with the pictures on the user's clipboard, for a ticket or a doc.
- `pdf`: one file, A4 or `paper: "letter"`, one or two steps per page.

Options: `paper` (pdf), `headings` (`numbered`, `step`, `title_only`), `image_scale` (1 or 2), and the `intro`, `notes` and `note_steps` switches. Ask for `dest` if the user did not give one; do not write into folders they did not name. Tell them where the files are.

## Do not

- Do not start a guide without saying so first, and do not start another to "retry" without asking.
- Do not try to read or record keystrokes. Click Guide never does.
- Do not paste guide text or pictures into other tools or services; the guide stays on the computer until the user exports it.
- Do not use `set_settings` to change capture or privacy behaviour.
