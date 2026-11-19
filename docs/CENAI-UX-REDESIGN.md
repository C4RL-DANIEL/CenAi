# CenAI unified UX redesign

Status: design and copy specification for the existing product. This document does not claim that roadmap items are implemented.

## Product direction

CenAI is a personal AI workspace. The Android app is the daily work surface; the website is the account, discovery, support and membership surface; Send.ai Control is a separate restricted operations console.

Use one visual language across all three:
- Warm neutral canvas, high-contrast ink, one restrained CenAI accent, semantic success/warning/error colors.
- Typography with clear hierarchy; avoid oversized gradients, glass effects on every surface, decorative metric cards and excessive rounded pills.
- 8 px spacing rhythm, consistent 10–14 px control radii, clear focus rings, comfortable touch targets and visible keyboard focus.
- Prefer clear page titles, one-sentence descriptions, one primary action, then grouped supporting actions.
- Every async page needs loading, empty, success, recoverable error and permission-denied states.
- Never display fabricated metrics, simulated connection status, fake activity, or success before the server/provider confirms it.
- Keep user content, admin operations and security-critical actions visually distinct.
- Respect reduced motion, screen readers, text scaling, keyboard navigation and color contrast.

## Roles and control boundaries

### Normal user account
Users may manage only their own profile, authentication methods, sessions, preferences, devices, data, membership and payment history. They may:
- Change display name; view their verified email; request password recovery; enroll or remove their own MFA.
- Review their own plan, subscription state, receipts/payment attempts and refund requests; start or cancel a subscription only where the provider lifecycle supports it.
- Configure personal provider connections, model defaults, local/offline models, voice, theme, privacy and telemetry consent.
- Export or delete their own data through an explicit, confirmed workflow.
- See their own usage and quota with reset dates and explanations.
They must never see member search, other users' details, global feature flags, provider secrets, admin API keys, audit events, release controls, global quotas or CMS publishing.

### Administrator / owner
Admin controls belong only in the private Send.ai Control route and must be enforced by server-side role checks on every request. Current backend roles are owner/admin; do not invent client-side role grants.
- Owner: global configuration and privileged operations supported by the backend.
- Admin: only operations explicitly authorized by the backend role policy.
- Both: visible audit trail for consequential changes, confirmation for irreversible actions, MFA gate, session expiry and clear failure messages.
- Admin pages may show aggregate metrics and account metadata needed for operations, never user passwords, API keys, OAuth tokens, private prompts or private conversation bodies.
- A role label in the browser is presentation only. The control-plane function must authorize the action independently.

## Shared navigation and page structure

### Public website header
CenAI logo | Product | Guides | Security | Downloads | Account | Join beta
On narrow screens collapse to a labelled menu. Keep Account visible as a clear action.

### Authenticated account navigation
Overview | Profile | Security | Membership
A signed-in user's account area must not contain administrative tabs. Use a compact left rail on desktop and a horizontally scrollable tab row on narrow screens.

### Android app navigation
Primary destinations: Chat | Projects | Library | Agents | Settings
Use a bottom bar on compact widths; use a navigation rail on tablets/expanded windows. Keep History accessible from Chat's conversation drawer/list, not as a competing primary destination. The project currently has chat/history/settings flows; Projects and Library should appear only when those destinations are implemented, not as dead links.
On tablets, use a three-pane pattern when useful: conversation list, active conversation, optional context/artifact pane. On phones, keep the conversation full-screen and open secondary controls as sheets or focused screens.

### Send.ai Control navigation
Overview | People | Product controls | AI & models | Usage | Website | Billing | Beta & feedback | Audit
Map these labels to the existing supported tabs; do not remove existing operations. Keep the navigation in a persistent left sidebar on wide screens and a compact horizontal/overflow menu on small screens.

## Website page copy and layout

