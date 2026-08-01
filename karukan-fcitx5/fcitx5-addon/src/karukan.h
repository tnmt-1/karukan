/*
 * Karukan fcitx5 addon
 */

#ifndef FCITX5_KARUKAN_KARUKAN_H
#define FCITX5_KARUKAN_KARUKAN_H

#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/instance.h>

// Include the Rust FFI header
#include "../../include/karukan.h"

namespace fcitx {

class KarukanState;
class KarukanEngine;

// Candidate word class
class KarukanCandidateWord : public CandidateWord {
public:
    KarukanCandidateWord(KarukanEngine* engine, Text text, int index,
                         const std::string& description = "");
    void select(InputContext* inputContext) const override;

private:
    KarukanEngine* engine_;
    int index_;
};

// Candidate list class
class KarukanCandidateList : public CommonCandidateList {
public:
    KarukanCandidateList(KarukanEngine* engine, InputContext* ic);
    void updateCandidates(::KarukanEngine* rustEngine);

    // fcitx5 page-button support: prev/next arrows appear in the candidate
    // window when these report navigation is possible, and the buttons call
    // nextPage()/prevPage() (M10: mouse paging instead of PgUp/PgDn only).
    bool hasPrev() const override { return page_ > 0; }
    bool hasNext() const override { return page_ + 1 < totalPages_; }
    void nextPage() override;
    void prevPage() override;

private:
    KarukanEngine* engine_;
    InputContext* ic_;
    uint32_t page_{0};
    uint32_t totalPages_{1};
};

// Per-input-context state
class KarukanState : public InputContextProperty {
public:
    KarukanState(KarukanEngine* engine, InputContext* ic);
    ~KarukanState() override;

    void keyEvent(KeyEvent& keyEvent);
    /// Lazy one-time engine init (model load). Called from activate() so the
    /// first key event isn't blocked for seconds; keyEvent() falls back to it
    /// in case a frontend never activates.
    void initialize();
    void reset();
    void updateUI();

    ::KarukanEngine* rustEngine() { return rustEngine_; }

private:
    KarukanEngine* engine_;
    InputContext* ic_;
    ::KarukanEngine* rustEngine_{nullptr};
    bool engineInitialized_{false};
};

// Main engine class
class KarukanEngine : public InputMethodEngineV3 {
public:
    KarukanEngine(Instance* instance);
    ~KarukanEngine() override;

    void keyEvent(const InputMethodEntry& entry, KeyEvent& keyEvent) override;
    void reset(const InputMethodEntry& entry, InputContextEvent& event) override;
    void activate(const InputMethodEntry& entry, InputContextEvent& event) override;
    void deactivate(const InputMethodEntry& entry, InputContextEvent& event) override;

    Instance* instance() { return instance_; }

    void selectCandidate(InputContext* ic, int index);
    void nextCandidatePage(InputContext* ic);
    void prevCandidatePage(InputContext* ic);

    auto& factory() { return factory_; }

private:
    Instance* instance_;
    FactoryFor<KarukanState> factory_;
};

class KarukanEngineFactory : public AddonFactory {
public:
    AddonInstance* create(AddonManager* manager) override {
        return new KarukanEngine(manager->instance());
    }
};

}  // namespace fcitx

#endif  // FCITX5_KARUKAN_KARUKAN_H
