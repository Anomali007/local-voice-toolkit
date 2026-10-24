import type { ReactNode } from "react";
import { AccessibilityIcon, CheckIcon, LockIcon, MicIcon, SpeakerIcon } from "./ui";
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { usePermissions } from "../hooks/usePermissions";

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
  const [downloadErrors, setDownloadErrors] = useState<Record<string, string>>({});

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

  const downloadModel = async (modelId: string): Promise<boolean> => {
    try {
      setDownloadErrors((prev) => {
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
      setDownloading((prev) => new Set(prev).add(modelId));
      setDownloadProgress((prev) => ({ ...prev, [modelId]: 0 }));
      await invoke("download_model", { modelId });
      await loadModels();
      return true;
    } catch (err) {
      console.error("Failed to download model:", err);
      setDownloadErrors((prev) => ({ ...prev, [modelId]: `Download failed: ${String(err)}` }));
      return false;
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
    // Stop at the first failure so the error stays attached to the right item.
    for (const id of ["ggml-base.en.bin", "kokoro-v1.0.onnx", "voices-v1.0.bin"]) {
      if (!(await downloadModel(id))) return;
    }
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

  const openSystemPreferences = async (pane: string) => {
    try {
      if (pane === "Privacy_Microphone") {
        await invoke("open_system_settings", { pane: "microphone" });
      } else if (pane === "Privacy_Accessibility") {
        await invoke("open_system_settings", { pane: "accessibility" });
      }
    } catch (e) {
      console.error("Failed to open system settings:", e);
    }
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
    <div className="fixed inset-0 bg-canvas z-50 flex items-center justify-center p-8 overflow-y-auto">
      <div className="w-full max-w-lg">
        {/* Progress dots */}
        <div className="flex justify-center gap-2 mb-8">
          {["welcome", "permissions", "models", "hotkeys", "complete"].map((s, i) => (
            <div
              key={s}
              className={`w-2 h-2 rounded-full transition-colors ${
                s === step ? "bg-accent" : i < ["welcome", "permissions", "models", "hotkeys", "complete"].indexOf(step) ? "bg-accent/50" : "bg-sunken"
              }`}
            />
          ))}
        </div>

        {/* Step content */}
        <div className="bg-surface rounded-2xl p-8 shadow-pop">
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
              downloadErrors={downloadErrors}
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
      <div className="w-20 h-20 mx-auto mb-6 bg-accent text-accent-fg rounded-2xl flex items-center justify-center">
        <span className="text-3xl font-semibold tracking-tight">B³</span>
      </div>
      <h1 className="text-2xl font-bold text-fg mb-2">Welcome to Blah³</h1>
      <p className="text-muted mb-8">
        Your local voice toolkit for macOS. Speech-to-text and text-to-speech,
        powered by AI — running 100% offline on your Mac.
      </p>

      <div className="space-y-3 text-left mb-8">
        <Feature icon={<MicIcon className="h-5 w-5" />} title="Dictation Mode" description="Press a hotkey, speak, and text appears wherever your cursor is" />
        <Feature icon={<SpeakerIcon className="h-5 w-5" />} title="Screen Reader" description="Select any text and hear it read aloud with natural AI voices" />
        <Feature icon={<LockIcon className="h-5 w-5" />} title="100% Offline" description="All processing happens locally — your voice never leaves your Mac" />
      </div>

      <button
        onClick={onNext}
        className="btn-primary btn-lg w-full"
      >
        Get Started
      </button>
    </div>
  );
}

function Feature({ icon, title, description }: { icon: ReactNode; title: string; description: string }) {
  return (
    <div className="flex gap-3">
      <span className="mt-0.5 text-accent">{icon}</span>
      <div>
        <h3 className="text-sm font-medium text-fg">{title}</h3>
        <p className="text-xs text-muted">{description}</p>
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
  const permissions = usePermissions();

  return (
    <div>
      <h2 className="text-xl font-bold text-fg mb-2">Permissions Required</h2>
      <p className="text-muted text-sm mb-6">
        Blah³ needs two permissions to work properly. Click each button and approve the system dialog.
      </p>

      <div className="space-y-4 mb-8">
        <PermissionCard
          icon={<MicIcon className="h-5 w-5" />}
          title="Microphone Access"
          description="Required for speech-to-text dictation"
          buttonText="Grant Microphone Access"
          granted={permissions?.microphone}
          onClick={async () => {
            try {
              const result = await invoke<string>("request_microphone_access");
              if (result === "denied" || result === "restricted") {
                openSystemPreferences("Privacy_Microphone");
              }
            } catch {
              openSystemPreferences("Privacy_Microphone");
            }
          }}
        />
        <PermissionCard
          icon={<AccessibilityIcon className="h-5 w-5" />}
          title="Accessibility Access"
          description="Required to read selected text and paste transcriptions"
          buttonText="Grant Accessibility Access"
          granted={permissions?.accessibility}
          onClick={async () => {
            try {
              // Shows the system dialog that offers to open System Settings
              await invoke<boolean>("request_accessibility_access");
            } catch {
              openSystemPreferences("Privacy_Accessibility");
            }
          }}
        />
      </div>

      <p className="text-xs text-subtle mb-6 text-center">
        After granting permissions, you may need to restart Blah³ for changes to take effect.
      </p>

      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="btn-secondary btn-lg flex-1"
        >
          Back
        </button>
        <button
          onClick={onNext}
          className="btn-primary btn-lg flex-1"
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
  granted,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  buttonText: string;
  granted?: boolean;
  onClick: () => void;
}) {
  return (
    <div className={`bg-sunken rounded-lg p-4 border ${
      granted === true ? "border-ok/40" : granted === false ? "border-danger/40" : "border-transparent"
    }`}>
      <div className="flex items-start gap-3">
        <span className="mt-0.5 text-accent">{icon}</span>
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <h3 className="font-medium text-fg">{title}</h3>
            {granted !== undefined && (
              <span className={`text-xs ${granted ? "text-ok" : "text-danger"}`}>
                {granted ? "Granted" : "Not Granted"}
              </span>
            )}
          </div>
          <p className="text-xs text-muted mt-1">{description}</p>
          {!granted && (
            <button
              onClick={onClick}
              className="btn-secondary mt-3"
            >
              {buttonText}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function ModelsStep({
  downloadErrors,
  models,
  downloadProgress,
  downloading,
  modelsReady,
  onDownloadAll,
  onNext,
  onBack,
}: {
  downloadErrors: Record<string, string>;
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
      <h2 className="text-xl font-bold text-fg mb-2">Download AI Models</h2>
      <p className="text-muted text-sm mb-6">
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
          error={downloadErrors["ggml-base.en.bin"]}
        />
        <ModelItem
          name="Kokoro 82M"
          size="330 MB"
          description="Natural text-to-speech voices"
          status={ttsModel1?.status}
          progress={downloadProgress["kokoro-v1.0.onnx"]}
          isDownloading={downloading.has("kokoro-v1.0.onnx")}
          error={downloadErrors["kokoro-v1.0.onnx"]}
        />
        <ModelItem
          name="Kokoro Voice Styles"
          size="5 MB"
          description="Voice style vectors"
          status={ttsModel2?.status}
          progress={downloadProgress["voices-v1.0.bin"]}
          isDownloading={downloading.has("voices-v1.0.bin")}
          error={downloadErrors["voices-v1.0.bin"]}
        />
      </div>

      {!modelsReady && !anyDownloading && (
        <button
          onClick={handleDownload}
          disabled={isDownloading}
          className="btn-primary btn-lg w-full mb-4"
        >
          {Object.keys(downloadErrors).length > 0 ? "Retry download" : "Download All"} ({totalSize})
        </button>
      )}

      {modelsReady && (
        <div className="btn-primary btn-lg mb-4">
          <CheckIcon className="h-4 w-4 text-ok" />
          <span className="text-ok font-medium">All models ready!</span>
        </div>
      )}

      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="btn-secondary btn-lg flex-1"
        >
          Back
        </button>
        <button
          onClick={onNext}
          disabled={!modelsReady}
          className="btn-primary btn-lg flex-1"
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
  error,
}: {
  error?: string;
  name: string;
  size: string;
  description: string;
  status?: string;
  progress?: number;
  isDownloading: boolean;
}) {
  const isDownloaded = status === "downloaded";

  return (
    <div className="bg-sunken rounded-lg p-3">
      <div className="flex items-center justify-between">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-medium text-fg">{name}</h3>
            {isDownloaded && (
              <CheckIcon className="h-3.5 w-3.5 text-ok" />
            )}
          </div>
          <p className="text-xs text-muted">{description}</p>
        </div>
        <span className="text-xs text-subtle">{size}</span>
      </div>
      {isDownloading && (
        <div className="mt-2">
          <div className="h-1.5 bg-line rounded-full overflow-hidden">
            <div
              className="h-full bg-accent transition-all duration-300"
              style={{ width: `${progress || 0}%` }}
            />
          </div>
          <p className="text-xs text-muted mt-1">{progress || 0}%</p>
        </div>
      )}
      {error && (
        <p role="alert" className="mt-2 text-xs text-danger break-words">
          {error}
        </p>
      )}
    </div>
  );
}

function HotkeysStep({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  return (
    <div>
      <h2 className="text-xl font-bold text-fg mb-2">Keyboard Shortcuts</h2>
      <p className="text-muted text-sm mb-6">
        Use these hotkeys from anywhere on your Mac:
      </p>

      <div className="space-y-4 mb-8">
        <HotkeyCard
          keys={["right ⌥"]}
          title="Dictation"
          description="Tap to start, tap again to stop — or hold to talk. Text is automatically pasted."
        />
        <HotkeyCard
          keys={["⌥", "S"]}
          title="Read Aloud"
          description="Select text anywhere, then press to hear it spoken."
        />
      </div>

      <p className="text-xs text-subtle mb-6 text-center">
        You can customize these shortcuts in Settings.
      </p>

      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="btn-secondary btn-lg flex-1"
        >
          Back
        </button>
        <button
          onClick={onNext}
          className="btn-primary btn-lg flex-1"
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
    <div className="bg-sunken rounded-lg p-4">
      <div className="flex items-center gap-4">
        <div className="flex gap-1">
          {keys.map((key, i) => (
            <span key={i}>
              <kbd className="kbd">
                {key}
              </kbd>
              {i < keys.length - 1 && <span className="text-subtle mx-0.5">+</span>}
            </span>
          ))}
        </div>
        <div>
          <h3 className="font-medium text-fg">{title}</h3>
          <p className="text-xs text-muted">{description}</p>
        </div>
      </div>
    </div>
  );
}

function CompleteStep({ onComplete, onBack }: { onComplete: () => void; onBack: () => void }) {
  return (
    <div className="text-center">
      <div className="w-20 h-20 mx-auto mb-6 bg-ok/15 rounded-full flex items-center justify-center">
        <CheckIcon className="h-9 w-9 text-ok" />
      </div>
      <h2 className="text-2xl font-bold text-fg mb-2">You're All Set!</h2>
      <p className="text-muted mb-8">
        Blah³ is ready to use. Tap the <kbd className="kbd">Right ⌥</kbd> key to dictate!
      </p>

      <div className="bg-sunken rounded-lg p-4 mb-8 text-left">
        <h3 className="text-sm font-medium text-fg mb-2">Quick Tips:</h3>
        <ul className="text-xs text-muted space-y-1">
          <li>• The app runs in your menu bar — look for the icon up top</li>
          <li>• Silence detection will auto-stop recording when you pause</li>
          <li>• Download more models in the Models tab for better accuracy</li>
        </ul>
      </div>

      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="btn-secondary btn-lg flex-1"
        >
          Back
        </button>
        <button
          onClick={onComplete}
          className="btn-primary btn-lg flex-1"
        >
          Start Using Blah³
        </button>
      </div>
    </div>
  );
}
