import { describe, expect, it } from "vitest";
import { foldHeldKeys, isKeyHeld, keyLabel } from "#libs/directives/key-registry";

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
});
