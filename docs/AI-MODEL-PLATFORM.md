# CenAI AI model and API architecture
Reviewed: 2026-11-19

## Purpose and scope
CenAI is a multi-provider client and a controlled Cloudflare-backed service. It does not implement or train foundation-model weights. It integrates model families through protocol adapters, stores user-selected provider credentials in Android Keystore-backed storage, and routes managed inference through the Cloudflare Worker and authenticated server-side quota checks.

Keep model internals, API protocols and agent orchestration as separate layers. A model being based on a Transformer, mixture-of-experts (MoE), multimodal encoder, or reasoning-trained architecture does not imply that its API speaks the same wire protocol.

## Current model families and concepts
- Dense decoder-only Transformers: autoregressive token generation using self-attention and feed-forward blocks. Commonly exposed through message/completion APIs.
- Mixture-of-experts models: token routing activates a subset of expert feed-forward networks. This can increase capacity without activating every expert for every token. Do not infer architecture or expert count from marketing names; use provider metadata.
- Multimodal models: combine text with image, audio, video, or document representations. Requests use provider-specific content blocks and explicit media limits; a text-only adapter must not silently flatten unsupported media.
- Reasoning-oriented models: expose effort controls or intermediate metadata. Internal reasoning is not a user-facing answer. Store or forward provider-required opaque metadata only when that provider's tool loop requires it; never render private reasoning traces in chat.
- Tool-using models: return structured function/tool-call items. The host validates schema, applies allow-lists and user-approval policy, executes only registered functions, then returns tool results in the provider's expected format. Text that resembles a tool call is not equivalent to a typed tool-call event.

## Provider/API matrix
| Provider | Supported protocol in CenAI | Current/next integration approach |
| --- | --- | --- |
| OpenAI | Responses API | `POST /v1/responses`, SSE event decoding, text and user image inputs. Function calls and response-item replay need a separate typed tool-loop implementation before they are enabled. |
| Anthropic | Messages API | `POST /v1/messages`, `x-api-key`, `anthropic-version`, content-block streaming events. Preserve non-text blocks when adding tool use or server-side search. |
| Google Gemini | Legacy GenerateContent stream path | Kept as compatibility. Interactions API is the target, but migration requires persisting interaction IDs or exact typed step history; flattening into chat strings loses tool/thought state. |
| DeepSeek | OpenAI-compatible Chat Completions | `/chat/completions`; thinking and reasoning-effort parameters are model dependent. Never render `reasoning_content` in chat. Preserve it unchanged in future supported tool loops when required. |
| OpenRouter | OpenAI-compatible Chat Completions | `/api/v1/chat/completions`; model ID and provider routing policy remain explicit. Catalog discovery is supported. |
| Custom | OpenAI-compatible Chat Completions | HTTPS endpoint supplied by user. Treat endpoint as untrusted; never forward another provider's key or app session token to it. |
| CenAI Cloud | Authenticated Worker proxy | Worker validates account and quota before inference. Do not move managed provider keys into Android or bypass quota checks. |

## API mechanics and state
### OpenAI Responses
Responses is the modern OpenAI request surface for new integrations. Streams use typed events such as `response.output_text.delta`, `response.completed`, `response.failed`, and output-item events. Message history can contain text and image content. Future tool support must preserve all replayable output items, not just assistant text; tool calls must be validated and require the CenAI permission policy before execution.

### Anthropic Messages
Messages has its own request and stream format: system instruction is separate, content is an array of blocks, and streaming text arrives via content-block delta events. Tool use, server tools, citations and pause/resume states require keeping non-text blocks intact across turns. Do not parse Anthropic events as OpenAI deltas.

### Gemini Interactions
Google's Interactions API became the recommended/default interface in June 2026. It has typed step events and supports server-side state with `previous_interaction_id`; stateless mode requires resending exact prior steps. CenAI currently stores simple role/content messages, not the complete Interaction step graph. Safe migration: persist interaction ID per conversation and provider/model; bind it to owner/retention policy; clear it on conversation deletion or provider changes; handle deletion/expiry; then add typed function-call continuation. Until that contract exists, the legacy GenerateContent adapter remains explicitly marked as compatibility rather than pretending a lossy migration is complete.

