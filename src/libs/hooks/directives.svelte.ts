// 指令的前端内存态：数组顺序即展示顺序；写操作经命令落盘后刷新。
// 触发文本的拼音口径复用词库（`suggestLexiconPinyin` + 归一），触发匹配下期同样走词库口径。
import { SvelteSet } from "svelte/reactivity";
import commands from "#libs/commands";
import { reportCommandFailure } from "#libs/commands/cores";
import type { Directive, DirectiveAction } from "#libs/commands/types";
import { normalizeLexiconPinyin, suggestLexiconPinyin } from "#libs/hooks/lexicon.svelte";
import {
  correctionSetForDirective,
  foldPinyinText,
  type PinyinCorrectionSetting,
} from "#libs/pinyin-correction";
import { m } from "#libs/i18n/paraglide/messages";
import { toast } from "#libs/utils/toast";

/** 触发间隔上下界：毫秒，与后端 `INTERVAL_MS_*` 同口径，输入时钳制 */
export const DIRECTIVE_INTERVAL_MIN = 0;
/** 触发间隔上下界：毫秒，与后端 `INTERVAL_MS_*` 同口径，输入时钳制 */
export const DIRECTIVE_INTERVAL_MAX = 5000;

/** 跨页面共享的指令状态；首帧水合前为空表 */
export const directiveState = $state<{ value: Directive[] }>({ value: [] });

/** 拉取指令表；失败上报并返回假，调用方按此分支 */
export async function refreshDirectives(): Promise<boolean> {
  const result = await commands.listDirectives().result();
  if (result.status === "error") {
    reportCommandFailure("[directives] failed to load directives", result.error);
    return false;
  }
  directiveState.value = result.data;
  return true;
}

/** 同步快照并提示成功；调用方按返回值决定是否收起表单 */
async function settle(
  result: { status: "ok"; data: unknown } | { status: "error"; error: unknown },
  scope: string,
): Promise<boolean> {
  if (result.status === "error") {
    toast.error(m.directives_update_failed());
    reportCommandFailure(`[directives] ${scope} failed`, result.error);
    return false;
  }
  toast.success(m.directives_updated());
  await refreshDirectives();
  return true;
}

/** 新建指令输入：拼音缺省时后端按触发文本自动建议 */
export interface NewDirective {
  /** 展示名 */
  name: string;
  /** 触发文本 */
  triggerText: string;
  /** 触发拼音（空格分隔），缺省自动建议 */
  triggerPinyin?: string;
  /** 是否启用 */
  enabled: boolean;
  /** 事件间隔毫秒 */
  intervalMs: number;
  /** 按键事件序列 */
  actions: DirectiveAction[];
}

/** 新建指令 */
export async function createDirective(input: NewDirective): Promise<boolean> {
  return settle(
    await commands
      .createDirective(
        input.name,
        input.triggerText,
        input.triggerPinyin ?? null,
        input.enabled,
        input.intervalMs,
        input.actions,
      )
      .result(),
    "create directive",
  );
}

/** 更新指令输入：各字段缺省即不改，动作序列整包替换 */
export interface DirectivePatch {
  /** 展示名 */
  name?: string;
  /** 触发文本 */
  triggerText?: string;
  /** 触发拼音（空格分隔） */
  triggerPinyin?: string;
  /** 是否启用 */
  enabled?: boolean;
  /** 事件间隔毫秒 */
  intervalMs?: number;
  /** 按键事件序列 */
  actions?: DirectiveAction[];
}

/** 更新指令 */
export async function updateDirective(id: string, patch: DirectivePatch): Promise<boolean> {
  return settle(
    await commands
      .updateDirective(
        id,
        patch.name ?? null,
        patch.triggerText ?? null,
        patch.triggerPinyin ?? null,
        patch.enabled ?? null,
        patch.intervalMs ?? null,
        patch.actions ?? null,
      )
      .result(),
    "update directive",
  );
}

/** 删除指令 */
export async function deleteDirective(id: string): Promise<boolean> {
  return settle(await commands.deleteDirective(id).result(), "delete directive");
}

/** 移动指令：列表内换位；触顶 / 触底原地不动 */
export async function moveDirective(id: string, direction: "up" | "down"): Promise<boolean> {
  return settle(await commands.moveDirective(id, direction).result(), "move directive");
}

/** 移动指令到指定序号：拖拽排序用；越界后端钳制 */
export async function moveDirectiveTo(id: string, index: number): Promise<boolean> {
  return settle(await commands.moveDirectiveTo(id, index).result(), "move directive");
}

/** 设置指令启用状态 */
export async function setDirectiveEnabled(id: string, enabled: boolean): Promise<boolean> {
  return updateDirective(id, { enabled });
}

/** 为触发文本建议拼音；失败上报并返回空，调用方保留原值 */
export async function suggestDirectivePinyin(text: string): Promise<string> {
  return suggestLexiconPinyin(text);
}

/** 触发文本判重键：trim 后精确配对，与后端同口径 */
export function directiveTriggerKey(text: string): string {
  return text.trim();
}

/** 触发是否命中已存在（文本或拼音任一命中即真；更新时排除自身）；纯函数，可单测 */
export function isTriggerDuplicate(
  directives: Directive[],
  text: string,
  excludeId?: string,
  pinyin?: string,
): boolean {
  const key = directiveTriggerKey(text);
  if (key !== "" && directives.some((d) => d.trigger_text === key && d.id !== excludeId)) {
    return true;
  }
  const normalized = pinyin == null ? "" : normalizeDirectivePinyin(pinyin);
  return (
    normalized !== "" &&
    directives.some((d) => d.trigger_pinyin === normalized && d.id !== excludeId)
  );
}

/**
 * 按拼音匹配指令：归一化后精确相等，或 raw 文本 trim 后与触发文本相等；仅启用项参与。
 * 纠正设置生效时（总开关开、指令应用开、有选中规则），两侧拼音先同折叠再比对。
 */
export function findDirectiveByPinyin(
  directives: Directive[],
  spokenPinyin: string,
  rawText: string,
  correction: PinyinCorrectionSetting | null = null,
): Directive | null {
  const normalizedSpoken = normalizeDirectivePinyin(spokenPinyin);
  const folded = correction == null ? null : correctionSetForDirective(correction);
  const spoken =
    folded == null || folded.size === 0
      ? normalizedSpoken
      : foldPinyinText(normalizedSpoken, folded);
  const trimmed = rawText.trim();
  for (const directive of directives) {
    if (!directive.enabled) continue;
    const normalizedTrigger = normalizeDirectivePinyin(directive.trigger_pinyin);
    const trigger =
      folded == null || folded.size === 0
        ? normalizedTrigger
        : foldPinyinText(normalizedTrigger, folded);
    if (spoken !== "" && trigger !== "" && trigger === spoken) return directive;
    if (trimmed !== "" && directive.trigger_text === trimmed) return directive;
  }
  return null;
}

/** 动作序列收尾是否平衡：空序列、有残留按下均为否；纯函数，可单测 */
export function areActionsBalanced(actions: DirectiveAction[]): boolean {
  if (actions.length === 0) return false;
  const held = new SvelteSet<string>();
  for (const action of actions) {
    const key = action.key.trim();
    if (key === "") return false;
    if (action.direction === "press") {
      if (held.has(key)) return false;
      held.add(key);
    } else {
      if (!held.has(key)) return false;
      held.delete(key);
    }
  }
  return held.size === 0;
}

/** 拼音归一：与后端落盘形态同口径（复用词库实现） */
export function normalizeDirectivePinyin(pinyin: string): string {
  return normalizeLexiconPinyin(pinyin);
}
