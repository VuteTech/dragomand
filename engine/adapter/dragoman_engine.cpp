// SPDX-License-Identifier: GPL-3.0-or-later

#include "dragoman_engine.h"

#include <cstring>
#include <memory>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

#include "common/logging.h"  // marian::setThrowExceptionOnAbort
#include "translator/parser.h"
#include "translator/response.h"
#include "translator/response_options.h"
#include "translator/service.h"
#include "translator/translation_model.h"

namespace bergamot = marian::bergamot;

struct dg_error {
    std::string message;
};

struct dg_engine {
    std::unique_ptr<bergamot::BlockingService> service;
};

struct dg_model {
    std::shared_ptr<bergamot::TranslationModel> model;
};

struct dg_result {
    std::vector<std::string> texts;
};

namespace {

void store_error(dg_error **err, std::string message) {
    if (err != nullptr) {
        *err = new dg_error{std::move(message)};
    }
}

// Runs fn inside a catch-all so no exception crosses the C boundary.
// Returns fallback (NULL / nonzero) on failure.
template <typename T, typename Fn>
T guarded(dg_error **err, T fallback, Fn &&fn) {
    try {
        return fn();
    } catch (const std::exception &e) {
        store_error(err, e.what());
    } catch (...) {
        store_error(err, "unknown C++ exception in translation engine");
    }
    return fallback;
}

// YAML double-quoted scalar; handles the characters legal in paths.
std::string yaml_quote(const char *s) {
    std::string out = "\"";
    for (const char *p = s; *p != '\0'; ++p) {
        if (*p == '"' || *p == '\\') {
            out += '\\';
        }
        out += *p;
    }
    out += '"';
    return out;
}

}  // namespace

extern "C" {

size_t dg_engine_abi_version(void) { return DG_ENGINE_ABI_VERSION; }

const char *dg_error_message(const dg_error *err) {
    return err != nullptr ? err->message.c_str() : nullptr;
}

void dg_error_free(dg_error *err) { delete err; }

dg_engine *dg_engine_create(const dg_engine_options *options, dg_error **err) {
    return guarded<dg_engine *>(err, nullptr, [&]() -> dg_engine * {
        // Marian's ABORT macro calls std::abort() unless told to throw.
        // Process-wide and idempotent, but set it exactly once anyway.
        static std::once_flag abort_flag;
        std::call_once(abort_flag, []() { marian::setThrowExceptionOnAbort(true); });

        bergamot::BlockingService::Config config;
        if (options != nullptr) {
            config.cacheSize = options->cache_size;
            if (options->log_level != nullptr) {
                config.logger.level = options->log_level;
            }
        }
        auto engine = std::make_unique<dg_engine>();
        engine->service = std::make_unique<bergamot::BlockingService>(config);
        return engine.release();
    });
}

void dg_engine_destroy(dg_engine *engine) { delete engine; }

dg_model *dg_model_load(dg_engine *engine, const dg_model_files *files,
                        const char *config_yaml, dg_error **err) {
    if (engine == nullptr || files == nullptr || files->model_path == nullptr) {
        store_error(err, "dg_model_load: engine and files.model_path are required");
        return nullptr;
    }
    const bool shared_vocab = files->vocab_path != nullptr;
    if (!shared_vocab &&
        (files->src_vocab_path == nullptr || files->trg_vocab_path == nullptr)) {
        store_error(err, "dg_model_load: need vocab_path, or src_vocab_path and trg_vocab_path");
        return nullptr;
    }

    return guarded<dg_model *>(err, nullptr, [&]() -> dg_model * {
        std::string config;
        config += "models:\n  - ";
        config += yaml_quote(files->model_path);
        config += "\nvocabs:\n  - ";
        config += yaml_quote(shared_vocab ? files->vocab_path : files->src_vocab_path);
        config += "\n  - ";
        config += yaml_quote(shared_vocab ? files->vocab_path : files->trg_vocab_path);
        config += "\n";
        if (files->shortlist_path != nullptr) {
            config += "shortlist:\n  - ";
            config += yaml_quote(files->shortlist_path);
            config += "\n  - false\n";
        }
        if (config_yaml != nullptr) {
            config += config_yaml;
            config += "\n";
        }

        auto options = bergamot::parseOptionsFromString(config, /*validate=*/false);
        auto model = new dg_model{std::make_shared<bergamot::TranslationModel>(options)};
        return model;
    });
}

void dg_model_unload(dg_model *model) { delete model; }

int dg_translate_batch(dg_engine *engine, dg_model *first, dg_model *second,
                       const dg_text *segments, size_t n,
                       const dg_translate_options *options, dg_result **out,
                       dg_error **err) {
    if (engine == nullptr || first == nullptr || out == nullptr ||
        (segments == nullptr && n != 0)) {
        store_error(err, "dg_translate_batch: engine, first and out are required");
        return -1;
    }
    *out = nullptr;
    if (n == 0) {
        *out = new dg_result{};
        return 0;
    }

    return guarded<int>(err, -1, [&]() -> int {
        std::vector<std::string> sources;
        sources.reserve(n);
        for (size_t i = 0; i < n; ++i) {
            sources.emplace_back(segments[i].data, segments[i].len);
        }

        bergamot::ResponseOptions response_options;
        response_options.HTML = options != nullptr && options->html != 0;
        std::vector<bergamot::ResponseOptions> per_segment(n, response_options);

        std::vector<bergamot::Response> responses =
            second == nullptr
                ? engine->service->translateMultiple(first->model, std::move(sources),
                                                     per_segment)
                : engine->service->pivotMultiple(first->model, second->model,
                                                 std::move(sources), per_segment);
        if (responses.size() != n) {
            store_error(err, "translation returned " + std::to_string(responses.size()) +
                                 " responses for " + std::to_string(n) + " segments");
            return -1;
        }

        auto result = std::make_unique<dg_result>();
        result->texts.reserve(n);
        for (bergamot::Response &response : responses) {
            result->texts.push_back(std::move(response.target.text));
        }
        *out = result.release();
        return 0;
    });
}

size_t dg_result_len(const dg_result *result) {
    return result != nullptr ? result->texts.size() : 0;
}

dg_text dg_result_text(const dg_result *result, size_t index) {
    if (result == nullptr || index >= result->texts.size()) {
        return dg_text{nullptr, 0};
    }
    const std::string &text = result->texts[index];
    return dg_text{text.c_str(), text.size()};
}

void dg_result_free(dg_result *result) { delete result; }

}  // extern "C"
