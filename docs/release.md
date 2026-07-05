# Release runbook

How to cut a Syllepsis release. The pipeline builds installers for every desktop platform
(and, once PR 3 lands, Android), uploads them to a **draft** GitHub Release, and — after a
human publishes — deploys the landing page.

## One-time setup

Configure these before the first tagged release. None require workflow edits later.

### GitHub secrets

| Secret | Used by | Notes |
| --- | --- | --- |
| `GDRIVE_CLIENT_SECRET` | `release.yml` | Google Drive OAuth client secret, baked in via `option_env!`. |
| `TAURI_SIGNING_PRIVATE_KEY` | `release.yml` | From `cargo tauri signer generate`. **Required before PR 2 (updater) merges.** |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | `release.yml` | Password for the key above. |
| `CLOUDFLARE_API_TOKEN` | `website.yml` | Scoped `Cloudflare Pages: Edit`. |
| `CLOUDFLARE_ACCOUNT_ID` | `website.yml` | Cloudflare account ID. |
| `ANDROID_KEYSTORE_BASE64` | `android.yml` (PR 3) | base64 of the upload keystore. |
| `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | `android.yml` (PR 3) | |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | `release.yml` | Optional. The signing-env guard step no-ops until these are set; adding them enables signing with **no workflow change**. |

### Updater signing key

```sh
cargo tauri signer generate -w ~/.tauri/syllepsis.key
```

Put the **public** key into `crates/syllepsis-tauri/tauri.conf.json` (`plugins.updater.pubkey`,
added in PR 2). Put the **private** key + password into the `TAURI_SIGNING_*` secrets.

### Cloudflare Pages

Create a Pages project named `syllepsis` (Direct Upload), add custom domain `syllepsis.org`
(plus a `www` redirect), and mint an API token scoped to `Cloudflare Pages: Edit`.

## Cutting a release

1. **Bump versions** — keep these three in lockstep:
   - `crates/syllepsis-tauri/tauri.conf.json` → `version`
   - `crates/syllepsis-tauri/Cargo.toml` → `version`
   - `crates/syllepsis-core/Cargo.toml` → `version`
2. **Commit** the bump and merge to `main`.
3. **Tag and push**:
   ```sh
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
   This runs `release.yml`: it creates a **draft** release and builds all platforms, uploading
   installers plus `latest.json` into that draft.
4. **QA the draft** — from the draft release page, download and smoke-test at least one
   installer per OS (`.dmg`, NSIS `.exe`, `.AppImage`). Confirm:
   - the app launches,
   - Google Drive connect completes (proves `GDRIVE_CLIENT_SECRET` was baked),
   - `latest.json` is present with a signature for each platform.
5. **Publish** the release in the GitHub UI. Publishing fires `website.yml`, which regenerates
   `website/downloads.json` from the release assets and deploys syllepsis.org.

## Dry runs

Run `release.yml` via **workflow_dispatch** (Actions tab → Release → Run workflow) on any
branch. It builds every platform and uploads the bundles as **workflow artifacts** — no
release is touched. Use this to validate build changes before tagging.

## Pre-release rehearsal

Tag `v0.1.0-rc.1` (the `-` marks it prerelease automatically). Verify the draft has all
installers + a valid `latest.json`, install manually, then — after PR 2 — publish `rc.2` and
confirm an installed `rc.1` offers and applies the update. Delete the rc releases, then tag the
real `v0.1.0`.

## Notes

- `releases/latest/download/latest.json` (the updater endpoint) only resolves once a release is
  **published**, which is why builds go to a draft and a human publishes atomically.
- The committed `website/downloads.json` fallback keeps the site's download links valid until
  the first release is published.
