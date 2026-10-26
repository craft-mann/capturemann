# macOS permissions while developing

CaptureMann opens on a permissions screen (also Settings → Permissions) that lists what it can use,
what is granted and one button per permission. Nothing is requested until you press a button. After
granting Screen Recording, quit and reopen the app (the screen has a "Restart CaptureMann" button).

**Ad-hoc signatures change on every build, and macOS then drops the Screen Recording grant.** The
symptom is "permission is not granted" in a toast, or the banner "Screen Recording permission is
missing" at the top of the library. Fix it once by using a stable signing identity: run
`packaging/macos/create-dev-identity.sh` (it asks before touching your keychain). After that
`make bundle` signs with "CaptureMann Local Dev" automatically and the grant survives rebuilds.

Other things worth knowing:

- If a grant is stale, switch the CaptureMann entry off and on in System Settings → Privacy &
  Security, or remove it with `-` and add the app again. `tccutil reset` is the last resort.
- A grant made while the app was signed ad hoc keeps an old code requirement; macOS logs
  `Failed to match existing code requirement` (see `/usr/bin/log show --predicate 'subsystem == "com.apple.TCC"'`)
  and the permission stays "not granted" although the switch is on. Remove the entry and grant again.
- Each rebuilt app asks once for the login password to read its keychain item (the code hash
  changed). Choose **Always Allow**. A Developer ID-signed release does not ask after updates.
- Details and environment variables (`CM_BUNDLE_OUT`, `CM_FFMPEG_BUNDLE`, `CM_HARDENED`,
  `CM_SIGN_IDENTITY`): [`packaging/macos/README.md`](../../packaging/macos/README.md).
