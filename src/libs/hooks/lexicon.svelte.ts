// 词库的前端内存态：分组 + 词条 + 生效组 id 一次拉全，页面内按组筛选；
// 写操作经命令落盘后用返回快照回写——不为省一次 IPC 而复刻读取路径。
import commands from "#libs/commands";
import { reportCommandFailure } from "#libs/commands/cores";
import type { LexiconEntry, LexiconGroup, LexiconSnapshot } from "#libs/commands/types";
import { m } from "#libs/i18n/paraglide/messages";
import { toast } from "#libs/utils/toast";

/** 词库快照缺省值：空分组 + 空词条 + 空生效集（仅全局组生效） */
const EMPTY_SNAPSHOT: LexiconSnapshot = {
  version: 1,
  groups: [],
  entries: [],
  active_group_ids: [],
};

/** 跨页面共享的词库状态；首帧水合前为空快照 */
export const lexiconState = $state<{ value: LexiconSnapshot }>({ value: EMPTY_SNAPSHOT });

/** 拉取词库快照；失败上报并返回假，调用方按此分支 */
export async function refreshLexicon(): Promise<boolean> {
  const result = await commands.listLexicon().result();
  if (result.status === "error") {
    reportCommandFailure("[lexicon] failed to load lexicon", result.error);
    return false;
  }
  lexiconState.value = result.data;
  return true;
}

/** 同步快照并提示成功；调用方按返回值决定是否收起表单 */
async function settle<T>(
  result: { status: "ok"; data: T } | { status: "error"; error: unknown },
  onOk: (data: T) => void,
  scope: string,
): Promise<boolean> {
  if (result.status === "error") {
    toast.error(m.lexicon_update_failed());
    reportCommandFailure(`[lexicon] ${scope} failed`, result.error);
    return false;
  }
  onOk(result.data);
  toast.success(m.lexicon_updated());
  await refreshLexicon();
  return true;
}

/** 新建分组 */
export async function createLexiconGroup(name: string): Promise<boolean> {
  return settle(await commands.createLexiconGroup(name).result(), () => {}, "create group");
}

/** 重命名分组；全局组由后端拒绝 */
export async function renameLexiconGroup(id: string, name: string): Promise<boolean> {
  return settle(await commands.renameLexiconGroup(id, name).result(), () => {}, "rename group");
}

/** 删除分组；级联删词条，删的是生效分组则将其移出生效集 */
export async function deleteLexiconGroup(id: string): Promise<boolean> {
  return settle(await commands.deleteLexiconGroup(id).result(), () => {}, "delete group");
}

/** 移动分组：仅在用户分组之间换位，全局组恒置顶；触顶 / 触底原地不动 */
export async function moveLexiconGroup(id: string, direction: "up" | "down"): Promise<boolean> {
  return settle(await commands.moveLexiconGroup(id, direction).result(), () => {}, "move group");
}

/** 设置分组启用状态：增删生效集后整包落盘；全局组恒生效，不进集合 */
export async function setLexiconGroupEnabled(id: string, enabled: boolean): Promise<boolean> {
  const current = lexiconState.value.active_group_ids;
  const next = enabled
    ? [...current.filter((active) => active !== id), id]
    : current.filter((active) => active !== id);
  return settle(
    await commands.setActiveLexiconGroups(next).result(),
    () => {},
    "set active groups",
  );
}

/** 新建词条输入：拼音缺省时后端自动建议 */
export interface NewLexiconEntry {
  /** 所属分组 id */
  groupId: string;
  /** 词面 */
  text: string;
  /** 拼音（空格分隔），缺省自动建议 */
  pinyin?: string;
  /** 权重，缺省 0 */
  weight?: number;
  /** 是否启用，缺省 true */
  enabled?: boolean;
}

/** 新建词条 */
export async function createLexiconEntry(input: NewLexiconEntry): Promise<boolean> {
  return settle(
    await commands
      .createLexiconEntry(
        input.groupId,
        input.text,
        input.pinyin ?? null,
        input.weight ?? null,
        input.enabled ?? null,
      )
      .result(),
    () => {},
    "create entry",
  );
}

