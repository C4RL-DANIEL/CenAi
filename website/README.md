# CenAI website and Cloudflare Worker

CenAI public website and restricted administrator portal. The deployment is a single Cloudflare Worker with public static assets embedded into its module bundle and authenticated API routes handled at the edge.

## Hosting

- Worker: `cenai-website`
- Production URL: `https://cenai-website.aglibotcarldaniel.workers.dev`
- Source: this repository, `website/`
- Build: `npm run build`
- Deploy: `npm run deploy` (requires Wrangler authentication)
- Static assets are embedded into `dist/_worker.js`; no public server source directory is deployed.

## Routes

- `/`: public homepage.
- `/admin`: restricted administrator control center.
- `/beta`: beta application and feedback.
- `/api/admin` and `/api/beta`: authenticated Supabase control-plane proxy.
- `/api/admin-config`: public Supabase URL and publishable key.
- `/api/site-pages`: published CMS pages only.
- `/api/releases`: GitHub release feed.
- `/api/email-otp`: authenticated OTP delivery and verification.
- `/api/beta-notification`: administrator-reviewed beta email notifications.
- `/api/functions/:slug`: allowlisted Supabase Edge Function proxy, including PayPal billing, public API, billing, and telemetry.
- `/api/webhooks/paypal`: PayPal webhook proxy with signature headers preserved.
- `/api/health`: non-secret service and configuration status.

## Cloudflare runtime variables

Plain-text variables:
- `SUPABASE_URL`
- `SUPABASE_PUBLISHABLE_KEY`
- `CENAI_PUBLIC_URL`

Encrypted secrets required for email:
- `EMAIL_USER`
- `EMAIL_PASS` (Gmail app password)
- `EMAIL_OTP_SECRET` (random high-entropy HMAC secret)

Do not commit secrets. Configure them in Cloudflare Worker Settings > Variables and Secrets for production.

## Backend and data

Supabase remains the PostgreSQL and Auth system of record to preserve accounts and existing data. The control plane, PayPal billing, public API, billing, and telemetry remain deployed as Supabase Edge Functions and are routed through the Cloudflare Worker. No service-role key or payment secret is exposed to the browser or Worker bundle.

Email OTP challenge writes and verification now use the `cenai-email-otp` Edge Function with service-role-only database RPCs. The original user-callable OTP SECURITY DEFINER functions have had public/authenticated execution revoked.

## Local validation

Run `npm run check` and `npm run build`. The build excludes `api/`, internal setup docs, and Vercel configuration, then embeds only public assets in the Worker module.
