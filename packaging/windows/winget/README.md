# winget manifest (unsigned releases)

Decision O4 (maintainer, Nov 5, 2026): while CaptureMann is 0.x, Windows releases are **not code-signed**. They ship
with SHA-256 checksums in the release text and a winget manifest. winget accepts unsigned installers, and an install
through winget normally does not show the SmartScreen warning that a direct browser download does.

`CaptureMann.CaptureMann.*.yaml.template` are the three files of a winget manifest (version, installer, default
locale), with `@VERSION@`, `@URL@`, `@SHA256@` and `@DATE@` left to fill in. `render.sh` fills them from a published
release and writes the manifest folder `manifests/c/CaptureMann/CaptureMann/<version>/` that is submitted as a pull
request to `microsoft/winget-pkgs` (the maintainer submits it; nothing here pushes anywhere).

    packaging/windows/winget/render.sh 0.2.8 https://github.com/craft-mann/capturemann/releases/download/v0.2.8/capturemann-0.2.8-windows-x64-setup.exe <sha256> out/

Check before submitting: `winget validate --manifest <folder>` and `winget install --manifest <folder>` on a
Windows machine. The installer is the NSIS per-user setup (`/S` is the silent switch, no administrator needed).

Release text on the download page (while unsigned): the installer's SHA-256, and "Windows may show a SmartScreen
warning for a file downloaded in a browser: choose More info, then Run anyway. Installing with
`winget install CaptureMann.CaptureMann` avoids it."
