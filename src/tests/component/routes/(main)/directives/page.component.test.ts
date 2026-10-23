import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import type { Directive } from "#libs/commands/types";
import Page from "../../../../../routes/(main)/directives/+page.svelte";

// bits-ui 组件在 jsdom 下缺失的浏览器 API，就地补齐（同词库页测试）。
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

// 纯替身：指令 hook 全部 mock，只验证指令页的组装——
// 列表、详情切换、启用、删除确认、新建弹窗的录制接线。
// 纯函数（isTriggerDuplicate / areActionsBalanced）保留实现，其余行为全部 mock。
const refreshMock = vi.hoisted(() => vi.fn());
const createMock = vi.hoisted(() => vi.fn());
const updateMock = vi.hoisted(() => vi.fn());
const deleteMock = vi.hoisted(() => vi.fn());
const moveMock = vi.hoisted(() => vi.fn());
const moveToMock = vi.hoisted(() => vi.fn());
const enableMock = vi.hoisted(() => vi.fn());
const suggestMock = vi.hoisted(() => vi.fn());
const toastMocks = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn(), info: vi.fn() }));
const directiveStateMock = vi.hoisted((): { value: Directive[] } => ({ value: [] }));

function makeDirective(partial: Partial<Directive> & { id: string }): Directive {
  return {
    name: "测试",
    trigger_text: "打开记事本",
    trigger_pinyin: "da kai ji shi ben",
    enabled: true,
    interval_ms: 50,
    actions: [
      { key: "Control", direction: "press" },
      { key: "C", direction: "press" },
      { key: "C", direction: "release" },
      { key: "Control", direction: "release" },
    ],
    ...partial,
  };
}

vi.mock("#libs/hooks/directives.svelte", async (importOriginal) => {
  const actual = await importOriginal<typeof import("#libs/hooks/directives.svelte")>();
  return {
    ...actual,
    directiveState: directiveStateMock,
    refreshDirectives: refreshMock,
    createDirective: createMock,
    updateDirective: updateMock,
    deleteDirective: deleteMock,
    moveDirective: moveMock,
    moveDirectiveTo: moveToMock,
    setDirectiveEnabled: enableMock,
    suggestDirectivePinyin: suggestMock,
  };
});

vi.mock("#libs/utils/toast", () => ({ toast: toastMocks }));

beforeEach(() => {
  vi.clearAllMocks();
  directiveStateMock.value = [
    makeDirective({ id: "d-1", name: "记事本", trigger_text: "打开记事本" }),
    makeDirective({ id: "d-2", name: "计算器", trigger_text: "打开计算器", enabled: false }),
  ];
  refreshMock.mockResolvedValue(true);
  createMock.mockResolvedValue(true);
  updateMock.mockResolvedValue(true);
  deleteMock.mockResolvedValue(true);
  moveMock.mockResolvedValue(true);
  moveToMock.mockResolvedValue(true);
  enableMock.mockResolvedValue(true);
  suggestMock.mockResolvedValue("fu zhi");
});

afterEach(() => {
  cleanup();
  document.body.removeAttribute("style");
});

/** 渲染并等到首次水合完成（加载门控消退） */
async function renderReady(): Promise<void> {
  render(Page);
  await waitFor(() => {
    expect(screen.queryByText("Loading directives…")).toBeNull();
  });
}

