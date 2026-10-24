const SYMBOLS: Record<string, string> = {
  command: "⌘",
  commandorcontrol: "⌘",
  cmd: "⌘",
  super: "⌘",
  control: "⌃",
  ctrl: "⌃",
  option: "⌥",
  alt: "⌥",
  shift: "⇧",
};

/** Split a backend hotkey ("Option+S", "RightOption") into keycap labels. */
export function hotkeyKeys(hotkey: string): string[] {
  if (!hotkey) return [];
  if (hotkey === "RightOption") return ["Right ⌥"];
  return hotkey.split("+").map((part) => {
    const p = part.trim();
    return SYMBOLS[p.toLowerCase()] ?? p.toUpperCase();
  });
}
