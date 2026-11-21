# CenAI Voice cloud inference

This service hosts the Qwen3-TTS model on a CUDA GPU. Android contains only the CenAI client and playback path; no speech model weights or backend credentials are shipped in the APK.

## Voice identity

The initial server model is `Qwen/Qwen3-TTS-12Hz-1.7B-Base`. A fixed reference recording and its exact transcript are used to create the reusable `CenAI Original · English` voice-clone prompt at server startup. This makes the voice profile consistent across requests.

**Do not use an unlicensed or non-consensual voice recording.** Record or commission a voice actor, obtain written permission for AI synthesis and distribution, and keep the recording outside the public repository. Recommended starting reference: clean, dry, single-speaker English speech with no music or background voices. The transcript must match the recording exactly. The recording is mounted into the server at `/run/secrets/cenai-voice-reference.wav`; it is never sent from the Android app.

The Qwen model and tokenizer are downloaded by Hugging Face on the server at first startup. Keep the model cache on a persistent volume.

## Required server configuration

- CUDA-capable Linux GPU host; the 1.7B model needs materially more VRAM than the Android client.
- `CENAI_TTS_API_KEY`: random secret of at least 32 characters.
- `CENAI_VOICE_REFERENCE_AUDIO`: path to the licensed reference WAV.
- `CENAI_VOICE_REFERENCE_TEXT`: exact transcript of that WAV.
- Optional `CENAI_VOICE_MODEL_ID`: defaults to the 1.7B Base checkpoint.
- Optional `CENAI_TTS_ATTENTION`: defaults to PyTorch SDPA; use `flash_attention_2` only when installed and supported.
- Optional `CENAI_TTS_MAX_CONCURRENCY`: defaults to 1 to protect GPU memory.

Example:

```sh
docker build -t cenai-voice:latest voice-server
docker run --gpus all --restart unless-stopped -p 8080:8080 \
  -e CENAI_TTS_API_KEY="$CENAI_TTS_API_KEY" \
  -e CENAI_VOICE_REFERENCE_AUDIO=/run/secrets/cenai-voice-reference.wav \
  -e CENAI_VOICE_REFERENCE_TEXT="$CENAI_VOICE_REFERENCE_TEXT" \
  -v /secure/cenai-voice-reference.wav:/run/secrets/cenai-voice-reference.wav:ro \
  -v cenai-hf-cache:/opt/huggingface \
  cenai-voice:latest
```

Terminate TLS at a trusted reverse proxy or GPU platform ingress. Expose only `/healthz`, `/readyz`, and `/v1/audio/speech`. Do not expose the model service directly to the public internet without the gateway.

## Cloudflare Worker secrets

Configure these secrets on the Worker that serves `CLOUD_VOICE_URL`:

- `CENAI_TTS_BACKEND_URL`: HTTPS origin of this GPU service, without a path.
- `CENAI_TTS_BACKEND_TOKEN`: the same random value as `CENAI_TTS_API_KEY`.

The Worker validates the Supabase access token, derives the immutable user UUID from Supabase Auth, and forwards only that verified identity plus the request to this service. The backend token never enters the APK. Add a Cloudflare rate-limit rule for `/api/voice/speech` before enabling public traffic.

## API contract

`POST /v1/audio/speech` with a server-only bearer token and verified `X-CenAI-User-ID` header. JSON body:

```json
{
  "input": "Hello from CenAI.",
  "language": "English",
  "voice": "cenai-en-v1",
  "response_format": "wav"
}
```

The response is PCM16 WAV. Supported Qwen language labels: English, Chinese, Japanese, Korean, German, French, Russian, Portuguese, Spanish and Italian. English is the first release-quality target; other languages need separate evaluation before being marketed as equally accurate.

## Important deployment status

The application gateway and this server implementation are source code only until a GPU host, licensed reference recording, and Cloudflare secrets are configured. The service deliberately fails closed rather than silently using Google TTS or an unrelated public endpoint.
