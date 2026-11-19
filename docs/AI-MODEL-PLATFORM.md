# CenAI model, API and agent architecture
Research baseline: 2026-11-19

This document separates **model architecture**, **training/post-training**, **API protocol**, and **application orchestration**. They are different layers. A provider's API label does not reveal how its weights are built, and an API-compatible endpoint does not guarantee that two models support the same features.

## Executive design decisions

1. Keep a provider-neutral conversation domain model, but make each provider adapter responsible for its exact request, response, stream, tool, and error contract.
2. Keep the user's own provider credentials in Android Keystore-backed storage. Keep CenAI-managed inference credentials and account entitlements on the Cloudflare Worker / server side; never ship those secrets in the APK.
3. Use OpenAI Responses as the modern OpenAI adapter. Use Anthropic Messages, DeepSeek's documented Chat Completions dialect, and OpenRouter's OpenAI-compatible protocol as distinct adapters.
4. Keep Gemini GenerateContent as a compatibility adapter while a persistent, owner-bound conversation-state model is designed for Gemini Interactions. Do not flatten its typed execution steps into ordinary chat strings.
5. Do not pretend model discovery is a compatibility test. Model IDs are catalog metadata; only a provider-authenticated test call establishes that the user's account can access a model.
6. Treat tool calls as typed, untrusted proposals. A deterministic local policy gate—not the model—decides whether an action can run.
7. Keep the current phone/tablet navigation and account/admin interfaces as presentation over the existing CenAI services. This refresh does not replace Supabase identity, billing or membership state, the Cloudflare Worker, or the Rust JNI core.

## 1. Modern model architecture families

### 1.1 Dense decoder-only Transformers

The dominant chat-model design is an autoregressive decoder. Text and other inputs are tokenized or encoded into representations. Self-attention forms queries, keys and values and lets each token attend to earlier context; feed-forward blocks transform those representations. At inference, the model predicts the next token repeatedly.

- **Strengths:** mature training and serving stack, excellent in-context reasoning and general-purpose language behavior, broad ecosystem.
- **Costs:** full attention has quadratic sequence-length work in the simple dense formulation; key/value caches grow with the retained context and concurrent sequences consume memory.
- **Serving implications:** token streaming, context-window limits, input/output token accounting, prompt caching, batching, KV-cache policy, cancellation and backpressure matter to the client even though most are hidden behind provider APIs.
- **What CenAI can infer:** almost nothing reliable from an opaque model ID alone. Use provider documentation and model metadata, not name-based guesses.

The original Transformer paper introduced attention-only sequence transduction and highlighted the parallelism advantage over recurrence. It did not define today's whole model ecosystem.

### 1.2 Attention-efficient Transformers

Production Transformers may combine grouped-query or multi-query attention, sliding/local windows, global attention, sparse attention, KV sharing, quantization, chunking, and other memory/latency optimizations. Some architectures interleave local attention with occasional global layers to lower KV-cache growth while retaining long-range links.

- **Strengths:** keep attention's content-addressable retrieval while controlling cache or compute cost.
- **Trade-offs:** local windows can miss distant detail unless global layers, retrieval, or memory mechanisms bridge it; different optimizations affect exact serving limits.
- **Application implications:** do not hard-code context or memory claims. Read the provider's declared limits and observe actual usage.

### 1.3 Sparse Mixture-of-Experts (MoE)

A router selects one or more expert feed-forward networks for each token (or token group). Only selected experts execute for that token. The total parameter count can be much larger than the active parameter count, so MoE can increase capacity without proportionally increasing per-token compute.

- **Strengths:** large capacity at a lower active-compute budget.
- **Costs:** routing and all-to-all communication, expert load imbalance, capacity management, routing instability and hardware/topology complexity.
- **Application implications:** “parameter count” is not the same as active compute or price. The client should display provider pricing/usage when provided, never estimate from a model name.
- **Known variants:** top-k or top-1 routing, expert parallelism, shared experts, fine-grained experts, modality-specific or shared routing. Implementations differ.

