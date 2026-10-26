# AI-7 manual script: the text model on the AI page (local models)

Needs a person at a Mac with Ollama and/or LM Studio installed. Remote and Anthropic providers are
deferred (maintainer decision Oct 26, 2026): the page offers None and On this Mac only. Nothing here
sends data to another host; the page only talks to loopback. Record the result in `verification/`.

## Setup
- Own instance: separate `CAPTUREMANN_SOCKET`, `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`
  (do not use your daily copy). Use `make bundle-dev` / `make run-bundle-dev` only if you want the
  bundle; never `make bundle` over a running copy.
- Pull one small model in Ollama (for example `ollama pull phi4-mini`) and, optionally, load one in
  LM Studio and start its local server (port 1234).

## 1. Detection (AC1)
1. Stop Ollama and LM Studio. Open the AI page, Text model, choose On this Mac.
   Expected: both rows say Not running; the page says "Start Ollama or LM Studio, then press Check again."
2. Start Ollama, press Check again. Expected: Ollama says Running (127.0.0.1:11434), the model
   picker shows "Model - N on Ollama".
3. Start LM Studio's server, Check again. Expected: LM Studio says Running (127.0.0.1:1234).

## 2. Model and context
1. Open the picker, choose a model. Expected: its context size shows beside it when the server reports
   it (for example "262K context"); `capturemann ai text-model` shows the same model and
   `context_tokens`.
2. Switch to the other server. Expected: the model is cleared and must be chosen again.

## 3. Test
1. Press Test. Expected: "Replied in N s", "Structured output works" (or the retry note) and
   "only talks to 127.0.0.1". `capturemann ai text-model test` agrees.
2. Quit the server, press Test. Expected: the test fails with "Start Ollama or LM Studio, or choose
   another provider".
3. Choose None, press nothing: the Test row is gone. With provider None no request is made
   (check with a network monitor or Little Snitch: only loopback traffic, and none with None).

## 4. A custom address
1. Type `https://api.example.com/v1`, Save. Expected: refused in the page with "Only addresses on this
   Mac work here..." and nothing is saved (`capturemann ai text-model` unchanged).
2. Type `http://127.0.0.1:1234/v1`, Save, then type the model name and Save. Expected: Test works
   against that server.

## 5. Recommended for your Mac
- Expected: the header says "this Mac has N GB" with this Mac's real memory (check About This Mac),
  the rows that fit are marked "fits this Mac", every licence chip is MIT or Apache-2.0, and the note
  says the default is decided by S8.

## 6. Speaker labels (AI-8, no model yet)
- Expected: the card says "Speaker model not available yet"; the switch "Find speakers after each
  transcript" is off and does not move; "Your name in transcripts" shows Me, Save keeps a new name
  and `capturemann settings get profile.display_name` agrees; an empty box returns to Me.

## 7. Agents cannot change these settings
- `capturemann settings set ai.text.provider none` over the socket is refused (`app_only_setting`).

## Layout
- Check English, Vietnamese and `CAPTUREMANN_PSEUDO_LOCALE=1`: the provider control, the server rows
  and the Recommended table do not overlap or cut text at the narrowest window width.