/** 更新词条输入：各字段缺省即不改 */
export interface LexiconEntryPatch {
  /** 移动到分组 */
  groupId?: string;
  /** 词面 */
  text?: string;
  /** 拼音（空格分隔） */
  pinyin?: string;
  /** 权重 */
  weight?: number;
  /** 是否启用 */
  enabled?: boolean;
}

/** 更新词条 */
export async function updateLexiconEntry(id: string, patch: LexiconEntryPatch): Promise<boolean> {
  return settle(
    await commands
      .updateLexiconEntry(
        id,
        patch.groupId ?? null,
        patch.text ?? null,
        patch.pinyin ?? null,
        patch.weight ?? null,
        patch.enabled ?? null,
      )
      .result(),
    () => {},
    "update entry",
  );
}

/** 删除词条 */
export async function deleteLexiconEntry(id: string): Promise<boolean> {
  return settle(await commands.deleteLexiconEntry(id).result(), () => {}, "delete entry");
}

/** 批量删除词条：逐个落盘后统一刷新，一次提示；失败逐项上报 */
export async function batchDeleteLexiconEntries(ids: string[]): Promise<boolean> {
  let failed = 0;
  for (const id of ids) {
    const result = await commands.deleteLexiconEntry(id).result();
    if (result.status === "error") {
      failed += 1;
      reportCommandFailure("[lexicon] batch delete failed", result.error);
    }
  }
  await refreshLexicon();
  if (failed > 0) {
    toast.error(m.lexicon_update_failed());
    return false;
  }
  toast.success(m.lexicon_updated());
  return true;
}

/** 批量启用 / 禁用词条：逐个落盘后统一刷新，一次提示 */
export async function batchSetLexiconEntriesEnabled(
  ids: string[],
  enabled: boolean,
): Promise<boolean> {
  let failed = 0;
  for (const id of ids) {
    const result = await commands.updateLexiconEntry(id, null, null, null, null, enabled).result();
    if (result.status === "error") {
      failed += 1;
      reportCommandFailure("[lexicon] batch enable failed", result.error);
    }
  }
  await refreshLexicon();
  if (failed > 0) {
    toast.error(m.lexicon_update_failed());
    return false;
  }
  toast.success(m.lexicon_updated());
  return true;
}

/** 为词面建议拼音；失败上报并返回空，调用方保留原值 */
export async function suggestLexiconPinyin(text: string): Promise<string> {
  const result = await commands.suggestLexiconPinyin(text).result();
  if (result.status === "error") {
    reportCommandFailure("[lexicon] suggest pinyin failed", result.error);
    return "";
  }
  return result.data;
}

/** 分组展示名：全局组按语言渲染，普通组用存量名 */
export function groupLabel(group: LexiconGroup): string {
  return group.is_global ? m.lexicon_global_name() : group.name;
}

/** 按组过滤词条：保持后端顺序（落盘顺序），前端不重排 */
export function entriesOfGroup(entries: LexiconEntry[], groupId: string): LexiconEntry[] {
  return entries.filter((entry) => entry.group_id === groupId);
}

/** 词条排序取值：`added` 为后端落盘顺序，其余按拼音或权重升降 */
export type LexiconSortMode = "added" | "pinyin-asc" | "pinyin-desc" | "weight-asc" | "weight-desc";

/** 词条筛选取值：按启用状态过滤，`all` 不过滤 */
export type LexiconFilterMode = "all" | "enabled" | "disabled";

/** 默认排序：添加顺序（后端落盘顺序） */
export const DEFAULT_LEXICON_SORT: LexiconSortMode = "added";

/** 默认筛选：全部展示 */
export const DEFAULT_LEXICON_FILTER: LexiconFilterMode = "all";

/** 排序持久化键名，改名即视为放弃老用户存量 */
export const LEXICON_SORT_STORAGE_KEY = "app.lexicon-sort";

