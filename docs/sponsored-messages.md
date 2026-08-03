# Sponsored messages and privacy

r2modmac can show an occasional, compact sponsored message only inside
**Preferences → Support r2modmac**. It is not a popup, banner, dialog, progress message, or
part of installation, updates, Sync, Apply, Repair, errors, warnings, or onboarding.

## Your controls

- **Sponsored messages** is enabled by default and can be disabled at any time in Preferences.
- Disabling it immediately removes the current message, clears the local sponsor state (including
  the random installation identifier), and prevents future sponsor requests.
- **Show less frequently** reduces the local limit to at most once every 7 days and once per 30
  days.
- **Reset sponsor cache** removes recently shown, dismissed, and cached sponsor messages. It keeps
  the random installation identifier so a reset cannot bypass the network's frequency limits.

The rest of r2modmac works exactly the same whether this feature is on or off.

## What is sent when enabled

The desktop app creates one random UUID for the installation only after a sponsor request is
eligible. It is stored locally in r2modmac's app-data directory and is sent to the sponsor proxy
as `subject`. It is not derived from a username, account, device fingerprint, game, mod, profile,
file, path, or application activity.

The request to the r2modmac proxy also contains only these static values:

- category: `gaming-mod-manager`
- placement: `preferences-support`

The proxy maps the fixed r2modmac category to ADtention's documented `general` category, then
forwards that category and the subject for delivery and frequency limiting.
ADtention may use delivery metadata for billable impressions. r2modmac does not add first-party
analytics or telemetry to this integration.

## What is never sent by r2modmac for sponsorship

- Mod lists, mod names, mod versions, package metadata, or game selection
- Profiles, configuration content, local files, paths, usernames, or account details
- Search terms, clicks outside the optional sponsor link, workflow activity, errors, terminal
  output, prompts, or content from the application
- Hardware identifiers or device fingerprints

## Hosting and links

The request goes through a small Vercel Function before ADtention. Like any HTTPS host, Vercel can
process technical connection metadata such as IP address and request time. The function contains no
custom request logging, database, analytics, or tracking code. The optional sponsor link opens only
after you click it and must use HTTPS.

This means the feature is not described as “tracker-free”. The precise promise is: no application
content or personal data is sent by r2modmac for sponsorship, and you can disable sponsorship at
any time.

## Why the module is isolated

The sponsor module is limited to its own Tauri commands and the `services/adtention-proxy` service.
It has no dependency on profiles, mod management, game installation, or Sync/Apply. It can be
disabled or removed without changing those core flows.
