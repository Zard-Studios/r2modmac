# r2modmac sponsor Worker

Cloudflare Worker proxy for r2modmac's optional sponsored messages. It exposes only
`POST /api/sponsor`, accepts the app's fixed category and placements plus a random installation
UUID, and returns only validated sponsor copy, an opaque impression ID, and an optional HTTPS URL.

The two deployments are isolated:

- `r2modmac-sponsor-staging` uses the ADtention test Publisher ID.
- `r2modmac-sponsor-production` uses the ADtention live Publisher ID.

Publisher IDs are configured per environment with Wrangler secrets and are never stored in this
directory. The Worker has no database, analytics, custom request logging, or dynamic upstream URL.

```sh
npm install
npx wrangler secret put ADTENTION_PUBLISHER_ID --env staging
npm run deploy:staging

npx wrangler secret put ADTENTION_PUBLISHER_ID --env production
npm run deploy:production
```

The desktop release is compiled with the full production endpoint in
`R2MODMAC_SPONSOR_PROXY_URL`. Without that compile-time value, sponsor requests remain disabled.

## Local development

`npm run dev` (root `npm run dev` also does this automatically) runs `wrangler dev` fully locally
via workerd — no Cloudflare login or deployment involved. Copy `.dev.vars.example` to `.dev.vars`
and set the sandbox `ADTENTION_PUBLISHER_ID` to see real sponsor copy; without it, requests still
work but resolve to an empty 204, same as a misconfigured deployment would.
