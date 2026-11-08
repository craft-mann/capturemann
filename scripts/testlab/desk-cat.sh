#!/usr/bin/env bash
# Runs ON the lab laptop: prints /tmp/wlx/<name>.txt (output of desk-bg.sh <name> ...)
cat "/tmp/wlx/$1.txt" 2>&1 | tail -${2:-40} | cut -c1-300
