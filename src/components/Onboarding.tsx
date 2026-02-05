import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";

type Step = "welcome" | "permissions" | "models" | "hotkeys" | "complete";

interface OnboardingProps {
  onComplete: () => void;
}

interface Model {
  id: string;
  name: string;
  model_type: string;
  size_display: string;
  status: string;
}

export default function Onboarding({ onComplete }: OnboardingProps) {
  const [step, setStep] = useState<Step>("welcome");
  const [models, setModels] = useState<Model[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [downloading, setDownloading] = useState<Set<string>>(new Set());
  const [modelsReady, setModelsReady] = useState(false);

  // Load models on mount
  useEffect(() => {
    loadModels();

    const unlisten = listen<[string, { percentage: number }]>(
      "model-download-progress",
      (event) => {
        const [modelId, progress] = event.payload;
        setDownloadProgress((prev) => ({
          ...prev,
          [modelId]: progress.percentage,
        }));
      }
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Check if required models are downloaded
  useEffect(() => {
    const sttModel = models.find((m) => m.id === "ggml-base.en.bin");
    const ttsModel1 = models.find((m) => m.id === "kokoro-v1.0.onnx");
    const ttsModel2 = models.find((m) => m.id === "voices-v1.0.bin");

    const ready =
      sttModel?.status === "downloaded" &&
      ttsModel1?.status === "downloaded" &&
      ttsModel2?.status === "downloaded";

    setModelsReady(ready);
  }, [models]);

  const loadModels = async () => {
    try {
      const result = await invoke<Model[]>("list_models");
      setModels(result);
    } catch (err) {
      console.error("Failed to load models:", err);
    }
  };

  const downloadModel = async (modelId: string) => {
    try {
      setDownloading((prev) => new Set(prev).add(modelId));
      setDownloadProgress((prev) => ({ ...prev, [modelId]: 0 }));
      await invoke("download_model", { modelId });
      await loadModels();
    } catch (err) {
      console.error("Failed to download model:", err);
    } finally {
      setDownloading((prev) => {
        const next = new Set(prev);
        next.delete(modelId);
        return next;
      });
    }
  };

  const downloadRecommendedModels = async () => {
    // Download in sequence: STT first, then TTS
    await downloadModel("ggml-base.en.bin");
    await downloadModel("kokoro-v1.0.onnx");
    await downloadModel("voices-v1.0.bin");
  };

  const handleComplete = async () => {
    try {
      const settings = await invoke<Record<string, unknown>>("get_settings");
      await invoke("update_settings", {
        settings: { ...settings, onboarding_completed: true },
      });
      onComplete();
    } catch (err) {
      console.error("Failed to save onboarding status:", err);
      onComplete();
    }
  };

  const openSystemPreferences = (pane: string) => {
    open(`x-apple.systempreferences:com.apple.preference.security?${pane}`);
  };

  const nextStep = () => {
    const steps: Step[] = ["welcome", "permissions", "models", "hotkeys", "complete"];
    const currentIndex = steps.indexOf(step);
    if (currentIndex < steps.length - 1) {
      setStep(steps[currentIndex + 1]);
    }
  };

  const prevStep = () => {
    const steps: Step[] = ["welcome", "permissions", "models", "hotkeys", "complete"];
    const currentIndex = steps.indexOf(step);
    if (currentIndex > 0) {
      setStep(steps[currentIndex - 1]);
    }
  };

  return (
    <div className="fixed inset-0 bg-slate-900 z-50 flex items-center justify-center p-6">
      <div className="w-full max-w-lg">
        {/* Progress dots */}
        <div className="flex justify-center gap-2 mb-8">
          {["welcome", "permissions", "models", "hotkeys", "complete"].map((s, i) => (
            <div
              key={s}
              className={`w-2 h-2 rounded-full transition-colors ${
                s === step ? "bg-sky-500" : i < ["welcome", "permissions", "models", "hotkeys", "complete"].indexOf(step) ? "bg-sky-500/50" : "bg-slate-700"
              }`}
            />
          ))}
        </div>

        {/* Step content */}
        <div className="bg-slate-800 rounded-2xl p-8 shadow-2xl">
          {step === "welcome" && (
            <WelcomeStep onNext={nextStep} />
          )}
          {step === "permissions" && (
            <PermissionsStep
              onNext={nextStep}
              onBack={prevStep}
              openSystemPreferences={openSystemPreferences}
            />
          )}
          {step === "models" && (
            <ModelsStep
              models={models}
              downloadProgress={downloadProgress}
              downloading={downloading}
              modelsReady={modelsReady}
              onDownloadAll={downloadRecommendedModels}
              onNext={nextStep}
              onBack={prevStep}
            />
          )}
          {step === "hotkeys" && (
            <HotkeysStep onNext={nextStep} onBack={prevStep} />
          )}
          {step === "complete" && (
            <CompleteStep onComplete={handleComplete} onBack={prevStep} />
          )}
        </div>
      </div>
    </div>
  );
}

function WelcomeStep({ onNext }: { onNext: () => void }) {
  return (
    <div className="text-center">
      <div className="w-20 h-20 mx-auto mb-6 bg-gradient-to-br from-sky-400 to-indigo-500 rounded-2xl flex items-center justify-center">
        <span className="text-4xl">B³</span>
      </div>
      <h1 className="text-2xl font-bold text-white mb-2">Welcome to Blah³</h1>
      <p className="text-slate-400 mb-8">
        Your local voice toolkit for macOS. Speech-to-text and text-to-speech,
        powered by AI — running 100% offline on your Mac.
      </p>

      <div className="space-y-3 text-left mb-8">
        <Feature icon="🎤" title="Dictation Mode" description="Press a hotkey, speak, and text appears wherever your cursor is" />
        <Feature icon="📖" title="Screen Reader" description="Select any text and hear it read aloud with natural AI voices" />
        <Feature icon="🔒" title="100% Offline" description="All processing happens locally — your voice never leaves your Mac" />
      </div>

      <button
        onClick={onNext}
        className="w-full py-3 bg-sky-500 hover:bg-sky-600 text-white font-medium rounded-lg transition-colors"
      >
        Get Started
      </button>
    </div>
  );
}

function Feature({ icon, title, description }: { icon: string; title: string; description: string }) {
  return (
    <div className="flex gap-3">
      <span className="text-xl">{icon}</span>
      <div>
        <h3 className="text-sm font-medium text-white">{title}</h3>
        <p className="text-xs text-slate-400">{description}</p>
      </div>
    </div>
  );
}

function PermissionsStep({
  onNext,
  onBack,
  openSystemPreferences,
}: {
  onNext: () => void;
  onBack: () => void;
  openSystemPreferences: (pane: string) => void;
}) {
  return (
    <div>
      <h2 className="text-xl font-bold text-white mb-2">Permissions Required</h2>
      <p className="text-slate-400 text-sm mb-6">
        Blah³ needs two permissions to work properly. Click each button to open System Settings.
      </p>

      <div className="space-y-4 mb-8">
        <PermissionCard
          icon="🎤"
          title="Microphone Access"
          description="Required for speech-to-text dictation"
          buttonText="Open Microphone Settings"
          onClick={() => openSystemPreferences("Privacy_Microphone")}
        />
        <PermissionCard
          icon="♿"
          title="Accessibility Access"
          description="Required to read selected text and paste transcriptions"
          buttonText="Open Accessibility Settings"
          onClick={() => openSystemPreferences("Privacy_Accessibility")}
        />
      </div>

      <p className="text-xs text-slate-500 mb-6 text-center">
        After granting permissions, you may need to restart Blah³ for changes to take effect.
      </p>

      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="flex-1 py-3 bg-slate-700 hover:bg-slate-600 text-white font-medium rounded-lg transition-colors"
        >
          Back
        </button>
        <button
          onClick={onNext}
          className="flex-1 py-3 bg-sky-500 hover:bg-sky-600 text-white font-medium rounded-lg transition-colors"
        >
          Continue
        </button>
      </div>
    </div>
  );
}

function PermissionCard({
  icon,
  title,
  description,
  buttonText,
  onClick,
}: {
  icon: string;
  title: string;
  description: string;
  buttonText: string;
  onClick: () => void;
}) {
  return (
    <div className="bg-slate-700/50 rounded-lg p-4">
      <div className="flex items-start gap-3">
        <span className="text-2xl">{icon}</span>
        <div className="flex-1">
          <h3 className="font-medium text-white">{title}</h3>
          <p className="text-xs text-slate-400 mt-1">{description}</p>
          <button
            onClick={onClick}
            className="mt-3 px-3 py-1.5 text-xs bg-slate-600 hover:bg-slate-500 text-white rounded transition-colors"
          >
            {buttonText}
          </button>
        </div>
      </div>
    </div>
  );
}

function ModelsStep({
  models,
  downloadProgress,
  downloading,
  modelsReady,
  onDownloadAll,
  onNext,
  onBack,
}: {
  models: Model[];
  downloadProgress: Record<string, number>;
  downloading: Set<string>;
  modelsReady: boolean;
  onDownloadAll: () => void;
  onNext: () => void;
  onBack: () => void;
}) {
  const [isDownloading, setIsDownloading] = useState(false);

  const handleDownload = async () => {
    setIsDownloading(true);
    await onDownloadAll();
    setIsDownloading(false);
  };

  const sttModel = models.find((m) => m.id === "ggml-base.en.bin");
  const ttsModel1 = models.find((m) => m.id === "kokoro-v1.0.onnx");
  const ttsModel2 = models.find((m) => m.id === "voices-v1.0.bin");

  const totalSize = "~477 MB";
  const anyDownloading = downloading.size > 0;

  return (
    <div>
      <h2 className="text-xl font-bold text-white mb-2">Download AI Models</h2>
      <p className="text-slate-400 text-sm mb-6">
        Blah³ uses local AI models for speech recognition and synthesis.
        We recommend starting with these models:
      </p>

      <div className="space-y-3 mb-6">
        <ModelItem
          name="Whisper Base (English)"
          size="142 MB"
          description="Fast, accurate speech-to-text"
          status={sttModel?.status}
          progress={downloadProgress["ggml-base.en.bin"]}
          isDownloading={downloading.has("ggml-base.en.bin")}
        />
        <ModelItem
          name="Kokoro 82M"
          size="330 MB"
          description="Natural text-to-speech voices"
          status={ttsModel1?.status}
          progress={downloadProgress["kokoro-v1.0.onnx"]}
          isDownloading={downloading.has("kokoro-v1.0.onnx")}
        />
        <ModelItem
          name="Kokoro Voice Styles"
          size="5 MB"
          description="Voice style vectors"
          status={ttsModel2?.status}
          progress={downloadProgress["voices-v1.0.bin"]}
          isDownloading={downloading.has("voices-v1.0.bin")}
        />
      </div>

      {!modelsReady && !anyDownloading && (
        <button
          onClick={handleDownload}
          disabled={isDownloading}
          className="w-full py-3 mb-4 bg-sky-500 hover:bg-sky-600 disabled:bg-slate-600 text-white font-medium rounded-lg transition-colors"
        >
          Download All ({totalSize})
        </button>
      )}

      {modelsReady && (
        <div className="flex items-center justify-center gap-2 mb-4 py-3 bg-green-500/10 rounded-lg">
          <span className="text-green-400">✓</span>
          <span className="text-green-400 font-medium">All models ready!</span>
        </div>
      )}

      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="flex-1 py-3 bg-slate-700 hover:bg-slate-600 text-white font-medium rounded-lg transition-colors"
        >
          Back
        </button>
        <button
          onClick={onNext}
          disabled={!modelsReady}
          className="flex-1 py-3 bg-sky-500 hover:bg-sky-600 disabled:bg-slate-600 disabled:cursor-not-allowed text-white font-medium rounded-lg transition-colors"
        >
          {modelsReady ? "Continue" : "Download models first"}
        </button>
      </div>
    </div>
  );
}

function ModelItem({
  name,
  size,
  description,
  status,
  progress,
  isDownloading,
}: {
  name: string;
  size: string;
  description: string;
  status?: string;
  progress?: number;
  isDownloading: boolean;
}) {
  const isDownloaded = status === "downloaded";

  return (
    <div className="bg-slate-700/50 rounded-lg p-3">
      <div className="flex items-center justify-between">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-medium text-white">{name}</h3>
            {isDownloaded && (
              <span className="text-green-400 text-xs">✓</span>
            )}
          </div>
          <p className="text-xs text-slate-400">{description}</p>
        </div>
        <span className="text-xs text-slate-500">{size}</span>
      </div>
      {isDownloading && (
        <div className="mt-2">
          <div className="h-1.5 bg-slate-600 rounded-full overflow-hidden">
            <div
              className="h-full bg-sky-500 transition-all duration-300"
              style={{ width: `${progress || 0}%` }}
            />
          </div>
          <p className="text-xs text-slate-400 mt-1">{progress || 0}%</p>
        </div>
      )}
    </div>
  );
}

function HotkeysStep({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  return (
    <div>
      <h2 className="text-xl font-bold text-white mb-2">Keyboard Shortcuts</h2>
      <p className="text-slate-400 text-sm mb-6">
        Use these hotkeys from anywhere on your Mac:
      </p>

      <div className="space-y-4 mb-8">
        <HotkeyCard
          keys={["⌘", "⇧", "D"]}
          title="Dictation"
          description="Hold to record, release to transcribe. Text is automatically pasted."
        />
        <HotkeyCard
          keys={["⌘", "⇧", "S"]}
          title="Read Aloud"
          description="Select text anywhere, then press to hear it spoken."
        />
      </div>

      <p className="text-xs text-slate-500 mb-6 text-center">
        You can customize these shortcuts in Settings.
      </p>

      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="flex-1 py-3 bg-slate-700 hover:bg-slate-600 text-white font-medium rounded-lg transition-colors"
        >
          Back
        </button>
        <button
          onClick={onNext}
          className="flex-1 py-3 bg-sky-500 hover:bg-sky-600 text-white font-medium rounded-lg transition-colors"
        >
          Continue
        </button>
      </div>
    </div>
  );
}

function HotkeyCard({
  keys,
  title,
  description,
}: {
  keys: string[];
  title: string;
  description: string;
}) {
  return (
    <div className="bg-slate-700/50 rounded-lg p-4">
      <div className="flex items-center gap-4">
        <div className="flex gap-1">
          {keys.map((key, i) => (
            <span key={i}>
              <kbd className="px-2 py-1 bg-slate-600 rounded text-white text-sm font-mono">
                {key}
              </kbd>
              {i < keys.length - 1 && <span className="text-slate-500 mx-0.5">+</span>}
            </span>
          ))}
        </div>
        <div>
          <h3 className="font-medium text-white">{title}</h3>
          <p className="text-xs text-slate-400">{description}</p>
        </div>
      </div>
    </div>
  );
}

function CompleteStep({ onComplete, onBack }: { onComplete: () => void; onBack: () => void }) {
  return (
    <div className="text-center">
      <div className="w-20 h-20 mx-auto mb-6 bg-green-500/20 rounded-full flex items-center justify-center">
        <span className="text-4xl">✓</span>
      </div>
      <h2 className="text-2xl font-bold text-white mb-2">You're All Set!</h2>
      <p className="text-slate-400 mb-8">
        Blah³ is ready to use. Try pressing <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300 text-sm">⌘</kbd> + <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300 text-sm">⇧</kbd> + <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300 text-sm">D</kbd> to dictate!
      </p>

      <div className="bg-slate-700/50 rounded-lg p-4 mb-8 text-left">
        <h3 className="text-sm font-medium text-white mb-2">Quick Tips:</h3>
        <ul className="text-xs text-slate-400 space-y-1">
          <li>• The app runs in your menu bar — look for the icon up top</li>
          <li>• Silence detection will auto-stop recording when you pause</li>
          <li>• Download more models in the Models tab for better accuracy</li>
        </ul>
      </div>

      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="flex-1 py-3 bg-slate-700 hover:bg-slate-600 text-white font-medium rounded-lg transition-colors"
        >
          Back
        </button>
        <button
          onClick={onComplete}
          className="flex-1 py-3 bg-green-500 hover:bg-green-600 text-white font-medium rounded-lg transition-colors"
        >
          Start Using Blah³
        </button>
      </div>
    </div>
  );
}
