# CenAI engineering architecture

## Current production boundaries

- Android client (app/): Kotlin + Jetpack Compose UI, provider orchestration, encrypted credentials, local chat history, local knowledge, speech, agent approvals, and optional local inference.
- Native core (core/): Rust JNI utilities. Android UI and provider networking must not depend on UI composables for business logic.
- Cloud edge (website/worker.js): Cloudflare Worker owns public HTTP routing, static-site delivery, authenticated proxying, OTP transport, AI quota-gated inference, health reporting, and scheduled billing cleanup.
- Website presentation (website/*.html, CSS, JS): static public pages and browser-side account, billing, and admin experiences. Browser code must never contain service-role keys, provider secrets, PayPal secrets, or signing credentials.
- System of record: Supabase Auth and Postgres remain authoritative for users, roles, billing entitlements, quota reservations, CMS content, and telemetry. Cloudflare is the public edge and deployment target, not a replacement database.
- External integrations: PayPal billing is mediated by the Supabase Edge Function and verified webhooks; email OTP is delivered by the Worker after a Supabase-backed challenge is created; GitHub Releases supplies app updates.

## Intended dependency direction

```text
Android Compose UI -> feature state / ViewModels -> repositories -> provider / Supabase / Worker APIs
Website pages -> API client -> Cloudflare Worker routes -> Supabase Edge Functions / Auth / Postgres
Cloudflare Worker -> Workers AI binding (only after quota reservation)
```

Keep UI state out of repositories, provider HTTP details out of composables, and authorization decisions in server-side Supabase functions. Client-side checks are presentation only; the backend must independently authorize every privileged operation.

## Refactor boundaries

### Android

- ui/components: stateless reusable presentation components.
- ui/screens: screen composition and navigation.
- features/chat: chat state, message actions, streaming lifecycle.
- data: persistence and repository implementations.
- domain: provider-neutral models and use cases.
- platform: Android permissions, speech, file access, WorkManager, Keystore, and JNI adapters.

The first extraction moves reusable chat/home composables out of the oversized screen file without changing their behavior or public app flows. Continue by moving screen state and request orchestration into a lifecycle-aware ViewModel, then split Settings, Chat, History, Agent, and Diagnostics into feature modules only after tests cover existing behavior.

### Website and edge

- website/pages: static page markup and page-specific presentation.
- website/assets: shared CSS, browser modules, and icons.
- website/worker: route handlers, auth-aware proxies, static asset delivery, scheduled jobs.
- supabase/functions: authoritative business operations and database access.
- supabase/migrations: versioned schema changes.

The current Worker uses an allowlisted GitHub raw-content origin for static files as a deployment bridge. The durable target is Cloudflare Workers Static Assets with Worker-first handling for API routes. Migrate only after the asset manifest and routing behavior are validated against production paths.

## Security invariants

1. No secrets or service-role keys in browser bundles, Android resources, Git history, or public health responses.
2. Every admin, billing, quota, and account mutation validates the user's session and role server-side.
3. Payment entitlements change only after verified provider-side events or trusted server-side capture.
4. Quota is reserved before inference and recorded after success or failure; client-supplied usage is never trusted.
5. Proxy routes are explicit allowlists with bounded request bodies and methods.
6. User-controlled content is escaped before HTML email or DOM insertion.
7. Local agent actions remain narrowly scoped and approval-gated.

## Deployment and verification

- Cloudflare only for website/edge production: GitHub Actions builds and deploys website/wrangler.jsonc to Worker cenai-website.
- Android: GitHub Actions runs JVM tests, emulator smoke tests, and APK builds. APK artifacts are delivered through GitHub Releases or the app updater, not Cloudflare.
- Supabase: schema migrations and Edge Functions are deployed independently; never assume a Worker deploy also migrates the database.
- A successful build is not a successful production deploy. Verify Worker health, public page routes, API method handling, authentication, billing test paths, and Android emulator tests separately.
- Keep Vercel out of the production deployment path.