The Switch Transformer paper is an influential sparse MoE reference. Apple's 2025 foundation-model report describes a Parallel-Track MoE server architecture with track-level parallelism, local expert groups, and alternating local/global attention. These are disclosed examples, not assumptions about closed models.

### 1.4 State-space and recurrent sequence models

Structured State Space Models (SSMs) maintain a recurrent state that is updated as sequence elements arrive. Mamba makes the state update input-dependent (“selective”), enabling the model to choose which information to propagate or forget. Its sequence-time scaling can be linear rather than the quadratic pattern of dense full attention.

- **Strengths:** efficient sequence processing and bounded recurrent state; can reduce growing KV-cache costs.
- **Trade-offs:** compressed recurrent state is not identical to an attention cache that can directly re-read arbitrary earlier tokens. Exact long-range retrieval and task quality must be evaluated, not assumed.
- **Application implications:** APIs generally hide the backbone. The client should not expose “Mamba mode” or infer SSM usage unless the provider explicitly declares it.

### 1.5 Hybrid attention + state-space models

Hybrid designs interleave exact attention with recurrent/state-space layers. The goal is to combine attention's detailed retrieval with a smaller state for broad context. Research in 2026 includes replacing selected pretrained Transformer layers with SSM layers and evaluating different state update families.

- **Strengths:** potential lower cache/memory and higher concurrent throughput while retaining some attention layers.
- **Trade-offs:** architecture design, conversion, distillation, kernel support and quality are model-specific. Research results are not a performance guarantee for every provider model.
- **Application implications:** treat context size, latency and cache policy as provider-reported/runtime-observed metadata, not architecture guesses.

### 1.6 Multimodal architectures

“Multimodal” describes capability, not one architecture. Common implementation patterns include:

- **Separate encoders + language backbone:** vision/audio encoders turn source media into embeddings or tokens consumed by the language model through projection or cross-attention.
- **Unified token sequence:** text, image, audio or video are represented in a shared/discrete token space and trained under a unified sequence objective.
- **Modality-specific heads:** a shared backbone may understand multiple modalities but use separate output heads for speech, images or video.
- **Interleaved media:** video is represented by temporally ordered frames, audio segments and timestamps; documents may require OCR/layout/table structure in addition to plain text.

Images may be tiled/cropped into multiple visual tokens. Audio may use codec tokens or learned acoustic representations; speech output may be token-autoregressive or use a separate decoder. Video adds temporal structure and can multiply input cost. A text endpoint does not automatically support every modality.

**CenAI rule:** content parts must remain typed. If an adapter does not support a modality or media size, return an explicit unsupported-input error; never silently drop or stringify binary media.

### 1.7 Autoregressive versus diffusion / flow-matching generation

Autoregressive language models generate ordered tokens one step at a time. Diffusion and flow-matching generators instead learn to transform noise or a noisy sample toward a data sample through an iterative denoising/velocity process. Discrete flow-matching work also explores unified multimodal generation with token representations.

- **Strengths of diffusion/flow approaches:** strong image/audio/video synthesis and flexible iterative refinement; can model spatial/temporal structure differently from next-token decoding.
- **Costs:** iterative sampling steps, modality-specific encoders/decoders, and different latency/quality trade-offs. They are not simply another Chat Completions model.
- **Application implications:** add separate media-generation jobs/endpoints, progress events, cancellation and file/result storage. Do not fake image/video generation by sending a text prompt to a chat adapter.

### 1.8 Reasoning and test-time compute

Reasoning-oriented models can be trained to allocate additional inference-time computation, use longer internal trajectories, or expose an effort/latency/quality knob. This is not necessarily a new backbone family: reasoning behavior can be layered on dense Transformers, MoE, multimodal, or hybrid models.

- **API implications:** providers expose different controls (for example reasoning effort, thinking level, or a thinking toggle). Values are not interchangeable across providers.
- **Privacy/UI rule:** never display hidden chain-of-thought or provider-internal reasoning fields as a chat response. Show a neutral “thinking” state or supported user-facing summary.
- **Tool loop rule:** a provider may require opaque prior reasoning/tool state to be returned on a later turn. Preserve it internally only as the adapter contract requires.

### 1.9 Retrieval, fine-tuning and agents are not model architectures

