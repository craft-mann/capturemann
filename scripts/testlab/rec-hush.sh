#!/usr/bin/env bash
# Runs on the lab laptop: mute the default output after <seconds> (the 22:00 Vietnam time rule for sound).
# rec-hush.sh <seconds>   (start with nohup in the background)
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
sleep "$1"
wpctl set-mute @DEFAULT_AUDIO_SINK@ 1
pkill -x ffplay
