# CenAI Voice

CenAI Voice is CenAI's branded speech product built on the open-source Qwen3-TTS model family.

## Product identity and attribution

- Product name: **CenAI Voice**
- Upstream model family: Qwen3-TTS
- Upstream source: https://github.com/QwenLM/Qwen3-TTS
- Upstream model and tokenizer licence: Apache-2.0. The GGUF conversion/tooling runtime is MIT-licensed. Retain required copyright, licence and NOTICE material when redistributing upstream code or model artifacts.
- CenAI owns its app experience, voice profile names, pronunciation policy, local model management and integration code. CenAI does not claim authorship of Qwen's model architecture or pretrained weights.
- Verify the licence and terms of each exact checkpoint and tokenizer artifact before bundling or distributing it. Do not remove upstream attribution.

## Model choice

Primary fine-tuning source:

`Qwen/Qwen3-TTS-12Hz-0.6B-Base`

Tokenizer:

`Qwen/Qwen3-TTS-Tokenizer-12Hz`

The 0.6B Base checkpoint is the selected starting point because Qwen documents Base models as supporting voice cloning and single-speaker fine-tuning. The initial CenAI target profile is `cenai_voice_en_v1` (English, `en-US`). The 0.6B CustomVoice checkpoint remains a baseline for quality comparisons; it is not the final CenAI voice identity. The 1.7B VoiceDesign checkpoint is an optional research/reference model and is not the Android default.

Qwen documents support for ten languages: Chinese, English, Japanese, Korean, German, French, Russian, Portuguese, Spanish and Italian. English is the first quality gate. Language availability is not a guarantee of equal pronunciation quality; each language must pass its own test set before being labelled production-ready. Filipino/Tagalog is not in the documented ten-language list and must not be advertised as natively supported by this model.

## Offline and Android requirements

The product requirement is offline-only synthesis after model installation:

- no Google TTS, Android system TTS, Gemini TTS, DashScope API, or cloud fallback;
- no audio or text leaves the device during local synthesis;
- model packs are downloaded explicitly, verified, stored in app-private storage, and usable after disconnecting the network;
- users can remove a model pack and see its disk usage;
- voice playback is cancelled on user request, app lifecycle shutdown, or account/session invalidation.

**Runtime implementation:** CenAI now includes an Android NDK build for the MIT-licensed `qwen3-tts.cpp` GGML runtime and a small CenAI C ABI bridge. Rust loads `libcenai_qwen_bridge.so` and invokes synthesis locally. The APK packages the native runtime; model weights are not bundled.

The explicit model installer downloads two pinned Q8_0 GGUF artifacts from [Serveurperso/Qwen3-TTS-GGUF](https://huggingface.co/Serveurperso/Qwen3-TTS-GGUF) at revision `b7ee2e8c7459c3bea99da23e3d178125a7d1713c`: the 0.6B Base talker (about 993 MB) and the shared 12Hz tokenizer (about 291 MB). Both files are SHA-256 checked before installation. The download is a one-time, user-initiated step; once installed, synthesis uses the local model only. No Google TTS, Android system TTS, Gemini TTS, DashScope API, or cloud fallback is permitted.

**Validation status:** source integration is in place, but the Android NDK build, APK packaging, actual model load and real-device synthesis still need successful CI/device verification. Do not claim offline playback is release-ready until those checks pass. Benchmark cold start, peak RAM, real-time factor, audio quality, thermal load and sustained playback on the REDMI Pad 2 Pro class of hardware.

## Voice identity and fine-tuning

The intended CenAI voice is an original, consistent English speaker profile, not a copy of a celebrity or another identifiable person. Use recordings from a voice actor who has explicitly consented to model training and commercial distribution. Keep dataset provenance, consent, transcript and licence records.

Qwen's current fine-tuning guide documents single-speaker fine-tuning for the 12Hz Base family. Its JSONL training records contain `audio`, `text`, and `ref_audio`; the preparation step extracts tokenizer audio codes, then `sft_12hz.py` fine-tunes the selected Base checkpoint.

Example starting workflow (run on a compatible cloud GPU; standard GitHub-hosted runners are not assumed to have GPUs):

```bash
git clone https://github.com/QwenLM/Qwen3-TTS.git
cd Qwen3-TTS
pip install -U qwen-tts
cd finetuning

python prepare_data.py \
  --device cuda:0 \
  --tokenizer_model_path Qwen/Qwen3-TTS-Tokenizer-12Hz \
  --input_jsonl train_raw.jsonl \
  --output_jsonl train_with_codes.jsonl

python sft_12hz.py \
  --init_model_path Qwen/Qwen3-TTS-12Hz-0.6B-Base \
  --output_model_path output/cenai-voice-en-v1 \
  --train_jsonl train_with_codes.jsonl \
  --batch_size 2 \
  --lr 2e-5 \
  --num_epochs 3 \
  --speaker_name cenai_voice_en_v1
```

This is a reproducible starting recipe, not a guaranteed optimal training configuration. Tune batch size and learning rate for the actual GPU and dataset. Keep validation/test utterances separate, inspect checkpoints, and never automatically promote a checkpoint just because training completed.

## English-first quality gate

Before release, evaluate:

1. Word error rate using independent ASR only as a measurement tool (not as a synthesis dependency).
2. Human intelligibility and naturalness ratings from native English listeners.
3. Pronunciation of names, abbreviations, numbers, dates, currencies, units, URLs and technical terms.
4. Sentence stress, punctuation pauses, long-form consistency and expressive control.
5. Voice identity consistency across short and long passages.
6. Cold-start latency, real-time factor, peak memory, thermal behaviour and interrupted playback on supported Android devices.

Keep the reference text set and scoring scripts versioned. Publish per-language quality status rather than one blanket "multilingual accurate" claim.

## Rust integration contract

`native/src/core/qwen_voice.rs` owns stable model metadata, the supported-language catalogue, the voice profile and fail-closed runtime status. The eventual inference backend must implement this contract without changing chat/provider authorization. The model pack must be independently versioned from the APK.

## Current implementation status

- [x] CenAI Voice product identity and Qwen upstream attribution policy.
- [x] Select Qwen3-TTS 12Hz 0.6B Base as the fine-tuning source.
- [x] English-first profile and ten-language metadata.
- [x] Offline-only/no-fallback contract and validation tests.
- [x] Fine-tuning workflow documentation.
- [ ] Acquire/record licensed English voice dataset and signed consent.
- [ ] Train and evaluate the CenAI English voice checkpoint.
- [x] Add Android NDK build and CenAI C ABI bridge for the native GGML inference runtime.
- [x] Add explicit Q8_0 model-pack download with pinned revision and SHA-256 checks.
- [x] Connect Rust read-aloud to local inference and Android PCM playback.
- [ ] Pass Rust/Android CI, model-load and synthesis tests.
- [ ] Add voice-profile recording and consent workflow.
- [ ] Fine-tune the English Base checkpoint on licensed voice-actor recordings and convert the result to GGUF.
- [ ] Benchmark offline quality, speed, memory and thermal performance on physical Android hardware.
