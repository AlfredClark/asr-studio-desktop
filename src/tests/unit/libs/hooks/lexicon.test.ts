import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_LEXICON_FILTER,
  DEFAULT_LEXICON_SORT,
  batchDeleteLexiconEntries,
  batchSetLexiconEntriesEnabled,
  filterLexiconEntries,
  isLexiconFilterMode,
  isLexiconSortMode,
  lexiconState,
  loadLexiconFilterMode,
  loadLexiconSortMode,
  moveLexiconGroup,
  refreshLexicon,
  setLexiconGroupEnabled,
  sortLexiconEntries,
} from "#libs/hooks/lexicon.svelte";
import type { LexiconEntry, LexiconSnapshot } from "#libs/commands/types";

vi.mock("#libs/commands", () => ({
  default: {
    listLexicon: vi.fn(),
    createLexiconGroup: vi.fn(),
    renameLexiconGroup: vi.fn(),
    deleteLexiconGroup: vi.fn(),
    moveLexiconGroup: vi.fn(),
    setActiveLexiconGroups: vi.fn(),
    createLexiconEntry: vi.fn(),
    updateLexiconEntry: vi.fn(),
    deleteLexiconEntry: vi.fn(),
    suggestLexiconPinyin: vi.fn(),
  },
}));

vi.mock("#libs/utils/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import commands from "#libs/commands";

const mocked = vi.mocked(commands);

type FakeResult =
  { status: "ok"; data: unknown } | { status: "error"; error: { kind: string; message: string } };

/** 与 `EnhancedCommand` 行为一致的替身：按分支跑回调后结算结果 */
function fakeCommand(result: FakeResult): {
  success: (handler: (data: never) => unknown) => unknown;
  failed: (failure: (never: never) => unknown) => unknown;
  result: () => Promise<FakeResult>;
} {
  const chain = {
    success(handler: (data: never) => unknown) {
      if (result.status === "ok") handler(result.data as never);
      return chain;
    },
    failed(handler: (failure: never) => unknown) {
      if (result.status === "error") handler(result.error as never);
      return chain;
    },
    result() {
      return Promise.resolve(result);
    },
  };
  return chain;
}

const snapshot: LexiconSnapshot = {
  version: 1,
  groups: [
    { id: "global", name: "", is_global: true },
    { id: "g-work", name: "工作", is_global: false },
  ],
  entries: [],
  active_group_ids: ["g-work"],
};

beforeEach(() => {
  vi.clearAllMocks();
  lexiconState.value = { version: 1, groups: [], entries: [], active_group_ids: [] };
  mocked.listLexicon.mockReturnValue(fakeCommand({ status: "ok", data: snapshot }) as never);
  mocked.setActiveLexiconGroups.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.moveLexiconGroup.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
});

describe("词库 hook", () => {
  it("刷新回写快照", async () => {
    expect(await refreshLexicon()).toBe(true);
    expect(lexiconState.value.active_group_ids).toEqual(["g-work"]);
  });

  it("启用分组追加 id 并去重", async () => {
    lexiconState.value = structuredClone(snapshot);

    expect(await setLexiconGroupEnabled("g-new", true)).toBe(true);
    expect(mocked.setActiveLexiconGroups).toHaveBeenCalledWith(["g-work", "g-new"]);

    // 已在集合中：不重复追加
    expect(await setLexiconGroupEnabled("g-work", true)).toBe(true);
    expect(mocked.setActiveLexiconGroups).toHaveBeenCalledWith(["g-work"]);
  });

  it("禁用分组移除 id", async () => {
    lexiconState.value = structuredClone(snapshot);

    expect(await setLexiconGroupEnabled("g-work", false)).toBe(true);
    expect(mocked.setActiveLexiconGroups).toHaveBeenCalledWith([]);
  });

  it("命令失败时返回假且不刷新", async () => {
    mocked.setActiveLexiconGroups.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    expect(await setLexiconGroupEnabled("g-work", false)).toBe(false);
    expect(mocked.listLexicon).not.toHaveBeenCalled();
  });

  it("移动分组透传方向并刷新", async () => {
    expect(await moveLexiconGroup("g-work", "up")).toBe(true);
    expect(mocked.moveLexiconGroup).toHaveBeenCalledWith("g-work", "up");
    expect(mocked.listLexicon).toHaveBeenCalled();

    mocked.moveLexiconGroup.mockReturnValueOnce(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );
    expect(await moveLexiconGroup("g-work", "down")).toBe(false);
  });

  it("批量删除逐个落盘后统一刷新", async () => {
    mocked.deleteLexiconEntry.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);

    expect(await batchDeleteLexiconEntries(["w-1", "w-2"])).toBe(true);
    expect(mocked.deleteLexiconEntry).toHaveBeenCalledTimes(2);
    expect(mocked.listLexicon).toHaveBeenCalled();
  });

  it("批量启用部分失败时返回假", async () => {
    mocked.updateLexiconEntry
      .mockReturnValueOnce(fakeCommand({ status: "ok", data: null }) as never)
      .mockReturnValueOnce(
        fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
      );

    expect(await batchSetLexiconEntriesEnabled(["w-1", "w-2"], true)).toBe(false);
    expect(mocked.updateLexiconEntry).toHaveBeenCalledTimes(2);
    expect(mocked.updateLexiconEntry).toHaveBeenCalledWith("w-1", null, null, null, null, true);
  });
});

