import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import type { ServiceMode, ServiceProvider } from "#libs/commands/types";
import Component from "../../../../../../routes/(main)/settings/_components/service-settings.svelte";

// jsdom 缺少指针捕获与滚动 API，bits-ui 下拉用得到，仅在本文件内就地补齐。
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

// 纯替身：分组依赖的外部状态（后端配置、提示、文件选择器）全部 mock，
// 只验证分组的接线逻辑——选项渲染、显隐、切换调用与成功/失败分支。
const updateConfigMock = vi.hoisted(() => vi.fn());
const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}));
const openDialogMock = vi.hoisted(() => vi.fn());
const configStateMock = vi.hoisted(() => ({
  value: {
    service_mode: "internal",
    service_server_path: "",
    service_port: 6006,
    service_threads: 0,
    service_provider: "cpu",
  },
}));
const configValueMock = vi.hoisted(() => (key: string, fallback: unknown) => {
  const state = configStateMock.value as unknown as Record<string, unknown> | null;
  return state?.[key] ?? fallback;
});

vi.mock("#libs/hooks/config.svelte", () => ({
  configState: configStateMock,
  configValue: configValueMock,
  updateConfig: updateConfigMock,
}));

vi.mock("#libs/utils/toast", () => ({ toast: toastMocks }));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: openDialogMock,
}));

afterEach(() => {
  cleanup();
  // bits-ui 下拉打开时给 body 加滚动锁定样式，jsdom 内卸载后不会自动还原，
  // 这里手动清理，避免泄漏到后续用例导致点击被 pointer-events 拦截。
  document.body.removeAttribute("style");
});

beforeEach(() => {
  vi.clearAllMocks();
  configStateMock.value = {
    service_mode: "internal",
    service_server_path: "",
    service_port: 6006,
    service_threads: 0,
    service_provider: "cpu",
  };
  // 成功路径：补丁合并回配置状态并返回真，与真实 `updateConfig` 的回写行为一致。
  updateConfigMock.mockImplementation(
    async (patch: {
      service_mode?: ServiceMode;
      service_server_path?: string;
      service_port?: number;
      service_threads?: number;
      service_provider?: ServiceProvider;
    }) => {
      configStateMock.value = { ...configStateMock.value, ...patch };
      return true;
    },
  );
});

describe("服务设置分组", () => {
  /** 在下拉中按值点选：jsdom 无布局，选项始终不可见，只能按 `data-value` 定位。 */
  async function chooseOption(
    user: ReturnType<typeof userEvent.setup>,
    triggerName: string,
    value: string,
  ): Promise<void> {
    await user.click(screen.getByRole("combobox", { name: triggerName }));
    await waitFor(() => {
      expect(document.querySelector(`[data-value="${value}"]`)).not.toBeNull();
    });
    await user.click(document.querySelector(`[data-value="${value}"]`) as HTMLElement);
  }

  it("内部模式只渲染模式选择，其余四项隐藏", () => {
    render(Component);

    expect(screen.getByText("Service")).not.toBeNull();
    expect(screen.getByRole("combobox", { name: "Service mode" })).not.toBeNull();
    expect(screen.queryByText("Server binary path")).toBeNull();
    expect(screen.queryByText("Port")).toBeNull();
    expect(screen.queryByText("Worker threads")).toBeNull();
    expect(screen.queryByText("Inference backend")).toBeNull();
  });

  it("本机模式展示四项外部设置", () => {
    configStateMock.value.service_mode = "external";
    render(Component);

    expect(screen.getByText("Server binary path")).not.toBeNull();
    expect(screen.getByText("Port")).not.toBeNull();
    expect(screen.getByText("Worker threads")).not.toBeNull();
    expect(screen.getByText("Inference backend")).not.toBeNull();
  });

  it("切换服务模式落盘并提示成功", async () => {
    const user = userEvent.setup();
    render(Component);

    await chooseOption(user, "Service mode", "external");

    expect(updateConfigMock).toHaveBeenCalledWith({ service_mode: "external" });
    expect(toastMocks.success).toHaveBeenCalledWith("Service settings updated");
  });

  it("切换失败提示失败", async () => {
    updateConfigMock.mockResolvedValue(false);
    const user = userEvent.setup();
    render(Component);

    await chooseOption(user, "Service mode", "external");

    expect(toastMocks.error).toHaveBeenCalledWith("Failed to update service settings");
  });

  it("端口非法输入提示且不落盘", async () => {
    configStateMock.value.service_mode = "external";
    const user = userEvent.setup();
    render(Component);

    const portInput = screen.getByLabelText("Port");
    await user.clear(portInput);
    await user.type(portInput, "99999");
    await user.tab();

    expect(toastMocks.error).toHaveBeenCalledWith("Port must be between 1 and 65535");
    expect(updateConfigMock).not.toHaveBeenCalled();
  });

  it("端口合法输入失焦落盘", async () => {
    configStateMock.value.service_mode = "external";
    const user = userEvent.setup();
    render(Component);

    const portInput = screen.getByLabelText("Port");
    await user.clear(portInput);
    await user.type(portInput, "7006");
    await user.tab();

    await waitFor(() => {
      expect(updateConfigMock).toHaveBeenCalledWith({ service_port: 7006 });
    });
    expect(toastMocks.success).toHaveBeenCalledWith("Service settings updated");
  });

  it("线程数非法输入提示且不落盘", async () => {
    configStateMock.value.service_mode = "external";
    const user = userEvent.setup();
    render(Component);

    const threadsInput = screen.getByLabelText("Worker threads");
    await user.clear(threadsInput);
    await user.type(threadsInput, "-1");
    await user.tab();

    expect(toastMocks.error).toHaveBeenCalledWith("Threads must be a non-negative integer");
    expect(updateConfigMock).not.toHaveBeenCalled();
  });

  it("浏览按钮经文件选择器落盘路径", async () => {
    configStateMock.value.service_mode = "external";
    openDialogMock.mockResolvedValue("/opt/sherpa-onnx/bin/server");
    const user = userEvent.setup();
    render(Component);

    await user.click(screen.getByRole("button", { name: "Browse…" }));

    await waitFor(() => {
      expect(openDialogMock).toHaveBeenCalledWith({ directory: false, multiple: false });
      expect(updateConfigMock).toHaveBeenCalledWith({
        service_server_path: "/opt/sherpa-onnx/bin/server",
      });
    });
  });

  it("取消文件选择静默返回", async () => {
    configStateMock.value.service_mode = "external";
    openDialogMock.mockResolvedValue(null);
    const user = userEvent.setup();
    render(Component);

    await user.click(screen.getByRole("button", { name: "Browse…" }));

    await waitFor(() => {
      expect(openDialogMock).toHaveBeenCalled();
    });
    expect(updateConfigMock).not.toHaveBeenCalled();
  });

  it("切换推理后端落盘", async () => {
    configStateMock.value.service_mode = "external";
    const user = userEvent.setup();
    render(Component);

    await chooseOption(user, "Inference backend", "cuda");

    expect(updateConfigMock).toHaveBeenCalledWith({ service_provider: "cuda" });
    expect(toastMocks.success).toHaveBeenCalledWith("Service settings updated");
  });
});
