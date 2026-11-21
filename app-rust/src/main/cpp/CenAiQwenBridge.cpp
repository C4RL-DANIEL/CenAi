#include "qwen3_tts_c.h"

#include <cstdint>

extern "C" {

QWEN3_TTS_API qwen3_tts_context_t* cenai_qwen_create() {
    return qwen3_tts_init();
}

QWEN3_TTS_API void cenai_qwen_destroy(qwen3_tts_context_t* context) {
    qwen3_tts_free(context);
}

QWEN3_TTS_API int32_t cenai_qwen_set_cpu_threads(int32_t threads) {
    return qwen3_tts_set_cpu_threads(threads);
}

QWEN3_TTS_API int32_t cenai_qwen_load_models(
    qwen3_tts_context_t* context,
    const char* model_directory,
    const char* model_name
) {
    return qwen3_tts_load_models_with_name(context, model_directory, model_name);
}

QWEN3_TTS_API qwen3_tts_result_t cenai_qwen_synthesize(
    qwen3_tts_context_t* context,
    const char* text,
    const char* speaker_embedding_file,
    int32_t language_id,
    int32_t cpu_threads
) {
    qwen3_tts_params_t params{};
    params.max_audio_tokens = 2048;
    params.temperature = 0.9f;
    params.top_p = 1.0f;
    params.top_k = 50;
    params.n_threads = cpu_threads > 0 ? cpu_threads : 4;
    params.print_progress = 0;
    params.print_timing = 0;
    params.repetition_penalty = 1.05f;
    params.language_id = language_id;
    params.instruction = nullptr;
    params.speaker = nullptr;
    params.vocoder_left_context_sec = 2.0f;

    if (speaker_embedding_file && speaker_embedding_file[0] != '\0') {
        return qwen3_tts_synthesize_with_speaker_embedding(
            context, text, speaker_embedding_file, params
        );
    }
    return qwen3_tts_synthesize(context, text, params);
}

QWEN3_TTS_API int32_t cenai_qwen_extract_speaker_embedding(
    qwen3_tts_context_t* context,
    const char* reference_wav,
    const char* output_path
) {
    return qwen3_tts_extract_speaker_embedding(context, reference_wav, output_path);
}

QWEN3_TTS_API void cenai_qwen_free_result(qwen3_tts_result_t result) {
    qwen3_tts_free_result(result);
}

QWEN3_TTS_API char* cenai_qwen_last_error(qwen3_tts_context_t* context) {
    return qwen3_tts_get_last_error(context);
}

QWEN3_TTS_API void cenai_qwen_free_string(char* value) {
    qwen3_tts_free_string(value);
}

}
