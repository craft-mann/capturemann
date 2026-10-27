# AI-7 manual script: the text model on the AI page (built-in model)

Needs a person at a Mac and a bundle that includes `llama-server` (`make llama`, then `make
bundle-dev`). Release 0.2.0 offers one text-model provider, the model built into CaptureMann
(maintainer decision Oct 27, 2026): the page offers None and Built in only. Ollama and LM Studio
are backlog items (later). Nothing here sends data to another host; the engine listens on
127.0.0.1 only. Record the result in `verification/`.

## Setup
- Own instance: separate `CAPTUREMANN_SOCKET`, `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`
  (do not use your daily copy). Use `make bundle-dev` / `make run-bundle-dev` only if you want the
  bundle; never `make bundle` over a running copy.
- Free disk space for one built-in model (the page shows its size before the download).

## 1. Choosing the provider (AC1)
1. Open the AI page, Text model. Expected: two choices, None and Built in; with None the page says
   "No text model. Reports and translation to other languages are off." and there is no Test row.
2. Choose Built in. Expected: the models for each memory tier are listed with size, memory, licence
   and the tag for this Mac's tier; `capturemann ai text-model` shows provider `embedded`.

## 2. Download, select, remove
1. Press Download on the model for this Mac. Expected: the size and licence are shown first; nothing
   is downloaded before you confirm; progress, then "Active".
2. `capturemann ai text-models` lists the same state. Select another downloaded model: the engine
   status line changes after the next request. Remove a model that is not in use: it disappears.

## 3. Test
1. Press Test. Expected: "Replied in N s", "Structured output works" (or the retry note) and
   "only talks to 127.0.0.1". `capturemann ai text-model test` agrees. The first Test starts the
   engine ("Engine loading the model..." then "Engine running ...").
2. Remove the `llama-server` helper from a copy of the bundle (or use a build without it), press
   Test. Expected: a plain "engine missing" message, nothing hangs.
3. Choose None: the Test row is gone. With provider None no request is made (check with a network
   monitor or Little Snitch: only loopback traffic, and none with None).

## 4. Speaker labels (AI-8)
- Expected: the card names the speaker models and the switch "Find speakers after each transcript"
  works only when they are installed; "Your name in transcripts" shows Me, Save keeps a new name and
  `capturemann settings get profile.display_name` agrees; an empty box returns to Me.

## 5. Agents cannot change these settings
- `capturemann settings set ai.text.provider none` over the socket is refused (`app_only_setting`).
- Put `"ai.text.provider": "local"` (or `openai_compatible`, `anthropic`) in the settings file by hand
  and restart: the page shows None and `capturemann ai text-model` says provider `none`.

## 6. One real use
- With the built-in model active, run one report (`AI-9.md` step 2) and one translation (`AI-5.md`
  step 5). Write down the model, its context size and the reply time; the default model is decided
  by spike S8, not here.

## Layout
- Check English, Vietnamese and `CAPTUREMANN_PSEUDO_LOCALE=1`: the provider control and the model
  rows do not overlap or cut text at the narrowest window width.