/** 筛选持久化键名，改名即视为放弃老用户存量 */
export const LEXICON_FILTER_STORAGE_KEY = "app.lexicon-filter";

/** 校验排序取值，脏数据回落时使用 */
export function isLexiconSortMode(value: unknown): value is LexiconSortMode {
  return (
    typeof value === "string" &&
    (
      ["added", "pinyin-asc", "pinyin-desc", "weight-asc", "weight-desc"] as readonly string[]
    ).includes(value)
  );
}

/** 校验筛选取值，脏数据回落时使用 */
export function isLexiconFilterMode(value: unknown): value is LexiconFilterMode {
  return (
    typeof value === "string" &&
    (["all", "enabled", "disabled"] as readonly string[]).includes(value)
  );
}

/** 读取持久化排序，缺失或非法一律回落添加顺序 */
export function loadLexiconSortMode(): LexiconSortMode {
  try {
    if (typeof localStorage === "undefined") {
      return DEFAULT_LEXICON_SORT;
    }
    const stored = localStorage.getItem(LEXICON_SORT_STORAGE_KEY);
    return isLexiconSortMode(stored) ? stored : DEFAULT_LEXICON_SORT;
  } catch {
    // 隐私模式等存储不可用时按默认值渲染，不阻塞首帧
    return DEFAULT_LEXICON_SORT;
  }
}

/** 读取持久化筛选，缺失或非法一律回落全部展示 */
export function loadLexiconFilterMode(): LexiconFilterMode {
  try {
    if (typeof localStorage === "undefined") {
      return DEFAULT_LEXICON_FILTER;
    }
    const stored = localStorage.getItem(LEXICON_FILTER_STORAGE_KEY);
    return isLexiconFilterMode(stored) ? stored : DEFAULT_LEXICON_FILTER;
  } catch {
    // 隐私模式等存储不可用时按默认值渲染，不阻塞首帧
    return DEFAULT_LEXICON_FILTER;
  }
}

/** 持久化排序：落盘失败仍返回，调用方照常切内存态保证本次会话可用 */
export function saveLexiconSortMode(next: LexiconSortMode): void {
  try {
    localStorage.setItem(LEXICON_SORT_STORAGE_KEY, next);
  } catch {
    // 落盘失败不抛错，内存态切换照常生效
  }
}

/** 持久化筛选：落盘失败仍返回，调用方照常切内存态保证本次会话可用 */
export function saveLexiconFilterMode(next: LexiconFilterMode): void {
  try {
    localStorage.setItem(LEXICON_FILTER_STORAGE_KEY, next);
  } catch {
    // 落盘失败不抛错，内存态切换照常生效
  }
}

/** 按启用状态过滤词条：`all` 原样返回，其余留对应分支 */
export function filterLexiconEntries(
  entries: LexiconEntry[],
  mode: LexiconFilterMode,
): LexiconEntry[] {
  if (mode === "enabled") {
    return entries.filter((entry) => entry.enabled);
  }
  if (mode === "disabled") {
    return entries.filter((entry) => !entry.enabled);
  }
  return entries;
}

/** 按模式排序词条：`added` 原样返回（后端落盘顺序），其余拷贝后排序 */
export function sortLexiconEntries(entries: LexiconEntry[], mode: LexiconSortMode): LexiconEntry[] {
  if (mode === "pinyin-asc" || mode === "pinyin-desc") {
    const direction = mode === "pinyin-asc" ? 1 : -1;
    return [...entries].sort(
      (first, second) =>
        (first.pinyin.localeCompare(second.pinyin, ["zh-CN", "en"]) ||
          first.text.localeCompare(second.text, ["zh-CN", "en"])) * direction,
    );
  }
  if (mode === "weight-asc" || mode === "weight-desc") {
    const direction = mode === "weight-asc" ? 1 : -1;
    return [...entries].sort((first, second) => (first.weight - second.weight) * direction);
  }
  return entries;
}