- **RAG** retrieves external documents and inserts relevant context. It changes the system around a model; it does not modify the base model weights.
- **Embeddings** map text/media into vectors for semantic retrieval or clustering; embedding APIs and generation APIs have different output shapes and cost models.
- **Fine-tuning / LoRA** changes or adapts model parameters. It is different from adding a system prompt or RAG.
- **Tool use / agents** are an orchestration loop: model proposes structured actions; host validates, authorizes, executes, and sends results back. An “agent” label does not guarantee safe autonomy.

## 2. Provider API contract matrix

| Provider | Current CenAI path | Wire contract | Notes / migration |
| --- | --- | --- | --- |
| OpenAI | Implemented in this branch | Responses API, `POST /v1/responses`, typed SSE | Text + user image input, locally replayed history, `store=false`; typed function-call item replay is not yet enabled. |
| Anthropic | Existing adapter | Messages API, `POST /v1/messages`, API-key header + version header, content-block stream | Stateless multi-turn history. Tool use requires a `tool_use` block / `stop_reason=tool_use` loop and exact `tool_result` return. |
| Google Gemini | Existing compatibility path | GenerateContent stream | Still works, but new work should target Interactions after durable interaction-state and retention rules are implemented. |
| DeepSeek | Existing OpenAI-compatible adapter + controls in this branch | Chat Completions dialect, `/chat/completions`, thinking and reasoning-effort controls | `reasoning_content` is hidden. If/when tool loops are added, preserve the provider-required opaque prior reasoning content for tool-enabled turns. |
| OpenRouter | Added in this branch | OpenAI-compatible Chat Completions, `/api/v1/chat/completions` | The user chooses a model ID; upstream provider routing is OpenRouter-controlled unless explicit routing parameters are added later. |
| Custom | Existing adapter + catalog lookup | OpenAI-compatible Chat Completions | HTTPS only for model discovery; treat remote endpoint as untrusted. Do not forward CenAI sessions or unrelated provider tokens. |
| CenAI Cloud | Existing Cloudflare Worker proxy | Authenticated server-managed route | Entitlement, quota, provider keys and policy remain server-side. Never bypass this route from the APK. |

### 2.1 OpenAI Responses

OpenAI recommends Responses for new projects. It represents output as typed items rather than a single chat message; items may be messages, function calls, and function-call outputs. It supports multimodal input and built-in agentic tools. Streaming emits typed events, including text delta and response lifecycle/error events.

CenAI implementation scope in this branch:
- Send local conversation messages as `input` and user image content as typed image parts.
- Stream `response.output_text.delta` using a common SSE decoder.
- Detect provider failure events.
- Set `store=false` because CenAI already replays its own locally stored conversation history and does not yet persist OpenAI response IDs.
- Do not claim function-call, remote MCP, file-search, computer-use, or response-item replay support until the app stores and validates the complete item graph and its approval flow is implemented.

### 2.2 Anthropic Messages

Messages accepts a list of user/assistant turns and content blocks; a system instruction is a separate top-level parameter. It supports stateless multi-turn conversations. Client-side tools return `tool_use` content blocks and `stop_reason=tool_use`; the application performs the operation and sends a corresponding `tool_result`. Server tools run on Anthropic infrastructure.

CenAI must retain block order and non-text content when implementing tool use or citations. Converting every block to a plain string would lose important protocol state.

### 2.3 Gemini Interactions

As of June 2026, Google documents Interactions as generally available and recommended for new Gemini projects; GenerateContent remains supported but is labelled legacy. An Interaction is a turn/session record with ordered execution steps, which can contain user input, model output, thoughts, function calls and results.

- Stateful continuation passes `previous_interaction_id`.
- Tool definitions, system instructions and generation settings must be supplied again for every interaction.
- By default, Interactions are stored. The docs describe 55-day paid-tier and 1-day free-tier retention. `store=false` opts out, but is incompatible with background execution and chaining via `previous_interaction_id`.
- A safe CenAI migration needs a conversation-to-provider state table, user ownership binding, model/provider binding, retention disclosure, deletion cleanup, cross-device consistency, and strict separation from local chat history.

