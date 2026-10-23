import { describe, expect, it } from "vitest";
import {
  fitScale,
  foldHeldKeys,
  isKeyHeld,
  KEYBOARD_ROWS,
  keyLabel,
  layoutSpanningCells,
  unclosedKeys,
} from "#libs/directives/key-registry";

describe("按键注册表", () => {
  it("未知键回落原标识", () => {
    expect(keyLabel("LControl")).toBe("Ctrl");
    expect(keyLabel("UpArrow")).toBe("↑");
    expect(keyLabel("OEMMinus")).toBe("-");
    expect(keyLabel("Numpad0")).toBe("0");
    expect(keyLabel("Nope")).toBe("Nope");
  });

  it("按下态按最后一个事件判定", () => {
    expect(isKeyHeld([], "Control")).toBe(false);
    expect(isKeyHeld([{ key: "Control", direction: "press" }], "Control")).toBe(true);
    expect(
      isKeyHeld(
        [
          { key: "Control", direction: "press" },
          { key: "Control", direction: "release" },
        ],
        "Control",
      ),
    ).toBe(false);
    expect(
      isKeyHeld(
        [
          { key: "Control", direction: "press" },
          { key: "C", direction: "press" },
        ],
        "C",
      ),
    ).toBe(true);
  });

  it("折叠前缀按下集合", () => {
    const actions = [
      { key: "Control", direction: "press" as const },
      { key: "C", direction: "press" as const },
      { key: "C", direction: "release" as const },
    ];
    expect(foldHeldKeys(actions, -1)).toEqual(new Set());
    expect(foldHeldKeys(actions, 0)).toEqual(new Set(["Control"]));
    expect(foldHeldKeys(actions, 1)).toEqual(new Set(["Control", "C"]));
    expect(foldHeldKeys(actions, 2)).toEqual(new Set(["Control"]));
    expect(foldHeldKeys(actions, 99)).toEqual(new Set(["Control"]));
    expect(foldHeldKeys([], 0)).toEqual(new Set());
  });

  it("未闭合按键按按下顺序返回", () => {
    expect(unclosedKeys([])).toEqual([]);
    // 配对齐全即闭合
    expect(
      unclosedKeys([
        { key: "Control", direction: "press" as const },
        { key: "Control", direction: "release" as const },
      ]),
    ).toEqual([]);
    // 按下顺序保留，中间松开不影响其余
    expect(
      unclosedKeys([
        { key: "Control", direction: "press" as const },
        { key: "C", direction: "press" as const },
        { key: "Control", direction: "release" as const },
      ]),
    ).toEqual(["C"]);
    // 重复按下只记一次，无故松开不影响
    expect(
      unclosedKeys([
        { key: "C", direction: "press" as const },
        { key: "C", direction: "press" as const },
        { key: "X", direction: "release" as const },
      ]),
    ).toEqual(["C"]);
  });
});

describe("键盘布局（三区列对齐）", () => {
  it("每行三区，主区 15 键单位", () => {
    expect(KEYBOARD_ROWS).toHaveLength(6);
    for (const row of KEYBOARD_ROWS) {
      expect(row).toHaveLength(3);
      const units = row[0].reduce((sum, cap) => sum + (cap.span ?? 1), 0);
      expect(units).toBeCloseTo(15, 5);
    }
  });

  it("方向键上下同列，其上方为导航键，再上方为功能键", () => {
    const nav = (row: number) => KEYBOARD_ROWS[row][1];
    // 功能键行导航区首位是 PrtSc
    expect(nav(0)[0].id).toBe("PrintScr");
    // 数字行导航区首位是 Ins：与 PrtSc 同列
    expect(nav(1)[0].id).toBe("Insert");
    // Shift 行 `↑` 居中，底行 `↓` 居中：同列
    expect(nav(4).map((cap) => cap.id)).toEqual(["__gap", "UpArrow", "__gap"]);
    expect(nav(5).map((cap) => cap.id)).toEqual(["LeftArrow", "DownArrow", "RightArrow"]);
    // Caps 行导航区全占位，数字区不左移
    expect(nav(3).every((cap) => cap.placeholder === true)).toBe(true);
  });

  it("数字区跨行键摆位正确", () => {
    const cells = layoutSpanningCells(KEYBOARD_ROWS.slice(1).map((row) => row[2]));
    const at = (row: number, col: number) =>
      cells.find((cell) => cell.row === row && cell.col === col);
    expect(at(2, 4)?.cap.id).toBe("Add");
    expect(at(2, 4)?.rowSpan).toBe(2);
    expect(at(3, 1)?.cap.id).toBe("Numpad4");
    expect(at(3, 4)).toBeUndefined();
    expect(at(4, 4)?.cap.id).toBe("NumpadEnter");
    expect(at(4, 4)?.rowSpan).toBe(2);
    expect(at(5, 1)?.cap.id).toBe("Numpad0");
    expect(at(5, 1)?.colSpan).toBe(2);
    expect(at(5, 3)?.cap.id).toBe("Decimal");
    expect(at(5, 4)).toBeUndefined();
  });

  it("平台不支持集与后端映射一致", () => {
    const byId = new Map<string, { unavailableOn?: string[] }>();
    for (const row of KEYBOARD_ROWS) {
      for (const cluster of row) {
        for (const cap of cluster) {
          if (cap.placeholder === true) continue;
          expect(byId.has(cap.id)).toBe(false);
          byId.set(cap.id, cap);
        }
      }
    }
    expect(byId.get("NumpadEnter")?.unavailableOn).toBeUndefined();
    expect(byId.get("Apps")?.unavailableOn).toEqual(["linux", "macos"]);
    for (const id of ["PrintScr", "ScrollLock", "Pause", "Insert"]) {
      expect(byId.get(id)?.unavailableOn).toEqual(["macos"]);
    }
    // `Num` 与 `Menu` 同策：有映射也不开放录制
    expect(byId.get("Numlock")?.unavailableOn).toEqual(["linux", "macos"]);
    expect(keyLabel("NumpadEnter")).toBe("Enter");
    expect(keyLabel("UpArrow")).toBe("↑");
  });

  it("自适应缩放比双向等比", () => {
    expect(fitScale(0, 800)).toBe(1);
    expect(fitScale(600, 0)).toBe(1);
    expect(fitScale(800, 800)).toBe(1);
    expect(fitScale(1200, 800)).toBeCloseTo(1.5, 5);
    expect(fitScale(400, 800)).toBeCloseTo(0.5, 5);
  });
});
