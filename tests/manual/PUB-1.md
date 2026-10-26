# PUB-1 manual script: connect YouTube with your own Google client, from scratch

Covers what no fake can: Google's own pages (the Cloud Console, the "Google hasn't verified this
app" screen, the consent screen), the real keychain, and a real revoke. The token plumbing, the
validation of the pasted values, the account file, Reconnect, the multi-channel card and the
revoke request are covered by `cargo test -p cm-auth -p cm-publish -p cm-desktop` against fakes
(PUB-1.AC1 and AC5 in part; `publish_connect::tests`, `cm_auth::revoke::tests`).

Run it with a **release bundle** (`make bundle`, run by the person who owns the app copy: macOS
permissions and the keychain entry follow the bundle id) or `cargo run -p cm-desktop` for a
second, throw-away data folder (`CAPTUREMANN_CONFIG_DIR=<empty dir>`, never your daily one).
Nothing is recorded in this script; the publish steps are in [`PUB-2.md`](PUB-2.md).

You need: a Google account (two for step 9), a **new** Google Cloud project made for this run (so
the setup is really from scratch), a stopwatch, and a second Wi-Fi or a way to go offline.

Terms: **the card** is Settings → Destinations → Publish to → YouTube. **The config folder** is
`~/Library/Application Support/CaptureMann` (or `CAPTUREMANN_CONFIG_DIR`).

1. **Start the clock (AC4).** Open the card in an app with no YouTube account. Expected: status
   "Not connected", the line that says what YouTube needs, one button "Connect account". Start the
   stopwatch and press it. Expected: the setup panel with five numbered steps, two cautions (the
   unverified-app screen with test users and the 7-day Testing limit; the daily upload limit and
   the privacy YouTube may keep), and the fields "Client ID" and "Client secret" (masked).
2. **Follow the steps exactly as written, with nothing else open.** Cloud Console → new project →
   Library → "YouTube Data API v3" → Enable → OAuth consent screen (External; app name; your Google
   address as a **test user**) → Credentials → OAuth client ID → **Desktop app** (a new one, not the
   Drive client). Note every place where the text and the console disagree (a renamed menu, a missing
   step) in the report.