### Home (/)
Title: "Your AI workspace, in one place."
Description: "Bring supported models, projects and permission-based agents together. Keep your work organized and your choices in your hands."
Primary action: "Get started"
Secondary action: "Explore the workspace"
Sections:
1. "One place for the work" — "Move between conversations, models and project context without losing your flow."
2. "Choose how AI works" — "Connect supported providers, select a model for the task, or use a compatible custom endpoint."
3. "Agents with clear boundaries" — "Review what an agent plans to do. Approve sensitive actions before anything changes."
4. "Keep useful context close" — "Organize conversations and reference selected files when you need them."
5. "Online or offline" — "Use supported cloud providers or import a compatible local model for offline text chat."
6. "Your settings stay yours" — "Control appearance, voice, privacy, usage and connected providers."
CTA: "Make space for better work." Button: "Join the CenAI beta"
Do not promise unshipped Projects, marketplace, browser automation or cloud model routing as live features. Label previews as "Planned" or "In development".

### Account (/account)
Page title: "Your CenAI account"
Description: "Manage your profile, sign-in security and membership."
Overview copy: "Your account at a glance"; show verified email, current membership and security status with timestamps.
Profile: "Choose how your name appears across CenAI." Fields: Display name, Email address (read-only unless verified change flow exists). Button: "Save profile".
Security: "Protect access to your account." Actions: "Send password reset email", "Set up authenticator", "Verify authenticator", "Remove authenticator", "Sign out of this device". Explain recovery and MFA status plainly.
Membership: "Review your plan and payment activity." Actions: "View plans", "Manage membership", "View payment details", "Request a refund". Refund copy: "Submitting a request does not issue a refund. CenAI will show the request status after it is reviewed."
States: "Checking your account…", "No membership is active", "Your email is verified", "Check your inbox for the verification link", "We couldn't load this section. Try again."

### Billing (/billing)
Title: "Choose the workspace access that fits."
Description: "Review the price, billing interval and renewal terms before continuing."
Plan cards must show price and currency, billing period, included entitlements, renewal/cancellation terms and provider. Never imply recurring billing is enabled if the server feature flag is off.
CTA: "Continue with PayPal" only when the plan and provider are available; otherwise "Currently unavailable".
Checkout failure: "PayPal did not confirm this payment. You have not been charged by CenAI based on this response. Check your PayPal activity before trying again."
Success page: "Payment confirmed" only after verified server-side confirmation; otherwise show "We're confirming your payment" and poll/retry safely.
Cancel page: "Checkout was cancelled. No new subscription was created by this attempt."

### Beta (/beta)
Title: "Help shape the next version of CenAI."
Description: "Apply for early access and share the workflows you want to improve."
Fields: Email address, Display name, What would you like to try? Button: "Submit application".
After submit: "Your application was received. We'll email you when its status changes." Never claim approval immediately.

### Downloads and updates (/updates.html)
Title: "CenAI releases"
Description: "Review the latest version, supported devices and release notes before installing."
Show version, release date, supported ABI, file size, SHA-256 checksum, signing status, known issues and minimum Android version only when verified from release metadata.
Actions: "Download APK", "View release notes", "Verify checksum".
Explain Android installation confirmation and same-signing-key requirement.

### Security (/security.html)
Title: "Security is part of the product."
Description: "Understand how CenAI handles credentials, permissions, data and connected services."
Sections: Device-side credential storage, cloud account boundaries, agent approvals, telemetry consent, account recovery, vulnerability reporting. Distinguish verified controls from planned controls.

### API (/api.html)
Title: "Build with the CenAI API."
Description: "Explore supported endpoints, authentication, quotas and integration status."
Show only live endpoints and scopes; include request examples, rate limits, error format, versioning and key-revocation instructions. Do not expose internal service credentials.

### Guides (/guides/)
Title: "Practical guides for better AI work."
Description: "Clear, task-focused help for models, projects, privacy and safe agents."
Guide cards: title, one-sentence outcome, reading time, last-reviewed date.

