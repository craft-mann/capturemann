# Quality reports

A snapshot of the checks for one commit (the commit is named at the top of each file). Regenerate
before a release with `make report` (it runs the whole test suite, about 5 minutes).

| File | What it is |
|---|---|
| [`TEST-REPORT.md`](TEST-REPORT.md) | `cargo test --workspace` results per test binary. Ignored tests need real hardware or accounts and are run by hand. |
| [`DEPENDENCY-REPORT.md`](DEPENDENCY-REPORT.md) | `cargo deny`: licences (no GPL/AGPL), RustSec advisories, sources. |
| [`SECURITY-SCAN.md`](SECURITY-SCAN.md) | Trivy scan of `Cargo.lock` and the tree for vulnerabilities and committed secrets. |
| [`sbom.cdx.json`](sbom.cdx.json) | CycloneDX software bill of materials. |

These are results from one Mac (Apple Silicon). CI runs the same checks on every push; the Trivy scan
also runs weekly.
