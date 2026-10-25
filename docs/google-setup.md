# Connect Google Drive and YouTube

CaptureMann does not ship a Google client. You create your own (free, about 10 minutes), so your
videos and sign-in never pass through anyone else's account or quota, and nothing is sent anywhere
until you connect. The app guides you through the same steps in Settings → Destinations.

What each connection can do:

| Connection | Google scope | What it allows |
|---|---|---|
| Google Drive | `drive.file` | Create and update only the files CaptureMann itself uploads. It cannot see the rest of your Drive. |
| YouTube | `youtube.upload`, `youtube` | Upload videos, set title, description, tags, playlist and thumbnail. Privacy is never public by default. |

Secrets (the client secret and the sign-in) are stored in the macOS Keychain, never in a file.

## 1. Create a Google Cloud project

1. Open the [Google Cloud Console](https://console.cloud.google.com/) and create a new project
   (any name, for example "CaptureMann").
2. **APIs & Services → Library**: enable **Google Drive API** for Drive, **YouTube Data API v3**
   for YouTube (enable both if you use both).

## 2. Set up the consent screen

1. **APIs & Services → OAuth consent screen** (in newer consoles: **Google Auth Platform**).
2. User type **External**. Fill in the app name and your email.
3. Add your Google address as a **test user**. While the app is in "Testing", only test users can
   sign in.

> **Testing mode caveats.** Google shows "Google hasn't verified this app" (choose **Advanced →
> Go to … (unsafe)**; it is your own project). In Testing mode the sign-in expires after 7 days;
> CaptureMann then shows **Reconnect needed** and one click signs you in again. Publishing the app
> to production removes the 7-day limit but may need Google's verification for the YouTube scopes.

## 3. Create the OAuth client

1. **APIs & Services → Credentials → Create credentials → OAuth client ID**.
2. Application type **Desktop app**. Create one client per service, or reuse one if both APIs are
   enabled in the same project.
3. Copy the **client ID** (ends in `.apps.googleusercontent.com`) and the **client secret**.

## 4. Connect in CaptureMann

1. **Settings → Destinations**. For Drive use **Google Drive**; for YouTube use **Publish to →
   YouTube**. Press **Connect account**.
2. Paste the client ID and secret, then **Continue in browser**.
3. Sign in with the test user and press **Allow**. The page says you can close the tab, and the app
   shows "Connected".
4. **Test connection** checks that the sign-in works. **Reconnect** appears if it expires or you
   revoke it.

## Limits and good to know

- YouTube allows about 100 uploads per day per Google Cloud project (counted in Pacific time).
  CaptureMann shows today's count on the account.
- A video can be **Private**, **Unlisted** or **Public**; the default for a new account is
  Unlisted. An agent can never publish without you approving an on-screen dialog.
- **Disconnect** asks Google to revoke the access and removes the sign-in from this Mac. Videos stay
  where they are. If you are offline, remove the app at
  [myaccount.google.com/permissions](https://myaccount.google.com/permissions).
- Quotas, consent-screen wording and menu names change on Google's side; if a step does not match,
  the in-app guide and Google's documentation win. Issues with this page are welcome.
