// 指令录制键盘：104 键布局（行 → 簇 → 键），录制键盘与序列预览共用。
// `id` 优先与 enigo `Key` 变体同名，下期触发直接映射；标点（`OEM*`）与小键盘专有键
// （`Numlock`/`Decimal`/`Add` 等）在 enigo 侧的平台映射留到触发阶段实现，本期后端只做非空校验。
// 数字区高键（`+`）与数字区回车（无 enigo 变体）暂不收录。
// 渲染时用行列序号做 key（同一 `id` 可能在多处出现，如高低两档 Shift 已用左右区分，此处无复用）。

/** 键帽：`id` 为落盘标识，`span` 为宽度份数（基准 1） */
export interface KeyCap {
  /** 落盘标识，优先与 enigo `Key` 变体同名 */
  id: string;
  /** 键帽展示文案 */
  label: string;
  /** 宽度份数，缺省 1 */
  span?: number;
}

/** 104 键盘布局：行数组，行内按簇留空隙（功能区 / 主区 / 导航区 / 数字区） */
export const KEYBOARD_ROWS: KeyCap[][][] = [
  [
    [{ id: "Escape", label: "Esc" }],
    [
      { id: "F1", label: "F1" },
      { id: "F2", label: "F2" },
      { id: "F3", label: "F3" },
      { id: "F4", label: "F4" },
    ],
    [
      { id: "F5", label: "F5" },
      { id: "F6", label: "F6" },
      { id: "F7", label: "F7" },
      { id: "F8", label: "F8" },
    ],
    [
      { id: "F9", label: "F9" },
      { id: "F10", label: "F10" },
      { id: "F11", label: "F11" },
      { id: "F12", label: "F12" },
    ],
    [
      { id: "PrintScr", label: "PrtSc" },
      { id: "ScrollLock", label: "ScrLk" },
      { id: "Pause", label: "Pause" },
    ],
  ],
  [
    [
      { id: "OEM3", label: "`" },
      { id: "Num1", label: "1" },
      { id: "Num2", label: "2" },
      { id: "Num3", label: "3" },
      { id: "Num4", label: "4" },
      { id: "Num5", label: "5" },
      { id: "Num6", label: "6" },
      { id: "Num7", label: "7" },
      { id: "Num8", label: "8" },
      { id: "Num9", label: "9" },
      { id: "Num0", label: "0" },
      { id: "OEMMinus", label: "-" },
      { id: "OEMPlus", label: "=" },
      { id: "Backspace", label: "Backspace", span: 2 },
    ],
    [
      { id: "Insert", label: "Ins" },
      { id: "Home", label: "Home" },
      { id: "PageUp", label: "PgUp" },
    ],
    [
      { id: "Numlock", label: "Num" },
      { id: "Divide", label: "/" },
      { id: "Multiply", label: "*" },
      { id: "Subtract", label: "-" },
    ],
  ],
  [
    [
      { id: "Tab", label: "Tab", span: 1.5 },
      { id: "Q", label: "Q" },
      { id: "W", label: "W" },
      { id: "E", label: "E" },
      { id: "R", label: "R" },
      { id: "T", label: "T" },
      { id: "Y", label: "Y" },
      { id: "U", label: "U" },
      { id: "I", label: "I" },
      { id: "O", label: "O" },
      { id: "P", label: "P" },
      { id: "OEM4", label: "[" },
      { id: "OEM6", label: "]" },
      { id: "OEM5", label: "\\", span: 1.5 },
    ],
    [
      { id: "Delete", label: "Del" },
      { id: "End", label: "End" },
      { id: "PageDown", label: "PgDn" },
    ],
    [
      { id: "Numpad7", label: "7" },
      { id: "Numpad8", label: "8" },
      { id: "Numpad9", label: "9" },
      { id: "Add", label: "+" },
    ],
  ],
  [
    [
      { id: "CapsLock", label: "Caps", span: 1.75 },
      { id: "A", label: "A" },
      { id: "S", label: "S" },
      { id: "D", label: "D" },
      { id: "F", label: "F" },
      { id: "G", label: "G" },
      { id: "H", label: "H" },
      { id: "J", label: "J" },
      { id: "K", label: "K" },
      { id: "L", label: "L" },
      { id: "OEM1", label: ";" },
      { id: "OEM7", label: "'" },
      { id: "Return", label: "Enter", span: 2.25 },
    ],
    [
      { id: "Numpad4", label: "4" },
      { id: "Numpad5", label: "5" },
      { id: "Numpad6", label: "6" },
    ],
  ],
  [
    [
      { id: "LShift", label: "Shift", span: 2.25 },
      { id: "Z", label: "Z" },
      { id: "X", label: "X" },
      { id: "C", label: "C" },
      { id: "V", label: "V" },
      { id: "B", label: "B" },
      { id: "N", label: "N" },
      { id: "M", label: "M" },
      { id: "OEMComma", label: "," },
      { id: "OEMPeriod", label: "." },
      { id: "OEM2", label: "/" },
      { id: "RShift", label: "Shift", span: 2.75 },
    ],
    [{ id: "UpArrow", label: "↑" }],
    [
      { id: "Numpad1", label: "1" },
      { id: "Numpad2", label: "2" },
      { id: "Numpad3", label: "3" },
    ],
  ],
  [
    [
      { id: "LControl", label: "Ctrl" },
      { id: "LWin", label: "Win" },
      { id: "LMenu", label: "Alt" },
      { id: "Space", label: "Space", span: 5 },
      { id: "RMenu", label: "Alt" },
      { id: "RWin", label: "Win" },
      { id: "Apps", label: "Menu" },
      { id: "RControl", label: "Ctrl" },
    ],
    [
      { id: "LeftArrow", label: "←" },
      { id: "DownArrow", label: "↓" },
      { id: "RightArrow", label: "→" },
    ],
    [
      { id: "Numpad0", label: "0", span: 2 },
      { id: "Decimal", label: "." },
    ],
  ],
];

/** 键帽展示文案：序列预览与详情共用，`id` 即注册表标识 */
export function keyLabel(id: string): string {
  for (const row of KEYBOARD_ROWS) {
    for (const cluster of row) {
      const hit = cluster.find((cap) => cap.id === id);
      if (hit !== undefined) return hit.label;
    }
  }
  return id;
}

/** 按键当前是否处于按下态：序列中该键最后一个事件为按下即真；纯函数，可单测 */
export function isKeyHeld(
  actions: Array<{ key: string; direction: "press" | "release" }>,
  key: string,
): boolean {
  for (let index = actions.length - 1; index >= 0; index -= 1) {
    if (actions[index].key === key) {
      return actions[index].direction === "press";
    }
  }
  return false;
}

/** 折叠指定前缀的按下集合：演示播放按游标推进时共用；纯函数，可单测 */
export function foldHeldKeys(
  actions: Array<{ key: string; direction: "press" | "release" }>,
  upToIndex: number,
): Set<string> {
  const held = new Set<string>();
  for (let index = 0; index <= upToIndex && index < actions.length; index += 1) {
    const action = actions[index];
    if (action.direction === "press") held.add(action.key);
    else held.delete(action.key);
  }
  return held;
}
