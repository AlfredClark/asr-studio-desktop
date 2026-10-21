import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_DEVICE_ID,
  TRANSCRIBE_BUSY_MESSAGE,
  deviceLabel,
  handleShortcutTrigger,
  isTranscribeBusyFailure,
  refreshServiceData,
  selectDevice,
  selectModel,
  serviceState,
  startService,
  startTestRecording,
  stopService,
  stopTestRecording,
} from "#libs/hooks/service.svelte";
import type { AudioDevice, ModelEntry } from "#libs/commands/types";
import { configState, type AppConfig } from "#libs/hooks/config.svelte";
import { m } from "#libs/i18n/paraglide/messages";
import { summarizeHistory } from "#libs/hooks/service.svelte";

vi.mock("#libs/commands", () => ({
  default: {
    listModels: vi.fn(),
    listInputDevices: vi.fn(),
    serviceStatus: vi.fn(),
    startService: vi.fn(),
    stopService: vi.fn(),
    startRecording: vi.fn(),
    stopRecording: vi.fn(),
    transcribeFile: vi.fn(),
    outputText: vi.fn(),
    isShortcutRegistered: vi.fn(),
  },
}));

vi.mock("#libs/utils/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));

const gotoMock = vi.hoisted(() => vi.fn());
vi.mock("$app/navigation", () => ({ goto: gotoMock }));

import commands from "#libs/commands";
import { toast } from "#libs/utils/toast";

const mocked = vi.mocked(commands);
const mockedToast = vi.mocked(toast);

const model: ModelEntry = {
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

const device: AudioDevice = { id: "0", name: "Microphone", is_default: true };

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

function runningStatus(): void {
  mocked.serviceStatus.mockReturnValue(
    fakeCommand({
      status: "ok",
      data: {
        state: "running",
        model_id: model.id,
        display_name: model.display_name,
        load_ms: 800,
      },
    }) as never,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  serviceState.status = null;
  serviceState.models = [];
  serviceState.devices = [];
  serviceState.selectedModel = null;
  serviceState.selectedDevice = null;
  serviceState.recording = false;
  serviceState.recognizing = false;
  serviceState.switching = false;
  serviceState.result = null;
  serviceState.history = [];
  mocked.listModels.mockReturnValue(fakeCommand({ status: "ok", data: [model] }) as never);
  mocked.listInputDevices.mockReturnValue(fakeCommand({ status: "ok", data: [device] }) as never);
  mocked.serviceStatus.mockReturnValue(
    fakeCommand({
      status: "ok",
      data: { state: "stopped", model_id: null, display_name: null, load_ms: null },
    }) as never,
  );
  mocked.startService.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.stopService.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.startRecording.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.stopRecording.mockReturnValue(
    fakeCommand({
      status: "ok",
      data: { path: "/records/a.wav", file_name: "a.wav", seconds: 2.5 },
    }) as never,
  );
  mocked.transcribeFile.mockReturnValue(
    fakeCommand({ status: "ok", data: { text: "你好", audio_sec: 2.5, infer_ms: 300 } }) as never,
  );
  mocked.outputText.mockReturnValue(
    fakeCommand({
      status: "ok",
      data: { via: "clipboard", fallback: false, reason: "as_configured" },
    }) as never,
  );
  mocked.isShortcutRegistered.mockReturnValue(fakeCommand({ status: "ok", data: true }) as never);
  configState.value = null;
});

describe("识别服务状态", () => {
  it("刷新补齐默认选中", async () => {
    await refreshServiceData();

    expect(serviceState.models).toEqual([model]);
    expect(serviceState.selectedModel).toBe(model.id);
    expect(serviceState.selectedDevice).toBe(device.id);
    expect(serviceState.status?.state).toBe("stopped");
  });

  it("系统默认合成项优先于物理默认标记", async () => {
    const synthetic: AudioDevice = {
      id: DEFAULT_DEVICE_ID,
      name: "System default",
      is_default: true,
    };
    mocked.listInputDevices.mockReturnValue(
      fakeCommand({ status: "ok", data: [synthetic, device] }) as never,
    );

    await refreshServiceData();

    expect(serviceState.selectedDevice).toBe(DEFAULT_DEVICE_ID);
  });

  it("合成项展示本地化文案，物理设备展示原名", () => {
    const synthetic: AudioDevice = {
      id: DEFAULT_DEVICE_ID,
      name: "System default",
      is_default: true,
    };
    expect(deviceLabel(synthetic)).toBe(m.service_device_system_default());
    expect(deviceLabel(device)).toBe(device.name);
  });

  it("启动服务成功后同步快照并提示", async () => {
    runningStatus();
    serviceState.selectedModel = model.id;

    await startService();

    expect(mocked.startService).toHaveBeenCalledWith(model.id);
    expect(serviceState.status?.state).toBe("running");
    expect(mockedToast.success).toHaveBeenCalled();
  });

  it("启动成功后语音键已注册不打扰", async () => {
    runningStatus();
    serviceState.selectedModel = model.id;
    configState.value = { shortcut_voice: "Ctrl+Alt+V" } as AppConfig;

    await startService();

    expect(mocked.isShortcutRegistered).toHaveBeenCalledWith("Ctrl+Alt+V");
    expect(mockedToast.warning).not.toHaveBeenCalled();
  });

  it("启动成功后语音键未注册提示换组合", async () => {
    runningStatus();
    serviceState.selectedModel = model.id;
    configState.value = { shortcut_voice: "Ctrl+Alt+V" } as AppConfig;
    mocked.isShortcutRegistered.mockReturnValue(
      fakeCommand({ status: "ok", data: false }) as never,
    );

    await startService();

    expect(mockedToast.warning).toHaveBeenCalled();
  });

  it("启动失败或未配置语音键时不核验注册", async () => {
    mocked.startService.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );
    serviceState.selectedModel = model.id;

    await startService();

    expect(mocked.isShortcutRegistered).not.toHaveBeenCalled();
  });

  it("启动失败时报错且不抛错", async () => {
    mocked.startService.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );
    serviceState.selectedModel = model.id;

    await startService();

    expect(mockedToast.error).toHaveBeenCalled();
  });

  it("停止服务后同步快照", async () => {
    await stopService();

    expect(mocked.stopService).toHaveBeenCalledOnce();
    expect(mockedToast.success).toHaveBeenCalled();
  });

  it("运行中切模型自动停旧启新", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    const other = { ...model, id: "qwen3-0.6b-int8" };

    await selectModel(other.id);

    expect(mocked.stopService).toHaveBeenCalledOnce();
    expect(mocked.startService).toHaveBeenCalledWith(other.id);
    expect(serviceState.selectedModel).toBe(other.id);
    expect(serviceState.switching).toBe(false);
  });

  it("停止态切模型只记录选择不启停", async () => {
    await selectModel(model.id);

    expect(serviceState.selectedModel).toBe(model.id);
    expect(mocked.stopService).not.toHaveBeenCalled();
    expect(mocked.startService).not.toHaveBeenCalled();
  });

  it("录制中切换设备被拦截", () => {
    serviceState.recording = true;
    serviceState.selectedDevice = device.id;

    selectDevice("1");

    expect(serviceState.selectedDevice).toBe(device.id);
  });

  it("完整测试链路回写结果", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;

    await startTestRecording();
    expect(serviceState.recording).toBe(true);
    expect(serviceState.result).toBeNull();

    await stopTestRecording();

    expect(serviceState.recording).toBe(false);
    expect(serviceState.recognizing).toBe(false);
    expect(serviceState.result?.text).toBe("你好");
    expect(serviceState.result?.fileName).toBe("a.wav");
    // 同一内核：成功结果记入会话历史（测试与快捷键两路共用）
    expect(serviceState.history).toHaveLength(1);
    expect(serviceState.history[0]?.text).toBe("你好");
  });

  it("服务未运行时不开始录制", async () => {
    serviceState.selectedDevice = device.id;

    await startTestRecording();

    expect(mocked.startRecording).not.toHaveBeenCalled();
  });

  it("转写失败时报错且不回写结果", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;
    mocked.transcribeFile.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await startTestRecording();
    await stopTestRecording();

    expect(serviceState.result).toBeNull();
    expect(serviceState.recognizing).toBe(false);
    expect(mockedToast.error).toHaveBeenCalled();
  });

  it("转写失败时不记入会话历史", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;
    mocked.transcribeFile.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await startTestRecording();
    await stopTestRecording();

    expect(serviceState.history).toHaveLength(0);
  });

  it("转写忙时提示稍候而非报失败", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;
    mocked.transcribeFile.mockReturnValue(
      fakeCommand({
        status: "error",
        error: { kind: "Internal", message: TRANSCRIBE_BUSY_MESSAGE },
      }) as never,
    );

    await startTestRecording();
    await stopTestRecording();

    expect(mockedToast.warning).toHaveBeenCalledWith(m.service_recognize_busy());
    expect(mockedToast.error).not.toHaveBeenCalled();
    expect(serviceState.history).toHaveLength(0);
  });

  it("忙判定只认忙字面量", () => {
    expect(isTranscribeBusyFailure(TRANSCRIBE_BUSY_MESSAGE)).toBe(true);
    expect(isTranscribeBusyFailure(`wrap: ${TRANSCRIBE_BUSY_MESSAGE}`)).toBe(true);
    expect(isTranscribeBusyFailure("service is not running")).toBe(false);
    expect(isTranscribeBusyFailure(null)).toBe(false);
  });

  it("会话历史超限截尾，最新在前", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;
    serviceState.history = Array.from({ length: 100 }, (_, index) => ({
      text: `旧${index}`,
      audioSec: 1,
      inferMs: 100,
      fileName: `old-${index}.wav`,
      id: index + 1,
      at: index,
    }));

    await startTestRecording();
    await stopTestRecording();

    expect(serviceState.history).toHaveLength(100);
    expect(serviceState.history[0]?.text).toBe("你好");
  });

  it("历史统计：空历史归零", () => {
    expect(summarizeHistory([])).toEqual({ count: 0, totalChars: 0, avgInferMs: null });
  });

  it("历史统计：条数、字符总数与推理耗时均值", () => {
    const stats = summarizeHistory([
      { text: "你好", audioSec: 2, inferMs: 1000, fileName: "a.wav", id: 1, at: 1 },
      { text: "世界！", audioSec: 3, inferMs: 3000, fileName: "b.wav", id: 2, at: 2 },
    ]);

    expect(stats).toEqual({ count: 2, totalChars: 5, avgInferMs: 2000 });
  });

  it("快捷键空闲时直接开始录制，不切页面不抢焦点", async () => {
    runningStatus();
    serviceState.selectedDevice = device.id;

    await handleShortcutTrigger();

    expect(gotoMock).not.toHaveBeenCalled();
    expect(mocked.startRecording).toHaveBeenCalledWith(device.id);
    expect(serviceState.recording).toBe(true);
  });

  it("快捷键热路径只同步状态不拉模型设备清单", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.models = [model];
    serviceState.selectedModel = model.id;
    serviceState.selectedDevice = device.id;

    await handleShortcutTrigger();

    expect(mocked.listModels).not.toHaveBeenCalled();
    expect(mocked.listInputDevices).not.toHaveBeenCalled();
    expect(mocked.serviceStatus).toHaveBeenCalled();
    expect(mocked.startRecording).toHaveBeenCalledWith(device.id);
  });

  it("快捷键冷启动补齐默认选中后再录制", async () => {
    runningStatus();

    await handleShortcutTrigger();

    expect(mocked.listModels).toHaveBeenCalled();
    expect(serviceState.selectedModel).toBe(model.id);
    expect(serviceState.selectedDevice).toBe(device.id);
    expect(mocked.startRecording).toHaveBeenCalledWith(device.id);
  });

  it("快捷键录制中按第二次停止转写并输出", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;
    serviceState.recording = true;

    await handleShortcutTrigger();

    expect(serviceState.recording).toBe(false);
    expect(serviceState.result?.text).toBe("你好");
    expect(mocked.outputText).toHaveBeenCalledWith("你好");
    expect(mockedToast.warning).not.toHaveBeenCalled();
  });

  it("快捷键降级输出时提示用户", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;
    serviceState.recording = true;
    mocked.outputText.mockReturnValue(
      fakeCommand({
        status: "ok",
        data: { via: "clipboard", fallback: true, reason: "injection_fallback" },
      }) as never,
    );

    await handleShortcutTrigger();

    expect(mockedToast.warning).toHaveBeenCalledWith(m.service_output_fallback());
  });

  it("无可输入焦点时提示已存入剪贴板", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;
    serviceState.recording = true;
    mocked.outputText.mockReturnValue(
      fakeCommand({
        status: "ok",
        data: { via: "clipboard", fallback: true, reason: "no_editable_focus" },
      }) as never,
    );

    await handleShortcutTrigger();

    expect(mockedToast.warning).toHaveBeenCalledWith(m.service_output_no_focus());
  });

  it("快捷键识别中直接忽略", async () => {
    serviceState.recognizing = true;

    await handleShortcutTrigger();

    expect(mocked.stopRecording).not.toHaveBeenCalled();
    expect(gotoMock).not.toHaveBeenCalled();
  });

  it("快捷键服务未运行时不做任何事", async () => {
    serviceState.selectedDevice = device.id;

    await handleShortcutTrigger();

    expect(gotoMock).not.toHaveBeenCalled();
    expect(mocked.startRecording).not.toHaveBeenCalled();
  });
});
