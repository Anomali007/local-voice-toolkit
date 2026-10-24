import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSTT } from "../hooks/useSTT";
import { usePermissions } from "../hooks/usePermissions";
import WaveformViz from "./WaveformViz";
import {
  AccessibilityIcon,
  AlertIcon,
  CheckIcon,
  CopyIcon,
  Keys,
  KeyboardIcon,
  MicIcon,
  Section,
  Spinner,
  StopIcon,
} from "./ui";

interface HotkeySettings {
  stt_hotkey: string;
  tts_hotkey: string;
}

export default function DictationPanel() {
  const { isRecording, isTranscribing, transcript, error, startRecording, stopRecording } = useSTT();
  const permissions = usePermissions();
  const [hotkeys, setHotkeys] = useState<HotkeySettings | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    invoke<HotkeySettings>("get_settings")
      .then(setHotkeys)
      .catch((err) => console.error("Failed to load hotkeys:", err));
  }, []);

  const handleCopy = async () => {
    if (transcript) {
      await navigator.clipboard.writeText(transcript);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const openPane = async (pane: "microphone" | "accessibility" | "input_monitoring") => {
    try {
      await invoke("open_system_settings", { pane });
    } catch (err) {
      console.error("Failed to open system settings:", err);
    }
  };

  const missing =
    permissions &&
    (!permissions.microphone || !permissions.accessibility || permissions.input_monitoring === false);
  const headline = isRecording
    ? "Listening…"
    : isTranscribing
    ? "Transcribing…"
    : missing
    ? "Needs a permission"
    : "Ready to dictate";

  return (
    <div className="space-y-6">
      {/* Hero */}
      <section className="card relative overflow-hidden">
        <div
          aria-hidden="true"
          className={`pointer-events-none absolute inset-x-0 top-0 h-40 transition-opacity duration-300 ${
            isRecording ? "opacity-40" : "opacity-[0.16]"
          }`}
          style={{
            backgroundImage: isRecording
              ? "radial-gradient(ellipse at 50% -20%, rgb(var(--c-rec)), transparent 70%)"
              : "radial-gradient(ellipse at 50% -20%, rgb(var(--c-accent)), transparent 70%)",
          }}
        />
        <div className="relative flex flex-col items-center px-6 pb-6 pt-8 text-center">
          <h1 className="page-title" aria-live="polite">
            {headline}
          </h1>

          {hotkeys ? (
            <>
              <div className="mt-5">
                <Keys hotkey={hotkeys.stt_hotkey} large />
              </div>
              <p className="mt-3 max-w-[38ch] text-[13px] text-muted">
                Tap or hold to talk. Your words are typed wherever the cursor is.
              </p>
            </>
          ) : (
            <p className="mt-3 text-[13px] text-muted">Dictate into any app with a shortcut.</p>
          )}

          <div className={`w-full max-w-sm ${isRecording || isTranscribing ? "mt-6 h-10" : ""}`}>
            {isRecording ? (
              <div className="h-10 rounded-lg bg-rec/10 px-3">
                <WaveformViz isActive={isRecording} />
              </div>
            ) : isTranscribing ? (
              <div className="flex h-10 items-center justify-center">
                <Spinner />
              </div>
            ) : null}
          </div>

          <button
            onClick={isRecording ? stopRecording : startRecording}
            disabled={isTranscribing}
            className={`${isRecording ? "btn-danger-solid" : "btn-secondary"} btn-lg mt-6`}
          >
            {isRecording ? (
              <>
                <StopIcon className="h-4 w-4" /> Stop recording
              </>
            ) : (
              <>
                <MicIcon className="h-4 w-4" /> Record from here
              </>
            )}
          </button>
        </div>
      </section>

      {error && (
        <p role="alert" className="alert-danger">
          {error}
        </p>
      )}

      {/* Transcript */}
      {transcript && (
        <Section title="Last transcript">
          <div className="card p-4">
            <p className="selectable whitespace-pre-wrap text-[14px] leading-relaxed text-fg">{transcript}</p>
            <div className="mt-3 flex justify-end">
              <button onClick={handleCopy} className="btn-quiet">
                {copied ? <CheckIcon className="h-4 w-4 text-ok" /> : <CopyIcon className="h-4 w-4" />}
                {copied ? "Copied" : "Copy"}
              </button>
            </div>
          </div>
        </Section>
      )}

      {/* Shortcuts */}
      <Section title="Shortcuts">
        <div className="card card-list">
          <div className="row">
            <span>Dictate</span>
            {hotkeys ? <Keys hotkey={hotkeys.stt_hotkey} /> : null}
          </div>
          <div className="row">
            <span>Read selection aloud</span>
            {hotkeys ? <Keys hotkey={hotkeys.tts_hotkey} /> : null}
          </div>
        </div>
      </Section>

      {/* Permissions */}
      <Section title="Access">
        <div className="card card-list">
          <PermissionLine
            Icon={MicIcon}
            label="Microphone"
            granted={permissions?.microphone}
            onFix={() => openPane("microphone")}
          />
          <PermissionLine
            Icon={AccessibilityIcon}
            label="Accessibility, for typing text"
            granted={permissions?.accessibility}
            onFix={() => openPane("accessibility")}
          />
          {permissions?.input_monitoring !== undefined && (
            <PermissionLine
              Icon={KeyboardIcon}
              label="Input Monitoring"
              hint="Needed for the right Option key"
              granted={permissions.input_monitoring}
              onFix={() => openPane("input_monitoring")}
            />
          )}
        </div>
      </Section>
    </div>
  );
}

function PermissionLine({
  Icon,
  label,
  hint,
  granted,
  onFix,
}: {
  Icon: (p: { className?: string }) => JSX.Element;
  label: string;
  hint?: string;
  granted?: boolean;
  onFix: () => void;
}) {
  return (
    <div className="row">
      <span className="flex items-center gap-2.5">
        <Icon className="h-[18px] w-[18px] text-accent" />
        <span>
          {label}
          {hint && <span className="block text-xs text-muted">{hint}</span>}
        </span>
      </span>
      {granted === undefined ? (
        <span className="text-xs text-subtle">Checking…</span>
      ) : granted ? (
        <span className="badge-ok gap-1">
          <CheckIcon className="h-3 w-3" /> Allowed
        </span>
      ) : (
        <span className="flex items-center gap-2">
          <span className="badge-danger gap-1">
            <AlertIcon className="h-3 w-3" /> Off
          </span>
          <button onClick={onFix} className="btn-secondary">
            Open System Settings
          </button>
        </span>
      )}
    </div>
  );
}