/** 排序与筛选纯函数的行级用例 */
function makeViewEntry(partial: Partial<LexiconEntry> & { id: string }): LexiconEntry {
  return {
    group_id: "g-work",
    text: "测试",
    pinyin: "ce shi",
    weight: 0,
    enabled: true,
    ...partial,
  };
}

describe("词条排序与筛选", () => {
  it("非法取值校验失败", () => {
    expect(isLexiconSortMode("unknown")).toBe(false);
    expect(isLexiconSortMode(null)).toBe(false);
    expect(isLexiconFilterMode("unknown")).toBe(false);
    expect(isLexiconFilterMode(null)).toBe(false);
    expect(isLexiconSortMode("pinyin-asc")).toBe(true);
    expect(isLexiconFilterMode("disabled")).toBe(true);
  });

  it("无存储环境回落默认值", () => {
    expect(loadLexiconSortMode()).toBe(DEFAULT_LEXICON_SORT);
    expect(loadLexiconFilterMode()).toBe(DEFAULT_LEXICON_FILTER);
  });

  it("按启用状态过滤", () => {
    const entries = [
      makeViewEntry({ id: "w-1", enabled: true }),
      makeViewEntry({ id: "w-2", enabled: false }),
    ];

    expect(filterLexiconEntries(entries, "all")).toEqual(entries);
    expect(filterLexiconEntries(entries, "enabled").map((entry) => entry.id)).toEqual(["w-1"]);
    expect(filterLexiconEntries(entries, "disabled").map((entry) => entry.id)).toEqual(["w-2"]);
  });

  it("添加顺序保持后端顺序", () => {
    const entries = [makeViewEntry({ id: "w-1" }), makeViewEntry({ id: "w-2" })];

    expect(sortLexiconEntries(entries, "added")).toBe(entries);
  });

  it("按拼音升降序排列，同音按词面兜底", () => {
    const entries = [
      makeViewEntry({ id: "w-low", pinyin: "zai", text: "在" }),
      makeViewEntry({ id: "w-high", pinyin: "cheng zhang", text: "成长" }),
      makeViewEntry({ id: "w-tie-b", pinyin: "ce shi", text: "b" }),
      makeViewEntry({ id: "w-tie-a", pinyin: "ce shi", text: "a" }),
    ];

    expect(sortLexiconEntries(entries, "pinyin-asc").map((entry) => entry.id)).toEqual([
      "w-tie-a",
      "w-tie-b",
      "w-high",
      "w-low",
    ]);
    expect(sortLexiconEntries(entries, "pinyin-desc").map((entry) => entry.id)).toEqual([
      "w-low",
      "w-high",
      "w-tie-b",
      "w-tie-a",
    ]);
  });

  it("按权重升降序排列，同权重保持原顺序", () => {
    const entries = [
      makeViewEntry({ id: "w-a", weight: 5 }),
      makeViewEntry({ id: "w-b", weight: -10 }),
      makeViewEntry({ id: "w-c", weight: 5 }),
    ];

    expect(sortLexiconEntries(entries, "weight-asc").map((entry) => entry.id)).toEqual([
      "w-b",
      "w-a",
      "w-c",
    ]);
    expect(sortLexiconEntries(entries, "weight-desc").map((entry) => entry.id)).toEqual([
      "w-a",
      "w-c",
      "w-b",
    ]);
  });
});
