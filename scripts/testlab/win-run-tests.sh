#!/usr/bin/env bash
# Gets the Windows test programs that the CI workflow "Windows tests" built (windows-tests.yml) onto the lab
# laptop and prints the commands that run them. Runs on the developer's Mac.
#
#   scripts/testlab/win-run-tests.sh <run id>          download that run's artifact, check it, push, unpack on the laptop
#   scripts/testlab/win-run-tests.sh latest            the newest successful run of the workflow on the branch (see BRANCH)
#   scripts/testlab/win-run-tests.sh <run id> --no-push   only download and check (to ./.testlab-downloads/<run id>)
#
# Settings (environment): REPO (default craft-mann/capturemann), BRANCH (default track/windows, used by `latest`),
# plus the lab settings of lab.sh (~/.config/capturemann-lab). The laptop must be up in Windows.
#
# Result on the laptop: C:\lab\tests\ (the programs, README.md, BUILD.txt) and C:\lab\win-run-in-session.ps1.
# The programs that need the desktop (screen, camera, microphone) run through win-run-in-session.ps1, because a
# command started over ssh is in session 0; scripts/testlab/README.md ("Windows test programs") has the rules.
# Hardware tests run only when the owner asked for them.
set -euo pipefail
die() { echo "win-run-tests.sh: $*" >&2; exit 1; }

here="$(cd "$(dirname "$0")" && pwd)"
lab="$here/lab.sh"
repo="${REPO:-craft-mann/capturemann}"
branch="${BRANCH:-track/windows}"
run="${1:-}"; [ -n "$run" ] || { sed -n '2,13p' "$0"; exit 1; }
push=1; [ "${2:-}" = "--no-push" ] && push=0
command -v gh >/dev/null || die "the GitHub CLI (gh) is needed"

if [ "$run" = latest ]; then
  run="$(gh run list --repo "$repo" --workflow windows-tests.yml --branch "$branch" --status success \
    --limit 1 --json databaseId --jq '.[0].databaseId')"
  [ -n "$run" ] && [ "$run" != null ] || die "no successful 'Windows tests' run on $branch"
  echo "latest successful run on $branch: $run"
fi

dest="$here/../../.testlab-downloads/$run"
rm -rf "$dest"; mkdir -p "$dest"
gh run download "$run" --repo "$repo" --name capturemann-windows-tests --dir "$dest"
zip="$dest/capturemann-windows-tests.zip"
[ -f "$zip" ] && [ -f "$zip.sha256" ] || die "the artifact has no capturemann-windows-tests.zip and .sha256 ($(ls "$dest"))"
want="$(cut -d' ' -f1 "$zip.sha256")"
have="$(shasum -a 256 "$zip" | cut -d' ' -f1)"
[ "$want" = "$have" ] || die "SHA-256 mismatch: expected $want, got $have"
echo "downloaded and checked: $zip ($have)"
[ "$push" = 1 ] || { echo "not pushed (--no-push)"; exit 0; }

"$lab" status | grep -q 'up: windows' || die "the laptop is not up in Windows (scripts/testlab/lab.sh status)"
"$lab" push windows "$zip" "$here/win-run-in-session.ps1" "$here/win-make-speech.ps1"
"$lab" windows '$ErrorActionPreference="Stop"; Remove-Item -Recurse -Force C:\lab\tests -ErrorAction SilentlyContinue; Expand-Archive C:\lab\capturemann-windows-tests.zip -DestinationPath C:\lab\tests -Force; Get-Content C:\lab\tests\BUILD.txt; (Get-ChildItem C:\lab\tests\*.exe).Count.ToString() + " programs in C:\lab\tests"'

cat <<'EOF'

Run a test in the logged-in session (screen, window, camera, microphone):
  scripts/testlab/lab.sh windows 'C:\lab\win-run-in-session.ps1 -Exe C:\lab\tests\cm-capture__real_display.exe -Arguments "--ignored --nocapture --test-threads=1"'
Run one over ssh (no desktop needed):
  scripts/testlab/lab.sh windows 'C:\lab\tests\cm-render__ocr_real.exe --ignored --nocapture --test-threads=1'
List what a program holds:   ... '<exe> --list --ignored'
The full list of programs and test names: scripts/ci/windows-tests-README.md (also C:\lab\tests\README.md).
Pull a result file back:     scripts/testlab/lab.sh pull windows 'C:\lab\run\session-<id>.txt' ./out
EOF
