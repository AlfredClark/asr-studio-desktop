import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import type { LexiconEntry, LexiconGroup } from "#libs/commands/types";
import Page from "../../../../../routes/(main)/lexicon/+page.svelte";

// bits-ui 组件在 jsdom 下缺失的浏览器 API，就地补齐（同模型页测试）。
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}
if (typeof ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  } as unknown as typeof ResizeObserver;
}

// 纯替身：词库 hook 全部 mock，只验证词库页的组装——
// 分组列表、生效切换、词条过滤、弹窗表单与删除确认的接线。
// 纯函数（groupLabel / entriesOfGroup）保留实现，其余行为全部 mock。
const refreshLexiconMock = vi.hoisted(() => vi.fn());
const createGroupMock = vi.hoisted(() => vi.fn());
const renameGroupMock = vi.hoisted(() => vi.fn());
const deleteGroupMock = vi.hoisted(() => vi.fn());
const setGroupsMock = vi.hoisted(() => vi.fn());
const createEntryMock = vi.hoisted(() => vi.fn());
const updateEntryMock = vi.hoisted(() => vi.fn());
const deleteEntryMock = vi.hoisted(() => vi.fn());
const batchDeleteMock = vi.hoisted(() => vi.fn());
const batchCopyMock = vi.hoisted(() => vi.fn());
const batchMoveMock = vi.hoisted(() => vi.fn());
const batchCreateMock = vi.hoisted(() => vi.fn());
const exportGroupMock = vi.hoisted(() => vi.fn());
const importGroupMock = vi.hoisted(() => vi.fn());
const batchSetEnabledMock = vi.hoisted(() => vi.fn());
const moveGroupMock = vi.hoisted(() => vi.fn());
const suggestPinyinMock = vi.hoisted(() => vi.fn());
const toastMocks = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn(), info: vi.fn() }));
const lexiconStateMock = vi.hoisted(
  (): {
    value: {
      version: number;
      groups: LexiconGroup[];
      entries: LexiconEntry[];
      active_group_ids: string[];
    };
  } => ({
    value: { version: 1, groups: [], entries: [], active_group_ids: [] },
  }),
);

const globalGroup: LexiconGroup = { id: "global", name: "", is_global: true };
const workGroup: LexiconGroup = { id: "g-work", name: "工作", is_global: false };
const dialectGroup: LexiconGroup = { id: "g-dialect", name: "方言", is_global: false };

function makeEntry(partial: Partial<LexiconEntry> & { id: string }): LexiconEntry {
  return {
    group_id: "global",
    text: "测试",
    pinyin: "ce shi",
    weight: 0,
    enabled: true,
    ...partial,
  };
}

vi.mock("#libs/hooks/lexicon.svelte", async (importOriginal) => {
  const actual = await importOriginal<typeof import("#libs/hooks/lexicon.svelte")>();
  return {
    ...actual,
    lexiconState: lexiconStateMock,
    refreshLexicon: refreshLexiconMock,
    createLexiconGroup: createGroupMock,
    renameLexiconGroup: renameGroupMock,
    deleteLexiconGroup: deleteGroupMock,
    setLexiconGroupEnabled: setGroupsMock,
    createLexiconEntry: createEntryMock,
    updateLexiconEntry: updateEntryMock,
    deleteLexiconEntry: deleteEntryMock,
    batchDeleteLexiconEntries: batchDeleteMock,
    batchCopyLexiconEntries: batchCopyMock,
    batchMoveLexiconEntries: batchMoveMock,
    batchCreateLexiconEntries: batchCreateMock,
    exportLexiconGroup: exportGroupMock,
    importLexiconGroup: importGroupMock,
    batchSetLexiconEntriesEnabled: batchSetEnabledMock,
    moveLexiconGroup: moveGroupMock,
    suggestLexiconPinyin: suggestPinyinMock,
  };
});

