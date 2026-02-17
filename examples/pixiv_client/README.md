# pixiv_client

A desktop Pixiv client MVP built with `bevy_xilem`.

It demonstrates:
- Pixiv OAuth token exchange (auth code + refresh token)
- Home / Ranking / Search feeds
- Thumbnail + avatar loading
- Illustration detail overlay and bookmark action

> This project is an experimental example for learning and prototyping.

## Run

From workspace root:

- `cargo run -p pixiv_client`

## Login Guide (English)

### Option A: Open browser directly (recommended)

1. Launch the app.
2. Click **Open Browser Login**.
   - The app will auto-generate a PKCE `code_verifier` (or reuse your current one).
   - Your default browser opens Pixiv login page.
   - The login URL follows official Android flow:
     - `https://app-api.pixiv.net/web/v1/login?code_challenge=...&code_challenge_method=S256&client=pixiv-android`
3. Complete login in browser.
4. After redirect, copy the `code` query value from the callback URL.
   - You can paste either:
     - the raw code, or
     - the full callback URL (the app will extract `code` automatically).
5. Back in app, click **Login (auth_code)**.
6. (Optional) Save and use the refresh token with **Refresh Token** later.

### How to get `code_verifier` + `auth_code`

- `code_verifier`
   - Click **Open Browser Login**.
   - Copy the value from the app field **PKCE code_verifier**.
   - Keep this value; it must match the authorization code you are exchanging.

- `auth_code`
   - Complete login in browser.
   - In the callback URL, find query parameter `code=...`.
   - **Official callback format** (from APK routing/parser):
      - `pixiv://account/login?code=...&via=login`
      - `via=signup` is used for sign-up flow.
   - `via` is required by official app routing parser.
   - A URL like below is **not** a usable auth code callback (it has no `code`):
      - `https://accounts.pixiv.net/post-redirect?.../start?...code_challenge=...`
   - Keep following redirects until you see a URL that contains `code=`.
   - You can paste either:
      - only the code value, or
      - the full callback URL (the app auto-extracts `code`).

### How to get `refresh_token`

- After **Login (auth_code)** succeeds, the app stores session tokens.
- The **Refresh token** input is auto-filled (if it was empty).
- Copy and save it for future logins.
- Later, you can paste this value and click **Refresh Token** to get a new access token.

### If **Open Browser Login** does not open a browser

- The app now tries:
   1. default browser integration, then
   2. OS fallback launcher (`open` on macOS, `xdg-open` on Linux, `start` on Windows).
- If both fail, the status line shows a full login URL; open it manually in your browser.

### If login shows `Network error: idp-urls not ready`

- This build now uses fallback Pixiv auth endpoints automatically.
- If you still see this message, make sure:
   1. your Auth code really contains `code=` (not just `post-redirect?...start?...`), and
   2. the `code_verifier` is the same one generated for that login attempt.

### If login shows redirect URI mismatch (`code:1508`)

- This usually means one of:
   1. `code_verifier` does not match the login attempt,
   2. auth code already expired/used, or
   3. redirect binding mismatch.
- This client now follows official exchange behavior:
   - token URL and `redirect_uri` come from `/idp-urls`
   - fallback redirect is `https://app-api.pixiv.net/web/v1/users/auth/pixiv/callback`
- If you still get `1508`, re-run **Open Browser Login**, keep the new `code_verifier`, and exchange immediately.

## Response Body Panel

- Network errors now keep full response details.
- The app shows them in a **scrollable response panel** (instead of putting giant text in status line).
- You can click **Copy Response Body** to copy full text to clipboard.

### Option B: Manual auth/refresh flow

If you already have `code_verifier` + `auth_code`, enter them directly and click **Login (auth_code)**.

If you already have a valid refresh token, paste it and click **Refresh Token**.

## Notes

- We currently use the **system browser** for login, not an embedded WebView.
- This avoids extra platform-specific WebView dependencies and keeps behavior close to the official app’s web auth flow.
- A true in-app WebView login can be added later, but it requires additional cross-platform integration work.
