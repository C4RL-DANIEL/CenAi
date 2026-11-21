# Send.ai control plane setup

## Public and private routes
- Public homepage: `/`
- Subscription checkout: `/billing` (requires a signed-in CenAI account and the global billing launch flag)
- Admin login: `/admin` (not linked from public navigation; obscurity is not authorization)
- Admin API: `/api/admin` (POST only; validates the Supabase Auth bearer token and checks active admin membership on every request)

## Required Vercel environment variables
Set these for Production and Preview only as appropriate:
- `SUPABASE_URL`: project URL.
- `SUPABASE_PUBLISHABLE_KEY`: Supabase publishable key (public identifier; not a secret).
- `EMAIL_USER`: Gmail account used as the CenAI verification sender.
- `EMAIL_PASS`: Gmail App Password for that sender (not the normal account password).
- `EMAIL_OTP_SECRET`: long random server-only HMAC secret used to hash OTP challenges.
- `GITHUB_TOKEN`: optional least-privilege GitHub token for private/release write operations; current panel only reads public releases.
- `CENAI_API_BASE_URL`: only after the API service is deployed.
- `AI_GATEWAY_API_KEY`: only after the AI gateway is provisioned.
- `BILLING_SECRET_KEY`: only after a billing provider account and webhook verification are configured.

Do not put secret keys in `NEXT_PUBLIC_*`, static files, GitHub source, or Android builds. Rotate any secret that has ever been committed.

## Supabase Edge Function secrets for PayPal Live billing
Set these in Supabase Edge Function Secrets (not Vercel):
- `PAYPAL_CLIENT_ID`
- `PAYPAL_CLIENT_SECRET`
- `PAYPAL_WEBHOOK_ID`
- `CENAI_PUBLIC_URL` (for example `https://cenai-website.vercel.app`)

Create the PayPal webhook against `https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-paypal-billing`. Subscribe to the billing subscription lifecycle events (created, activated, updated, cancelled, suspended, expired, payment failed) and the supported payment sale completion/refund/reversal events. The function verifies each signature and deduplicates event IDs.

The admin Billing Plans tab creates a PayPal product and recurring plan after an authorized owner/admin signs in. The configured `live_test` price of PHP 1.00 is a REAL recurring charge on PayPal Live, not a PayPal Sandbox test. Do not expose it to customers until the amount, renewal interval, cancellation flow, webhook delivery and refund handling have been checked.

The deployed billing function validates Stripe webhook signatures and deduplicates event IDs. Billing remains inactive until checkout and webhook tests pass.

## Database bootstrap
Run `supabase/migrations/202611150001_sendai_control_plane.sql` in the Supabase SQL Editor. Create the first admin in Supabase Auth, obtain that user's UUID, and run the owner bootstrap SQL at the bottom of the migration. Never add an admin email or password to source code.

## Current admin capabilities
- Supabase Auth password sign-in; bearer token remains in page memory and is not written to local storage.
- Server-side active-admin and role check on each API request.
- Overview, integration configuration status, public GitHub release listing, feature-flag reads and audited feature-flag writes, audit-log reads.
- Model registry management with provider/model metadata, visibility and enablement controls.
- API key creation and revocation. Raw keys are returned only once; SHA-256 hashes are stored in the database.
- Privacy-minimized usage analytics aggregated from server-side usage events.
- Supabase Edge Functions for API-key authenticated model listing and usage reporting.
- Control-plane tables have RLS enabled and no privileges for anon/authenticated roles. Only the server-side secret key can access them.

## Not yet activated
This is a protected control-plane foundation, not a fully deployed SaaS backend. The public API currently supports model listing and usage reporting only; chat completions, quotas and full SDK coverage are not implemented. PayPal Live checkout and webhook code is deployed, but the public billing flag remains OFF until a product/plan is created through the authorized admin UI and signed webhook, approval, activation, cancellation and refund paths are verified. Guardian device diagnostics currently run locally on Android; remote ingestion is not enabled. Staged rollout workers, MFA enforcement and mobile admin tooling are not implemented. Do not enable paid plans or remote device control until those services and their security tests are deployed. Configure MFA for admin accounts before production use; current login is password-based and should be treated as a restricted beta.

## Discord-only invitation registration

The owner/admin portal now includes **Registration links**. An authorized owner or admin can generate a random, single-use link with a 1-hour to 7-day expiry, copy it once, review redemption status, and revoke unused invitations. The browser never receives stored token hashes; the raw token is returned only at creation. The database redemption function locks the invitation row so concurrent redemption attempts cannot both succeed. Redemption requires a verified Supabase Discord identity and sets app_metadata.cenai_registration_verified=true and app_metadata.cenai_registration_method=discord. It never grants an admin role. After redemption, the user sets a 12-character-minimum app password so the native Android sign-in can use the verified Discord email. The registration policy cutover timestamp is stored in registration_policy; accounts created before it are grandfathered, while later accounts must have the Discord verification marker.

### Required activation steps

1. Apply supabase/migrations/20261121100000_discord_registration_invites.sql to the production Supabase project after the rewrite is approved for release.
2. In Supabase Dashboard → Authentication → Sign In / Providers → Discord, enable Discord and configure the Discord OAuth client ID and client secret.
3. In the Discord Developer Portal, add the Supabase callback URL https://bqkzehozezvepbthglyj.supabase.co/auth/v1/callback to the Discord application's OAuth2 redirect URLs.
4. In Supabase Dashboard → Authentication → URL Configuration, add https://cenai-website.cenai-app.workers.dev/register/complete to the redirect URL allow list. Keep the existing site URL and other redirect entries intact.
5. Deploy the updated cenai-control-plane Edge Function and Cloudflare Worker from the approved release branch.
6. Verify the full flow with a fresh invitation: OAuth via Discord, successful one-time redemption, replay rejection, expiry rejection, revoke rejection, and no admin role assignment.

The application callback uses OAuth authorization-code PKCE. Do not disable PKCE or put Discord client secrets in website assets, Worker variables exposed to the browser, Android builds, or Git. The Discord app's registered OAuth callback remains the Supabase /auth/v1/callback; /register/complete is the post-authentication redirect back to CenAI.

**Release gate:** The invitation feature is not live until the migration, Discord provider credentials, Supabase redirect allow list, Edge Function deployment, and Cloudflare Worker deployment are all confirmed. The rewrite branch must not be merged to main without the owner's explicit approval.