### Roadmap (/roadmap.html)
Title: "Where CenAI is going."
Description: "See what's available, what's being tested and what's planned."
Use three explicit statuses: Available, In development, Planned. Do not describe roadmap items as shipped.

### About (/about.html)
Title: "A workspace built around your choices."
Description: "CenAI brings supported AI providers and practical tools into one place, with user control at the center."
Include mission, product principles and current platform scope.

### Privacy (/privacy.html)
Title: "Your data. Clear choices."
Description: "Understand what CenAI stores, what stays on your device and how to manage your information."
Include data categories, purposes, retention, telemetry opt-in, processors, export/deletion and contact route. Keep the legal policy reviewed and accurate.

### Terms (/terms.html)
Title: "Terms of service"
Intro: "These terms explain the rules for using CenAI, memberships and connected services."
Use clear headings, effective date, service limitations, acceptable use, billing, cancellation, third-party terms and contact details. Legal review required.

### Contact (/contact.html)
Title: "How can we help?"
Description: "Tell us what happened and include the page, app version and steps to reproduce the issue. Never send passwords, API keys or verification codes."
Provide support categories and expected response expectations only if defined.

### Password recovery (/reset-password.html)
Title: "Choose a new password"
Description: "Use the secure link sent to your email. This link expires and can only be used for your account."
States: "Checking recovery link…", "Link expired or invalid", "Password updated. Sign in again."

### Billing result pages
Success: "Payment confirmation in progress" until webhook/provider verification is complete; then "Your payment is confirmed."
Cancel: "Checkout cancelled" with a link back to membership options.
Never infer payment success from a browser redirect alone.

## Send.ai Control page copy

### Sign-in and MFA
Sign-in title: "Administrator sign in"
Description: "Use an authorized administrator account. Every privileged action is checked by the server."
MFA title: "Confirm it's you"
Description: "Enter the code from your authenticator. If you haven't enrolled one, use the approved recovery method."
Buttons: "Verify and continue", "Use email verification instead", "Cancel and sign out".
Never expose setup secrets after enrollment is complete. Never make an admin MFA bypass available to normal users.

### Overview
Title: "Operations overview"
Description: "A live view of the services that keep CenAI running."
Groups: Platform health, authentication, email delivery, billing, AI gateway, database, release pipeline, recent incidents.
Show "Connected", "Degraded", "Not configured" or "Unknown" only from a real health check. Each metric has a time window and source.

### People & memberships
Title: "People and memberships"
Description: "Find an account and review its access, verification and payment-backed membership state."
Search placeholder: "Search by email or display name"
Actions: "View account", "Suspend sign-in", "Restore sign-in", "Review membership". Require a reason and confirmation for suspension. Never show passwords or provider secrets.

### Feature flags
Title: "Product controls"
Description: "Enable or pause supported capabilities. Changes are logged and take effect according to the server configuration."
Each row: flag name, purpose, current state, scope, last changed by and timestamp. Actions: "Enable", "Pause", "Save changes". Show dependency warnings; do not enable recurring billing until its complete lifecycle is verified.

### Audit log
Title: "Administrator activity"
Description: "Review who changed what, when and through which control-plane action."
Filters: Actor, action, target, date range, result. Detail view must redact secrets and private content.

### Integrations
Title: "Connected services"
Description: "Check whether required providers are configured. Secret values are never displayed here."
Rows: Supabase, Cloudflare, PayPal, email delivery, GitHub releases, AI providers. Actions should be diagnostics only unless an explicit supported configuration workflow exists.

### Release center
Title: "Release management"
Description: "Review published builds and rollout readiness before changing availability."
Show version, channel, commit, build checks, signature/checksum verification, rollout percentage and rollback readiness only when supported by backend data.
Actions: "View release", "Pause rollout", "Promote release", "Roll back" only when the corresponding backend operation exists and requires confirmation.