Do not silently switch an existing conversation to this API until that storage/retention behavior is deliberately designed and tested.

### 2.4 DeepSeek thinking and tool calls

DeepSeek documents OpenAI-compatible Chat Completions thinking controls: `thinking.type` can enable/disable thinking; `reasoning_effort` accepts provider-defined levels including low/high/max for supported models. Thinking mode is incompatible with some sampling parameters. The model's private reasoning may appear as `reasoning_content`.

When a request carries tools, DeepSeek requires previous `reasoning_content` to be preserved in subsequent context for supported tool-call flows. That field must never be shown to the user. CenAI's current branch adds the thinking/effort controls only; it does not claim to complete typed tool execution.

### 2.5 OpenRouter routing

OpenRouter separates a model ID from the upstream provider serving it. Its routing policy can use price, throughput, time-to-first-token, reliability, rate-limit behavior, and quality signals; request options can constrain provider order, fallback, parameters, privacy or price. The current CenAI adapter exposes a model choice and basic API compatibility. Advanced provider routing controls and visible cost/latency policy are a separate feature, not yet implemented.

## 3. Model discovery and capability registry

Current catalog endpoints used by the new Android model discovery helper:
- OpenAI: `GET /v1/models`
- Anthropic: `GET /v1/models` with API-key and API-version headers
- Gemini: `GET /v1beta/models` using an API key
- DeepSeek: `GET /models`
- OpenRouter: `GET /api/v1/models`
- Custom OpenAI-compatible HTTPS base: `GET {base}/models`

The helper currently extracts IDs only. It does not invent vision, audio, context, pricing, tool, latency or region capability labels. Catalog visibility also does not guarantee entitlement to run the model. A real connection/completion test and provider error are authoritative.

Recommended next step: maintain a provider/model capability registry with source, last-verified timestamp, modalities, context size, tool/structured-output flags, pricing metadata, regional constraints, and deprecation status. Every field should be source-backed and expire/revalidate; do not infer capability solely from a model name.

## 4. Agent and tool architecture

Use a brokered architecture rather than letting an LLM directly call arbitrary Android APIs:

1. **Planner/coordinator:** selected model proposes a bounded plan. Plan text is untrusted input.
2. **Tool catalog:** only registered, narrow tools appear in model context; schemas have strict types, enums, and size limits.
3. **Policy/authorization gate:** check signed-in user, device/session, app permission, target object, scope, action risk, and policy version. Fail closed on timeout.
4. **Approval UX:** show the exact action, target, argument summary and consequence. Bind approval to the exact action and arguments with expiry; changed arguments require a new approval.
5. **Executor:** deterministic allow-listed code only. No arbitrary shell, broad filesystem, contacts/messages or unbounded network access.
6. **Result adapter:** return typed tool results in the provider's exact format; label remote content as untrusted to resist indirect prompt injection.
7. **Audit receipt:** record actor, action, target, policy result, approval, timestamp, outcome and correlation ID. Never log API keys, OAuth tokens, session cookies, raw private prompts or full sensitive documents.

MCP standardizes host/client/server communication using JSON-RPC, capabilities, resources, prompts and tools. MCP does not replace CenAI authentication, app permissions or user approval. Follow OAuth resource/audience rules for remote MCP. Never forward a token issued to one resource server as a token to a different downstream API.

Security priorities map to OWASP's LLM risks: prompt injection, sensitive data disclosure, supply-chain risk, improper output handling, data/model poisoning, excessive agency, system-prompt leakage, vector weaknesses, misinformation, and unbounded consumption.

## 5. Reliability, privacy and evaluation

