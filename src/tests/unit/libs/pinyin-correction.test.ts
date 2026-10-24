// 拼音纠正前端镜像的单测：折叠口径与后端 `cores::pinyin_correction` 锁死一致。
import { describe, expect, it } from "vitest";
import {
  correctionSetForDirective,
  correctionSetForRecognition,
  foldPinyinSyllable,
  foldPinyinText,
  isDirectiveCorrectionActive,
  isGroupSelected,
  isRecognitionCorrectionActive,
  PINYIN_CORRECTION_GROUPS,
  sanitizePinyinCorrectionRules,
  type PinyinCorrectionSetting,
} from "#libs/pinyin-correction";

function setting(partial: Partial<PinyinCorrectionSetting>): PinyinCorrectionSetting {
  return {
    enabled: true,
    rules: ["z_zh"],
    applyRecognition: true,
    applyDirective: true,
    ...partial,
  };
}

describe("拼音纠正折叠", () => {
  it("空规则集恒等", () => {
    const enabled = new Set<string>();
    expect(foldPinyinSyllable("zhi", enabled)).toBe("zhi");
    expect(foldPinyinText("zhi dao", enabled)).toBe("zhi dao");
  });

  it("翘舌向平舌折叠", () => {
    const enabled = new Set(["z_zh", "c_ch", "s_sh"]);
    expect(foldPinyinSyllable("zhi", enabled)).toBe("zi");
    expect(foldPinyinSyllable("chi", enabled)).toBe("ci");
    expect(foldPinyinSyllable("shi", enabled)).toBe("si");
  });

  it("前后鼻音向短尾折叠", () => {
    const enabled = new Set(["an_ang", "en_eng", "in_ing", "ian_iang", "uan_uang"]);
    expect(foldPinyinText("fan shen pin lian guan", enabled)).toBe("fan shen pin lian guan");
    expect(foldPinyinSyllable("fang", enabled)).toBe("fan");
    expect(foldPinyinSyllable("liang", enabled)).toBe("lian");
    expect(foldPinyinSyllable("guang", enabled)).toBe("guan");
  });

  it("声母与复韵母折叠", () => {
    const enabled = new Set(["n_l", "r_l", "f_h", "ie_ve", "ou_iu"]);
    expect(foldPinyinSyllable("nai", enabled)).toBe("lai");
    expect(foldPinyinSyllable("re", enabled)).toBe("le");
    expect(foldPinyinSyllable("fa", enabled)).toBe("ha");
    // 声母折叠先行：`nve` 先变 `lve` 再按 `ve → ie` 得 `lie`，与 `nie → lie` 会合
    expect(foldPinyinSyllable("nve", enabled)).toBe("lie");
    expect(foldPinyinSyllable("nie", enabled)).toBe("lie");
    expect(foldPinyinSyllable("liu", enabled)).toBe("lou");
  });

  it("韵母折叠不碰声母", () => {
    const enabled = new Set(["ie_ve", "ou_iu"]);
    expect(foldPinyinSyllable("xie", enabled)).toBe("xie");
    expect(foldPinyinSyllable("nve", enabled)).toBe("nie");
  });
});

describe("拼音纠正门控", () => {
  it("清洗只留已知去重保序", () => {
    expect(sanitizePinyinCorrectionRules([" z_zh ", "nope", "", "z_zh", "n_l"])).toEqual([
      "z_zh",
      "n_l",
    ]);
  });

  it("识别与指令门控要求三者齐备", () => {
    expect(isRecognitionCorrectionActive(setting({}))).toBe(true);
    expect(isRecognitionCorrectionActive(setting({ enabled: false }))).toBe(false);
    expect(isRecognitionCorrectionActive(setting({ applyRecognition: false }))).toBe(false);
    expect(isRecognitionCorrectionActive(setting({ rules: [] }))).toBe(false);
    expect(isDirectiveCorrectionActive(setting({ applyDirective: false }))).toBe(false);
  });

  it("门控失败时规则集为空", () => {
    expect(correctionSetForRecognition(setting({ enabled: false })).size).toBe(0);
    expect(correctionSetForDirective(setting({ applyDirective: false })).size).toBe(0);
    expect(correctionSetForRecognition(setting({})).has("z_zh")).toBe(true);
  });

  it("分组全选判断", () => {
    const flat = PINYIN_CORRECTION_GROUPS[0];
    expect(isGroupSelected(new Set(["z_zh", "c_ch", "s_sh"]), flat)).toBe(true);
    expect(isGroupSelected(new Set(["z_zh"]), flat)).toBe(false);
  });
});
