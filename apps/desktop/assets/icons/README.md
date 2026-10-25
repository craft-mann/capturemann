# Icons

Official [Lucide](https://lucide.dev) SVGs, copied unmodified from the npm package
`lucide-static` **v1.60.0** (ISC license, see `LICENSE`; portions MIT from Feather). Only the
icons the app uses are kept here. 24x24 viewBox, 2 px stroke, round caps and joins, tinted with
the element's text color (`currentColor`).

To add one: copy `package/icons/<name>.svg` from `npm pack lucide-static` (same version),
and list it in `ICONS` in `src/ui/paint.rs`.

The redesign icon kit (`design/icons/`, Lucide v1.60.0 plus the 15 CaptureMann `cm-*` glyphs, which
are Apache-2.0 like the app) is copied here for the icons the plan uses; `ui::paint::ASSETS` lists
each one and a test (`icons_registered_and_wellformed`) checks the files and `design/icons/icon-map.csv`.
New files from the kit may carry a leading comment instead of the `class="lucide ..."` attribute.

## Brand marks

`brand-*.svg` are the marks of the services the app talks to (YouTube, TikTok, Facebook, Google Drive, OneDrive,
Dropbox). They are tinted like the other icons, in the theme's text colour (never the service's own colour; owner
decision, Oct 24, 2026); `ui/brand.rs` maps services to marks.

- YouTube, TikTok, Facebook, Google Drive: single-path marks on a 24 grid from the owner's brand set (Simple
  Icons paths, CC0). The marks themselves belong to their owners.
- OneDrive and Dropbox: SVG Repo files the owner supplied (`onedrive-svgrepo-com.svg`,
  `dropbox-svgrepo-com.svg`), **MIT** (credited to Blivesta's Flexicon and Kenan Gundogan's Fontisto). Their
  notice is `BRAND-LICENSE.txt` and is repeated in the generated `THIRD_PARTY_LICENSES.md`. The Dropbox icon is
  scaled from a 32 grid to the 24 grid; nothing else is changed.
