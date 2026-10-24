// 拼音纠正的前端注册表与折叠实现：Rust `cores::pinyin_correction` 的镜像。
// 规则 id、分组、折叠顺序三处必须与后端同口径，两边用同一组夹具锁死一致。
// 页面组件只做渲染与接线，匹配逻辑走这里的纯函数，便于单测。

/** 规则 id 全集：与后端 `KNOWN_RULE_IDS` 同字面量同顺序，改名需两端同步 */
export const PINYIN_CORRECTION_RULE_IDS = [
  "z_zh",
  "c_ch",
  "s_sh",
  "an_ang",
  "en_eng",
  "in_ing",
  "n_l",
  "r_l",
  "f_h",
  "ian_iang",
  "uan_uang",
  "ie_ve",
  "ou_iu",
] as const;

/** 规则 id 类型 */
export type PinyinCorrectionRuleId = (typeof PINYIN_CORRECTION_RULE_IDS)[number];

/** 分组 id：弹窗分组标题的 i18n 键后缀，改名需同步文案 */
export type PinyinCorrectionGroupId = "flat" | "nasal" | "lateral" | "labial" | "rhyme";

/** 分组：id + 规则列表，顺序即弹窗展示顺序 */
export interface PinyinCorrectionGroup {
  /** 分组 id */
  id: PinyinCorrectionGroupId;
  /** 组内规则 id */
  rules: PinyinCorrectionRuleId[];
}

/** 分组注册表：5 组 13 条，与定稿方案一致 */
export const PINYIN_CORRECTION_GROUPS: PinyinCorrectionGroup[] = [
  { id: "flat", rules: ["z_zh", "c_ch", "s_sh"] },
  { id: "nasal", rules: ["an_ang", "en_eng", "in_ing"] },
  { id: "lateral", rules: ["n_l", "r_l"] },
  { id: "labial", rules: ["f_h"] },
  { id: "rhyme", rules: ["ian_iang", "uan_uang", "ie_ve", "ou_iu"] },
];

/** 规则展示：对照形 + 示例，与后端 `rule_examples` 同文案 */
export const PINYIN_CORRECTION_RULE_META: Record<PinyinCorrectionRuleId, string> = {
  z_zh: "zh ↔ z · 知道 ~ 自道",
  c_ch: "ch ↔ c · 吃饭 ~ 辞饭",
  s_sh: "sh ↔ s · 上海 ~ 桑海",
  an_ang: "an ↔ ang · 搬家 ~ 帮家",
  en_eng: "en ↔ eng · 真实 ~ 真时",
  in_ing: "in ↔ ing · 拼音 ~ 拼因",
  n_l: "n ↔ l · 牛奶 ~ 流奶",
  r_l: "r ↔ l · 热爱 ~ 勒爱",
  f_h: "f ↔ h · 反复 ~ 很复",
  ian_iang: "ian ↔ iang · 天亮 ~ 天量",
  uan_uang: "uan ↔ uang · 开关 ~ 宽关",
  ie_ve: "ie ↔ üe · 谢谢 ~ 血血",
  ou_iu: "ou ↔ iu · 朋友 ~ 平友",
};

/** 是否为已知规则 id */
export function isPinyinCorrectionRuleId(id: string): id is PinyinCorrectionRuleId {
  return (PINYIN_CORRECTION_RULE_IDS as readonly string[]).includes(id);
}

/** 规则清洗：trim 去空、只留已知、保序去重；与后端 `sanitize_rules` 同口径 */
export function sanitizePinyinCorrectionRules(rules: string[]): PinyinCorrectionRuleId[] {
  const seen = new Set<string>();
  const out: PinyinCorrectionRuleId[] = [];
  for (const rule of rules) {
    const id = rule.trim();
    if (id === "" || !isPinyinCorrectionRuleId(id) || seen.has(id)) continue;
    seen.add(id);
    out.push(id);
  }
  return out;
}