vi.mock("#libs/utils/toast", () => ({ toast: toastMocks }));

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  lexiconStateMock.value = {
    version: 1,
    groups: [globalGroup, workGroup, dialectGroup],
    entries: [
      makeEntry({ id: "w-g1", text: "权力", pinyin: "quan li", weight: 5 }),
      makeEntry({ id: "w-w1", group_id: "g-work", text: "在", pinyin: "zai", weight: 0 }),
      makeEntry({ id: "w-w2", group_id: "g-work", text: "成长", pinyin: "cheng zhang", weight: 0 }),
      makeEntry({ id: "w-d1", group_id: "g-dialect", text: "覅", pinyin: "fiao", weight: 0 }),
    ],
    active_group_ids: ["g-work"],
  };
  refreshLexiconMock.mockResolvedValue(true);
  createGroupMock.mockResolvedValue(true);
  renameGroupMock.mockResolvedValue(true);
  deleteGroupMock.mockResolvedValue(true);
  setGroupsMock.mockResolvedValue(true);
  createEntryMock.mockResolvedValue(true);
  updateEntryMock.mockResolvedValue(true);
  deleteEntryMock.mockResolvedValue(true);
  batchDeleteMock.mockResolvedValue(true);
  batchCopyMock.mockResolvedValue({ copied: 1, skipped: 0, ok: true });
  batchMoveMock.mockResolvedValue({ copied: 1, skipped: 0, ok: true });
  batchCreateMock.mockResolvedValue({ created: 1, skipped: 1, ok: true });
  exportGroupMock.mockResolvedValue(true);
  importGroupMock.mockResolvedValue({ id: "g-new", name: "导入", created: 2, skipped: 0 });
  batchSetEnabledMock.mockResolvedValue(true);
  moveGroupMock.mockResolvedValue(true);
  suggestPinyinMock.mockResolvedValue("quan li");
});

afterEach(() => {
  cleanup();
  document.body.removeAttribute("style");
});

/** 渲染并等到首次水合完成（加载门控消退） */
async function renderReady(): Promise<void> {
  render(Page);
  await waitFor(() => {
    expect(refreshLexiconMock).toHaveBeenCalled();
    expect(screen.queryByText("Loading lexicon…")).toBeNull();
  });
}