### Model registry
Title: "Model catalog"
Description: "Manage models that CenAI exposes through its managed API."
Fields: Provider, provider model ID, public name, description, availability, public listing. Warn before disabling a model used by active integrations.

### API keys
Title: "API access keys"
Description: "Create and revoke keys for external integrations. A new key is shown once; copy it now and store it securely."
Buttons: "Create key", "Copy key", "Revoke key". Never retrieve or display an existing raw key.

### Analytics
Title: "Usage and reliability"
Description: "Understand aggregate usage and service health without collecting private prompts or responses."
Show requests, success rate, latency percentiles, provider/model distribution and failure categories with selected date range and clear sample size. Do not fabricate missing data.

### Website editor
Title: "Website content"
Description: "Edit supported page text and metadata. Preview changes before publishing."
Actions: "Save draft", "Preview", "Publish", "Unpublish". Show draft/published state and last editor. Escape content and restrict allowed fields; do not allow arbitrary script or HTML injection.

### API quotas
Title: "API usage policies"
Description: "Set request and token limits for managed API accounts. These policies do not meter direct provider calls made from a user's device."
Fields: Account UUID, daily request limit, monthly token limit, enforcement state. Show current usage and reset window where available.

### Billing and refunds
Title: "Plans and payments"
Description: "Manage customer-facing PayPal plans and review refund requests."
Separate plan configuration from refund review. Before a refund: show customer, payment, amount, currency, reason, refund state and provider confirmation. Buttons: "Approve request", "Reject request", "Issue refund". Issuing a refund is irreversible and requires a final confirmation. Show provider-confirmed result only; never mark refunded after an error.

### Beta and feedback
Title: "Early access"
Description: "Review beta applications and product feedback."
Filters: Pending, Approved, Rejected, Revoked, All. Actions: "Approve access", "Reject application", "Revoke access". Require reason for rejection/revocation and log every decision.

## Android screen copy and layout

### Chat
Title: "What would you like to work on?"
Subtitle: "Choose a model, add context, and start anywhere."
Composer placeholder: "Message CenAI…"
Composer actions: Attach, Voice, Model, Send. Make the selected provider/model visible without consuming the whole composer.
Empty-state prompts: "Explain a difficult topic", "Summarize a document", "Help me plan a project", "Review this code".
Show streaming state as "Generating…" with a clear Stop action. If a request fails, preserve the draft and offer "Retry" and "Edit message".
Message actions: Copy, Regenerate, Edit and resend, Read aloud, Share. Show source citations where web/research tools are actually used.

### Conversation history
Title: "Your conversations"
Description: "Find, search and continue previous work."
Search placeholder: "Search conversations"
Empty state: "Your conversations will appear here after your first chat."
Actions: New chat, Rename, Delete, Export. Confirm deletion and explain whether it is recoverable.

### Projects (when implemented)
Title: "Projects"
Description: "Keep conversations, files and instructions together around a goal."
Actions: New project, Add files, Set project instructions, Move conversation. Do not expose a dead navigation destination before its backend/local persistence exists.

### Library / Knowledge (when implemented)
Title: "Knowledge library"
Description: "Manage the files and notes you choose to make available as context."
Show file type, size, source, indexing state and last updated. Actions: Add files, Search, Remove. Explain that removing a file from the library does not necessarily delete the original file from device storage.

### Agents
Title: "Agents"
Description: "Give CenAI a task, review its plan and approve actions that affect your device or accounts."
Sections: Available tools, Active tasks, Waiting for approval, Completed tasks, History.
Approval card: action, target, data to be shared or changed, reversibility, requested permissions. Buttons: "Approve once", "Edit request", "Reject", "Stop task".
Keep read, prepare and act levels separate. Never imply unrestricted or unattended device control.

