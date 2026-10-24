import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { enable as enableAutostart, disable as disableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";
import { open } from "@tauri-apps/plugin-shell";
import HotkeyRecorder from "./HotkeyRecorder";
import { usePermissions } from "../hooks/usePermissions";
import { AccessibilityIcon, CheckIcon, KeyboardIcon, MicIcon, Section, SettingRow, Spinner, Toggle } from "./ui";

interface Settings {
  stt_hotkey: string;
  tts_hotkey: string;
  stt_model: string;
  tts_voice: string;
  tts_speed: number;
  auto_paste: boolean;
  launch_at_login: boolean;
  menu_bar_mode: boolean;
  // Silence detection settings
  silence_detection_enabled: boolean;
  silence_threshold: number;
  silence_duration: number;
  // Onboarding
  onboarding_completed: boolean;
  // Overlay
  overlay_position: string;
  overlay_size: string;
  overlay_max_lines: number;
  overlay_click_through: boolean;
  overlay_offset_x: number;
  overlay_offset_y: number;
  // Dictation extras
  remove_fillers: boolean;
  history_enabled: boolean;
  history_max_entries: number;
  sound_on_start: boolean;
  sound_on_stop: boolean;
  sound_start_name: string;
  sound_stop_name: string;
  sound_volume: number;
  hold_space_to_pause: boolean;
}

/** Mirrors the backend defaults, for a settings file written by an older build. */
const EXTRA_DEFAULTS = {
  remove_fillers: true,
  history_enabled: true,
  history_max_entries: 1000,
  sound_on_start: false,
  sound_on_stop: false,
  sound_start_name: "Tink",
  sound_stop_name: "Pop",
  sound_volume: 0.5,
  hold_space_to_pause: true,
};

const HISTORY_SIZES = [100, 500, 1000, 5000] as const;
const FALLBACK_SOUNDS = ["Tink", "Pop", "Glass", "Purr", "Morse", "Bottle", "Frog", "Ping", "Hero", "Submarine"];

const OVERLAY_POSITIONS = [
  ["top-left", "Top left"],
  ["top-center", "Top center"],
  ["top-right", "Top right"],
  ["bottom-left", "Bottom left"],
  ["bottom-center", "Bottom center"],
  ["bottom-right", "Bottom right"],
] as const;

const OVERLAY_SIZES = [
  ["compact", "Compact"],
  ["regular", "Regular"],
  ["large", "Large"],
] as const;

interface HardwareProfile {
  chip: string;
  chip_name: string;
  ram_gb: number;
  cpu_cores: number;
  has_neural_engine: boolean;
  has_metal: boolean;
  recommended_tier: string;
}

export default function SettingsPanel() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [hardware, setHardware] = useState<HardwareProfile | null>(null);
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [sounds, setSounds] = useState<string[]>(FALLBACK_SOUNDS);
  const [historyNote, setHistoryNote] = useState<string | null>(null);
  const [confirmClearHistory, setConfirmClearHistory] = useState(false);
  const permissions = usePermissions();

  useEffect(() => {
    loadSettings();
    loadHardware();
    loadSounds();
    syncAutostartState();
  }, []);

  // Sync the settings with actual autostart state on mount
  const syncAutostartState = async () => {
    try {
      const enabled = await isAutostartEnabled();
      const currentSettings = await invoke<Settings>("get_settings");

      // If settings don't match actual state, update settings to match
      if (currentSettings.launch_at_login !== enabled) {
        const newSettings = { ...currentSettings, launch_at_login: enabled };
        await invoke("update_settings", { settings: newSettings });
        setSettings(newSettings);
      }
    } catch (err) {
      console.error("Failed to sync autostart state:", err);
    }
  };

  const loadSettings = async () => {
    try {
      const result = await invoke<Settings>("get_settings");
      setSettings({ ...EXTRA_DEFAULTS, ...result });
      setLoadError(null);
    } catch (err) {
      console.error("Failed to load settings:", err);
      setLoadError(String(err));
    }
  };

  const loadSounds = async () => {
    try {
      const list = await invoke<string[]>("list_system_sounds");
      if (Array.isArray(list) && list.length > 0) setSounds(list);
    } catch {
      // keep the built-in list
    }
  };

  const previewSound = async (name: string) => {
    try {
      await invoke("preview_sound", { name, volume: settings?.sound_volume ?? 0.5 });
    } catch (err) {
      console.error("Failed to preview sound:", err);
    }
  };

  const clearHistory = async () => {
    try {
      await invoke("clear_history");
      setHistoryNote("History cleared");
    } catch (err) {
      setHistoryNote(`Couldn't clear history: ${err}`);
    } finally {
      setConfirmClearHistory(false);
    }
  };

  const loadHardware = async () => {
    try {
      const result = await invoke<HardwareProfile>("get_hardware_info");
      setHardware(result);
    } catch (err) {
      console.error("Failed to load hardware info:", err);
    }
  };

  const saveSettings = async (newSettings: Settings) => {
    setSaving(true);
    try {
      await invoke("update_settings", { settings: newSettings });
      setSettings(newSettings);
      setSaveError(null);
    } catch (err) {
      console.error("Failed to save settings:", err);
      // Backend rejected the change (e.g. invalid/conflicting hotkey): keep the old value shown.
      setSaveError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const updateSetting = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    if (settings) {
      const newSettings = { ...settings, [key]: value };
      saveSettings(newSettings);
    }
  };

  const openSystemPreferences = async (pane: string) => {
    // Use invoke to run shell command — URL schemes are unreliable on Sonoma+
    try {
      if (pane === "Privacy_Microphone") {
        await invoke("open_system_settings", { pane: "microphone" });
      } else if (pane === "Privacy_Accessibility") {
        await invoke("open_system_settings", { pane: "accessibility" });
      } else if (pane === "Privacy_InputMonitoring") {
        await invoke("open_system_settings", { pane: "input_monitoring" });
      }
    } catch (e) {
      console.error("Failed to open system settings:", e);
      // Last resort fallback
      try {
        await open(`x-apple.systempreferences:com.apple.preference.security?${pane}`);
      } catch {
        console.error("All methods to open system settings failed");
      }
    }
  };

  // Typing "-" or clearing the box is not a number yet: wait for a real one.
  const commitNudge = (key: "overlay_offset_x" | "overlay_offset_y", raw: string) => {
    const n = Number(raw);
    if (raw.trim() !== "" && Number.isFinite(n)) updateSetting(key, Math.round(n));
  };

  const previewOverlay = async () => {
    try {
      await invoke("preview_overlay");
    } catch (err) {
      console.error("Failed to preview overlay:", err);
    }
  };

  // Special handler for launch at login - calls the autostart API
  const updateLaunchAtLogin = async (enabled: boolean) => {
    if (!settings) return;

    setSaving(true);
    try {
      // Update the system autostart setting
      if (enabled) {
        await enableAutostart();
      } else {
        await disableAutostart();
      }

      // Verify it worked
      const actualState = await isAutostartEnabled();
      if (actualState !== enabled) {
        console.warn("Autostart state mismatch - requested:", enabled, "actual:", actualState);
      }

      // Save to our settings
      const newSettings = { ...settings, launch_at_login: actualState };
      await invoke("update_settings", { settings: newSettings });
      setSettings(newSettings);
    } catch (err) {
      console.error("Failed to update launch at login:", err);
    } finally {
      setSaving(false);
    }
  };

  if (!settings && loadError) {
    return (
      <div role="alert" className="alert-danger space-y-2 !p-4">
        <p className="font-medium">Couldn't load settings</p>
        <p className="text-xs break-words">{loadError}</p>
        <button onClick={loadSettings} className="btn-secondary">
          Retry
        </button>
      </div>
    );
  }

  if (!settings) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner className="h-6 w-6" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Hotkeys */}
      <Section title="Keyboard Shortcuts">
        {saveError && (
          <p role="alert" className="alert-danger mb-2">
            {saveError}
          </p>
        )}
        <div className="card card-list">
          <SettingRow label="Dictate" hint="Right Option works with a tap or a hold.">
            <div className="flex flex-col items-end gap-2">
              <label className="flex cursor-pointer items-center gap-2 text-xs text-muted">
                <input
                  type="checkbox"
                  checked={settings.stt_hotkey === "RightOption"}
                  onChange={(e) =>
                    updateSetting("stt_hotkey", e.target.checked ? "RightOption" : "Option+D")
                  }
                  className="h-3.5 w-3.5 accent-[rgb(var(--c-accent))]"
                />
                Right ⌥ key alone (tap or hold)
              </label>
              {settings.stt_hotkey !== "RightOption" && (
                <HotkeyRecorder
                  value={settings.stt_hotkey}
                  onChange={(value) => updateSetting("stt_hotkey", value)}
                />
              )}
            </div>
          </SettingRow>
          <SettingRow label="Read aloud">
            <HotkeyRecorder
              value={settings.tts_hotkey}
              onChange={(value) => updateSetting("tts_hotkey", value)}
            />
          </SettingRow>
        </div>
      </Section>

      {/* Behavior */}
      <Section title="Behavior">
        <div className="card card-list">
          <SettingRow label="Auto-paste transcription" hint="Type the text into the app you were using.">
            <Toggle
              label="Auto-paste transcription"
              checked={settings.auto_paste}
              onChange={(v) => updateSetting("auto_paste", v)}
            />
          </SettingRow>
          <SettingRow label="Launch at login">
            <Toggle
              label="Launch at login"
              checked={settings.launch_at_login}
              onChange={(v) => updateLaunchAtLogin(v)}
            />
          </SettingRow>
          <SettingRow label="Remove filler words" hint="Drops “um”, “uh”, “erm”, “hmm” and similar from the text.">
            <Toggle
              label="Remove filler words"
              checked={settings.remove_fillers}
              onChange={(v) => updateSetting("remove_fillers", v)}
            />
          </SettingRow>
          <SettingRow
            label="Hold Space to pause"
            hint="While recording, hold Space to pause and let go to continue. A quick tap still types a space. Needs Accessibility."
          >
            <Toggle
              label="Hold Space to pause"
              checked={settings.hold_space_to_pause}
              onChange={(v) => updateSetting("hold_space_to_pause", v)}
            />
          </SettingRow>
        </div>
      </Section>

      {/* Sounds */}
      <Section title="Sounds" note="Short cues from the macOS system sounds. The start sound plays once the microphone is live.">
        <div className="card card-list">
          {(
            [
              ["Start", "sound_on_start", "sound_start_name"],
              ["Stop", "sound_on_stop", "sound_stop_name"],
            ] as const
          ).map(([label, onKey, nameKey]) => (
            <SettingRow
              key={onKey}
              label={`${label} sound`}
              hint={onKey === "sound_on_stop" ? "Also plays a softer sound when you cancel with Esc." : undefined}
            >
              <div className="flex items-center gap-2">
                <select
                  aria-label={`${label} sound`}
                  value={settings[nameKey]}
                  disabled={!settings[onKey]}
                  onChange={(e) => updateSetting(nameKey, e.target.value)}
                >
                  {(sounds.includes(settings[nameKey]) ? sounds : [settings[nameKey], ...sounds]).map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={!settings[onKey]}
                  onClick={() => previewSound(settings[nameKey])}
                  aria-label={`Preview ${label.toLowerCase()} sound`}
                  className="btn-secondary"
                >
                  Preview
                </button>
                <Toggle
                  label={`Play ${label.toLowerCase()} sound`}
                  checked={settings[onKey]}
                  onChange={(v) => updateSetting(onKey, v)}
                />
              </div>
            </SettingRow>
          ))}
          <SettingRow label="Volume">
            <div className="flex items-center gap-2">
              <input
                type="range"
                aria-label="Sound volume"
                min={0}
                max={1}
                step={0.05}
                value={settings.sound_volume}
                onChange={(e) => updateSetting("sound_volume", parseFloat(e.target.value))}
                className="w-32"
              />
              <span className="w-9 text-right text-[13px] tabular-nums text-muted">
                {Math.round(settings.sound_volume * 100)}%
              </span>
            </div>
          </SettingRow>
        </div>
      </Section>

      {/* History */}
      <Section title="History" note="Finished dictations are saved only on this Mac, in a file only you can read. Text is never written to the log.">
        <div className="card card-list">
          <SettingRow label="Save history">
            <Toggle
              label="Save history"
              checked={settings.history_enabled}
              onChange={(v) => updateSetting("history_enabled", v)}
            />
          </SettingRow>
          <SettingRow label="Keep the latest" hint="Older dictations are removed automatically.">
            <select
              aria-label="History size"
              value={settings.history_max_entries}
              onChange={(e) => updateSetting("history_max_entries", parseInt(e.target.value, 10))}
            >
              {(HISTORY_SIZES.some((n) => n === settings.history_max_entries)
                ? [...HISTORY_SIZES]
                : [settings.history_max_entries, ...HISTORY_SIZES]
              ).map((n) => (
                <option key={n} value={n}>
                  {n.toLocaleString()} dictations
                </option>
              ))}
            </select>
          </SettingRow>
          <SettingRow label="Clear history" hint={historyNote ?? "Deletes every saved dictation."}>
            {confirmClearHistory ? (
              <div className="flex gap-2">
                <button type="button" onClick={() => setConfirmClearHistory(false)} className="btn-secondary">
                  Keep
                </button>
                <button type="button" onClick={clearHistory} className="btn-danger-solid">
                  Delete all
                </button>
              </div>
            ) : (
              <button type="button" onClick={() => setConfirmClearHistory(true)} className="btn-danger">
                Clear history…
              </button>
            )}
          </SettingRow>
        </div>
      </Section>

      {/* Overlay */}
      <Section title="Overlay" note="The floating panel shown while you dictate and while Blah³ reads aloud.">
        <div className="card card-list">
          <SettingRow label="Position" hint="Stays inside the menu bar and Dock.">
            <select
              aria-label="Overlay position"
              value={settings.overlay_position}
              onChange={(e) => updateSetting("overlay_position", e.target.value)}
            >
              {OVERLAY_POSITIONS.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </SettingRow>
          <SettingRow label="Nudge" hint="Move it from that spot, in points.">
            <div className="flex items-center gap-2 text-xs text-muted">
              <label className="flex items-center gap-1">
                X
                <input
                  type="number"
                  aria-label="Overlay nudge X"
                  step={4}
                  defaultValue={settings.overlay_offset_x}
                  onChange={(e) => commitNudge("overlay_offset_x", e.target.value)}
                  className="!w-16"
                />
              </label>
              <label className="flex items-center gap-1">
                Y
                <input
                  type="number"
                  aria-label="Overlay nudge Y"
                  step={4}
                  defaultValue={settings.overlay_offset_y}
                  onChange={(e) => commitNudge("overlay_offset_y", e.target.value)}
                  className="!w-16"
                />
              </label>
            </div>
          </SettingRow>
          <SettingRow label="Size">
            <div className="flex gap-1.5" role="group" aria-label="Overlay size">
              {OVERLAY_SIZES.map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  aria-pressed={settings.overlay_size === value}
                  onClick={() => updateSetting("overlay_size", value)}
                  className={settings.overlay_size === value ? "btn-primary" : "btn-secondary"}
                >
                  {label}
                </button>
              ))}
            </div>
          </SettingRow>
          <SettingRow label="Transcript lines" hint="How many lines of live text to show.">
            <div className="flex items-center gap-3">
              <input
                type="range"
                aria-label="Transcript lines"
                min="2"
                max="8"
                step="1"
                value={settings.overlay_max_lines}
                onChange={(e) => updateSetting("overlay_max_lines", parseInt(e.target.value, 10))}
                className="!w-28"
              />
              <span className="w-4 text-right text-[13px] tabular-nums text-muted">{settings.overlay_max_lines}</span>
            </div>
          </SettingRow>
          <SettingRow
            label="Click-through while dictating"
            hint="The overlay ignores the mouse so it never blocks what is under it. Read-aloud always takes clicks, for its buttons."
          >
            <Toggle
              label="Click-through while dictating"
              checked={settings.overlay_click_through}
              onChange={(v) => updateSetting("overlay_click_through", v)}
            />
          </SettingRow>
          <SettingRow label="Preview" hint="Shows the overlay for about 3 seconds.">
            <button type="button" onClick={previewOverlay} className="btn-secondary">
              Show preview
            </button>
          </SettingRow>
        </div>
      </Section>

      {/* Silence Detection */}
      <Section title="Silence Detection" note="Stop recording on its own after you finish speaking.">
        <div className="card card-list">
          <SettingRow label="Enable auto-stop">
            <Toggle
              label="Enable auto-stop"
              checked={settings.silence_detection_enabled}
              onChange={(v) => updateSetting("silence_detection_enabled", v)}
            />
          </SettingRow>
          {settings.silence_detection_enabled && (
            <>
              <SettingRow label="Silence duration">
                <div className="flex items-center gap-3">
                  <input
                    type="range"
                    aria-label="Silence duration"
                    min="0.5"
                    max="5"
                    step="0.5"
                    value={settings.silence_duration}
                    onChange={(e) => updateSetting("silence_duration", parseFloat(e.target.value))}
                    className="!w-28"
                  />
                  <span className="w-8 text-right text-[13px] tabular-nums text-muted">{settings.silence_duration}s</span>
                </div>
              </SettingRow>
              <SettingRow label="Sensitivity">
                <div className="flex items-center gap-3">
                  <input
                    type="range"
                    aria-label="Sensitivity"
                    min="0.001"
                    max="0.1"
                    step="0.005"
                    value={settings.silence_threshold}
                    onChange={(e) => updateSetting("silence_threshold", parseFloat(e.target.value))}
                    className="!w-28"
                  />
                  <span className="w-12 text-right text-[13px] text-muted">
                    {settings.silence_threshold < 0.02 ? "High" : settings.silence_threshold < 0.05 ? "Medium" : "Low"}
                  </span>
                </div>
              </SettingRow>
            </>
          )}
        </div>
      </Section>

      {/* Permissions */}
      <Section title="Permissions" note="After changing a permission you may need to restart Blah³.">
        <div className="card card-list">
          <PermissionRow
            Icon={MicIcon}
            title="Microphone"
            description="Needed to hear you"
            granted={permissions?.microphone}
            onOpen={() => openSystemPreferences("Privacy_Microphone")}
          />
          <PermissionRow
            Icon={AccessibilityIcon}
            title="Accessibility"
            description="Needed to read selected text and type transcriptions"
            granted={permissions?.accessibility}
            onOpen={() => openSystemPreferences("Privacy_Accessibility")}
          />
          {permissions?.input_monitoring !== undefined && (
            <PermissionRow
              Icon={KeyboardIcon}
              title="Input Monitoring"
              description="Needed for the right Option key"
              granted={permissions.input_monitoring}
              onOpen={() => openSystemPreferences("Privacy_InputMonitoring")}
            />
          )}
        </div>
      </Section>

      {/* Hardware */}
      {hardware && (
        <Section title="This Mac">
          <div className="card card-list">
            <SettingRow label="Chip">
              <span className="text-[13px] text-muted">{hardware.chip_name}</span>
            </SettingRow>
            <SettingRow label="Memory">
              <span className="text-[13px] text-muted">{hardware.ram_gb} GB</span>
            </SettingRow>
            <SettingRow label="CPU cores">
              <span className="text-[13px] text-muted">{hardware.cpu_cores}</span>
            </SettingRow>
            <SettingRow label="Recommended tier">
              <span className="text-[13px] capitalize text-muted">{hardware.recommended_tier}</span>
            </SettingRow>
            {hardware.has_neural_engine && (
              <div className="row">
                <span className="flex items-center gap-1.5 text-xs text-ok">
                  <CheckIcon className="h-3.5 w-3.5" /> Neural Engine available for faster transcription
                </span>
              </div>
            )}
          </div>
        </Section>
      )}

      <p className="h-4 text-center text-xs text-subtle" aria-live="polite">
        {saving ? "Saving…" : ""}
      </p>

      <p className="text-center text-xs text-subtle">Blah³ v0.1.0 · Local voice toolkit for macOS</p>
    </div>
  );
}

function PermissionRow({
  Icon,
  title,
  description,
  granted,
  onOpen,
}: {
  Icon: (p: { className?: string }) => JSX.Element;
  title: string;
  description: string;
  granted?: boolean;
  onOpen: () => void;
}) {
  return (
    <div className="row">
      <div className="flex min-w-0 items-center gap-3">
        <Icon className="h-5 w-5 shrink-0 text-accent" />
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <p className="text-[13px]">{title}</p>
            {granted !== undefined && (
              <span className={granted ? "badge-ok" : "badge-danger"}>{granted ? "Granted" : "Not Granted"}</span>
            )}
          </div>
          <p className="text-xs text-muted">{description}</p>
        </div>
      </div>
      <button onClick={onOpen} className="btn-secondary shrink-0">
        Open Settings
      </button>
    </div>
  );
}
