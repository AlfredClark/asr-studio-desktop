import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import type { AudioDevice, ModelEntry } from "#libs/commands/types";
import type { RecognitionRecord } from "#libs/hooks/service.svelte";
import Page from "../../../../../routes/(main)/service/+page.svelte";

// bits-ui 组件在 jsdom 下缺失的浏览器 API，就地补齐（同设置页测试）。
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

// 纯替身：服务 hook 全部 mock，只验证服务页的组装——标题、徽标、按钮可用性与接线。
const refreshServiceDataMock = vi.hoisted(() => vi.fn());
const selectModelMock = vi.hoisted(() => vi.fn());
const selectDeviceMock = vi.hoisted(() => vi.fn());
const startServiceMock = vi.hoisted(() => vi.fn());
const stopServiceMock = vi.hoisted(() => vi.fn());
const startTestRecordingMock = vi.hoisted(() => vi.fn());
const stopTestRecordingMock = vi.hoisted(() => vi.fn());
const toastMocks = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn(), warning: vi.fn() }));
const updateConfigMock = vi.hoisted(() => vi.fn());
const configStateMock = vi.hoisted(
  (): { value: { shortcut_output: string; shortcut_paste_restore?: boolean } | null } => ({
    value: { shortcut_output: "clipboard", shortcut_paste_restore: false },
  }),
);
const serviceStateMock = vi.hoisted(
  (): {
    status: { state: string; modelId: string | null; loadMs: number | null } | null;
    models: ModelEntry[];
    devices: AudioDevice[];
    selectedModel: string | null;
    selectedDevice: string | null;
    recording: boolean;
    recognizing: boolean;
    switching: boolean;
    result: { text: string; audioSec: number; inferMs: number; fileName: string } | null;
    history: RecognitionRecord[];
  } => ({
    status: null,
    models: [],
    devices: [],
    selectedModel: null,
    selectedDevice: null,
    recording: false,
    recognizing: false,
    switching: false,
    result: null,
    history: [],
  }),
);

const modelEntry: ModelEntry = {
  id: "sense-voice-int8",
  category: "sense_voice",
  display_name: "SenseVoice int8",
  tarball_bytes: 163_002_883,
  required_bytes: 239_549_735,
  status: "ready",
  local_path: "/models",
  page_url: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models",
  custom_dir: null,
  manual: false,
};

const deviceEntry: AudioDevice = { id: "0", name: "Microphone", is_default: true };

vi.mock("#libs/hooks/service.svelte", async (importOriginal) => {
  const actual = await importOriginal<typeof import("#libs/hooks/service.svelte")>();
  return {
    ...actual,
    serviceState: serviceStateMock,
    deviceLabel: (device: AudioDevice) => device.name,
    refreshServiceData: refreshServiceDataMock,
    selectModel: selectModelMock,
    selectDevice: selectDeviceMock,
    startService: startServiceMock,
    stopService: stopServiceMock,
    startTestRecording: startTestRecordingMock,
    stopTestRecording: stopTestRecordingMock,
  };
});

vi.mock("#libs/utils/toast", () => ({ toast: toastMocks }));

vi.mock("#libs/hooks/config.svelte", () => ({
  configState: configStateMock,
  updateConfig: updateConfigMock,
}));

beforeEach(() => {
  vi.clearAllMocks();
  // 纯对象替身无响应式：渲染前预填，组件首帧即读到
  serviceStateMock.status = { state: "stopped", modelId: null, loadMs: null };
  serviceStateMock.models = [modelEntry];
  serviceStateMock.devices = [deviceEntry];
  serviceStateMock.selectedModel = modelEntry.id;
  serviceStateMock.selectedDevice = deviceEntry.id;
  serviceStateMock.recording = false;
  serviceStateMock.recognizing = false;
  serviceStateMock.switching = false;
  serviceStateMock.result = null;
  serviceStateMock.history = [];
  configStateMock.value = { shortcut_output: "clipboard", shortcut_paste_restore: false };
  updateConfigMock.mockResolvedValue(true);
});

afterEach(() => {
  cleanup();
  document.body.removeAttribute("style");
});

