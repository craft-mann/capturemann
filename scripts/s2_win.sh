#!/usr/bin/env bash
# s2_win.sh '<PowerShell>': runs a command on the ThinkPad directly over ssh (no OS probe).
exec ssh -q -i ~/.ssh/id_ed25519 -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR trung@192.168.1.215 "$1"
