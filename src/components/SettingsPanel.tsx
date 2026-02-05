import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { enable as enableAutostart, disable as disableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";

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
}

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

  useEffect(() => {
    loadSettings();
    loadHardware();
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
      setSettings(result);
    } catch (err) {
      console.error("Failed to load settings:", err);
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
    } catch (err) {
      console.error("Failed to save settings:", err);
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

  if (!settings) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-sky-500"></div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Hardware Info */}
      {hardware && (
        <section className="bg-slate-800 rounded-lg p-4">
          <h2 className="text-sm font-medium text-slate-300 mb-3">System Information</h2>
          <div className="grid grid-cols-2 gap-3 text-sm">
            <div>
              <p className="text-slate-500">Chip</p>
              <p className="text-slate-200">{hardware.chip_name}</p>
            </div>
            <div>
              <p className="text-slate-500">Memory</p>
              <p className="text-slate-200">{hardware.ram_gb} GB</p>
            </div>
            <div>
              <p className="text-slate-500">CPU Cores</p>
              <p className="text-slate-200">{hardware.cpu_cores}</p>
            </div>
            <div>
              <p className="text-slate-500">Recommended Tier</p>
              <p className="text-slate-200 capitalize">{hardware.recommended_tier}</p>
            </div>
          </div>
          {hardware.has_neural_engine && (
            <p className="text-xs text-green-400 mt-2">✓ Neural Engine available for CoreML acceleration</p>
          )}
        </section>
      )}

      {/* Hotkeys */}
      <section>
        <h2 className="text-lg font-semibold text-slate-200 mb-3">Keyboard Shortcuts</h2>
        <div className="space-y-3">
          <SettingRow label="Dictation Hotkey">
            <input
              type="text"
              value={settings.stt_hotkey}
              onChange={(e) => updateSetting("stt_hotkey", e.target.value)}
              className="px-3 py-2 bg-slate-800 border border-slate-700 rounded text-slate-100 text-sm w-48"
            />
          </SettingRow>
          <SettingRow label="Read Aloud Hotkey">
            <input
              type="text"
              value={settings.tts_hotkey}
              onChange={(e) => updateSetting("tts_hotkey", e.target.value)}
              className="px-3 py-2 bg-slate-800 border border-slate-700 rounded text-slate-100 text-sm w-48"
            />
          </SettingRow>
        </div>
      </section>

      {/* Behavior */}
      <section>
        <h2 className="text-lg font-semibold text-slate-200 mb-3">Behavior</h2>
        <div className="space-y-3">
          <SettingRow label="Auto-paste transcription">
            <Toggle
              checked={settings.auto_paste}
              onChange={(v) => updateSetting("auto_paste", v)}
            />
          </SettingRow>
          <SettingRow label="Launch at login">
            <Toggle
              checked={settings.launch_at_login}
              onChange={(v) => updateLaunchAtLogin(v)}
            />
          </SettingRow>
          <SettingRow label="Menu bar mode">
            <Toggle
              checked={settings.menu_bar_mode}
              onChange={(v) => updateSetting("menu_bar_mode", v)}
            />
          </SettingRow>
        </div>
      </section>

      {/* Silence Detection */}
      <section>
        <h2 className="text-lg font-semibold text-slate-200 mb-3">Silence Detection</h2>
        <p className="text-xs text-slate-400 mb-3">
          Automatically stop recording when silence is detected after speaking.
        </p>
        <div className="space-y-3">
          <SettingRow label="Enable auto-stop">
            <Toggle
              checked={settings.silence_detection_enabled}
              onChange={(v) => updateSetting("silence_detection_enabled", v)}
            />
          </SettingRow>
          {settings.silence_detection_enabled && (
            <>
              <SettingRow label="Silence duration">
                <div className="flex items-center gap-2">
                  <input
                    type="range"
                    min="0.5"
                    max="5"
                    step="0.5"
                    value={settings.silence_duration}
                    onChange={(e) => updateSetting("silence_duration", parseFloat(e.target.value))}
                    className="w-24 accent-sky-500"
                  />
                  <span className="text-sm text-slate-400 w-12">{settings.silence_duration}s</span>
                </div>
              </SettingRow>
              <SettingRow label="Sensitivity">
                <div className="flex items-center gap-2">
                  <input
                    type="range"
                    min="0.001"
                    max="0.1"
                    step="0.005"
                    value={settings.silence_threshold}
                    onChange={(e) => updateSetting("silence_threshold", parseFloat(e.target.value))}
                    className="w-24 accent-sky-500"
                  />
                  <span className="text-sm text-slate-400 w-16">
                    {settings.silence_threshold < 0.02 ? "High" : settings.silence_threshold < 0.05 ? "Medium" : "Low"}
                  </span>
                </div>
              </SettingRow>
            </>
          )}
        </div>
      </section>

      {/* Save Indicator */}
      {saving && (
        <p className="text-xs text-slate-400 text-center">Saving...</p>
      )}

      {/* About */}
      <section className="text-center pt-4 border-t border-slate-800">
        <p className="text-sm text-slate-400">Blah³ v0.1.0</p>
        <p className="text-xs text-slate-500 mt-1">Local Voice Toolkit for macOS</p>
      </section>
    </div>
  );
}

function SettingRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-sm text-slate-300">{label}</span>
      {children}
    </div>
  );
}

function Toggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!checked)}
      className={`w-11 h-6 rounded-full transition-colors ${
        checked ? "bg-sky-500" : "bg-slate-700"
      }`}
    >
      <div
        className={`w-5 h-5 bg-white rounded-full shadow transition-transform ${
          checked ? "translate-x-5" : "translate-x-0.5"
        }`}
      />
    </button>
  );
}
