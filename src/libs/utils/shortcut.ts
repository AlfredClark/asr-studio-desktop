// 快捷键录制 helpers：键盘事件转加速器字符串，纯函数，可单测。

/** 修饰键快照 */
export interface ShortcutModifiers {
  /** 是否按下 Ctrl */
  ctrl: boolean;
  /** 是否按下 Alt */
  alt: boolean;
  /** 是否按下 Shift */
  shift: boolean;
  /** 是否按下 Super（Windows 键 / Command 键） */
  super: boolean;
}

/** 纯修饰键的 `KeyboardEvent.key` 取值 */
const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);

/** 加速器字符串里的修饰键名 */
const MODIFIER_LABELS = new Set(["Ctrl", "Alt", "Shift", "Super"]);

/** 是否为纯修饰键 */
export function isModifierKey(key: string): boolean {
  return MODIFIER_KEYS.has(key);
}

/** 是否按下任一修饰键 */
export function hasAnyModifier(modifiers: ShortcutModifiers): boolean {
  return modifiers.ctrl || modifiers.alt || modifiers.shift || modifiers.super;
}

/** 主键展示名：空格转 `Space`，单字符转大写，其余原样 */
export function normalizeKeyLabel(key: string): string {
  if (key === " ") return "Space";
  if (key.length === 1) return key.toUpperCase();
  return key;
}

/** 组装加速器字符串；调用方保证主键非空且含修饰键 */
export function formatAccelerator(modifiers: ShortcutModifiers, key: string): string {
  const parts: string[] = [];
  if (modifiers.ctrl) parts.push("Ctrl");
  if (modifiers.alt) parts.push("Alt");
  if (modifiers.shift) parts.push("Shift");
  if (modifiers.super) parts.push("Super");
  parts.push(normalizeKeyLabel(key));
  return parts.join("+");
}

/** 是否为合法加速器：至少两段，末段为主键 */
export function isValidAccelerator(value: string): boolean {
  const parts = value.split("+");
  if (parts.length < 2) return false;
  if (parts.some((part) => part.length === 0)) return false;
  return !MODIFIER_LABELS.has(parts[parts.length - 1]);
}
