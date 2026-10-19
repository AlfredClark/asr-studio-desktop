import { describe, expect, it } from "vitest";
import {
  formatAccelerator,
  hasAnyModifier,
  isModifierKey,
  isValidAccelerator,
  normalizeKeyLabel,
} from "#libs/utils/shortcut";

describe("快捷键录制 helpers", () => {
  it("识别纯修饰键", () => {
    for (const key of ["Control", "Alt", "Shift", "Meta"]) {
      expect(isModifierKey(key)).toBe(true);
    }
    for (const key of ["o", "Enter", "F12", "Space", "Escape"]) {
      expect(isModifierKey(key)).toBe(false);
    }
  });

  it("规范主键展示名", () => {
    expect(normalizeKeyLabel("o")).toBe("O");
    expect(normalizeKeyLabel(" ")).toBe("Space");
    expect(normalizeKeyLabel("F12")).toBe("F12");
    expect(normalizeKeyLabel("Enter")).toBe("Enter");
  });

  it("按固定顺序组装加速器字符串", () => {
    expect(formatAccelerator({ ctrl: true, alt: false, shift: true, super: false }, "o")).toBe(
      "Ctrl+Shift+O",
    );
    expect(formatAccelerator({ ctrl: false, alt: true, shift: false, super: true }, " ")).toBe(
      "Alt+Super+Space",
    );
  });

  it("判断修饰键是否按下", () => {
    expect(hasAnyModifier({ ctrl: false, alt: false, shift: false, super: false })).toBe(false);
    expect(hasAnyModifier({ ctrl: true, alt: false, shift: false, super: false })).toBe(true);
  });

  it("校验加速器合法性", () => {
    expect(isValidAccelerator("Ctrl+Shift+O")).toBe(true);
    expect(isValidAccelerator("Alt+Space")).toBe(true);
    expect(isValidAccelerator("")).toBe(false);
    expect(isValidAccelerator("O")).toBe(false);
    expect(isValidAccelerator("Ctrl+Shift")).toBe(false);
    expect(isValidAccelerator("Ctrl++O")).toBe(false);
  });
});
