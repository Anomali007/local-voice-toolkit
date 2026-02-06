import { useState, useEffect, useRef, useCallback } from "react";

interface HotkeyRecorderProps {
  value: string;
  onChange: (value: string) => void;
}

// Map browser event modifiers to backend format
const MODIFIER_TO_BACKEND: Record<string, string> = {
  metaKey: "CommandOrControl",
  ctrlKey: "Control",
  shiftKey: "Shift",
  altKey: "Alt",
};

// Map backend format to display symbols
const BACKEND_TO_DISPLAY: Record<string, string> = {
  CommandOrControl: "\u2318", // ⌘
  Control: "\u2303", // ⌃
  Shift: "\u21E7", // ⇧
  Alt: "\u2325", // ⌥
};

// Map key codes to backend key names
const KEY_TO_BACKEND: Record<string, string> = {
  // Letters
  KeyA: "A", KeyB: "B", KeyC: "C", KeyD: "D", KeyE: "E",
  KeyF: "F", KeyG: "G", KeyH: "H", KeyI: "I", KeyJ: "J",
  KeyK: "K", KeyL: "L", KeyM: "M", KeyN: "N", KeyO: "O",
  KeyP: "P", KeyQ: "Q", KeyR: "R", KeyS: "S", KeyT: "T",
  KeyU: "U", KeyV: "V", KeyW: "W", KeyX: "X", KeyY: "Y", KeyZ: "Z",
  // Numbers
  Digit0: "0", Digit1: "1", Digit2: "2", Digit3: "3", Digit4: "4",
  Digit5: "5", Digit6: "6", Digit7: "7", Digit8: "8", Digit9: "9",
  // Function keys
  F1: "F1", F2: "F2", F3: "F3", F4: "F4", F5: "F5", F6: "F6",
  F7: "F7", F8: "F8", F9: "F9", F10: "F10", F11: "F11", F12: "F12",
  // Special keys
  Space: "Space",
  Enter: "Enter",
  Escape: "Escape",
  Tab: "Tab",
  Backspace: "Backspace",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

// Convert backend format to display format
function backendToDisplay(backend: string): string {
  const parts = backend.split("+");
  const displayParts = parts.map((part) => {
    const trimmed = part.trim();
    // Check if it's a modifier
    if (BACKEND_TO_DISPLAY[trimmed]) {
      return BACKEND_TO_DISPLAY[trimmed];
    }
    // Otherwise it's a key
    return trimmed;
  });
  return displayParts.join(" + ");
}

export default function HotkeyRecorder({ value, onChange }: HotkeyRecorderProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [currentModifiers, setCurrentModifiers] = useState<string[]>([]);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const getModifiersFromEvent = useCallback((e: KeyboardEvent): string[] => {
    const mods: string[] = [];
    if (e.metaKey) mods.push(MODIFIER_TO_BACKEND.metaKey);
    if (e.ctrlKey && !e.metaKey) mods.push(MODIFIER_TO_BACKEND.ctrlKey);
    if (e.shiftKey) mods.push(MODIFIER_TO_BACKEND.shiftKey);
    if (e.altKey) mods.push(MODIFIER_TO_BACKEND.altKey);
    return mods;
  }, []);

  const isModifierKey = (code: string): boolean => {
    return ["MetaLeft", "MetaRight", "ControlLeft", "ControlRight",
            "ShiftLeft", "ShiftRight", "AltLeft", "AltRight"].includes(code);
  };

  useEffect(() => {
    if (!isRecording) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      // Handle Escape to cancel
      if (e.code === "Escape") {
        setIsRecording(false);
        setCurrentModifiers([]);
        return;
      }

      const mods = getModifiersFromEvent(e);
      setCurrentModifiers(mods);

      // If it's just a modifier key, show current state but don't save
      if (isModifierKey(e.code)) {
        return;
      }

      // Get the key name
      const keyName = KEY_TO_BACKEND[e.code];
      if (!keyName) {
        return; // Unknown key
      }

      // Require at least one modifier
      if (mods.length === 0) {
        return;
      }

      // Build the hotkey string
      const hotkey = [...mods, keyName].join("+");
      onChange(hotkey);
      setIsRecording(false);
      setCurrentModifiers([]);
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const mods = getModifiersFromEvent(e);
      setCurrentModifiers(mods);
    };

    const handleClickOutside = (e: MouseEvent) => {
      if (buttonRef.current && !buttonRef.current.contains(e.target as Node)) {
        setIsRecording(false);
        setCurrentModifiers([]);
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    window.addEventListener("keyup", handleKeyUp, true);
    document.addEventListener("mousedown", handleClickOutside);

    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
      window.removeEventListener("keyup", handleKeyUp, true);
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [isRecording, getModifiersFromEvent, onChange]);

  const handleClick = () => {
    setIsRecording(true);
    setCurrentModifiers([]);
  };

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation();
    onChange("");
    setIsRecording(false);
    setCurrentModifiers([]);
  };

  const displayValue = isRecording
    ? currentModifiers.length > 0
      ? currentModifiers.map((m) => BACKEND_TO_DISPLAY[m] || m).join(" + ") + " + ..."
      : "Press keys..."
    : value
    ? backendToDisplay(value)
    : "Click to set";

  return (
    <button
      ref={buttonRef}
      onClick={handleClick}
      className={`
        relative px-3 py-2 min-w-[160px] text-left
        bg-slate-800 border rounded text-sm
        transition-all duration-150
        ${isRecording
          ? "border-sky-500 ring-2 ring-sky-500/30"
          : "border-slate-700 hover:border-slate-600"
        }
        ${value ? "text-slate-100" : "text-slate-400"}
      `}
    >
      <span className="pr-6">{displayValue}</span>

      {/* Clear button */}
      {value && !isRecording && (
        <button
          onClick={handleClear}
          className="absolute right-2 top-1/2 -translate-y-1/2
                     w-5 h-5 flex items-center justify-center
                     text-slate-500 hover:text-slate-300
                     rounded hover:bg-slate-700 transition-colors"
          title="Clear hotkey"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 20 20"
            fill="currentColor"
            className="w-3.5 h-3.5"
          >
            <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
          </svg>
        </button>
      )}
    </button>
  );
}
