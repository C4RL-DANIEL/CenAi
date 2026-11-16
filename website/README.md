# CenAI website

CenAI public website and restricted operations portal.

## Hosting

The Cloudflare Pages project `cenai-website` builds from this directory:
- Build command: `npm run build`
- Output directory: `dist`
- Runtime: Cloudflare Pages advanced-mode Worker at `dist/_worker.js`
- Production site: `https://cenai-website.pages.dev` (custom domain can be attached after DNS ownership is confirmed)

The build script copies public website files into `dist`, excludes server source under `api/`, and packages the Cloudflare Worker as `_worker.js`. The Worker owns all `/api/*` requests and never serves source files from the API directory.

## Local validation

Run `npm run build` then inspect `dist/`. Run `npm run check` to syntax-check the Worker and build script.

## Routes

- `/`: public homepage.
- `/admin`: restricted administrator control center.
- `/beta`: beta application and feedback.
- `/api/admin` and `/api/beta`: authenticated Supabase control-plane proxy.
- `/api/email-otp`: authenticated email verification; Gmail SMTP credentials are server-side Cloudflare secrets.
- `/api/health`: non-secret integration configuration status.
- Other product services (public API, billing, PayPal billing and telemetry) remain in Supabase Edge Functions and must be proxied through the Worker or called from the app with their existing authentication.

## Required Cloudflare runtime variables

- `SUPABASE_URL`
- `SUPABASE_PUBLISHABLE_KEY`
- `CENAI_PUBLIC_URL`
- `EMAIL_USER`, `EMAIL_PASS`, `EMAIL_OTP_SECRET` for Gmail OTP

Do not commit credentials. Add OTP variables as encrypted production and preview environment secrets in Cloudflare Pages settings.

## Backend status

The admin and beta workflows use the Supabase `cenai-control-plane` function. Supabase remains the PostgreSQL/authentication system of record during migration to preserve accounts and data. Billing, PayPal, telemetry and public API are separately deployed Supabase Edge Functions; their secrets and webhook configuration remain server-side.
