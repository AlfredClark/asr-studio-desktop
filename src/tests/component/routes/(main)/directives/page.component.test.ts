import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/svelte";
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

  it("启用切换透传", async () => {
    const user = userEvent.setup();
    await renderReady();

    const toggles = screen.getAllByRole("button", { name: "Disable" });
    await user.click(toggles[0]);

    expect(enableMock).toHaveBeenCalledWith("d-1", false);
  });

  it("删除走二次确认并传 id", async () => {
    const user = userEvent.setup();
    await renderReady();

    const menus = screen.getAllByRole("button", { name: "More actions" });
    await user.click(menus[0]);
    await waitFor(() => {
      expect(screen.getByText("Delete")).not.toBeNull();
    });
    await user.click(screen.getByText("Delete"));

    const dialog = screen.getByRole("alertdialog");
    expect(dialog).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "Delete" }));

    expect(deleteMock).toHaveBeenCalledWith("d-1");
  });

  it("新建弹窗录制按键并清空复位", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New directive" }));
    await waitFor(() => {
      expect(screen.getByText("Action sequence (0)")).not.toBeNull();
    });
    const dialog = screen.getByRole("dialog");
    const dialogScope = within(dialog);

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

  it("保存新建透传全部字段", async () => {
    const user = userEvent.setup();
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New directive" }));
    await waitFor(() => {
      expect(screen.getByText("Action sequence (0)")).not.toBeNull();
    });
    await user.type(screen.getByLabelText("Name"), "复制");
    await user.type(screen.getByLabelText("Trigger text"), "复制");
    const ctrls = screen.getAllByRole("button", { name: "Ctrl" });
    await user.click(ctrls[0]);
    await user.click(ctrls[0]);
    await user.click(screen.getByRole("button", { name: "Save" }));

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
      expect(screen.getByText("Action sequence (0)")).not.toBeNull();
    });
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText("Name"), "重复");
    await user.type(within(dialog).getByLabelText("Trigger text"), "打开记事本");
    const ctrls = within(dialog).getAllByRole("button", { name: "Ctrl" });
    await user.click(ctrls[0]);
    await user.click(ctrls[0]);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(createMock).not.toHaveBeenCalled();
    expect(toastMocks.info).toHaveBeenCalled();
  });

  it("触发拼音重复时拦截保存", async () => {
    const user = userEvent.setup();
    suggestMock.mockResolvedValue("da kai ji shi ben");
    await renderReady();

    await user.click(screen.getByRole("button", { name: "New directive" }));
    await waitFor(() => {
      expect(screen.getByText("Action sequence (0)")).not.toBeNull();
    });
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText("Name"), "重复拼音");
    await user.type(within(dialog).getByLabelText("Trigger text"), "全新词");
    const ctrls = within(dialog).getAllByRole("button", { name: "Ctrl" });
    await user.click(ctrls[0]);
    await user.click(ctrls[0]);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(createMock).not.toHaveBeenCalled();
    expect(toastMocks.info).toHaveBeenCalled();
  });
});
