# Send.ai control plane setup

## Public and private routes
- Public homepage: `/`
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

## Database bootstrap
Run `supabase/migrations/202611150001_sendai_control_plane.sql` in the Supabase SQL Editor. Create the first admin in Supabase Auth, obtain that user's UUID, and run the owner bootstrap SQL at the bottom of the migration. Never add an admin email or password to source code.

## Current admin capabilities
- Supabase Auth password sign-in; bearer token remains in page memory and is not written to local storage.
- Server-side active-admin and role check on each API request.
- Overview, integration configuration status, public GitHub release listing, feature-flag reads and audited feature-flag writes, audit-log reads.
- Control-plane tables have RLS enabled and no privileges for anon/authenticated roles. Only the server-side secret key can access them.

## Not yet activated
This is a protected control-plane foundation, not a fully deployed SaaS backend. The CenAI API, billing, provider vault, Guardian ingestion, rollout workers, webhook receivers, MFA enrollment/enforcement, and mobile admin tooling are not implemented by this static panel. Do not enable paid plans or remote device control until those services and their security tests are deployed. Configure MFA for admin accounts before production use; current login is password-based and should be treated as a restricted beta.
