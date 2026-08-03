# r2modmac ADtention proxy

This Vercel Function is deliberately a privacy boundary: it accepts only the static
`gaming-mod-manager` category, the `preferences-support` placement, and a random persistent
installation UUID (`subject`) generated locally after sponsorship is enabled. It maps that fixed
category to ADtention's supported `general` category. It never receives a user ID, profile, game,
mod, path, activity, analytics event, device fingerprint, or application content.

## Deploy

1. Create a Vercel project rooted at `services/adtention-proxy`.
2. Set `ADTENTION_PUBLISHER_ID` in Vercel: use the sandbox publisher ID for Development and
   Preview, and the live publisher ID for Production only.
3. Disable Vercel Analytics for the project if it is enabled.
4. Add the deployed endpoint (for example `https://your-project.vercel.app/api/sponsor`) as the
   `R2MODMAC_SPONSOR_PROXY_URL` **GitHub Actions secret** in the r2modmac repository. The release
   workflow reads that secret at build time.

The desktop app has no default proxy URL. Without that compile-time setting, sponsorship is
inert: it makes no request and shows nothing. Any proxy, SDK, network, timeout, rate-limit, or
payload validation failure returns an empty `204` response and never affects core app behavior.

The function passes the random `subject` to ADtention because the SDK uses it to deliver and rate
limit sponsored messages. Do not replace it with a username, account ID, hardware ID, fingerprint,
profile data, or activity-derived identifier.

## What must stay private

Keep only payout/withdrawal secrets private; they are not used by this function and must never be
added to GitHub Actions, Vercel, a `.env` file committed to this repository, or the desktop app.
ADtention documents the publisher ID as public. The proxy URL is hidden from the repository and
workflow logs through a GitHub Actions secret, but it is not a security boundary: a released desktop
application and its network traffic can reveal it. Treat it as configuration, not a credential.
