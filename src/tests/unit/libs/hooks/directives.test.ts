import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  areActionsBalanced,
  createDirective,
  deleteDirective,
  directiveState,
  directiveTriggerKey,
  findDirectiveByPinyin,
  isTriggerDuplicate,
  moveDirective,
  normalizeDirectiveText,
  pinyinEditDistance,
  refreshDirectives,
  setDirectiveEnabled,
  updateDirective,
} from "#libs/hooks/directives.svelte";
import type { Directive, DirectiveAction } from "#libs/commands/types";

vi.mock("#libs/commands", () => ({
  default: {
    listDirectives: vi.fn(),
    createDirective: vi.fn(),
    updateDirective: vi.fn(),
    deleteDirective: vi.fn(),
    moveDirective: vi.fn(),
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

function makeDirective(partial: Partial<Directive> & { id: string }): Directive {
  return {
    name: "测试",
    trigger_text: "打开记事本",
    trigger_pinyin: "da kai ji shi ben",
    enabled: true,
    interval_ms: 50,
    tolerance: 0,
    actions: [],
    ...partial,
  };
}

function press(key: string): DirectiveAction {
  return { key, direction: "press" };
}

function release(key: string): DirectiveAction {
  return { key, direction: "release" };
}

beforeEach(() => {
  vi.clearAllMocks();
  directiveState.value = [];
  mocked.listDirectives.mockReturnValue(fakeCommand({ status: "ok", data: [] }) as never);
  mocked.createDirective.mockReturnValue(
    fakeCommand({ status: "ok", data: makeDirective({ id: "d-1" }) }) as never,
  );
  mocked.updateDirective.mockReturnValue(
    fakeCommand({ status: "ok", data: makeDirective({ id: "d-1" }) }) as never,
  );
  mocked.deleteDirective.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.moveDirective.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
});

describe("指令 hook", () => {
  it("刷新回写指令表", async () => {
    const rows = [makeDirective({ id: "d-1" })];
    mocked.listDirectives.mockReturnValue(fakeCommand({ status: "ok", data: rows }) as never);

    expect(await refreshDirectives()).toBe(true);
    expect(directiveState.value).toEqual(rows);
  });

  it("新建透传全部字段", async () => {
    const actions = [press("Control"), press("C"), release("C"), release("Control")];

    expect(
      await createDirective({
        name: "复制",
        triggerText: "复制",
        enabled: true,
        intervalMs: 50,
        tolerance: 1,
        actions,
      }),
    ).toBe(true);
    expect(mocked.createDirective).toHaveBeenCalledWith("复制", "复制", null, true, 50, 1, actions);
    expect(mocked.listDirectives).toHaveBeenCalled();
  });

  it("新建失败时返回假且不刷新", async () => {
    mocked.createDirective.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    expect(
      await createDirective({
        name: "复制",
        triggerText: "复制",
        enabled: true,
        intervalMs: 50,
        tolerance: 0,
        actions: [press("C"), release("C")],
      }),
    ).toBe(false);
    expect(mocked.listDirectives).not.toHaveBeenCalled();
  });

  it("更新透传补丁", async () => {
    expect(await updateDirective("d-1", { enabled: false })).toBe(true);
    expect(mocked.updateDirective).toHaveBeenCalledWith(
      "d-1",
      null,
      null,
      null,
      false,
      null,
      null,
      null,
    );
  });

  it("删除与移动透传 id", async () => {
    expect(await deleteDirective("d-1")).toBe(true);
    expect(mocked.deleteDirective).toHaveBeenCalledWith("d-1");
    expect(await moveDirective("d-1", "up")).toBe(true);
    expect(mocked.moveDirective).toHaveBeenCalledWith("d-1", "up");
    expect(await setDirectiveEnabled("d-1", false)).toBe(true);
    expect(mocked.updateDirective).toHaveBeenCalledWith(
      "d-1",
      null,
      null,
      null,
      false,
      null,
      null,
      null,
    );
  });
});

describe("指令纯函数", () => {
  it("触发键 trim 后比对", () => {
    expect(directiveTriggerKey("  打开记事本  ")).toBe("打开记事本");
  });

  it("触发判重排除自身且忽略空白", () => {
    const rows = [makeDirective({ id: "d-1", trigger_text: "打开记事本" })];
    expect(isTriggerDuplicate(rows, "打开记事本")).toBe(true);
    expect(isTriggerDuplicate(rows, "  打开记事本  ")).toBe(true);
    expect(isTriggerDuplicate(rows, "打开，记事本。")).toBe(true);
    expect(isTriggerDuplicate(rows, "打开记事本", "d-1")).toBe(false);
    expect(isTriggerDuplicate(rows, "关闭窗口", "d-1")).toBe(false);
    expect(isTriggerDuplicate(rows, "   ")).toBe(false);
    expect(isTriggerDuplicate(rows, "，。！")).toBe(false);
  });

  it("平衡序列通过", () => {
    expect(
      areActionsBalanced([press("Control"), press("C"), release("C"), release("Control")]),
    ).toBe(true);
  });

  it("空序列与残留按下不通过", () => {
    expect(areActionsBalanced([])).toBe(false);
    expect(areActionsBalanced([press("Control")])).toBe(false);
  });

  it("无按下先松开与重复按下不通过", () => {
    expect(areActionsBalanced([release("C")])).toBe(false);
    expect(areActionsBalanced([press("C"), press("C")])).toBe(false);
    expect(areActionsBalanced([press(" "), release(" ")])).toBe(false);
  });

  it("拼音判重归一后比对并排除自身", () => {
    const rows = [makeDirective({ id: "d-1", trigger_pinyin: "da kai ji shi ben" })];
    expect(isTriggerDuplicate(rows, "关闭窗口", undefined, "da  kai   ji shi ben")).toBe(true);
    expect(isTriggerDuplicate(rows, "关闭窗口", "d-1", "da kai ji shi ben")).toBe(false);
    expect(isTriggerDuplicate(rows, "关闭窗口", undefined, "guan bi chuang kou")).toBe(false);
    expect(isTriggerDuplicate(rows, "关闭窗口", undefined, "  ")).toBe(false);
    expect(isTriggerDuplicate(rows, "关闭窗口")).toBe(false);
  });

  it("拼音匹配仅命中启用项，文本相等亦命中", () => {
    const rows = [
      makeDirective({ id: "d-1", trigger_text: "打开记事本", trigger_pinyin: "da kai ji shi ben" }),
      makeDirective({
        id: "d-2",
        trigger_text: "关闭窗口",
        trigger_pinyin: "guan bi chuang kou",
        enabled: false,
      }),
    ];
    expect(findDirectiveByPinyin(rows, "da kai ji shi ben", "乱码")?.id).toBe("d-1");
    // 停用项即使拼音命中也不返回
    expect(findDirectiveByPinyin(rows, "guan bi chuang kou", "乱码")).toBeNull();
    // 文本精确相等兜底（多音字读音不同的场景）
    expect(findDirectiveByPinyin(rows, "zhong qing", "打开记事本")?.id).toBe("d-1");
    expect(findDirectiveByPinyin(rows, "", "")).toBeNull();
  });

  it("纠正开启时平翘舌互通，关闭时不互通", () => {
    const rows = [makeDirective({ id: "d-1", trigger_text: "知道", trigger_pinyin: "zhi dao" })];
    const correction = {
      enabled: true,
      rules: ["z_zh"],
      applyRecognition: true,
      applyDirective: true,
    };
    expect(findDirectiveByPinyin(rows, "zi dao", "乱码", correction)?.id).toBe("d-1");
    // 总开关关闭：不互通
    expect(
      findDirectiveByPinyin(rows, "zi dao", "乱码", { ...correction, enabled: false }),
    ).toBeNull();
    // 指令应用关闭：不互通
    expect(
      findDirectiveByPinyin(rows, "zi dao", "乱码", { ...correction, applyDirective: false }),
    ).toBeNull();
    // 识别应用关闭不影响指令
    expect(
      findDirectiveByPinyin(rows, "zi dao", "乱码", { ...correction, applyRecognition: false })?.id,
    ).toBe("d-1");
  });

  it("触发文本归一去掉标点符号与空白", () => {
    expect(normalizeDirectiveText("打开，记事本。")).toBe("打开记事本");
    expect(normalizeDirectiveText("打开,记事本!")).toBe("打开记事本");
    expect(normalizeDirectiveText("  打开 记事本  ")).toBe("打开记事本");
    expect(normalizeDirectiveText("，。！？")).toBe("");
  });

  it("文本去标点后相等亦命中，纯标点不命中", () => {
    const rows = [
      makeDirective({ id: "d-1", trigger_text: "打开记事本", trigger_pinyin: "da kai ji shi ben" }),
    ];
    expect(findDirectiveByPinyin(rows, "乱码拼音", "打开记事本。")?.id).toBe("d-1");
    expect(findDirectiveByPinyin(rows, "乱码拼音", "打开，记事本")?.id).toBe("d-1");
    expect(findDirectiveByPinyin(rows, "乱码拼音", "打开记事本！")?.id).toBe("d-1");
    expect(findDirectiveByPinyin(rows, "", "，。！")).toBeNull();
  });

  it("音节编辑距离按增删替计数", () => {
    expect(pinyinEditDistance(["da", "kai"], ["da", "kai"])).toBe(0);
    expect(pinyinEditDistance(["da", "kai", "ji"], ["da", "kai", "si"])).toBe(1);
    expect(pinyinEditDistance(["da", "kai"], ["da", "kai", "ji"])).toBe(1);
    expect(pinyinEditDistance(["da", "kai", "ji"], ["da", "kai"])).toBe(1);
    expect(pinyinEditDistance(["a"], ["b", "c"])).toBe(2);
    expect(pinyinEditDistance([], [])).toBe(0);
  });

  it("容错关闭时一字之差不命中，开启后命中", () => {
    const rows = [
      makeDirective({
        id: "d-1",
        trigger_text: "打开记事本",
        trigger_pinyin: "da kai ji shi ben",
        tolerance: 1,
      }),
    ];
    // 差一个音节：容错 1 命中
    expect(findDirectiveByPinyin(rows, "da kai ji si ben", "乱码")?.id).toBe("d-1");
    // 差两个音节：超出容错仍不命中
    expect(findDirectiveByPinyin(rows, "da kai ji si men", "乱码")).toBeNull();
    // 容错 0 时一字之差不命中
    const strict = [makeDirective({ id: "d-1", trigger_pinyin: "da kai ji shi ben" })];
    expect(findDirectiveByPinyin(strict, "da kai ji si ben", "乱码")).toBeNull();
  });

  it("文本精确优先于拼音模糊，模糊按距离择优", () => {
    const rows = [
      makeDirective({
        id: "d-1",
        trigger_text: "关闭窗口",
        trigger_pinyin: "guan bi chuang kou",
        tolerance: 2,
      }),
      makeDirective({
        id: "d-2",
        trigger_text: "打开记事本",
        trigger_pinyin: "da kai ji shi ben",
        tolerance: 0,
      }),
    ];
    // d-2 文本精确命中，压过 d-1 的拼音模糊
    expect(findDirectiveByPinyin(rows, "guan bi chuang kou", "打开记事本")?.id).toBe("d-2");
    // 同距离时按数组顺序首个胜出
    const tied = [
      makeDirective({ id: "d-1", trigger_pinyin: "da kai ji shi ben", tolerance: 1 }),
      makeDirective({ id: "d-2", trigger_pinyin: "da kai ji shi ben", tolerance: 1 }),
    ];
    expect(findDirectiveByPinyin(tied, "da kai ji si ben", "乱码")?.id).toBe("d-1");
  });

  it("单字触发词只走精确匹配", () => {
    const rows = [makeDirective({ id: "d-1", trigger_pinyin: "hao", tolerance: 2 })];
    expect(findDirectiveByPinyin(rows, "hao", "乱码")?.id).toBe("d-1");
    expect(findDirectiveByPinyin(rows, "gao", "乱码")).toBeNull();
  });
});
