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
