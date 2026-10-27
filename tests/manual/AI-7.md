# AI-7 manual script: the text model on the AI page (built-in model)

Needs a person at a Mac and a bundle that includes `llama-server` (`make llama`, then `make
bundle-dev`). Release 0.2.0 offers one text-model provider, the model built into CaptureMann
(maintainer decision Oct 27, 2026): the page lists the built-in models (no None / Built in control). Ollama and LM Studio
are backlog items (later). Nothing here sends data to another host; the engine listens on
127.0.0.1 only. Record the result in `verification/`.

## Setup
- Own instance: separate `CAPTUREMANN_SOCKET`, `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`
  (do not use your daily copy). Use `make bundle-dev` / `make run-bundle-dev` only if you want the
  bundle; never `make bundle` over a running copy.
- Free disk space for one built-in model (the page shows its size before the download).

## 1. The model list (AC1)
1. Open the AI page, Text model. Expected: no None / Built in control; three models are listed
   (Gemma 4 E2B tagged Recommended, Qwen3.5 4B, Qwen3.5 2B) with size, memory and licence; a model
   that needs more memory than this Mac has free shows a warning. With none downloaded there is no
   Test row and `capturemann ai text-model` shows provider `none`.
2. Download a model (step 2). Expected: when no model was in use, the download is selected by itself
   when it arrives; `capturemann ai text-model` shows provider `embedded`.

## 2. Download, select, remove
1. Press Download on the model for this Mac. Expected: the size and licence are shown first; nothing
   is downloaded before you confirm; progress, then "In use" (it is used at once when none was in use).
2. `capturemann ai text-models` lists the same state. Press Use this model on another downloaded one: the engine
   status line changes after the next request, and `capturemann ai text-models use <id>` does the
   same (provider `embedded`). Remove a model that is not in use: it disappears; the one in use
   stays on.

## 3. Test
1. Press Test. Expected: "Replied in N s", "Structured output works" (or the retry note) and
   "only talks to 127.0.0.1". `capturemann ai text-model test` agrees. The first Test starts the
   engine ("Engine loading the model..." then "Engine running ...").
2. Remove the `llama-server` helper from a copy of the bundle (or use a build without it), press
   Test. Expected: a plain "engine missing" message, nothing hangs.
3. Press Don’t use a model on the active row (or remove the model in use): the Test row is gone and
   `capturemann ai text-model` says provider `none`. With provider none no request is made (check with a
   network monitor or Little Snitch: only loopback traffic, and none when off).

## 4. Speaker labels (AI-8)
- Expected: the card names the speaker models and the switch "Find speakers after each transcript"
  works (the models ship inside the app; a build without them says "Not available in this build"); "Your name in transcripts" shows Me, Save keeps a new name and
  `capturemann settings get profile.display_name` agrees; an empty box returns to Me.

## 5. Agents cannot change these settings
- `capturemann settings set ai.text.provider none` over the socket is refused (`app_only_setting`).
- Put `"ai.text.provider": "local"` (or `openai_compatible`, `anthropic`) in the settings file by hand
  and restart: no model shows as in use and `capturemann ai text-model` says provider `none`. The same
  for an old `"ai.text.embedded_model": "text-8gb"`: no model is selected, nothing crashes.

## 6. One real use
- With the built-in model active, run one report (`AI-9.md` step 2) and one translation (`AI-5.md`
  step 5). Write down the model, its context size and the reply time; Gemma 4 E2B is the recommended default (S8 checks it),
  so report the scores and do not change it here.

## Layout
- Check English, Vietnamese and `CAPTUREMANN_PSEUDO_LOCALE=1`: the model
  rows do not overlap or cut text at the narrowest window width.
