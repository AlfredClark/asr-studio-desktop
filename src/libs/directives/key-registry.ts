// 指令录制键盘：104 键布局（行 → 区 → 键），录制键盘与序列预览共用。
// 三区按真机列对齐：主区 15 键单位、导航区 3 单位、数字区 4 单位；
// 功能键行主区用占位垫出 `Esc` 与 `F` 分组间隔，保证 `PrtSc` 组落在 `Ins` 组正上方。
// `placeholder` 只占位不渲染按钮（完全不可见，不进 Tab、不上报点击）；
// `rowSpan` 给纵跨两行的 `+` 与小键盘回车用；
// `unavailableOn` 标出不开放录制的平台：enigo 无映射，或有映射但不建议录制（如 `Num`）；
// 演示只读展示不变，存量序列仍可回放；与 `features/directives.rs::enigo_key` 大体成对维护。
// `NumpadEnter` 在 enigo 侧无专有变体，归并到主回车（`Return`）回放。
// 渲染时用行列序号做 key（占位 `__gap` 多处复用，无复用 id 问题）。

/** 落盘标识的平台集合：与后端 `enigo_key` 的平台分支同名 */
export type DesktopPlatform = "windows" | "linux" | "macos";

/** 键帽：`id` 为落盘标识，`span` 为宽度份数（基准 1） */
export interface KeyCap {
  /** 落盘标识，优先与 enigo `Key` 变体同名 */
  id: string;
  /** 键帽展示文案 */
  label: string;
  /** 宽度份数，缺省 1 */
  span?: number;
  /** 跨行数（数字区 `+` 与小键盘回车为 2），缺省 1 */
  rowSpan?: number;
  /** 占位：只占布局空间，不渲染按钮 */
  placeholder?: true;
  /** 不支持的平台：录制时禁用，与后端映射成对维护 */
  unavailableOn?: DesktopPlatform[];
}

/** 占位键：布局垫片，渲染为不可见盒 */
function gap(span: number = 1): KeyCap {
  return { id: "__gap", label: "", span, placeholder: true };
}

/** 104 键盘布局：行数组，行内按区排列（主区 / 导航区 / 数字区），区宽恒定以保证列对齐 */
export const KEYBOARD_ROWS: KeyCap[][][] = [
  [
    [
      { id: "Escape", label: "Esc" },
      gap(1),
      { id: "F1", label: "F1" },
      { id: "F2", label: "F2" },
      { id: "F3", label: "F3" },
      { id: "F4", label: "F4" },
      gap(0.5),
      { id: "F5", label: "F5" },
      { id: "F6", label: "F6" },
      { id: "F7", label: "F7" },
      { id: "F8", label: "F8" },
      gap(0.5),
      { id: "F9", label: "F9" },
      { id: "F10", label: "F10" },
      { id: "F11", label: "F11" },
      { id: "F12", label: "F12" },
    ],
    [
      { id: "PrintScr", label: "PrtSc", unavailableOn: ["macos"] },
      { id: "ScrollLock", label: "ScrLk", unavailableOn: ["macos"] },
      { id: "Pause", label: "Pause", unavailableOn: ["macos"] },
    ],
    [gap(), gap(), gap(), gap()],
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
      { id: "Insert", label: "Ins", unavailableOn: ["macos"] },
      { id: "Home", label: "Home" },
      { id: "PageUp", label: "PgUp" },
    ],
    [
      // `Num` 与 `Menu` 同策：有映射也不开放录制，只在 Windows 可点
      { id: "Numlock", label: "Num", unavailableOn: ["linux", "macos"] },
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
      { id: "Add", label: "+", rowSpan: 2 },
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
    [gap(), gap(), gap()],
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
    [gap(), { id: "UpArrow", label: "↑" }, gap()],
    [
      { id: "Numpad1", label: "1" },
      { id: "Numpad2", label: "2" },
      { id: "Numpad3", label: "3" },
      { id: "NumpadEnter", label: "Enter", rowSpan: 2 },
    ],
  ],
  [
    [
      { id: "LControl", label: "Ctrl", span: 1.25 },
      { id: "LWin", label: "Win", span: 1.25 },
      { id: "LMenu", label: "Alt", span: 1.25 },
      { id: "Space", label: "Space", span: 6.25 },
      { id: "RMenu", label: "Alt", span: 1.25 },
      { id: "RWin", label: "Win", span: 1.25 },
      { id: "Apps", label: "Menu", span: 1.25, unavailableOn: ["linux", "macos"] },
      { id: "RControl", label: "Ctrl", span: 1.25 },
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

/** 跨行摆位结果：数字区网格的显式落格 */
export interface PlacedCap {
  /** 键帽（含占位） */
  cap: KeyCap;
  /** 列（1 起） */
  col: number;
  /** 行（1 起，相对传入的行数组） */
  row: number;
  /** 跨列数 */
  colSpan: number;
  /** 跨行数 */
  rowSpan: number;
}

/** 数字区跨行摆位：按序落格，跳过被跨行键占据的格；纯函数，可单测 */
export function layoutSpanningCells(rows: KeyCap[][]): PlacedCap[] {
  const cols = 4;
  const occupied = new Set<string>();
  const placed: PlacedCap[] = [];
  rows.forEach((caps, rowIndex) => {
    const row = rowIndex + 1;
    let col = 1;
    for (const cap of caps) {
      while (col <= cols && occupied.has(`${row}:${col}`)) col += 1;
      if (col > cols) break;
      const colSpan = Math.round(cap.span ?? 1);
      const rowSpan = cap.rowSpan ?? 1;
      placed.push({ cap, col, row, colSpan, rowSpan });
      for (let r = row; r < row + rowSpan; r += 1) {
        for (let c = col; c < col + colSpan; c += 1) occupied.add(`${r}:${c}`);
      }
      col += colSpan;
    }
  });
  return placed;
}

/** 自适应缩放比：演示区按容器宽等比缩放键盘（双向）；任一尺寸无效回落 1；纯函数，可单测 */
export function fitScale(containerWidth: number, naturalWidth: number): number {
  if (!(containerWidth > 0) || !(naturalWidth > 0)) return 1;
  return containerWidth / naturalWidth;
}

/** 键帽展示文案：序列预览与详情共用，`id` 即注册表标识 */
export function keyLabel(id: string): string {
  for (const row of KEYBOARD_ROWS) {
    for (const cluster of row) {
      const hit = cluster.find((cap) => !cap.placeholder && cap.id === id);
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

/** 未闭合的按键：按按下顺序返回仍处于按下态的键；纯函数，可单测 */
export function unclosedKeys(
  actions: Array<{ key: string; direction: "press" | "release" }>,
): string[] {
  const held: string[] = [];
  for (const action of actions) {
    if (action.direction === "press") {
      if (!held.includes(action.key)) held.push(action.key);
    } else {
      const index = held.indexOf(action.key);
      // 无故松开（无对应按下）不影响闭合判定，保存校验会单独拦截
      if (index !== -1) held.splice(index, 1);
    }
  }
  return held;
}
