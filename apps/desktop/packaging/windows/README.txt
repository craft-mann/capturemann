Windows packaging (not built yet)

- Executable and installer icon: packaging/app.ico (design/brand/app-icon/app.ico; 16-256 px).
  Embed it with a build script (for example the `winres` crate) or the installer tool.
- MSIX / Store tiles: design/brand/app-icon/png/windows/app-icon-{44,71,150,310}.png.
- Notification area icons are embedded from assets/tray/*.ico (src/platform.rs picks the
  dark or light variant and the recording variant).
