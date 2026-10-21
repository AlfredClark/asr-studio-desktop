import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import Row from "../../../../../../components/common/shortcut-record-row.svelte";
import Group from "../../../../../../routes/(main)/settings/_components/shortcut-settings.svelte";

// 纯替身：配置状态、注册查询与提示全部 mock，只验证录制接线——
// 徽标渲染、录制开始、有效组合保存、注册失败提示、取消与清空分支；两行共用同一行组件。
const updateConfigMock = vi.hoisted(() => vi.fn());
const isShortcutRegisteredMock = vi.hoisted(() => vi.fn());
const reportFailureMock = vi.hoisted(() => vi.fn());
const serviceStateMock = vi.hoisted((): { status: { state: string } | null } => ({
  status: null,
}));
const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
}));
const configStateMock = vi.hoisted(() => ({
  value: {
    locale: "en",
    auto_start: false,
    remember_window: false,
    auto_check_update: false,
    tray_enabled: true,
    close_behavior: "prompt",
    shortcut_quick_open: "",
    shortcut_output: "clipboard",
    shortcut_paste_restore: false,
    custom_model_dirs: {},
    active_lexicon_groups: [],
    shortcut_voice: "",
    schema_version: 1,
  },
}));

vi.mock("#libs/hooks/config.svelte", () => ({
  configState: configStateMock,
  updateConfig: updateConfigMock,
}));

vi.mock("#libs/commands", () => ({
  default: { isShortcutRegistered: isShortcutRegisteredMock },
}));

vi.mock("#libs/commands/cores", () => ({
  reportCommandFailure: reportFailureMock,
}));

vi.mock("#libs/hooks/service.svelte", () => ({
  serviceState: serviceStateMock,
}));

vi.mock("#libs/utils/toast", () => ({ toast: toastMocks }));

type FakeResult =
  { status: "ok"; data: unknown } | { status: "error"; error: { kind: string; message: string } };

/** 与 `EnhancedCommand` 行为一致的替身：只实现本组件用到的 `result()` */
function fakeCommand(result: FakeResult): { result: () => Promise<FakeResult> } {
  return { result: () => Promise.resolve(result) };
}

beforeEach(() => {
  vi.clearAllMocks();
  configStateMock.value = {
    locale: "en",
    auto_start: false,
    remember_window: false,
    auto_check_update: false,
    tray_enabled: true,
    close_behavior: "prompt",
    shortcut_quick_open: "",
    shortcut_output: "clipboard",
    shortcut_paste_restore: false,
    custom_model_dirs: {},
    active_lexicon_groups: [],
    shortcut_voice: "",
    schema_version: 1,
  };
  // 成功路径：写后回填配置状态并返回真，与真实 `updateConfig` 的回写行为一致
  updateConfigMock.mockImplementation(
    async (patch: { shortcut_quick_open?: string; shortcut_voice?: string }) => {
      configStateMock.value = {
        ...configStateMock.value,
        shortcut_quick_open: patch.shortcut_quick_open ?? configStateMock.value.shortcut_quick_open,
        shortcut_voice: patch.shortcut_voice ?? configStateMock.value.shortcut_voice,
      };
      return true;
    },
  );
  serviceStateMock.status = null;
  isShortcutRegisteredMock.mockReturnValue(fakeCommand({ status: "ok", data: true }));
});

afterEach(() => {
  cleanup();
});

describe("快捷键设置分组", () => {
  it("只渲染快速显示一行，语音识别已移到服务页", () => {
    render(Group);

    expect(screen.getByText("Quick open app")).not.toBeNull();
    expect(screen.queryByText("Voice recognition")).toBeNull();
  });
});

