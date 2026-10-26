#!/usr/bin/env bash
# Writes the quality report into reports/: test results, dependency licences and advisories, a
# Trivy scan and a CycloneDX SBOM. Run it on a clean checkout of the commit you release:
#   scripts/quality-report.sh                       # runs `cargo test --workspace` itself
#   CM_REPORT_TEST_LOG=<file> scripts/quality-report.sh   # reuse the output of a test run
# Needs: cargo, python3, and (each optional, skipped with a note when missing) cargo-deny, trivy.
set -uo pipefail
cd "$(dirname "$0")/.."
out=reports
mkdir -p "$out"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

commit=$(git rev-parse --short HEAD)
dirty=""; [ -n "$(git status --porcelain)" ] && dirty=" (working tree has uncommitted changes)"
python3 -I - "$tmp/meta.json" "$commit$dirty" <<'PY'
import json, platform, subprocess, sys, datetime
rustc = subprocess.run(["rustc", "-V"], capture_output=True, text=True).stdout.strip()
json.dump({"commit": sys.argv[2], "date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M UTC"),
           "rustc": rustc, "host": f"{platform.system()} {platform.machine()}"}, open(sys.argv[1], "w"))
PY

# 1. Tests
log=${CM_REPORT_TEST_LOG:-}
if [ -z "$log" ]; then
  log=$tmp/test.log
  cargo test --workspace --locked --no-fail-fast >"$log" 2>&1
fi
python3 -I scripts/test_summary.py "$log" "$out/TEST-REPORT.md" "$tmp/meta.json"
test_status=$?

# 2. Licences and advisories
{
  echo "# Dependency checks"; echo
  echo "- **commit**: $commit$dirty"; echo
  if command -v cargo-deny >/dev/null; then
    echo '## cargo deny (licences, bans, sources, advisories)'; echo
    deny=$(cargo deny check licenses bans sources advisories 2>&1)
    echo '```'; echo "$deny" | grep -E '^(advisories|bans|licenses|sources) |^error\['; echo '```'; echo
    echo "$(echo "$deny" | grep -c '^warning\[duplicate\]') crates appear in more than one version (informational: pulled in by GPUI and its dependencies)." 
  else echo "cargo-deny is not installed; this check was skipped."; fi
} >"$out/DEPENDENCY-REPORT.md"

# 3. Trivy (vulnerabilities in Cargo.lock, committed secrets) and the SBOM
if command -v trivy >/dev/null; then
  scanners=vuln,secret
  # Trivy 0.4x+ calls the misconfiguration scanner `misconfig`; older ones do not know it.
  trivy fs --help 2>&1 | grep -q misconfig && scanners=vuln,secret,misconfig
  trivy fs . --scanners "$scanners" --skip-dirs target,spikes/spike-out,spike-out \
    --format json --output "$tmp/trivy.json" >/dev/null 2>&1
  trivy fs . --skip-dirs target,spikes/spike-out,spike-out --format cyclonedx \
    --output "$out/sbom.cdx.json" >/dev/null 2>&1
  python3 -I - "$tmp/trivy.json" "$tmp/meta.json" "$scanners" "$(trivy --version | head -1 | sed s/^Version:\ //)" >"$out/SECURITY-SCAN.md" <<'PY'
import json, sys, collections
d = json.load(open(sys.argv[1])); meta = json.load(open(sys.argv[2]))
vulns, secrets, misconf = [], [], []
for r in d.get("Results") or []:
    for v in r.get("Vulnerabilities") or []: vulns.append((r["Target"], v))
    for s in r.get("Secrets") or []: secrets.append((r["Target"], s))
    for m in r.get("Misconfigurations") or []: misconf.append((r["Target"], m))
sev = collections.Counter(v["Severity"] for _, v in vulns)
print("# Security scan (Trivy)\n")
print(f"- **commit**: {meta['commit']}\n- **date**: {meta['date']}\n- **tool**: {sys.argv[4]}\n- **scanners**: {sys.argv[3]}")
print(f"- **dependency vulnerabilities**: {len(vulns)} " + (", ".join(f"{k} {n}" for k, n in sorted(sev.items())) or "(none)"))
print(f"- **secrets**: {len(secrets)}\n- **misconfigurations**: {len(misconf)}\n")
print("The scan covers the tracked tree and `Cargo.lock`. Fake credentials used on purpose by tests are")
print("allow-listed in `trivy-secret.yaml`. The same scan runs weekly in CI (`.github/workflows/security.yml`).\n")
if vulns:
    print("| Advisory | Package | Installed | Fixed in | Severity | Title |\n|---|---|---|---|---|---|")
    for t, v in vulns:
        print(f"| {v['VulnerabilityID']} | `{v['PkgName']}` | {v['InstalledVersion']} | {v.get('FixedVersion','-')} | {v['Severity']} | {v.get('Title','')} |")
PY
else
  echo "trivy is not installed; the scan was skipped." >"$out/SECURITY-SCAN.md"
fi
echo "reports written to $out/"
exit $test_status