describe("词库页", () => {
  it("挂载即刷新，渲染分组与生效组词条", async () => {
    await renderReady();

    await waitFor(() => {
      expect(refreshLexiconMock).toHaveBeenCalledOnce();
    });
    // 全局组按语言渲染 + 常驻“始终启用”，工作组（生效）显示“已启用”徽标
    expect(screen.getByText("Global lexicon")).not.toBeNull();
    expect(screen.getByText("Always on")).not.toBeNull();
    expect(screen.getByText("工作")).not.toBeNull();
    expect(screen.getByText("Enabled")).not.toBeNull();
    // 全局组无启用开关；未生效的方言组有“启用”按钮
    const globalRow = screen.getByText("Global lexicon").closest("div.rounded-lg") as HTMLElement;
    expect(within(globalRow).queryByRole("button", { name: "Enable" })).toBeNull();
    expect(within(globalRow).queryByRole("button", { name: "Disable" })).toBeNull();
    // 右侧只列所选分组（= 生效的工作组）的词条，全局词条在全局分组下查看
    expect(screen.getByText("在")).not.toBeNull();
    expect(screen.getByText("成长")).not.toBeNull();
    expect(screen.queryByText("覅")).toBeNull();
    expect(screen.queryByText("权力")).toBeNull();
  });

  it("选中全局组展示全局词条", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("Global lexicon").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: /Global lexicon/ }));

    expect(screen.getByText("权力")).not.toBeNull();
    expect(screen.queryByText("成长")).toBeNull();
  });

  it("空词条显示空态提示", async () => {
    lexiconStateMock.value = {
      version: 1,
      groups: [globalGroup],
      entries: [],
      active_group_ids: [],
    };
    await renderReady();

    expect(screen.getByText(/No entries yet/)).not.toBeNull();
  });

  it("新建分组经命令落盘", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.type(screen.getByPlaceholderText("Group name"), "新分组");
    await user.click(screen.getByRole("button", { name: "New group" }));

    expect(createGroupMock).toHaveBeenCalledWith("新分组");
  });

  it("启用分组追加到生效集", async () => {
    const user = userEvent.setup();
    await renderReady();

    // 方言组行内的“启用”开关
    const row = screen.getByText("方言").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Enable" }));

    expect(setGroupsMock).toHaveBeenCalledWith("g-dialect", true);
  });

  it("禁用分组从生效集移除", async () => {
    const user = userEvent.setup();
    await renderReady();

    // 工作组行内的“禁用”开关
    const row = screen.getByText("工作").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Disable" }));

    expect(setGroupsMock).toHaveBeenCalledWith("g-work", false);
  });

  it("切换分组过滤右侧词条", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("方言").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: /方言/ }));

    expect(screen.getByText("覅")).not.toBeNull();
    expect(screen.queryByText("成长")).toBeNull();
  });

  it("新建词条弹窗保存调命令", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New entry" }));
    await user.type(screen.getByLabelText(/Word/), "权利");
    await user.type(screen.getByLabelText(/Pinyin/), "quan li");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(createEntryMock).toHaveBeenCalledWith(
      expect.objectContaining({ text: "权利", pinyin: "quan li" }),
    );
  });

  it("词条开关切换调更新命令", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("成长").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("switch"));

    expect(updateEntryMock).toHaveBeenCalledWith("w-w2", { enabled: false });
  });

  it("删除词条走二次确认", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("成长").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Delete" }));
    const dialog = screen.getByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));

    expect(deleteEntryMock).toHaveBeenCalledWith("w-w2");
  });

  it("删除分组走二次确认", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("工作").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "More actions" }));
    await waitFor(() => {
      expect(screen.getByText("Delete group")).not.toBeNull();
    });
    await user.click(screen.getByText("Delete group"));
    const dialog = screen.getByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));

    expect(deleteGroupMock).toHaveBeenCalledWith("g-work");
  });

  it("上移分组调命令", async () => {
    const user = userEvent.setup();
    await renderReady();

    // 方言组在末位，可上移
    const row = screen.getByText("方言").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "More actions" }));
    await waitFor(() => {
      expect(screen.getByText("Move group up")).not.toBeNull();
    });
    await user.click(screen.getByText("Move group up"));

    expect(moveGroupMock).toHaveBeenCalledWith("g-dialect", "up");
  });

  // bits-ui 菜单项用 aria-disabled 表达禁用（div 无 disabled 属性）
  const menuItemDisabled = (name: string): boolean | undefined =>
    screen.getByText(name).closest("[role=menuitem]")?.getAttribute("aria-disabled") === "true";

  it("紧随全局组的分组上移禁用", async () => {
    const user = userEvent.setup();
    await renderReady();

    const workRow = screen.getByText("工作").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(workRow).getByRole("button", { name: "More actions" }));
    await waitFor(() => {
      expect(screen.getByText("Move group up")).not.toBeNull();
    });
    expect(menuItemDisabled("Move group up")).toBe(true);
    expect(menuItemDisabled("Move group down")).toBe(false);
  });

  it("末位分组下移禁用", async () => {
    const user = userEvent.setup();
    await renderReady();

    const dialectRow = screen.getByText("方言").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(dialectRow).getByRole("button", { name: "More actions" }));
    await waitFor(() => {
      expect(screen.getByText("Move group down")).not.toBeNull();
    });
    expect(menuItemDisabled("Move group down")).toBe(true);
    expect(menuItemDisabled("Move group up")).toBe(false);
  });

  it("首屏先展示加载态", () => {
    render(Page);

    expect(screen.getByText("Loading lexicon…")).not.toBeNull();
  });

  it("加载失败展示错误与重试", async () => {
    const user = userEvent.setup();
    refreshLexiconMock.mockResolvedValueOnce(false);
    render(Page);

    await waitFor(() => {
      expect(screen.getByText("Failed to load lexicon")).not.toBeNull();
    });
    await user.click(screen.getByRole("button", { name: "Retry" }));

    expect(refreshLexiconMock).toHaveBeenCalledTimes(2);
  });

  it("回车提交新建分组", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.type(screen.getByPlaceholderText("Group name"), "新分组");
    await user.keyboard("{Enter}");

    expect(createGroupMock).toHaveBeenCalledWith("新分组");
  });

  it("选中分组行带当前位置标记", async () => {
    await renderReady();

    const active = screen.getByRole("button", { name: /工作/ });
    expect(active.getAttribute("aria-current")).toBe("true");
    const inactive = screen.getByRole("button", { name: /方言/ });
    expect(inactive.hasAttribute("aria-current")).toBe(false);
  });

  it("新建时输入词面自动填充拼音，手改后不再覆盖", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New entry" }));
    await user.type(screen.getByLabelText(/Word/), "权力");
    const pinyinValue = (): string => (screen.getByLabelText(/Pinyin/) as HTMLInputElement).value;
    // 防抖后自动回填
    await waitFor(
      () => {
        expect(pinyinValue()).toBe("quan li");
      },
      { timeout: 3000 },
    );
    expect(suggestPinyinMock).toHaveBeenCalledWith("权力");

    // 手改拼音后锁定：再改词面不触发新的 suggest，且值保持不动
    const calls = suggestPinyinMock.mock.calls.length;
    await user.clear(screen.getByLabelText(/Pinyin/));
    await user.type(screen.getByLabelText(/Pinyin/), "ql");
    await user.type(screen.getByLabelText(/Word/), "好");
    await new Promise((resolve) => setTimeout(resolve, 600));
    expect(suggestPinyinMock.mock.calls.length).toBe(calls);
    expect(pinyinValue()).toBe("ql");
  });

  it("空拼音保存时自动填充建议", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New entry" }));
    await user.type(screen.getByLabelText(/Word/), "权力");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(suggestPinyinMock).toHaveBeenCalledWith("权力");
    expect(createEntryMock).toHaveBeenCalledWith(
      expect.objectContaining({ text: "权力", pinyin: "quan li" }),
    );
  });

  it("无选中时不展示批量操作入口", async () => {
    await renderReady();

    expect(screen.queryByRole("button", { name: /Batch actions/ })).toBeNull();
  });

  it("勾选后批量启用调命令", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("成长").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("checkbox", { name: "成长" }));

    // 批量下拉出现并可操作
    const trigger = screen.getByRole("button", { name: /Batch actions/ });
    expect(trigger).not.toBeNull();
    await user.click(trigger);
    await waitFor(() => {
      expect(screen.getByText("Enable selected")).not.toBeNull();
    });
    await user.click(screen.getByText("Enable selected"));

    expect(batchSetEnabledMock).toHaveBeenCalledWith(["w-w2"], true);
  });

  it("勾选后批量禁用调命令", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("成长").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("checkbox", { name: "成长" }));
    await user.click(screen.getByRole("button", { name: /Batch actions/ }));
    await waitFor(() => {
      expect(screen.getByText("Disable selected")).not.toBeNull();
    });
    await user.click(screen.getByText("Disable selected"));

    expect(batchSetEnabledMock).toHaveBeenCalledWith(["w-w2"], false);
  });

  it("批量删除走二次确认并传选中 id", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("成长").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("checkbox", { name: "成长" }));
    await user.click(screen.getByRole("button", { name: /Batch actions/ }));
    await waitFor(() => {
      expect(screen.getByText("Delete selected")).not.toBeNull();
    });
    await user.click(screen.getByText("Delete selected"));

    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByText("Delete selected entries?")).not.toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));

    expect(batchDeleteMock).toHaveBeenCalledWith(["w-w2"]);
  });

  it("批量复制经二级菜单选目标分组并保持选中", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("成长").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("checkbox", { name: "成长" }));
    await user.click(screen.getByRole("button", { name: /Batch actions/ }));
    await waitFor(() => {
      expect(screen.getByText("Copy selected to")).not.toBeNull();
    });
    await user.hover(screen.getByText("Copy selected to"));
    await waitFor(() => {
      expect(screen.getByText("方言 (1)")).not.toBeNull();
    });
    // 当前分组不在目标列表中，全局组在
    expect(screen.queryByText("工作 (2)")).toBeNull();
    expect(screen.getByText("Global lexicon (1)")).not.toBeNull();
    await user.click(screen.getByText("方言 (1)"));

    expect(batchCopyMock).toHaveBeenCalledWith(["w-w2"], "g-dialect");
    // 复制保留选中：批量入口仍在
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Batch actions \(1\)/ })).not.toBeNull();
    });
  });

  it("批量转移经二级菜单选目标分组并清空选中", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("成长").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("checkbox", { name: "成长" }));
    await user.click(screen.getByRole("button", { name: /Batch actions/ }));
    await waitFor(() => {
      expect(screen.getByText("Move selected to")).not.toBeNull();
    });
    await user.hover(screen.getByText("Move selected to"));
    await waitFor(() => {
      expect(screen.getByText("方言 (1)")).not.toBeNull();
    });
    await user.click(screen.getByText("方言 (1)"));

    expect(batchMoveMock).toHaveBeenCalledWith(["w-w2"], "g-dialect");
    // 转移清空选中：批量入口消失
    await waitFor(() => {
      expect(screen.queryByRole("button", { name: /Batch actions/ })).toBeNull();
    });
  });

  it("展示排序筛选与选择菜单入口", async () => {
    await renderReady();

    expect(screen.getByRole("combobox", { name: "Sort" })).not.toBeNull();
    expect(screen.getByRole("combobox", { name: "Filter" })).not.toBeNull();
    expect(screen.getByRole("button", { name: "Selection" })).not.toBeNull();
  });

  it("筛选禁用词条并在无匹配时提示", async () => {
    const user = userEvent.setup();
    lexiconStateMock.value = {
      version: 1,
      groups: [globalGroup, workGroup, dialectGroup],
      entries: [
        makeEntry({ id: "w-w1", group_id: "g-work", text: "在", pinyin: "zai", enabled: true }),
        makeEntry({
          id: "w-w2",
          group_id: "g-work",
          text: "成长",
          pinyin: "cheng zhang",
          enabled: false,
        }),
      ],
      active_group_ids: ["g-work"],
    };
    await renderReady();

    await user.click(screen.getByRole("combobox", { name: "Filter" }));
    await waitFor(() => {
      expect(document.querySelector('[data-value="disabled"]')).not.toBeNull();
    });
    await user.click(document.querySelector('[data-value="disabled"]') as HTMLElement);

    await waitFor(() => {
      expect(screen.queryByText("在")).toBeNull();
    });
    expect(screen.getByText("成长")).not.toBeNull();
    expect(localStorage.getItem("app.lexicon-filter")).toBe("disabled");
  });

  it("无匹配筛选显示空态提示", async () => {
    const user = userEvent.setup();
    await renderReady();

    // 工作组两条均为启用，切到“未启用”即无匹配
    await user.click(screen.getByRole("combobox", { name: "Filter" }));
    await waitFor(() => {
      expect(document.querySelector('[data-value="disabled"]')).not.toBeNull();
    });
    await user.click(document.querySelector('[data-value="disabled"]') as HTMLElement);

    await waitFor(() => {
      expect(screen.getByText("No entries match the filter")).not.toBeNull();
    });
  });

  it("排序切换改顺序并持久化", async () => {
    const user = userEvent.setup();
    lexiconStateMock.value = {
      version: 1,
      groups: [globalGroup, workGroup, dialectGroup],
      entries: [
        makeEntry({ id: "w-w1", group_id: "g-work", text: "在", pinyin: "zai", weight: 0 }),
        makeEntry({
          id: "w-w2",
          group_id: "g-work",
          text: "成长",
          pinyin: "cheng zhang",
          weight: 0,
        }),
      ],
      active_group_ids: ["g-work"],
    };
    await renderReady();

    await user.click(screen.getByRole("combobox", { name: "Sort" }));
    await waitFor(() => {
      expect(document.querySelector('[data-value="pinyin-asc"]')).not.toBeNull();
    });
    await user.click(document.querySelector('[data-value="pinyin-asc"]') as HTMLElement);

    await waitFor(() => {
      const rows = screen.getAllByRole("checkbox").map((box) => box.getAttribute("aria-label"));
      expect(rows).toEqual(["成长", "在"]);
    });
    expect(localStorage.getItem("app.lexicon-sort")).toBe("pinyin-asc");
  });

  it("选择菜单选中全部可见词条", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "Selection" }));
    await waitFor(() => {
      expect(screen.getByText("Select all")).not.toBeNull();
    });
    await user.click(screen.getByText("Select all"));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Batch actions \(2\)/ })).not.toBeNull();
    });
  });

  it("选择菜单清空全组词条走二次确认", async () => {
    const user = userEvent.setup();
    await renderReady();

    // 无需事先选中：直接打开选择菜单清空整组
    await user.click(screen.getByRole("button", { name: "Selection" }));
    await waitFor(() => {
      expect(screen.getByText("Clear all entries")).not.toBeNull();
    });
    await user.click(screen.getByText("Clear all entries"));

    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("Clear all entries?")).not.toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));

    expect(batchDeleteMock).toHaveBeenCalledWith(["w-w1", "w-w2"]);
  });

  it("选择菜单打开批量添加弹窗", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "Selection" }));
    await waitFor(() => {
      expect(screen.getByText("Add entries in bulk")).not.toBeNull();
    });
    await user.click(screen.getByText("Add entries in bulk"));

    await waitFor(() => {
      expect(screen.getByText(/Separate entries with spaces/)).not.toBeNull();
    });
    expect(screen.getByLabelText("Entry list")).not.toBeNull();
  });

  it("批量添加解析去重并按词音对落盘", async () => {
    const user = userEvent.setup();
    suggestPinyinMock.mockImplementation(async (text: string) =>
      text === "成长" ? "cheng zhang" : "ce shi",
    );
    await renderReady();

    await user.click(screen.getByRole("button", { name: "Selection" }));
    await waitFor(() => {
      expect(screen.getByText("Add entries in bulk")).not.toBeNull();
    });
    await user.click(screen.getByText("Add entries in bulk"));

    // 成长已在工作组存在（同词同音跳过），测试为新词
    await user.type(screen.getByLabelText("Entry list"), "成长 测试");
    await waitFor(
      () => {
        expect(screen.getByText("1 to add, 1 duplicates skipped")).not.toBeNull();
      },
      { timeout: 3000 },
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(batchCreateMock).toHaveBeenCalledWith("g-work", [
      { text: "成长", pinyin: "cheng zhang" },
      { text: "测试", pinyin: "ce shi" },
    ]);
  });

  it("单条新建重复词音对不落盘且弹窗保持打开", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New entry" }));
    await user.type(screen.getByLabelText(/Word/), "在");
    // 等自动拼音回填后手工改成存量读音，构造词音双命中
    await waitFor(
      () => {
        expect((screen.getByLabelText(/Pinyin/) as HTMLInputElement).value).toBe("quan li");
      },
      { timeout: 3000 },
    );
    await user.clear(screen.getByLabelText(/Pinyin/));
    await user.type(screen.getByLabelText(/Pinyin/), "zai");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(createEntryMock).not.toHaveBeenCalled();
    expect(toastMocks.info).toHaveBeenCalled();
    // 弹窗保持打开：保存按钮仍在
    expect(screen.getByRole("button", { name: "Save" })).not.toBeNull();
  });

  it("分组更多操作含导出分组项", async () => {
    const user = userEvent.setup();
    await renderReady();

    const row = screen.getByText("工作").closest("div.rounded-lg") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "More actions" }));
    await waitFor(() => {
      expect(screen.getByText("Export group")).not.toBeNull();
    });
    await user.click(screen.getByText("Export group"));

    expect(exportGroupMock).toHaveBeenCalledWith("g-work");
  });

  it("底部导入按钮调导入并切组", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "Import group" }));

    expect(importGroupMock).toHaveBeenCalledOnce();
  });
});
