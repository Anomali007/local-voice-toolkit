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
      <div className="min-h-screen bg-slate-900 flex items-center justify-center">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-sky-500" />
      </div>
    );
  }

  // Show onboarding if not completed
  if (showOnboarding) {
    return <Onboarding onComplete={handleOnboardingComplete} />;
  }

  const tabs: { id: Tab; label: string; icon: string }[] = [
    { id: "dictation", label: "Dictation", icon: "🎤" },
    { id: "dictionary", label: "Words", icon: "📚" },
    { id: "reader", label: "Reader", icon: "📖" },
    { id: "models", label: "Models", icon: "🧠" },
    { id: "settings", label: "Settings", icon: "⚙️" },
  ];

  return (
    <div className="min-h-screen bg-slate-900 text-slate-100">
      {/* Global status indicator for hotkey actions */}
      <StatusIndicator />
      {/* Header */}
      <header className="bg-slate-800 border-b border-slate-700 px-4 py-3">
        <div className="flex items-center justify-between">
          <h1 className="text-xl font-bold text-white">Blah³</h1>
          <span className="text-xs text-slate-400">Voice Toolkit</span>
        </div>
      </header>

      {/* Tab Navigation */}
      <nav className="bg-slate-800/50 border-b border-slate-700">
        <div className="flex">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
                activeTab === tab.id
                  ? "text-sky-400 border-b-2 border-sky-400 bg-slate-800/50"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/30"
              }`}
            >
              <span className="mr-2">{tab.icon}</span>
              {tab.label}
            </button>
          ))}
        </div>
      </nav>

      {/* Content */}
      <main className="p-6">
        {activeTab === "dictation" && <DictationPanel />}
        {activeTab === "dictionary" && <DictionaryPanel />}
        {activeTab === "reader" && <ScreenReader />}
        {activeTab === "models" && <ModelManager />}
        {activeTab === "settings" && <SettingsPanel />}
      </main>
    </div>
  );
}

export default App;
