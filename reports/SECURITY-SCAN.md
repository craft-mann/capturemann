# Security scan (Trivy)

- **commit**: a913277
- **date**: 2026-10-28 04:10 UTC
- **tool**: 0.38.3
- **scanners**: vuln,secret
- **dependency vulnerabilities**: 0 (none)
- **secrets**: 0
- **misconfigurations**: 0

The scan covers the tracked tree and `Cargo.lock`. Fake credentials used on purpose by tests are
allow-listed in `trivy-secret.yaml`. The same scan runs weekly in CI (`.github/workflows/security.yml`).

