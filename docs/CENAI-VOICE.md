# CenAI Voice

CenAI Voice is an authenticated, cloud-hosted speech synthesis feature. The Android client contains no speech model weights and does not call Google TTS or an Android system TTS fallback.

## Current model

- Provider: Cloudflare Workers AI.
- Model: `@cf/deepgram/aura-1`.
- Product profile: `cenai-en-v1`.
- Speaker preset: `angus`.
- Validated language exposed by this release: English only.
- Output: linear PCM in a WAV container for Android playback.
- The model and inference run on Cloudflare; no GPU host or Qwen weights are required.

Do not describe this profile as a custom-trained CenAI voice. It is CenAI's product profile using the Aura-1 speaker preset. Add other languages or speaker presets only after pronunciation, intelligibility, naturalness, and playback validation.

## Request and privacy flow

```text
CenAI Android (Rust)
  -> HTTPS + signed-in Supabase access token
  -> CenAI Cloudflare Worker /api/voice/speech
       -> verifies the Supabase user
       -> reserves account and global daily quota through cenai-ai-quota
       -> Cloudflare Workers AI @cf/deepgram/aura-1
       -> WAV response to Android
       -> records reservation outcome
```

- The Worker derives the immutable user UUID from Supabase Auth; clients cannot supply it.
- Speech text is sent to Cloudflare for synthesis. Do not log request text or store returned audio.
- Requests are limited to 1,000 characters and English only.
- Each account has a 1,000-character daily limit.
- The shared CenAI Workers AI gateway reserves at most 8,000 Neurons per UTC day across managed chat and voice, leaving 2,000 Neurons as a safety buffer against the 10,000-Neuron Free allocation. Aura-1 has a 4,000-Neuron sub-allocation and each account receives up to 1,000 characters per UTC day.
- Quota is reserved before inference and is not refunded on provider failure, because a failed upstream request may still consume compute.
- Cloudflare Workers AI pricing lists Aura-1 at 1,363.64 Neurons per 1,000 input characters. Usage is character-based, not generated audio minutes.
- Cloudflare's 10,000 free Neurons reset at 00:00 UTC. Further usage can fail on the Free plan.

## Required rollout

1. Apply `supabase/migrations/20261122100000_cenai_aura1_voice_quotas.sql` to the linked Supabase project.
2. Deploy the updated `cenai-ai-quota` Supabase Edge Function.
3. Deploy the updated Worker from `website/` with the existing `AI` binding in `website/wrangler.jsonc`.
4. After authenticated synthesis, quota enforcement, WAV format, and Android playback tests pass, an administrator may enable the `voice.cloud.enabled` feature flag.
5. Verify daily per-account and global usage against the Cloudflare dashboard before public rollout.

No new TTS API key or GPU-host secret is required. Do not enable the feature flag before the migration, Edge Function, and Worker are deployed and tested.

## Acceptance tests

- Signed-out and anonymous accounts are rejected.
- Registration and MFA policies are enforced by the quota Edge Function.
- Non-English requests, empty text, control characters, and requests over 1,000 characters are rejected.
- Per-user daily characters and global daily Neuron reservations are atomic under concurrent requests.
- Requests are blocked when either quota is exhausted.
- The Worker rejects invalid audio content types and oversized audio.
- Rust validates RIFF/WAVE headers and rejects stale account responses.
- Android playback, stop/cancel, account switching, network loss, and service-unavailable behavior are verified on API 35 emulator and a physical REDMI Pad 2 Pro.

## Status

- [x] Rust authenticated cloud WAV client and account-switch protection.
- [x] Cloudflare Workers AI binding configured in Wrangler.
- [x] Aura-1 Worker integration with English-only validation and audio response checks.
- [x] Per-user voice allowance, 4,000-Neuron voice sub-allocation, and shared 8,000-Neuron budget across managed chat and voice.
- [x] Removed Qwen/GPU-host dependency from the active Worker voice route.
- [ ] Apply both Supabase quota migrations and deploy the updated Edge Function.
- [ ] Deploy the Worker and test real Aura-1 WAV output.
- [ ] Validate voice playback and cancellation on emulator and physical Android hardware.
- [ ] Add a live in-app usage meter based on server-reported quota.
- [ ] Enable the feature flag only after end-to-end verification.