describe("服务页", () => {
  it("挂载即刷新并渲染统计、记录与状态徽标", async () => {
    render(Page);

    await waitFor(() => {
      expect(refreshServiceDataMock).toHaveBeenCalledOnce();
    });
    expect(screen.getByText("Recognitions")).not.toBeNull();
    expect(screen.getByText("Total characters")).not.toBeNull();
    expect(screen.getByText("Avg recognition time")).not.toBeNull();
    expect(screen.getByText("Recognition history")).not.toBeNull();
    expect(screen.getByText("Stopped")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Start service" })).not.toBeNull();
    expect(screen.getByRole("button", { name: "Start recognizing" })).not.toBeNull();
  });

  it("空历史显示零统计与空态提示", () => {
    render(Page);

    const stats = within(screen.getByTestId("service-stats"));
    expect(stats.getAllByText("0", { exact: true })).toHaveLength(2);
    expect(stats.getByText("0 s")).not.toBeNull();
    expect(screen.getByText("No recognitions yet — records will appear here")).not.toBeNull();
  });

  it("历史记录渲染统计与条目，点击展开全文", async () => {
    const user = userEvent.setup();
    const longText = `识别结果全文${"补字".repeat(40)}结尾`;
    serviceStateMock.history = [
      { text: "你好世界", audioSec: 2.5, inferMs: 300, fileName: "a.wav", id: 2, at: 2 },
      {
        text: longText,
        audioSec: 5,
        inferMs: 1500,
        fileName: "b.wav",
        id: 1,
        at: 1,
      },
    ];
    render(Page);

    // 条数 2、字符总数、推理耗时均值 (300+1500)/2=900ms → 0.9 s
    const stats = screen.getByTestId("service-stats");
    expect(stats.textContent).toContain("2");
    expect(stats.textContent).toContain("0.9 s");
    expect(screen.getByText("你好世界")).not.toBeNull();

    const recordButton = screen.getByRole("button", { name: new RegExp("补字") });
    expect(recordButton.getAttribute("aria-expanded")).toBe("false");
    await user.click(recordButton);

    expect(recordButton.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText(longText)).not.toBeNull();
  });

  it("服务未运行时录制按钮禁用", () => {
    render(Page);

    expect(screen.getByRole("button", { name: "Start recognizing" }).hasAttribute("disabled")).toBe(
      true,
    );
  });

  it("运行时录制按钮可用，点开始调命令", async () => {
    const user = userEvent.setup();
    serviceStateMock.status = { state: "running", modelId: modelEntry.id, loadMs: 800 };
    render(Page);

    const recordButton = screen.getByRole("button", { name: "Start recognizing" });
    expect(recordButton.hasAttribute("disabled")).toBe(false);
    await user.click(recordButton);

    expect(startTestRecordingMock).toHaveBeenCalledOnce();
  });

  it("录制中按钮变结束并识别", async () => {
    const user = userEvent.setup();
    serviceStateMock.status = { state: "running", modelId: modelEntry.id, loadMs: 800 };
    serviceStateMock.recording = true;
    render(Page);

    await user.click(screen.getByRole("button", { name: "Stop and recognize" }));

    expect(stopTestRecordingMock).toHaveBeenCalledOnce();
  });

  it("识别中按钮禁用并提示", () => {
    serviceStateMock.status = { state: "running", modelId: modelEntry.id, loadMs: 800 };
    serviceStateMock.recognizing = true;
    render(Page);

    const button = screen.getByRole("button", { name: "Recognizing…" });
    expect(button.hasAttribute("disabled")).toBe(true);
  });

  it("旧结果回显已由历史列表承接，不再单独展示", () => {
    serviceStateMock.result = {
      text: "你好世界",
      audioSec: 2.5,
      inferMs: 300,
      fileName: "recording-20261020-100000.wav",
    };
    render(Page);

    expect(screen.queryByText("recording-20261020-100000.wav")).toBeNull();
  });

  it("运行时显示停止按钮", async () => {
    const user = userEvent.setup();
    serviceStateMock.status = { state: "running", modelId: modelEntry.id, loadMs: 800 };
    render(Page);

    await user.click(screen.getByRole("button", { name: "Stop service" }));

    expect(stopServiceMock).toHaveBeenCalledOnce();
  });

  it("控制分组含语音快捷键行", () => {
    render(Page);

    expect(screen.getByText("Voice recognition")).not.toBeNull();
    expect(
      screen.getByText("Press the global shortcut to start or stop recognition", {
        exact: false,
      }),
    ).not.toBeNull();
  });

  it("无模型时提示去下载", () => {
    serviceStateMock.models = [];
    serviceStateMock.selectedModel = null;
    render(Page);

    expect(
      screen.getByText("No models available yet, download one on the Models page first"),
    ).not.toBeNull();
  });

  it("切换输出方式经命令落盘", async () => {
    const user = userEvent.setup();
    render(Page);

    await user.click(screen.getByRole("button", { name: "Output method" }));
    await waitFor(() => {
      expect(document.querySelector('[data-value="paste"]')).not.toBeNull();
    });
    await user.click(document.querySelector('[data-value="paste"]') as HTMLElement);

    expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_output: "paste" });
  });

  it("仅粘贴模式展示恢复开关，其它模式隐藏", async () => {
    configStateMock.value = { shortcut_output: "paste", shortcut_paste_restore: false };
    const { unmount } = render(Page);
    expect(screen.getByRole("switch", { name: "Restore clipboard after paste" })).not.toBeNull();
    unmount();
    cleanup();

    configStateMock.value = { shortcut_output: "clipboard", shortcut_paste_restore: false };
    render(Page);
    expect(screen.queryByRole("switch", { name: "Restore clipboard after paste" })).toBeNull();
  });

  it("恢复开关切换经命令落盘", async () => {
    const user = userEvent.setup();
    configStateMock.value = { shortcut_output: "paste", shortcut_paste_restore: false };
    render(Page);

    await user.click(screen.getByRole("switch", { name: "Restore clipboard after paste" }));

    expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_paste_restore: true });
  });
});