describe("指令页", () => {
  it("挂载拉取并渲染列表与首项详情", async () => {
    await renderReady();

    expect(refreshMock).toHaveBeenCalledOnce();
    expect(screen.getAllByText("记事本").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("计算器")).not.toBeNull();
    expect(screen.getByTestId("directive-detail")).not.toBeNull();
    // 首项自动选中：详情展示其触发文本，演示区标题与提示一并渲染
    const detail = within(screen.getByTestId("directive-detail"));
    expect(detail.getByText("打开记事本")).not.toBeNull();
    expect(detail.getByText("Effect preview")).not.toBeNull();
  });

  it("空表显示空态", async () => {
    directiveStateMock.value = [];
    await renderReady();

    expect(screen.getByText("No directives yet — create one to start")).not.toBeNull();
  });

  it("点击列表项切换详情", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByText("计算器"));

    expect(screen.getAllByText("打开计算器").length).toBeGreaterThan(0);
  });

  it("演示暂停后可手动步进且首尾循环", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "Pause preview" }));

    // 4 个事件 + 空态共 5 态：从任意位置出发，前进 5 次必回原点
    const order = ["0 of 4", "1 of 4", "2 of 4", "3 of 4", "4 of 4"];
    const stepText = (): string => screen.getByText(/\d of 4/).textContent ?? "";
    const start = stepText();
    expect(order).toContain(start);
    const forward = screen.getByRole("button", { name: "Step forward" });
    for (let i = 0; i < 5; i += 1) await user.click(forward);
    expect(stepText()).toBe(start);
    // 再进一步：走到循环下一态
    await user.click(forward);
    expect(stepText()).toBe(order[(order.indexOf(start) + 1) % order.length]);

    // 后退同样回绕：回到起点
    const back = screen.getByRole("button", { name: "Step back" });
    await user.click(back);
    expect(stepText()).toBe(start);
  });

  it("拖拽手柄排序透传目标序号", async () => {
    await renderReady();

    const grips = screen.getAllByRole("button", { name: "Drag to reorder" });
    // 手柄按下武装首行：包装器变为可拖拽
    fireEvent.mouseDown(grips[0]);
    const firstRow = grips[0].closest("div.relative") as HTMLElement;
    expect(firstRow.getAttribute("draggable")).toBe("true");

    // 拖首行到次行下半：目标序号 = 1 + 1 - 1（源在上扣除移除位移）
    fireEvent.dragStart(firstRow);
    const secondRow = grips[1].closest("div.relative") as HTMLElement;
    fireEvent.dragOver(secondRow);
    fireEvent.drop(secondRow);

    expect(moveToMock).toHaveBeenCalledWith("d-1", 1);
  });

  it("手柄键盘方向键单步移动", async () => {
    await renderReady();

    const grips = screen.getAllByRole("button", { name: "Drag to reorder" });
    fireEvent.keyDown(grips[0], { key: "ArrowDown" });

    expect(moveMock).toHaveBeenCalledWith("d-1", "down");
  });

  it("启用切换透传", async () => {
    const user = userEvent.setup();
    await renderReady();

    const toggles = screen.getAllByRole("switch", { name: "Disable" });
    await user.click(toggles[0]);

    expect(enableMock).toHaveBeenCalledWith("d-1", false);
  });

  it("删除走二次确认并传 id", async () => {
    const user = userEvent.setup();
    await renderReady();

    const deletes = screen.getAllByRole("button", { name: "Delete" });
    await user.click(deletes[0]);

    const dialog = screen.getByRole("alertdialog");
    expect(dialog).not.toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));

    expect(deleteMock).toHaveBeenCalledWith("d-1");
  });

  /** 新建弹窗走到按键步：填触发词后下一步 */
  async function gotoKeysStep(
    user: ReturnType<typeof userEvent.setup>,
  ): Promise<ReturnType<typeof within>> {
    await user.click(screen.getByRole("button", { name: "New directive" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Next" })).not.toBeNull();
    });
    const dialog = screen.getByRole("dialog");
    const dialogScope = within(dialog);
    await user.type(dialogScope.getByLabelText("Name"), "复制");
    await user.type(dialogScope.getByLabelText("Trigger text"), "复制");
    await user.click(dialogScope.getByRole("button", { name: "Next" }));
    await waitFor(() => {
      expect(dialogScope.getByText("Action sequence (0)")).not.toBeNull();
    });
    return dialogScope;
  }

  it("第一步空字段下一步拦截并内联报错", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New directive" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Next" })).not.toBeNull();
    });
    await user.click(screen.getByRole("button", { name: "Next" }));

    expect(screen.getByText("Name is required")).not.toBeNull();
    expect(screen.getByText("Trigger text is required")).not.toBeNull();
    // 未进入按键步
    expect(screen.queryByText("Action sequence (0)")).toBeNull();
  });

  it("按住模式 mousedown 按下 mouseup 松开", async () => {
    const user = userEvent.setup();
    await renderReady();
    const dialogScope = await gotoKeysStep(user);

    const ctrl = dialogScope.getAllByRole("button", { name: "Ctrl" })[0];
    expect(ctrl.getAttribute("aria-pressed")).toBe("false");
    fireEvent.mouseDown(ctrl);
    expect(ctrl.getAttribute("aria-pressed")).toBe("true");
    expect(dialogScope.getByText("1. Ctrl ↓")).not.toBeNull();
    fireEvent.mouseUp(ctrl);
    expect(ctrl.getAttribute("aria-pressed")).toBe("false");
    expect(dialogScope.getByText("2. Ctrl ↑")).not.toBeNull();
  });

  it("点按模式交替按下松开并清空复位", async () => {
    const user = userEvent.setup();
    await renderReady();
    const dialogScope = await gotoKeysStep(user);

    await user.click(dialogScope.getByRole("button", { name: "Tap to toggle" }));

    const ctrl = dialogScope.getAllByRole("button", { name: "Ctrl" })[0];
    expect(ctrl.getAttribute("aria-pressed")).toBe("false");
    await user.click(ctrl);
    expect(ctrl.getAttribute("aria-pressed")).toBe("true");
    expect(dialogScope.getByText("1. Ctrl ↓")).not.toBeNull();

    await user.click(dialogScope.getByRole("button", { name: "C" }));
    expect(dialogScope.getByText("2. C ↓")).not.toBeNull();

    await user.click(dialogScope.getByRole("button", { name: "Clear sequence" }));
    expect(dialogScope.queryByText("1. Ctrl ↓")).toBeNull();
    expect(ctrl.getAttribute("aria-pressed")).toBe("false");
  });

  it("单颗事件可独立移除", async () => {
    const user = userEvent.setup();
    await renderReady();
    const dialogScope = await gotoKeysStep(user);

    await user.click(dialogScope.getByRole("button", { name: "Tap to toggle" }));
    await user.click(dialogScope.getAllByRole("button", { name: "Ctrl" })[0]);
    await user.click(dialogScope.getByRole("button", { name: "C" }));
    expect(dialogScope.getByText("2. C ↓")).not.toBeNull();

    await user.click(dialogScope.getAllByRole("button", { name: "Remove this event" })[0]);
    expect(dialogScope.queryByText("1. Ctrl ↓")).toBeNull();
    expect(dialogScope.getByText("1. C ↓")).not.toBeNull();
  });

  it("未闭合时自动闭合按钮按序补松开", async () => {
    const user = userEvent.setup();
    await renderReady();
    const dialogScope = await gotoKeysStep(user);

    // 空序列不展示自动闭合
    expect(dialogScope.queryByRole("button", { name: "Auto release" })).toBeNull();

    // 切点按模式录 Ctrl + C：两键按住未放
    await user.click(dialogScope.getByRole("button", { name: "Tap to toggle" }));
    await user.click(dialogScope.getAllByRole("button", { name: "Ctrl" })[0]);
    await user.click(dialogScope.getByRole("button", { name: "C" }));
    expect(dialogScope.getByText("2. C ↓")).not.toBeNull();

    await user.click(dialogScope.getByRole("button", { name: "Auto release" }));
    // 按按下顺序补松开：先 Ctrl 后 C
    expect(dialogScope.getByText("3. Ctrl ↑")).not.toBeNull();
    expect(dialogScope.getByText("4. C ↑")).not.toBeNull();
    // 闭合后按钮收起
    expect(dialogScope.queryByRole("button", { name: "Auto release" })).toBeNull();
  });

  it("保存新建透传全部字段", async () => {
    const user = userEvent.setup();
    await renderReady();
    const dialogScope = await gotoKeysStep(user);

    // 点按模式录一对按放，经确认步保存
    await user.click(dialogScope.getByRole("button", { name: "Tap to toggle" }));
    const ctrls = dialogScope.getAllByRole("button", { name: "Ctrl" });
    await user.click(ctrls[0]);
    await user.click(ctrls[0]);
    await user.click(dialogScope.getByRole("button", { name: "Next" }));
    await waitFor(() => {
      expect(dialogScope.getByRole("button", { name: "Save" })).not.toBeNull();
    });
    await user.click(dialogScope.getByRole("button", { name: "Save" }));

    expect(createMock).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "复制",
        triggerText: "复制",
        actions: [
          { key: "LControl", direction: "press" },
          { key: "LControl", direction: "release" },
        ],
      }),
    );
  });

  it("触发文本重复时拦截保存", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New directive" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Next" })).not.toBeNull();
    });
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText("Name"), "重复");
    await user.type(within(dialog).getByLabelText("Trigger text"), "打开记事本");
    await user.click(within(dialog).getByRole("button", { name: "Next" }));

    // 内联判重，未进入按键步
    await waitFor(() => {
      expect(within(dialog).getByText("This trigger text already exists")).not.toBeNull();
    });
    expect(within(dialog).queryByText("Action sequence (0)")).toBeNull();
    expect(createMock).not.toHaveBeenCalled();
  });

  it("触发拼音重复时拦截保存", async () => {
    const user = userEvent.setup();
    suggestMock.mockResolvedValue("da kai ji shi ben");
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New directive" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Next" })).not.toBeNull();
    });
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText("Name"), "重复拼音");
    await user.type(within(dialog).getByLabelText("Trigger text"), "全新词");
    await user.click(within(dialog).getByRole("button", { name: "Next" }));

    await waitFor(() => {
      expect(within(dialog).getByText("This trigger text already exists")).not.toBeNull();
    });
    expect(createMock).not.toHaveBeenCalled();
  });
});