### Settings
Use a full-screen settings destination with category list and focused detail content; avoid a tall, crowded modal.
- Appearance: Theme, color style, text size, motion, chat density.
- AI & models: Provider, model, custom endpoint, connection test, system instructions, output limit, provider-specific credentials.
- Voice: Input language, output language, speech rate, pitch, voice test, read responses aloud.
- Agents & permissions: Planning mode, tool allowlist, approval preferences, active tasks, permission history.
- Offline Brain: Installed model, import model, storage use, CPU/resource warning, local-only mode, remove model.
- Knowledge: Local retrieval, indexed sources, clear local index.
- Privacy & data: Local memory, cloud analytics consent, Guardian cloud consent, export data, delete data.
- Device & diagnostics: Guardian snapshot, app monitor, recent errors, diagnostic export. Make clear which data stays local and what is sent only with consent.
- Updates: Current version, channel, check for updates, release notes, checksum/signature result.
- Account: Signed-in email, membership link, security and sign out.
Use short explanatory text under each setting; don't expose admin-only controls in user settings.

### Guardian / app monitor
Title: "Device health"
Description: "A local view of CenAI performance and device resources."
Show RAM, app heap, storage, battery, network, uptime, last crash category and sample time. Explain that Android schedules background checks on a best-effort basis. Actions: "Refresh", "Export diagnostics", "Clear local history".

### Update flow
Title: "CenAI is up to date" or "An update is available"
Show current version, available version, release notes, checksum status and signature compatibility. Button: "Download update". Explain that Android asks the user to approve installation.

### Offline Brain
Title: "Run AI on this device"
Description: "Import a compatible LiteRT-LM model to use text chat without a network connection."
Show model compatibility, download/import source, file size, storage required, estimated resource requirements and license reminder. Never claim a model is installed until verified.

## Additional product features to prioritize

1. Projects and conversation organization: project folders, project instructions, pinned chats, tags and conversation branching.
2. Knowledge Vault: user-selected documents, local indexing, source citations, per-project access and one-tap removal.
3. Task-aware Model Hub: compare configured models by capability, context limit, local/cloud, cost metadata when verified, and connection status; allow manual choice and transparent routing.
4. Agent task center: task timeline, resumable state, approval inbox, stop control, bounded retries and action audit history.
5. Research mode: web sources, citations, source list, freshness and exportable research brief. Do not show citations for sources not actually retrieved.
6. Creative Studio: structured writing, code, image-generation integrations only when a supported provider is configured, with output history.
7. Import/export: conversation JSON/Markdown, project archive and user data export with explicit scope.
8. Accessibility and language: English/Filipino interface copy, larger text, contrast, TalkBack labels, keyboard and tablet layouts.

## Delivery order

1. Implement shared design tokens and responsive navigation; preserve current routes and IDs used by scripts.
2. Refresh Send.ai Control layout and copy while retaining existing action wiring and backend checks.
3. Refresh account/billing/beta/public pages with consistent components and truthful states.
4. Split the Android settings modal into a responsive settings destination; improve chat/history/agent hierarchy.
5. Add new capabilities only with persistence, server authorization, error handling, tests and documentation.
6. Validate mobile, tablet, dark/light, screen reader, reduced motion, keyboard, offline, and low-memory behavior.

## Acceptance checklist

- No normal-user route renders admin tabs or privileged API actions.
- Server rejects unauthorized admin operations regardless of browser UI.
- Existing admin action IDs and API contracts continue to work.
- No fake metrics, false payment success, false release readiness or unsupported feature claims.
- Every interactive control has a real action or is clearly disabled with an explanation.
- Every page has loading, empty, error, success and denied states where applicable.
- No secrets, user prompts or private response bodies are exposed in admin analytics or logs.
- Website routes remain compatible with the Cloudflare Worker static asset allowlist.
- Android uses adaptive layouts for phones/tablets and preserves current Kotlin/Compose + Rust JNI architecture.
- GitHub Actions tests and builds pass before any release or deployment.
