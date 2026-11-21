# macOS signing and notarization

The macOS app is signed with a Developer ID Application certificate and
notarized by Apple, so it opens without a Gatekeeper warning. Both steps are
optional: without credentials the build still works and the DMG ships
`UNVERIFIED_APP_INSTRUCTIONS.txt` (see `scripts/app-is-verified.sh`).

`scripts/macos-sign-and-notarize.sh` does the work. Tauri signs the main
executable only, so the script signs every other binary in the bundle (the
Apple Silicon dylibs in `Contents/Resources`), seals the app again, submits it
to Apple and staples the ticket. Notarization is not left to Tauri: it would
run before those dylibs are signed and Apple would reject the app.

## Release builds (GitHub Actions)

`release.yml` reads these repository secrets. Each one reaches only the step
that needs it, and a missing one makes the build fall back to unsigned.

| Secret | What it holds |
| --- | --- |
| `APPLE_CERTIFICATE` | the Developer ID Application `.p12`, base64 encoded |
| `APPLE_CERTIFICATE_PASSWORD` | the password of that `.p12` |
| `APPLE_SIGNING_IDENTITY` | the certificate's full name in the keychain |
| `APPLE_API_ISSUER` | issuer ID of the App Store Connect API key |
| `APPLE_API_KEY` | key ID of that API key |
| `APPLE_API_KEY_P8` | the contents of the API key's `.p8` file |

To encode the certificate on a Mac: `base64 -i certificate.p12 | pbcopy`.

The workflow builds a throwaway keychain with a random password and deletes it,
and the API key file, when the job ends, even if it fails.

## Local builds

Set these in your shell before `./build-local.sh macos`. Leave them out for an
unsigned build.

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: …"
export APPLE_API_KEY_PATH=/path/to/AuthKey_XXXX.p8
export APPLE_API_KEY=XXXX
export APPLE_API_ISSUER=…
```

## Keeping it private

Certificates, private keys and API keys never go in the repository. Their file
types are in `.gitignore` as a safety net. The certificate expires after about
a year; create a new one and update the secrets before it does.