3. **Pasting wrong values.** Press "Continue in browser" with empty fields, then with
   `my-project`, then with a client ID that has a space in it. Expected: a message under the
   field each time ("Paste the client ID.", "That doesn't look like a Google client ID. It ends with
   .apps.googleusercontent.com.", "The client ID has no spaces in it; copy it again."), and no
   browser opens.
4. **The unverified-app screen (AC1).** Paste the real client ID and secret, press "Continue in
   browser". Expected: the system browser opens Google. Sign in with the test user. Google shows
   "Google hasn't verified this app": choose **Advanced**, then **Go to … (unsafe)**. The consent
   screen names the YouTube scopes (upload and manage); press **Allow**. Expected: a page says
   you can close the tab, the app shows "Connected as <channel name>", then "Done". Stop the
   stopwatch at "Connected": **record the minutes** (pass: 15 or fewer, for a first-timer; AC4 asks
   for three people).
5. **Nothing secret in a file (AC1).** In the config folder run
   `grep -rIl "GOCSPX\|ya29\.\|1//" .` with your client secret and a token prefix: no file matches.
   `publish_accounts.json` holds the id, platform, channel name, channel id and default privacy
   only. In Keychain Access, search `oauth.google.youtube-`: two items under the service
   `dev.capturemann.app` (the sign-in, and one whose name ends in `.client` for the client ID and secret).
6. **Who may sign in.** Repeat step 4 with a Google account that is **not** a test user. Expected:
   Google refuses with "Access blocked … has not completed the Google verification process"
   (error 403 `access_denied`); CaptureMann shows the failure in plain words in the panel and
   stays on the form, or ends the attempt after the 5 minute timeout; Cancel always works. Then
   press **Deny** at the consent screen with the real account: back to the form with a message, no
   account is added.
7. **Test connection.** Manage → Test connection. Expected: "Works: signed in as <channel>."
   Go offline and press it again: a "Could not reach the server" message, **not** "Reconnect needed".
8. **Default visibility.** Manage → choose Private, then close and reopen the panel: the choice
   stays; the card line reads "Default visibility: Private". New accounts start at Unlisted.
9. **A second channel.** Use a second Google account that is a test user (or add the same one
   again and pick another channel in the browser's channel chooser). Card → **Add another channel**
   → the setup form opens empty (it is a new account): paste the same client ID and secret. Expected: the card lists both channels, the first with a **Default** pill, each with its
   own Manage button; Manage on the second → "Make this the default channel" → the pill moves, and
   the publish sheet (Publish on any exported recording) starts with that channel selected and
   switches between the two.
10. **Reconnect after a revoke.** At `myaccount.google.com/permissions` remove the app's access.
    In CaptureMann: Manage → Test connection. Expected: "The connection was revoked or has expired.
    Connect the account again." and the card shows **Reconnect needed** (amber) with a **Reconnect**
    button. Press it: the panel offers "Sign in again" with the saved client ID shown (never the
    secret); press it, repeat steps 4's browser part. Expected: back to Connected as the same channel,
    default visibility and Default pill kept. Press Reconnect and sign in with another channel
    instead: the panel says it is a different channel than before.
11. **Disconnect, online (AC5).** Manage → Disconnect… Expected text: CaptureMann asks Google to
    revoke the access, deletes the sign-in from this Mac, videos stay. Confirm. Expected: the
    channel leaves the card at once (or within a second) and **no warning line** appears. At
    `myaccount.google.com/permissions` the app is gone within a minute (reload). The two keychain
    items are gone; `publish_accounts.json` has no entry for it; the videos are still on YouTube.
12. **Disconnect, offline (AC5, best effort).** Connect again, turn Wi-Fi off, Disconnect… and
    confirm. Expected: the channel is signed out here and the card shows in amber: "<name> is signed
    out here; Google could not be reached to revoke access, you can remove it at
    myaccount.google.com/permissions." Turn Wi-Fi on: the app **is still listed** at Google until you
    remove it by hand (this is the documented outcome, record it). The keychain items are gone.
13. **Google Drive too (STO-2).** Settings → Destinations → Google Drive connected earlier: Disconnect
    shows the Google text ("asks Google to revoke") and the permissions page loses the Drive client;
    offline it shows the same sentence as step 12. Two destinations on one Google account: the
    first Disconnect revokes nothing (the account is still used), the last one does.
14. **Testing mode (not repeatable in one sitting).** Leave the consent screen in "Testing". Seven
    days later press Test connection: expected "Reconnect needed". Then use **Publish app** on the
    consent screen and reconnect: it should now stay signed in. Write down what you saw and when.

Pass criteria: steps 1 to 12 pass; setup (step 1 to the "Connected" in step 4) takes 15 minutes or
less; no secret in any file; Disconnect removes the app from Google's permissions page when online
and says how to finish by hand when offline.

## Privacy

Uses your own Google account and Cloud project only. No recording, camera, microphone or screen
capture. Use a throw-away channel; do not paste your client secret anywhere but the app. Delete the
Cloud project afterwards if it was made for this run.

## Report

Copy the template in [`../../verification/README.md`](../../verification/README.md) to
`verification/PUB-1-<date>-<who>.md`: one row per step above, the setup time in minutes, the places
where the guide and the console disagreed, and the offline-revoke observation. Say "verified" in a
spec only when that report exists and every step passed.

## Status of the author's runs

(none yet; the maintainer's first connect on Oct 23, 2026 is in
`verification/STO-2-PUB-2-gdrive-youtube-2026-10-23-maintainer.md`, as reported, steps 4 only)
