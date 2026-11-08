# L1 Linux 0.3.0: built-in text model download and one generation (x86_64, ThinkPad T490)

- **Date:** 2026-11-08
- **Who:** Claude (AI agent, packaging track), nothing run by a person
- **Machine:** the lab ThinkPad T490 (Intel UHD 620), Ubuntu 24.04; CPU only, llama-server run at nice 19
- **Package:** the 0.2.9 `.deb` installed on the lab (`/opt/capturemann/lib/capturemann`: `capturemann`, `cm-daemon`, `llama-server`), used read-only; config and library were isolated (`CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_SOCKET` in `~/wt/`), the lab's own settings untouched
- **Status:** PARTIAL

| Step | Result | Note |
| --- | --- | --- |
| Download `qwen3.5-2b` (1,270,808,032 bytes) with the product's own store code (`TextModelStore::download`: catalogue URL, resume, checksum) | PASS | called from a throwaway cm-ai example, because the daemon's `download_text_model` is a Confirm-tier action and a headless daemon answers `no_confirmer`; the app's confirmation dialog was not used |
| SHA-256 of the file | PASS | `0bfe35af...a826cc`, equal to the pinned value in `text_models.json` |
| `capturemann ai text-models` lists it as downloaded; `ai text-models use qwen3.5-2b` | PASS | |
| `capturemann ai text-model test` (daemon starts the package's `llama-server`, loads the model, one short structured-output prompt) | PASS | answered in 7234 ms, "structured output works", context 16384 |
| Model file and test folders deleted afterwards | DONE | |
| Quality of the answer, a report on a real transcript, the AI page download button, Vulkan | NOT RUN | |
