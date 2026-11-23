# CenAI Ecosystem SDK

The version 1 TypeScript client is a dependency-free adapter for external apps that have a **user-approved CenAI app registration and scope grant**. It currently exposes the supported per-user context capabilities: `context.read`, `context.write`, and `context.delete`.

## Security model

- The app must be registered in CenAI with a trusted publisher and a manifest that declares the requested scopes.
- The signed-in user must explicitly grant those scopes in CenAI. Revoking the grant blocks later calls.
- The app obtains the user's short-lived Supabase access token through its own authenticated session and supplies it through `getAccessToken`.
- Never embed a Supabase service-role key, private signing key, or shared app secret in an app.
- The server derives the authenticated user from the bearer token and checks that identity, the registered app, the current grant, request freshness, capability and payload bounds. `userScope` is only a consistency field; it is not an authorization credential.
- Each request has a random 256-bit ID and a five-minute freshness window. The SDK never retries a side-effecting request automatically.

## Usage

```js
import { CenAiEcosystemClient } from "@cenai/ecosystem-client";

const ecosystem = new CenAiEcosystemClient({
  supabaseUrl: "https://YOUR_PROJECT.supabase.co",
  publishableKey: "YOUR_SUPABASE_PUBLISHABLE_KEY",
  appId: "com.example.notes",
  userId: currentUser.id,
  getAccessToken: async () => (await sup¶»§q«^