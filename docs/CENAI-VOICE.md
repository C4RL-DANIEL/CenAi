# CenAI Voice

CenAI Voice is CenAI's branded, cloud-hosted speech product. The Android app contains the product experience and authenticated API client; the Qwen3-TTS model, tokenizer, and fixed voice reference remain on CenAI's GPU inference host.

## Product identity and attribution

- Product: **CenAI Voice**
- First profile: **CenAI Original · English · en-US** (`cenai-en-v1`)
- Model foundation: `Qwen/Qwen3-TTS-12Hz-1.7B-Base`
- Upstream: https://github.com/QwenLM/Qwen3-TTS
- Model licence: Apache-2.0. Keep the required upstream notices. CenAI owns its app integration, service, profile names, pronunciation policy and evaluation process; CenAI does not claim authorship of Qwen's architecture or pretrained weights.

The fixed CenAI voice is created from a licensed voice-actor reference recording and its exact transcript. The reference is stored only on the private inference host. It is not bundled in the APK or sent by each Android user. A trained CenAI fine-tuned checkpoint is a later milestone; the current design uses Qwen Base voice-cloning conditioning, not a newly trained model.

## Cloud architecture

```text
CenAI Android (Rust)
  └─ HTTPS + user's Supabase access token
      └─ CenAI Cloudflare Worker /api/voice/speech
          ├─ validates the Supabase user session
          ├─ derives the immutable user UUID
          └─ authenticated server-to-server request
              └─ private GPU host /v1/audio/speech
                  ├─ Qwen3-TTS 1.7B Base + tokenizer
                  ├─ private CenAI voice reference prompt
                  └─ PCM16 WAV response → Android playback
```

- No Qwen model weights, voice recordings, or backend service secrets are shipped in the Android app.
- No Google TTS, Android system TTS, Gemini TTS, DashScope, or silent cloud-provider fallback is used.
- The user must be signed in. The Worker verifies the JWT with Supabase Auth and sends the resulting user UUID to the private inference host; the app cannot choose or spoof this identity.
- The Worker-to-GPU credential is a server secret and must never be placed in the app, website JavaScript, or repository.
- Speech text leaves the device for cloud synthesis. The Worker and inference host must not log speech text or store generated audio. This cloud processing must be clearly disclosed to users.
- Add Cloudflare rate limiting and per-account usage metering before public release.

## Languages and quality

The Qwen model catalogue documents ten languages: English, Chinese, Japanese, Korean, German, French, Russian, Portuguese, Spanish and Italian. English is the first release-quality target. Model language support is not proof of equal quality. Each additional language needs native-speaker pronunciation, intelligibility and naturalness tests. Filipino/Tagalog is not in the documented language list and must not be marketed as natively supported.

English quality gates:
1. WER measured with independent ASR as an evaluation tool only.
2. Native-listener intelligibility and naturalness ratings.
3. Pronunciation of names, abbreviations, numbers, dates, currencies, units, URLs and technical terms.
4. Stress, punctuation pauses, long-form stability and voice identity consistency.
5. First-audio latency, total generation time, concurrency and GPU memory.
6. Network interruption, retry, account-switch and playback-cancellation tests.

## GPU service deployment

The FastAPI implementation lives in `voice-server/`. It loads the Qwen model once at startup and caches a voice-clone prompt. A CUDA-capable Linux host and a persistent Hugging Face cache are required.

Required server environment:
- `CENAI_TTS_API_KEY`: random secret of at least 32 characters.
- `CENAI_VOICE_REFERENCE_AUDIO`: private path to the licensed WAV reference recording.
- `CENAI_VOICE_REFERENCE_TEXT`: exact transcript.
- Optional `CENAI_VOICE_MODEL_ID` (defaults to the 1.7B Base model), `CENAI_TTS_ATTENTION` (defaults to SDPA), and `CENAI_TTS_MAX_CONCURRENCY` (defaults to 1).

Build with `docker build -t cenai-voice:latest voice-server`. Run behind HTTPS ingress and mount the recording read-only. See [voice-server/README.md](../voice-server/README.md) for the deployment command and API contract.

Required Cloudflare Worker secrets:
- `CENAI_TTS_BACKEND_URL`: private GPU service HTTPS origin.
- `CENAI_TTS_BACKEND_TOKEN`: the same server-only random value as `CENAI_TTS_API_KEY`.

The Worker route intentionally returns HTTP 503 until these secrets are configured. Do not configure a public third-party TTS endpoint as a shortcut.

## Fine-tuning path

After the voice-actor dataset and written consent are secured, use Qwen's documented 12Hz Base single-speaker fine-tuning pipeline on a compatible cloud GPU. Keep train, validation and test recordings separate; preserve dataset provenance and licence records; compare checkpoints using the English quality gates above. Do not automatically promote a checkpoint when training completes. The deployed service should select the validated checkpoint by server-side configuration, independently of the Android APK.

## Implementation status

- [x] English-first CenAI Voice profile and ten-language metadata.
- [x] Rust client for authenticated cloud WAV synthesis with account-switch checks and a dedicated 240-second timeout.
- [x] Cloudflare Worker gateway with Supabase JWT validation, immutable user identity forwarding, payload limits and private backend token.
- [x] GPU FastAPI inference service with model caching, fixed voice-clone prompt, request validation and bounded concurrency.
- [x] Android read-aloud wired to the cloud gateway; no local model download or Google TTS synthesis path.
- [x] Removed the Qwen C++/GGML build from the APK workflow to keep the client small.
- [ ] Provide and approve a licensed English voice-actor reference recording and transcript.
- [ ] Provision the CUDA GPU host and persistent model cache.
- [ ] Configure Cloudflare Worker secrets and rate limits.
- [ ] Run end-to-end inference and audio playback tests on Android.
- [ ] Fine-tune and evaluate a dedicated CenAI English checkpoint.
- [ ] Evaluate and enable additional languages individually.
