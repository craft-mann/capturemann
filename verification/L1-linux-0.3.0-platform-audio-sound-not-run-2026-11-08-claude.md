# L1 Linux 0.3.0 platform: chime player and system-audio capture (NEW-C, AUD-2)

Result: NOT RUN (quiet hours).

The lab lock was held by other agents until 22:00 Vietnam time. The maintainer's rule forbids sound playing after 22:00, so the chime player (`pw-play`, `paplay`, `aplay` order) and the 4 s sink-monitor recording while a tone plays were not run. The player order, fall-through to the next program and the error text are covered by unit tests with fake program names only (`cm-audio` `chime::tests`). Script ready for a later run: `plat-hw.sh audio` (not committed). Hardware tests to run: `cargo test -p cm-audio --test hardware -- --ignored system_audio_hears_a_chime_played_by_another_process system_audio_records_four_seconds`.
