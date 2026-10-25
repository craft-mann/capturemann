# CaptureMann — Brand & UI Kit v0.1

The logo, app icons, tray icons, favicon, design tokens and UI screens for CaptureMann.

- **Coding agents (Claude Code etc.):** start with [`CLAUDE.md`](CLAUDE.md). It has the specs, the rules and which file goes where.
- **Logo files:** `brand/logo/svg/` (text converted to outlines, so no font is needed).
- **App icons:** `brand/app-icon/` has `AppIcon.icns` for macOS and `app.ico` for Windows, plus PNGs at every size.
- **Tokens:** `tokens/` contains JSON, CSS variables, a Tailwind preset and a XAML ResourceDictionary.
- **Screens:** the reference boards (`screens/` and `redesign/`, HTML and PNG) are design working files kept out of this repository; the tokens, icons, brand marks and fonts here are what the app builds from. Visual decisions are summarised in [`docs/DESIGN.md`](../docs/DESIGN.md).

To drop this kit into your app repo, copy this folder to `design/` and add one line to the repo's own `CLAUDE.md`:

```
For any UI, branding or icon work, follow design/CLAUDE.md.
```

Fonts: Geist and Geist Mono, under the SIL Open Font License 1.1 (`fonts/OFL-LICENSE.txt`).
