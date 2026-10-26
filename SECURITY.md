# Security policy

CaptureMann records your screen, camera and microphone and can hold OAuth sign-ins for Google Drive
and YouTube, so we take security reports seriously.

## Reporting a vulnerability

**Please do not open a public issue for a security problem.** Use GitHub's private vulnerability
reporting instead:

<https://github.com/craft-mann/capturemann/security/advisories/new>

Include what you found, the version or commit, the steps to reproduce it and what an attacker gains.
We will acknowledge a report within 7 days, keep you informed while we work on a fix, and credit you
in the release notes if you wish. Please give us a reasonable time to ship a fix before you disclose.

## Supported versions

CaptureMann is at 0.1.x. Only the latest release and `main` receive fixes.

## Scope

In scope: the desktop app, the `capturemann` CLI, the MCP server (`cm-mcp`), the daemon and its IPC,
secret handling (OAuth tokens and client credentials in the OS keychain), the permission and
confirmation gates for agents, and the build and bundling scripts.

Known and documented limits (not vulnerabilities by themselves):

- Per-client agent policies are keyed by a name the client declares. They stop mistakes, not a
  hostile local process. See `specs/02-agent-interface.md`.
- Anything that can run code as you can talk to the daemon's local socket.
- Local builds are signed with a local identity or ad hoc, and are not notarized yet.

Out of scope: vulnerabilities in third-party services (Google, YouTube) or in dependencies without a
way to exploit them through CaptureMann (report those upstream; `cargo deny` runs in CI).

## Privacy

CaptureMann has no telemetry and makes no network request unless you connect a destination or
download a model. The privacy policy is at <https://www.craftmann.app/capturemann/privacy/>.
