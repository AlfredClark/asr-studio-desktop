import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import type { AudioDevice, ModelEntry } from "#libs/commands/types";
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
const toastMocks = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn() }));
const updateConfigMock = vi.hoisted(() => vi.fn());
const configStateMock = vi.hoisted((): { value: { shortcut_output: string } | null } => ({
  value: { shortcut_output: "clipboard" },
}));
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
};

const deviceEntry: AudioDevice = { id: "0", name: "Microphone", is_default: true };

vi.mock("#libs/hooks/service.svelte", () => ({
  serviceState: serviceStateMock,
  deviceLabel: (device: AudioDevice) => device.name,
  refreshServiceData: refreshServiceDataMock,
  selectModel: selectModelMock,
  selectDevice: selectDeviceMock,
  startService: startServiceMock,
  stopService: stopServiceMock,
  startTestRecording: startTestRecordingMock,
  stopTestRecording: stopTestRecordingMock,
}));

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
  configStateMock.value = { shortcut_output: "clipboard" };
  updateConfigMock.mockResolvedValue(true);
});

afterEach(() => {
  cleanup();
  document.body.removeAttribute("style");
});

describe("服务页", () => {
  it("挂载即刷新并渲染标题与徽标", async () => {
    render(Page);

    await waitFor(() => {
      expect(refreshServiceDataMock).toHaveBeenCalledOnce();
    });
    expect(screen.getByText("Service")).not.toBeNull();
    expect(screen.getByText("Stopped")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Start service" })).not.toBeNull();
    expect(screen.getByRole("button", { name: "Start recognizing" })).not.toBeNull();
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

  it("识别结果回显文本与用时", () => {
    serviceStateMock.status = { state: "running", modelId: modelEntry.id, loadMs: 800 };
    serviceStateMock.result = {
      text: "你好世界",
      audioSec: 2.5,
      inferMs: 300,
      fileName: "recording-20261020-100000.wav",
    };
    render(Page);

    expect(screen.getByText("你好世界")).not.toBeNull();
    expect(screen.getByText("recording-20261020", { exact: false })).not.toBeNull();
  });

  it("运行时显示停止按钮", async () => {
    const user = userEvent.setup();
    serviceStateMock.status = { state: "running", modelId: modelEntry.id, loadMs: 800 };
    render(Page);

    await user.click(screen.getByRole("button", { name: "Stop service" }));

    expect(stopServiceMock).toHaveBeenCalledOnce();
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
});
