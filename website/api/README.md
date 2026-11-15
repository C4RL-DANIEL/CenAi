# CenAI website and API

- `GET /api/health`: reports public site status and safe integration readiness flags. Secret values are never returned.
- `GET /api/releases`: reads published GitHub releases and returns sanitized release metadata.
- `GET /api/v1/models`: public API model list; requires a `cai_live_` API key and the `api.public.enabled` feature flag.
- `GET /api/v1/usage`: 30-day aggregate usage for the API key owner; requires the `usage:read` scope.
- `POST /api/admin`: authenticated admin proxy to the Supabase control plane.
- `POST /api/email-otp`: Gmail SMTP account email verification.

## Supabase services
- `cenai-control-plane`: administrator RBAC, audit logs, beta review, feature flags, model registry, API key creation/revocation and usage analytics.
- `cenai-public-api`: API-key-authenticated model and usage read endpoints. It is disabled by default.
- `cenai-billing`: Stripe Checkout and signature-verified webhook handler. It returns a configuration error until required secrets and price IDs are set.
- All private tables use RLS and deny `anon` and `authenticated` table access. Server-side functions use the service role only.

## Required Vercel environment variables
- `SUPABASE_URL`, `SUPABASE_PUBLISHABLE_KEY`
- `EMAIL_USER`, `EMAIL_PASS`, `EMAIL_OTP_SECRET` for Gmail verification
- `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `STRIPE_PRICE_SUPPORTER`, `STRIPE_PRICE_PRO`, `CENAI_PUBLIC_URL` to activate Stripe billing

Never put service-role keys, Stripe secrets, Gmail app passwords, or HMAC secrets in public variables, static files, GitHub source, or Android builds. Do not enable paid plans until checkout and webhook tests pass.
