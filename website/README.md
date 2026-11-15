# CenAI website

Static, dependency-free product and developer portal preview.

## Local preview
Serve this directory with any static HTTP server, or deploy it as a static site with the Vercel project root set to `website`.

## Pages
- `index.html`: product overview, platform, security principles, API overview, pricing philosophy, and Send.ai control-plane preview.
- `security.html`: security requirements and threat model priorities.
- `api.html`: proposed API contract (not live endpoints).
- `roadmap.html`: roadmap and confirmed product decisions.

## Important
This is a frontend foundation only. The Send.ai dashboard, authentication, API key lifecycle, billing, analytics, release controls, model registry, and Guardian ingestion require a secured backend and configured production services. The UI deliberately labels preview values and does not fabricate live metrics or credentials.
