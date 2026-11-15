# CenAI website API functions

- `GET /api/health`: reports website availability and whether required Send.ai environment variable names are configured. It never returns secret values. A configured flag is not a connectivity check.
- `GET /api/releases`: reads published releases from the public CenAI GitHub repository and returns sanitized release metadata.

## Send.ai environment prerequisites
Configure these in Vercel only after the corresponding services exist:
- `CENAI_API_BASE_URL`
- `CENAI_DATABASE_URL`
- `CENAI_AUTH_SECRET`

Do not set placeholder values. The current health endpoint intentionally reports the control plane as not configured until all are present. Authentication, database connectivity, admin RBAC, and audit storage are not implemented by these two endpoints.