### DeepSeek thinking and tools
DeepSeek exposes a thinking toggle and effort controls. Some tool workflows require returning previous `reasoning_content` as opaque provider metadata. CenAI exposes a provider-specific effort setting and does not show hidden reasoning. Tool-call integration is not enabled by the text-stream parser.

### OpenRouter routing
A model router separates model selection from upstream provider selection. CenAI exposes discovered model IDs and an explicit user choice. Automatic cost/latency routing and fallback require clear price/quality constraints, error classification and telemetry before they can be enabled by default.

## Model discovery
The client requests IDs from provider-owned catalogs:
- OpenAI-compatible catalogs: `GET /v1/models` or provider base `/models`.
- Anthropic: `GET /v1/models` with its API-key and version headers.
- Gemini: `GET /v1beta/models` using the user's key.
The list is discovery metadata only: account access, region, deprecation, modalities, context window, price and feature support remain provider-authoritative. Do not manufacture capability badges from model IDs.

## Agent architecture and security
1. Coordinator/planner: selected model proposes a plan; plan output is untrusted.
2. Policy layer: validates tool name, schema, allow-list, arguments, scope and risk.
3. Local executor: runs registered deterministic operations only. No arbitrary shell, broad file-system sweep, contacts/messages or unbounded network side effects.
4. Approval gate: sensitive Android actions require explicit confirmation at action time. Destructive actions require additional confirmation and reason.
5. Background worker: explicit one-off tasks, bounded retries/timeouts, cancellation, local result storage and private notifications.
6. Audit/telemetry: record action type, result, latency and error category with consent. Never record API keys, OAuth tokens, full prompts or private responses.

MCP standardizes how clients connect to external tools and context. It is not a model architecture and does not replace CenAI authorization, permission and approval. Treat each MCP server as an external tool provider with scopes, consent, timeouts and revocation.

## Reliability and evaluation
- Decode SSE per the event-stream format: comments, event names, multiple data lines, blank-line dispatch, [DONE], and EOF dispatch.
- Separate transport success, provider completion state and useful text output.
- Bound output tokens, body sizes, timeouts and retries; cancel the HTTP call when the coroutine is cancelled.
- Normalize provider errors without logging credentials or raw private request/response bodies.
- Maintain protocol fixtures per provider. Test partial/multi-line SSE, missing terminal event, error event, empty response, cancellation, malformed JSON, image parts and unsupported tools.
- Model discovery does not prove that an account can call a model; the connection test remains authoritative.
- Do not enable autonomous web access, remote computer use, local shell access or model-driven delegation until policy, approvals, sandboxing and audit are implemented and tested.

## Primary references
- OpenAI Responses API: https://developers.openai.com/api/reference/resources/responses/methods/create
- OpenAI Responses streaming: https://developers.openai.com/api/docs/guides/streaming-responses
- OpenAI migration guide: https://developers.openai.com/api/docs/guides/migrate-to-responses
- Anthropic Messages: https://docs.anthropic.com/en/api/messages
- Anthropic tool use: https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/overview
- Gemini Interactions: https://ai.google.dev/gemini-api/docs/interactions
- Gemini streaming and function calls: https://ai.google.dev/gemini-api/docs/streaming
- DeepSeek thinking mode: https://api-docs.deepseek.com/guides/thinking_mode/
- DeepSeek tool calls: https://api-docs.deepseek.com/guides/tool_calls
- OpenRouter routing: https://openrouter.ai/docs/guides/routing/provider-routing
- Android adaptive navigation: https://developer.android.com/develop/adaptive-apps/guides/build-adaptive-navigation
- Cloudflare Workers AI bindings: https://developers.cloudflare.com/workers-ai/configuration/bindings/