describe("快捷键录制行", () => {
  it("空态显示占位与录制按钮", () => {
    const { container } = render(Row, { props: { kind: "quick_open" } });

    expect(within(container as HTMLElement).getByText("Not set")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Record shortcut" })).not.toBeNull();
  });

  it("已存组合按分段渲染徽标并提供清除按钮", () => {
    configStateMock.value = { ...configStateMock.value, shortcut_quick_open: "Ctrl+Shift+O" };
    const { container } = render(Row, { props: { kind: "quick_open" } });
    const scope = within(container as HTMLElement);

    expect(scope.getByText("Ctrl")).not.toBeNull();
    expect(scope.getByText("Shift")).not.toBeNull();
    expect(scope.getByText("O")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Clear" })).not.toBeNull();
  });

  it("停止态保存语音键只落盘不查注册", async () => {
    const user = userEvent.setup();
    serviceStateMock.status = null;
    render(Row, { props: { kind: "voice" } });
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));
    fireEvent.keyDown(window, { key: "v", ctrlKey: true, altKey: true });

    await vi.waitFor(() => {
      expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_voice: "Ctrl+Alt+V" });
    });
    await vi.waitFor(() => {
      expect(toastMocks.success).toHaveBeenCalled();
    });
    expect(isShortcutRegisteredMock).not.toHaveBeenCalled();
  });

  it("运行态保存语音键查注册，未注册提示换组合", async () => {
    const user = userEvent.setup();
    serviceStateMock.status = { state: "running" };
    isShortcutRegisteredMock.mockReturnValue(fakeCommand({ status: "ok", data: false }));
    render(Row, { props: { kind: "voice" } });
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));
    fireEvent.keyDown(window, { key: "v", ctrlKey: true, altKey: true });

    await vi.waitFor(() => {
      expect(isShortcutRegisteredMock).toHaveBeenCalledWith("Ctrl+Alt+V");
    });
    await vi.waitFor(() => {
      expect(toastMocks.warning).toHaveBeenCalled();
    });
    expect(toastMocks.success).not.toHaveBeenCalled();
  });

  it("语音行读写独立字段", async () => {
    const user = userEvent.setup();
    const { container } = render(Row, { props: { kind: "voice" } });
    const scope = within(container as HTMLElement);

    expect(scope.getByText("Not set")).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));
    fireEvent.keyDown(window, { key: "v", ctrlKey: true, altKey: true });

    await vi.waitFor(() => {
      expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_voice: "Ctrl+Alt+V" });
    });
    expect(updateConfigMock).not.toHaveBeenCalledWith(
      expect.objectContaining({ shortcut_quick_open: expect.anything() }),
    );
  });

  it("有效组合落盘并注册成功后提示更新", async () => {
    const user = userEvent.setup();
    render(Row, { props: { kind: "quick_open" } });
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));

    fireEvent.keyDown(window, { key: "o", ctrlKey: true, shiftKey: true });

    await vi.waitFor(() => {
      expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_quick_open: "Ctrl+Shift+O" });
    });
    await vi.waitFor(() => {
      expect(toastMocks.success).toHaveBeenCalled();
    });
    expect(toastMocks.warning).not.toHaveBeenCalled();
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("注册未生效时保留新值并提示换组合", async () => {
    const user = userEvent.setup();
    isShortcutRegisteredMock.mockReturnValue(fakeCommand({ status: "ok", data: false }));
    render(Row, { props: { kind: "quick_open" } });
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));

    fireEvent.keyDown(window, { key: "o", ctrlKey: true, shiftKey: true });

    await vi.waitFor(() => {
      expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_quick_open: "Ctrl+Shift+O" });
    });
    await vi.waitFor(() => {
      expect(toastMocks.warning).toHaveBeenCalled();
    });
    expect(toastMocks.success).not.toHaveBeenCalled();
  });

  it("落盘失败时报错且不查注册状态", async () => {
    const user = userEvent.setup();
    updateConfigMock.mockImplementation(async () => false);
    render(Row, { props: { kind: "quick_open" } });
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));

    fireEvent.keyDown(window, { key: "o", ctrlKey: true });

    await vi.waitFor(() => {
      expect(toastMocks.error).toHaveBeenCalled();
    });
    expect(isShortcutRegisteredMock).not.toHaveBeenCalled();
  });

  it("纯单键不保存，继续等待组合", async () => {
    const user = userEvent.setup();
    render(Row, { props: { kind: "quick_open" } });
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));

    fireEvent.keyDown(window, { key: "o" });

    expect(updateConfigMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Quick open app" }).textContent).toContain(
      "Press shortcut",
    );
  });

  it("按 Esc 取消录制且不保存", async () => {
    const user = userEvent.setup();
    render(Row, { props: { kind: "quick_open" } });
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(screen.getByText("Not set")).not.toBeNull();
    });
    expect(updateConfigMock).not.toHaveBeenCalled();
  });

  it("按 Backspace 清空已存组合", async () => {
    const user = userEvent.setup();
    configStateMock.value = { ...configStateMock.value, shortcut_quick_open: "Ctrl+Shift+O" };
    render(Row, { props: { kind: "quick_open" } });
    await user.click(screen.getByRole("button", { name: "Record shortcut" }));

    fireEvent.keyDown(window, { key: "Backspace" });

    await vi.waitFor(() => {
      expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_quick_open: "" });
    });
    await vi.waitFor(() => {
      expect(toastMocks.success).toHaveBeenCalled();
    });
  });

  it("点击清除按钮清空已存组合", async () => {
    const user = userEvent.setup();
    configStateMock.value = { ...configStateMock.value, shortcut_quick_open: "Ctrl+Shift+O" };
    render(Row, { props: { kind: "quick_open" } });

    await user.click(screen.getByRole("button", { name: "Clear" }));

    await vi.waitFor(() => {
      expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_quick_open: "" });
    });
    await vi.waitFor(() => {
      expect(toastMocks.success).toHaveBeenCalled();
    });
  });
});