- Parse SSE correctly: comments, event names, multiple data fields, blank-line framing, EOF dispatch and provider-specific error events.
- Separate HTTP success, provider terminal status, and non-empty user-facing output.
- Enforce connect/read/call deadlines, output-token limits, request-size limits, cancellation, and bounded retries with jitter.
- Do not automatically retry non-idempotent tool calls, charges, refunds, account mutations or deployments.
- Normalize provider errors; never persist raw credentials or unredacted request/response bodies in logs.
- Track time-to-first-token, completion time, token counts, provider/model, HTTP/error class and tool outcome only with appropriate consent and retention limits.
- Build adapter fixtures for empty responses, truncated SSE, multi-line events, error events, cancellation, malformed JSON, image inputs, unsupported content, tool call/result round trips and provider-specific opaque state.
- Evaluate quality using a fixed, versioned CenAI task set; measure accuracy, instruction adherence, hallucination/citation behavior, tool precision, refusal behavior, latency, cost and battery/network impact.
- A provider/model switch should preserve conversation history but invalidate incompatible server-side state and cached capability metadata.

## 6. Current implementation vs. next milestones

| Area | Current branch | Next deliberate milestone |
| --- | --- | --- |
| Android design system | Calm Studio default and one-time migration from legacy default | Visual review on phone/tablet/dynamic font and accessibility settings |
| Navigation | Phone bottom bar, tablet rail, duplicate top-bar destinations removed | Test posture/window resizing, keyboard and split-screen |
| OpenAI | Responses text/image streaming, error handling, local replay, store=false | Typed output-item storage and function call loop |
| Anthropic | Existing basic Messages adapter | Preserve full content-block/tool state; add tool loop |
| Gemini | GenerateContent compatibility | Explicit Interactions state and retention migration |
| DeepSeek | Thinking toggle and effort control | Preserve opaque reasoning/tool state when tool calls are added |
| OpenRouter | Provider and catalog selection | Provider constraints, fallback UX, privacy and price controls |
| Models | Provider catalog IDs | Verified capability/pricing/context registry |
| Agents | Planner setting, local allow-listed tools, approvals and WorkManager tasks | Permission gate receipts, exact-argument approval, more tests; then limited MCP client |
| Website admin/account | Responsive design layer, existing logic retained | Browser visual review, mobile usability tests and accessibility audit |
| Cloud AI | Existing Worker/auth/quota path preserved | Server-side router, budget/rate policies, observability and provider health checks |

## Primary research and official documentation

### Architecture and model research
- Vaswani et al., *Attention Is All You Need* (Transformer): https://arxiv.org/abs/1706.03762
- Fedus, Zoph and Shazeer, *Switch Transformers* (sparse MoE): https://arxiv.org/abs/2101.03961
- Gu and Dao, *Mamba: Linear-Time Sequence Modeling with Selective State Spaces*: https://arxiv.org/abs/2312.00752
- Apple, *Apple Intelligence Foundation Language Models* (PT-MoE, local/global attention, quantization and KV sharing): https://arxiv.org/html/2507.13575
- Chattopadhyay et al., *Priming: Hybrid State Space Models From Pre-trained Transformers* (2026): https://arxiv.org/html/2605.08301
- *NExT-OMNI: Towards Any-to-Any Omnimodal Foundation Models with Discrete Flow Matching*: https://arxiv.org/abs/2510.13721
- ERNIE 5.0 Technical Report (unified autoregressive multimodal and sparse MoE): https://arxiv.org/abs/2602.04705

### APIs and orchestration
- OpenAI Responses migration: https://developers.openai.com/api/docs/guides/migrate-to-responses
- OpenAI Responses streaming events: https://developers.openai.com/api/reference/resources/responses/streaming-events
- Anthropic Messages API: https://docs.anthropic.com/en/api/messages
- Anthropic tool use: https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/overview
- Gemini Interactions API: https://ai.google.dev/gemini-api/docs/interactions
- DeepSeek thinking mode: https://api-docs.deepseek.com/guides/thinking_mode/
- DeepSeek tool calls: https://api-docs.deepseek.com/guides/tool_calls
- OpenRouter provider routing: https://openrouter.ai/docs/guides/routing/provider-routing
- Model Context Protocol specification: https://modelcontextprotocol.io/specification/2025-11-25
- OWASP Top 10 for LLM Applications: https://genai.owasp.org/llm-top-10/
- Android adaptive navigation: https://developer.android.com/develop/adaptive-apps/guides/build-adaptive-navigation
- Cloudflare Workers AI bindings: https://developers.cloudflare.com/workers-ai/configuration/bindings/
