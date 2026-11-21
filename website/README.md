# CenAI website and Cloudflare Worker

CenAI public website and restricted administrator portal. The deployment is a single Cloudflare Worker with authenticated API routes handled at the edge. Public static files are fetched from the allowlisted GitHub repository on a cache miss and cached at Cloudflare's edge. Public editorial pages, SEO metadata, robots.txt and sitemap.xml are served through the same allowlist. This is the current bridge while the Cloudflare Git build integration is unavailable; the Worker and API runtime are hosted directly on Cloudflare.

## Hosting

- Worker: `cenai-website`
- Production URL: `https://cenai-website.cenai-app.workers.dev`
- Source: this repository, `website/`
- Build: `npm run build`
- Deploy: GitHub Actions workflow `.github/workflows/cloudflare-deploy.yml`; requires repository secret `CLOUDFLARE_API_TOKEN` with Workers Scripts:Edit and Workers AI:Edit. Manual `npm run deploy` requires Wrangler authentication.
- Static assets are embedded into `dist/_worker.js`; no public server source directory is deployed.

## Routes

- `/`: public homepage.
- `/admin`: restricted administrator control center.
- `/beta`: beta application and feedback.
- `/register`: invitation-only Discord registration and app password setup.
- `/api/admin` and `/api/beta`: authenticated Supabase control-plane proxy.
- `/api/admin-config`: public Supabase URL and publishable key.
- `/api/site-pages`: published CMS pages only.
- `/api/releases`: GitHub release feed.
- `/api/email-otp`: authenticated OTP delivery and verification.
- `/api/beta-notification`: administrator-reviewed beta email notifications.
- `/api/functions/:slug`: allowlisted Supabase Edge Function proxy, including PayPal billing, public API, billing, and telemetry.
- `/api/webhooks/paypal`: PayPal webhook proxy with signature headers preserved.
- `/api/ai/chat/completions`: quota-gated, session-authenticated Workers AI chat endpoint.
- `/api/health`: non-secret service and configuration status.

## Cloudflare runtime variables

Plain-text variables:
- `EMAIL_USER` (Gmail sender address; non-secret)
- `SUPABASE_URL`
- `SUPABASE_PUBLISHABLE_KEY`

Encrypted secrets:
- `EMAIL_PASS` (required Gmail app password; never store it in `vars`)
- `EMAIL_OTP_SECRET` (high-entropy HMAC secret)

The Wrangler config declares `EMAIL_PASS` in `secrets.required`, so a future deploy is blocked until the encrypted binding exists. After rotating the exposed Gmail app password, add the replacement directly in Cloudflare (do not paste it into chat or commit it):

```sh
npx wrangler secret put EMAIL_PASS --config website/wrangler.jsonc
```

Do not commit secrets. Production secrets are managed in Cloudflare Worker Settings > Variables and Secrets. The repository's Cloudflare deploy workflow also needs the `CLOUDFLARE_API_TOKEN` GitHub Actions secret to deploy automatically.

## Backend and data

Supabase remains the PostgreSQL and Auth system of record to preserve accounts and existing data. The control plane, PayPal billing, public API, billing, and telemetry remain deployed as Supabase Edge Functions and are routed through the Cloudflare Worker. No service-role key or payment secret is exposed to the browser or Worker bundle. Cloud AI uses the Cloudflare `AI` binding and a service-role-only quota reservation RPC. The Gemma 4 model is registered. Runtime access is controlled by the `ai.cloud.enabled` feature flag, with per-user request and token quotas enforced before inference; the admin integrations panel reports the current state.

Email OTP challenge writes and verification now use the `cenai-email-otp` Edge Function with service-role-only database RPCs. The original user-callable OTP SECURITY DEFINER functions have had public/authenticated execution revoked.

## Local validation

Run `npm run check` and `npm run build`. The build excludes `api/`, internal setup docs, and Vercel configuration, then embeds only public assets in the Worker module.

## Search and publisher readiness

- Public editorial content lives in `guides/` and is linked from the homepage.
- `robots.txt` allows public content discovery while excluding private account, billing, admin and API routes.
- `sitemap.xml` lists indexable public pages only.
- Privacy, terms and contact pages are published and linked in the footer.
- AdSense code and `ads.txt` are intentionally not populated until the owner supplies the real publisher ID from an approved AdSense account. Never publish a placeholder seller ID.
- The Cloudflare Worker CSP permits the standard Google advertising script and ad-serving domains, but no ads are loaded until a real publisher ID is configured.


## Engineering architecture

See [../docs/ENGINEERING-ARCHITECTURE.md](../docs/ENGINEERING-ARCHITECTURE.md) for platform boundaries, dependency direction, security invariants, refactor boundaries, and deployment verification.


## Private Discord registration invitations

The restricted admin portal has a Registration links panel backed by the registration_invites table in Supabase. It issues one-time, expiring links for the dedicated /register page. Tokens are generated with Web Crypto, only their SHA-256 digests are stored, and the Supabase Auth Discord identity is checked server-side before atomic redemption. See ADMIN-SETUP.md for the required provider, redirect allow-list, migration, and deployment configuration.
