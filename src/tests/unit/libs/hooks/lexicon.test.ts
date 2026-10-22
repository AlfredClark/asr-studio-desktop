import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_LEXICON_FILTER,
  DEFAULT_LEXICON_SORT,
  LEXICON_BATCH_ADD_LIMIT,
  batchCreateLexiconEntries,
  batchCopyLexiconEntries,
  batchDeleteLexiconEntries,
  batchMoveLexiconEntries,
  batchSetLexiconEntriesEnabled,
  filterLexiconEntries,
  isLexiconFilterMode,
  isLexiconSortMode,
  lexiconState,
  loadLexiconFilterMode,
  loadLexiconSortMode,
  moveLexiconGroup,
  normalizeLexiconPinyin,
  parseBatchEntryTexts,
  refreshLexicon,
  setLexiconGroupEnabled,
  sortLexiconEntries,
  suggestLexiconFileName,
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
    batchDeleteLexiconEntries: vi.fn(),
    batchCopyLexiconEntries: vi.fn(),
    batchMoveLexiconEntries: vi.fn(),
    batchSetLexiconEntriesEnabled: vi.fn(),
    suggestLexiconPinyin: vi.fn(),
  },
}));

vi.mock("#libs/utils/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
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

  it("批量删除单事务落盘后统一刷新", async () => {
    mocked.batchDeleteLexiconEntries.mockReturnValue(
      fakeCommand({ status: "ok", data: 2 }) as never,
    );

    expect(await batchDeleteLexiconEntries(["w-1", "w-2"])).toBe(true);
    expect(mocked.batchDeleteLexiconEntries).toHaveBeenCalledWith(["w-1", "w-2"]);
    expect(mocked.listLexicon).toHaveBeenCalled();
  });

  it("批量删除失败时返回假", async () => {
    mocked.batchDeleteLexiconEntries.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    expect(await batchDeleteLexiconEntries(["w-1", "w-2"])).toBe(false);
  });

  it("批量启用单事务落盘", async () => {
    mocked.batchSetLexiconEntriesEnabled.mockReturnValue(
      fakeCommand({ status: "ok", data: 2 }) as never,
    );

    expect(await batchSetLexiconEntriesEnabled(["w-1", "w-2"], true)).toBe(true);
    expect(mocked.batchSetLexiconEntriesEnabled).toHaveBeenCalledWith(["w-1", "w-2"], true);
    expect(mocked.listLexicon).toHaveBeenCalled();
  });

  it("批量启用失败时返回假", async () => {
    mocked.batchSetLexiconEntriesEnabled.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    expect(await batchSetLexiconEntriesEnabled(["w-1", "w-2"], true)).toBe(false);
  });

  it("批量复制透传目标分组并返回汇总", async () => {
    mocked.batchCopyLexiconEntries.mockReturnValue(
      fakeCommand({ status: "ok", data: { copied: 2, skipped: 1 } }) as never,
    );

    const result = await batchCopyLexiconEntries(["w-1", "w-2"], "g-life");
    expect(mocked.batchCopyLexiconEntries).toHaveBeenCalledWith(["w-1", "w-2"], "g-life");
    expect(result).toEqual({ copied: 2, skipped: 1, ok: true });
    expect(mocked.listLexicon).toHaveBeenCalled();
  });

  it("批量复制失败时返回假且不刷新", async () => {
    mocked.batchCopyLexiconEntries.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    const result = await batchCopyLexiconEntries(["w-1"], "g-life");
    expect(result).toEqual({ copied: 0, skipped: 0, ok: false });
    expect(mocked.listLexicon).not.toHaveBeenCalled();
  });

  it("批量转移透传目标分组并返回汇总", async () => {
    mocked.batchMoveLexiconEntries.mockReturnValue(
      fakeCommand({ status: "ok", data: { copied: 1, skipped: 1 } }) as never,
    );

    const result = await batchMoveLexiconEntries(["w-1", "w-2"], "g-life");
    expect(mocked.batchMoveLexiconEntries).toHaveBeenCalledWith(["w-1", "w-2"], "g-life");
    expect(result).toEqual({ copied: 1, skipped: 1, ok: true });
    expect(mocked.listLexicon).toHaveBeenCalled();
  });

  it("批量转移失败时返回假且不刷新", async () => {
    mocked.batchMoveLexiconEntries.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    const result = await batchMoveLexiconEntries(["w-1"], "g-life");
    expect(result).toEqual({ copied: 0, skipped: 0, ok: false });
    expect(mocked.listLexicon).not.toHaveBeenCalled();
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

describe("批量添加解析与判重", () => {
  it("归一化拼音收缩多余空白", () => {
    expect(normalizeLexiconPinyin("quan  li ")).toBe("quan li");
    expect(normalizeLexiconPinyin("zai")).toBe("zai");
    expect(normalizeLexiconPinyin("")).toBe("");
  });

  it("解析批量输入去空并保序去重", () => {
    expect(parseBatchEntryTexts("权力 在 权力\n成长\t在")).toEqual(["权力", "在", "成长"]);
    expect(parseBatchEntryTexts("")).toEqual([]);
    expect(parseBatchEntryTexts("   \n ")).toEqual([]);
  });

  it("上限为 100", () => {
    expect(LEXICON_BATCH_ADD_LIMIT).toBe(100);
  });

  it("组内词音配对命中跳过，同词不同音照常创建", async () => {
    lexiconState.value = {
      version: 1,
      groups: [],
      entries: [makeViewEntry({ id: "w-1", group_id: "g-work", text: "在", pinyin: "zai" })],
      active_group_ids: [],
    };
    mocked.createLexiconEntry.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);

    const result = await batchCreateLexiconEntries("g-work", [
      { text: "在", pinyin: "zai" },
      { text: "在", pinyin: "zai  tong" },
      { text: "成长", pinyin: "cheng zhang" },
    ]);

    expect(result).toEqual({ created: 2, skipped: 1, ok: true });
    expect(mocked.createLexiconEntry).toHaveBeenCalledTimes(2);
    expect(mocked.createLexiconEntry).toHaveBeenCalledWith("g-work", "在", "zai tong", null, null);
    expect(mocked.listLexicon).toHaveBeenCalled();
  });

  it("跨组同词音不判重", async () => {
    lexiconState.value = {
      version: 1,
      groups: [],
      entries: [makeViewEntry({ id: "w-1", group_id: "g-other", text: "在", pinyin: "zai" })],
      active_group_ids: [],
    };
    mocked.createLexiconEntry.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);

    const result = await batchCreateLexiconEntries("g-work", [{ text: "在", pinyin: "zai" }]);

    expect(result).toEqual({ created: 1, skipped: 0, ok: true });
  });

  it("全部重复时一次信息提示且不落盘", async () => {
    const { toast } = await import("#libs/utils/toast");
    lexiconState.value = {
      version: 1,
      groups: [],
      entries: [makeViewEntry({ id: "w-1", group_id: "g-work", text: "在", pinyin: "zai" })],
      active_group_ids: [],
    };

    const result = await batchCreateLexiconEntries("g-work", [{ text: "在", pinyin: "zai" }]);

    expect(result).toEqual({ created: 0, skipped: 1, ok: true });
    expect(mocked.createLexiconEntry).not.toHaveBeenCalled();
    expect(vi.mocked(toast.info)).toHaveBeenCalled();
  });

  it("落盘失败时返回假并错误提示", async () => {
    lexiconState.value = { version: 1, groups: [], entries: [], active_group_ids: [] };
    mocked.createLexiconEntry.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    const result = await batchCreateLexiconEntries("g-work", [{ text: "在", pinyin: "zai" }]);

    expect(result.ok).toBe(false);
    expect(result.created).toBe(0);
  });
});

describe("导出文件名建议", () => {
  it("清洗路径分隔符与控制字符", () => {
    expect(suggestLexiconFileName("工作/分组:二")).toBe("lexicon-工作_分组_二.json");
    expect(suggestLexiconFileName("a*b?c")).toBe("lexicon-a_b_c.json");
  });

  it("空名回落通用名", () => {
    expect(suggestLexiconFileName("")).toBe("lexicon-group.json");
    expect(suggestLexiconFileName("   ")).toBe("lexicon-group.json");
    expect(suggestLexiconFileName("...")).toBe("lexicon-group.json");
  });

  it("正常名直接拼接", () => {
    expect(suggestLexiconFileName("工作")).toBe("lexicon-工作.json");
  });
});