/** 单个音节折叠：与后端 `fold_syllable` 同顺序同范式，未启用规则时原样返回 */
export function foldPinyinSyllable(syllable: string, enabled: ReadonlySet<string>): string {
  let out = syllable;
  if (enabled.has("z_zh") && out.startsWith("zh")) out = `z${out.slice(2)}`;
  if (enabled.has("c_ch") && out.startsWith("ch")) out = `c${out.slice(2)}`;
  if (enabled.has("s_sh") && out.startsWith("sh")) out = `s${out.slice(2)}`;
  if (enabled.has("n_l") && out.startsWith("n")) out = `l${out.slice(1)}`;
  if (enabled.has("r_l") && out.startsWith("r")) out = `l${out.slice(1)}`;
  if (enabled.has("f_h") && out.startsWith("f")) out = `h${out.slice(1)}`;
  if (enabled.has("ian_iang") && out.endsWith("iang")) {
    out = `${out.slice(0, -4)}ian`;
  } else if (enabled.has("uan_uang") && out.endsWith("uang")) {
    out = `${out.slice(0, -4)}uan`;
  } else {
    if (enabled.has("an_ang") && out.endsWith("ang")) out = `${out.slice(0, -3)}an`;
    if (enabled.has("en_eng") && out.endsWith("eng")) out = `${out.slice(0, -3)}en`;
    if (enabled.has("in_ing") && out.endsWith("ing")) out = `${out.slice(0, -3)}in`;
  }
  if (enabled.has("ie_ve")) {
    if (out.endsWith("ve")) {
      out = `${out.slice(0, -2)}ie`;
    } else if (out.includes("v")) {
      out = out.replaceAll("v", "ie");
    }
  }
  if (enabled.has("ou_iu") && out.endsWith("iu")) out = `${out.slice(0, -2)}ou`;
  return out;
}

/** 整串拼音折叠：按空白切分逐音节折叠后单空格连接，与后端 `fold_pinyin_text` 同口径 */
export function foldPinyinText(pinyin: string, enabled: ReadonlySet<string>): string {
  return pinyin
    .split(/\s+/)
    .map((syllable) => foldPinyinSyllable(syllable, enabled))
    .filter((syllable) => syllable !== "")
    .join(" ");
}

/** 拼音纠正设置快照：调用方从 `configValue` 组装 */
export interface PinyinCorrectionSetting {
  /** 总开关 */
  enabled: boolean;
  /** 选中规则 */
  rules: string[];
  /** 应用于识别 */
  applyRecognition: boolean;
  /** 应用于指令 */
  applyDirective: boolean;
}

/** 识别纠正是否生效：总开关开、识别应用开、且有选中规则 */
export function isRecognitionCorrectionActive(setting: PinyinCorrectionSetting): boolean {
  return setting.enabled && setting.applyRecognition && setting.rules.length > 0;
}

/** 指令纠正是否生效：总开关开、指令应用开、且有选中规则 */
export function isDirectiveCorrectionActive(setting: PinyinCorrectionSetting): boolean {
  return setting.enabled && setting.applyDirective && setting.rules.length > 0;
}

/** 识别用规则集：门控失败时返回空集，调用方无需再分支 */
export function correctionSetForRecognition(setting: PinyinCorrectionSetting): Set<string> {
  if (!isRecognitionCorrectionActive(setting)) return new Set();
  return new Set(sanitizePinyinCorrectionRules(setting.rules));
}

/** 指令用规则集：门控失败时返回空集，调用方无需再分支 */
export function correctionSetForDirective(setting: PinyinCorrectionSetting): Set<string> {
  if (!isDirectiveCorrectionActive(setting)) return new Set();
  return new Set(sanitizePinyinCorrectionRules(setting.rules));
}

/** 组内是否全选：纯函数，供全选按钮判断文案 */
export function isGroupSelected(
  selected: ReadonlySet<string>,
  group: PinyinCorrectionGroup,
): boolean {
  return group.rules.every((rule) => selected.has(rule));
}
