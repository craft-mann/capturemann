# L1 Linux 0.3.0 platform: microphone list from the PipeWire registry (NEW-B, AUD-1)

Result: PARTIAL (one input device only; no default change, no Bluetooth).

- Machine: ThinkPad T490, Ubuntu 24.04, x86_64, GNOME 46 on X11, PipeWire 1.0.5 + WirePlumber, reached over ssh.
- Build: branch `track/linux-platform`, debug. Date 2026-11-08. Run by the Claude agent.
- Run: `cargo test -p cm-audio --lib the_real_registry -- --ignored --nocapture` (opens no stream).

## Result

- source `alsa_input.pci-0000_00_1f.3.analog-stereo` "Built-in Audio Analog Stereo", 2 channels
- sink `alsa_output.pci-0000_00_1f.3.analog-stereo`
- defaults read by `pw-metadata` equal the ones in `pw-dump` (source and sink).

The list is parsed from `pw-dump` JSON (no new library); defaults from the `default` metadata; the id is `node.name`, passed to PipeWire as `target.object`. `needs_reopen` compares `pw-metadata` every 2 s with the default seen at start (only for a recording on the default device); it was tested by logic only (fixtures), not by changing the default on the laptop.

## Not run

A second input device, a Bluetooth headset, a default change during a recording, recording from a non-default id.
