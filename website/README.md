# CenAI website

CenAI public website and restricted operations portal. The public pages are static; serverless API routes connect the beta and administrator workflows to Supabase.

## Local preview
Serve this directory with any static HTTP server, or deploy it as a static site with the Vercel project root set to `website`.

## Pages
- `index.html`: public product overview, platform, security principles, API overview and plans.
- `admin/index.html`: restricted administrator sign-in and control center. Public registration is disabled; the pre-provisioned owner can sign in using the `CenAI` username alias or owner email, request password recovery, and change the password from an authenticated session.
- `api/admin.js` and `api/beta.js`: server-side proxies to the Supabase control-plane function. Bearer tokens are forwarded after proper header parsing.
- `api/email-otp.js`: signed-in account email verification through Gmail SMTP using server-only Vercel environment variables.
- `api/health.js`: non-secret integration configuration status, including email OTP readiness.
- `beta.html`: public beta registration, email verification and feedback.
- `security.html`: security requirements and threat model priorities.
- `api.html`: proposed API contract (not live endpoints).
- `roadmap.html`: roadmap and confirmed product decisions.

## Remaining backend work
The administrator and beta workflows are connected to Supabase. Billing, analytics, API key lifecycle, release signing and staged rollout, public developer API, model registry, and Guardian ingestion remain unimplemented or disabled until their dedicated backend services are built. The control panel labels these honestly rather than fabricating live metrics or credentials.
