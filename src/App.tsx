import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import DictationPanel from "./components/DictationPanel";
import ScreenReader from "./components/ScreenReader";
import DictionaryPanel from "./components/DictionaryPanel";
import ModelManager from "./components/ModelManager";
import SettingsPanel from "./components/SettingsPanel";
import StatusIndicator from "./components/StatusIndicator";
import Onboarding from "./components/Onboarding";
import { BookIcon, ChipIcon, GearIcon, MicIcon, SpeakerIcon, Spinner } from "./components/ui";

type Tab = "dictation" | "dictionary" | "reader" | "models" | "settings";

interface AppSettings {
  onboarding_completed: boolean;
}

function App() {
  const [activeTab, setActiveTab] = useState<Tab>("dictation");
  const [showOnboarding, setShowOnboarding] = useState<boolean | null>(null);

  useEffect(() => {
    checkOnboardingStatus();
  }, []);

  // "Settings…" in the menu bar menu opens the window on a given tab.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<Tab>("navigate", (event) => setActiveTab(event.payload)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const checkOnboardingStatus = async () => {
    try {
      const settings = await invoke<AppSettings>("get_settings");
      setShowOnboarding(!settings.onboarding_completed);
    } catch (err) {
      console.error("Failed to check onboarding status:", err);
      setShowOnboarding(false);
    }
  };

  const handleOnboardingComplete = () => {
    setShowOnboarding(false);
  };

  // Show loading state while checking onboarding status
  if (showOnboarding === null) {
    return (
      <div className="flex h-screen items-center justify-center bg-canvas">
        <Spinner className="h-6 w-6" />
      </div>
    );
  }

  // Show onboarding if not completed
  if (showOnboarding) {
    return <Onboarding onComplete={handleOnboardingComplete} />;
  }

  const tabs: { id: Tab; label: string; Icon: (p: { className?: string }) => JSX.Element }[] = [
    { id: "dictation", label: "Dictation", Icon: MicIcon },
    { id: "dictionary", label: "Words", Icon: BookIcon },
    { id: "reader", label: "Reader", Icon: SpeakerIcon },
    { id: "models", label: "Models", Icon: ChipIcon },
    { id: "settings", label: "Settings", Icon: GearIcon },
  ];

  return (
    <div className="flex h-screen bg-canvas text-fg">
      {/* Global status indicator for hotkey actions */}
      <StatusIndicator />

      {/* Sidebar */}
      <nav
        aria-label="Sections"
        className="flex w-[184px] shrink-0 flex-col bg-sidebar px-3 pb-4 pt-5"
        style={{ boxShadow: "inset -0.5px 0 0 rgb(var(--c-line))" }}
      >
        <div className="mb-5 flex items-center gap-2 px-2">
          <span
            className="flex h-6 w-6 items-center justify-center rounded-md bg-brand shadow-sm"
            aria-hidden="true"
          >
            <MicIcon className="h-3.5 w-3.5" />
          </span>
          <span className="text-[15px] font-semibold tracking-[-0.01em]">Blah³</span>
        </div>

        <ul className="space-y-0.5">
          {tabs.map(({ id, label, Icon }) => {
            const active = activeTab === id;
            return (
              <li key={id}>
                <button
                  onClick={() => setActiveTab(id)}
                  aria-current={active ? "page" : undefined}
                  className={`flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] font-medium transition-colors ${
                    active
                      ? "bg-brand shadow-sm"
                      : "text-fg/80 hover:bg-fg/[0.06]"
                  }`}
                >
                  <Icon className="h-[18px] w-[18px]" />
                  {label}
                </button>
              </li>
            );
          })}
        </ul>

        <p className="mt-auto px-2 text-[11px] text-subtle">Runs fully offline</p>
      </nav>

      {/* Content */}
      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[600px] px-7 pb-10 pt-7">
          {activeTab === "dictation" && <DictationPanel />}
          {activeTab === "dictionary" && <DictionaryPanel />}
          {activeTab === "reader" && <ScreenReader />}
          {activeTab === "models" && <ModelManager />}
          {activeTab === "settings" && <SettingsPanel />}
        </div>
      </main>
    </div>
  );
}

export default App;
