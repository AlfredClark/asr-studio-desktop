import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_DEVICE_ID,
  SELECTED_DEVICE_STORAGE_KEY,
  SELECTED_MODEL_STORAGE_KEY,
  TRANSCRIBE_BUSY_MESSAGE,
  TRANSCRIBE_CANCELLED_MESSAGE,
  deviceLabel,
  handleDirectiveTrigger,
  handleQuickLexiconTrigger,
  handleShortcutTrigger,
  isTranscribeBusyFailure,
  isTranscribeCancelledFailure,
  loadStoredSelection,
  refreshServiceData,
  selectDevice,
  selectModel,
  serviceState,
  startService,
  startTestRecording,
  stopService,
  stopTestRecording,
} from "#libs/hooks/service.svelte";
import type { AudioDevice, Directive, ModelEntry } from "#libs/commands/types";
import type { LexiconGroup } from "#libs/commands/types";
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
    transcribeFileRaw: vi.fn(),
    cancelTranscription: vi.fn(),
    outputText: vi.fn(),
    isShortcutRegistered: vi.fn(),
    quickAddSelectionToLexicon: vi.fn(),
    replayDirective: vi.fn(),
  },
}));

const directiveStateMock = vi.hoisted((): { value: Directive[] } => ({ value: [] }));
const findDirectiveByPinyinMock = vi.hoisted(() => vi.fn());
const refreshDirectivesMock = vi.hoisted(() => vi.fn());
const suggestDirectivePinyinMock = vi.hoisted(() => vi.fn());
vi.mock("#libs/hooks/directives.svelte", () => ({
  directiveState: directiveStateMock,
  findDirectiveByPinyin: findDirectiveByPinyinMock,
  normalizeDirectivePinyin: (pinyin: string) => pinyin,
  refreshDirectives: refreshDirectivesMock,
  suggestDirectivePinyin: suggestDirectivePinyinMock,
}));

const lexiconStateMock = vi.hoisted(
  (): {
    value: {
      version: number;
      groups: LexiconGroup[];
      entries: never[];
      active_group_ids: string[];
    };
  } => ({
    value: { version: 1, groups: [], entries: [], active_group_ids: [] },
  }),
);
const refreshLexiconMock = vi.hoisted(() => vi.fn());
vi.mock("#libs/hooks/lexicon.svelte", () => ({
  groupLabel: (group: { is_global: boolean; name: string }) =>
    group.is_global ? "Global lexicon" : group.name,
  lexiconState: lexiconStateMock,
  refreshLexicon: refreshLexiconMock,
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
  local_bytes: 0,
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
  mocked.quickAddSelectionToLexicon.mockReturnValue(
    fakeCommand({ status: "ok", data: { added: 0, skipped: 0 } }) as never,
  );
  mocked.transcribeFileRaw.mockReturnValue(
    fakeCommand({
      status: "ok",
      data: { text: "你好", audio_sec: 2.5, infer_ms: 300 },
    }) as never,
  );
  mocked.replayDirective.mockReturnValue(
    fakeCommand({ status: "ok", data: { executed: 4 } }) as never,
  );
  refreshDirectivesMock.mockResolvedValue(true);
  suggestDirectivePinyinMock.mockResolvedValue("da kai ji shi ben");
  findDirectiveByPinyinMock.mockReturnValue(null);
  refreshLexiconMock.mockResolvedValue(true);
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

  it("运行中切模型失败回滚选中", async () => {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedModel = model.id;
    mocked.startService.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await selectModel("qwen3-0.6b-int8");

    expect(serviceState.selectedModel).toBe(model.id);
    expect(mockedToast.error).toHaveBeenCalled();
  });

  it("切换中重复点选直接忽略", async () => {
    serviceState.switching = true;

    await selectModel("qwen3-0.6b-int8");

    expect(serviceState.selectedModel).toBeNull();
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

  it("取消判定只认取消字面量", () => {
    expect(isTranscribeCancelledFailure(TRANSCRIBE_CANCELLED_MESSAGE)).toBe(true);
    expect(isTranscribeCancelledFailure(`wrap: ${TRANSCRIBE_CANCELLED_MESSAGE}`)).toBe(true);
    expect(isTranscribeCancelledFailure(TRANSCRIBE_BUSY_MESSAGE)).toBe(false);
    expect(isTranscribeCancelledFailure(null)).toBe(false);
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
      kind: "voice" as const,
      output: null,
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
      {
        text: "你好",
        audioSec: 2,
        inferMs: 1000,
        fileName: "a.wav",
        id: 1,
        at: 1,
        kind: "voice",
        output: null,
      },
      {
        text: "世界！",
        audioSec: 3,
        inferMs: 3000,
        fileName: "b.wav",
        id: 2,
        at: 2,
        kind: "directive",
        output: null,
      },
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
    expect(serviceState.history[0]).toMatchObject({ text: "你好", kind: "voice" });
    // 实际投递通道回填历史：默认 mock 走剪贴板无降级
    expect(serviceState.history[0]).toMatchObject({ output: "clipboard" });
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
    // 降级后实际通道为剪贴板，回填历史而非配置值
    expect(serviceState.history[0]).toMatchObject({ output: "clipboard" });
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

describe("快捷词库触发", () => {
  function runningService(): void {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
  }

  function workLexicon(): void {
    lexiconStateMock.value = {
      version: 1,
      groups: [
        { id: "global", name: "", is_global: true },
        { id: "g-work", name: "工作", is_global: false },
      ],
      entries: [],
      active_group_ids: ["g-work"],
    };
    configState.value = { quick_lexicon_group: "g-work" } as AppConfig;
  }

  it("运行中有写入时刷新并提示", async () => {
    runningService();
    workLexicon();
    mocked.quickAddSelectionToLexicon.mockReturnValue(
      fakeCommand({ status: "ok", data: { added: 2, skipped: 1 } }) as never,
    );

    await handleQuickLexiconTrigger();

    expect(mocked.quickAddSelectionToLexicon).toHaveBeenCalledOnce();
    expect(refreshLexiconMock).toHaveBeenCalledOnce();
    expect(mockedToast.success).toHaveBeenCalledWith(
      m.lexicon_quick_added({ count: 2, name: "工作" }),
    );
  });

  it("零写入时静默返回", async () => {
    runningService();
    workLexicon();

    await handleQuickLexiconTrigger();

    expect(mocked.quickAddSelectionToLexicon).toHaveBeenCalledOnce();
    expect(refreshLexiconMock).not.toHaveBeenCalled();
    expect(mockedToast.success).not.toHaveBeenCalled();
    expect(mockedToast.error).not.toHaveBeenCalled();
  });

  it("命令失败只上报不提示", async () => {
    runningService();
    workLexicon();
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.quickAddSelectionToLexicon.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await handleQuickLexiconTrigger();

    expect(refreshLexiconMock).not.toHaveBeenCalled();
    expect(mockedToast.success).not.toHaveBeenCalled();
    expect(mockedToast.error).not.toHaveBeenCalled();
    expect(errorSpy).toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  it("服务未运行时直接忽略", async () => {
    serviceState.status = { state: "stopped", modelId: null, loadMs: null };

    await handleQuickLexiconTrigger();

    expect(mocked.quickAddSelectionToLexicon).not.toHaveBeenCalled();
  });
});

describe("指令触发", () => {
  function runningRecording(): void {
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedDevice = device.id;
    serviceState.recording = true;
  }

  it("空闲时直接开始录制", async () => {
    runningStatus();
    serviceState.selectedDevice = device.id;

    await handleDirectiveTrigger();

    expect(mocked.startRecording).toHaveBeenCalledWith(device.id);
    expect(serviceState.recording).toBe(true);
  });

  it("录制中按第二次走原始文本转写并回放命中指令", async () => {
    runningRecording();
    findDirectiveByPinyinMock.mockReturnValue({ id: "d-1" } as Directive);

    await handleDirectiveTrigger();

    expect(mocked.transcribeFileRaw).toHaveBeenCalledWith("/records/a.wav");
    expect(mocked.transcribeFile).not.toHaveBeenCalled();
    expect(findDirectiveByPinyinMock).toHaveBeenCalled();
    expect(mocked.replayDirective).toHaveBeenCalledWith("d-1");
    expect(serviceState.history[0]).toMatchObject({ text: "你好", kind: "directive" });
    expect(mockedToast.success).not.toHaveBeenCalled();
    expect(mockedToast.error).not.toHaveBeenCalled();
  });

  it("未命中时静默返回", async () => {
    runningRecording();
    findDirectiveByPinyinMock.mockReturnValue(null);

    await handleDirectiveTrigger();

    expect(refreshDirectivesMock).toHaveBeenCalledOnce();
    expect(mocked.replayDirective).not.toHaveBeenCalled();
    expect(mockedToast.success).not.toHaveBeenCalled();
    expect(mockedToast.error).not.toHaveBeenCalled();
    expect(mockedToast.warning).not.toHaveBeenCalled();
  });

  it("转写失败走内核提示且不回放", async () => {
    runningRecording();
    mocked.transcribeFileRaw.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await handleDirectiveTrigger();

    expect(mockedToast.error).toHaveBeenCalledWith(m.service_recognize_failed());
    expect(mocked.replayDirective).not.toHaveBeenCalled();
  });

  it("回放失败时提示", async () => {
    runningRecording();
    findDirectiveByPinyinMock.mockReturnValue({ id: "d-1" } as Directive);
    mocked.replayDirective.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await handleDirectiveTrigger();

    expect(mockedToast.error).toHaveBeenCalledWith(m.directives_replay_failed());
  });

  it("服务未运行时直接忽略", async () => {
    serviceState.status = { state: "stopped", modelId: null, loadMs: null };

    await handleDirectiveTrigger();

    expect(mocked.startRecording).not.toHaveBeenCalled();
    expect(mocked.transcribeFileRaw).not.toHaveBeenCalled();
    expect(mocked.replayDirective).not.toHaveBeenCalled();
  });
});

describe("选中持久化", () => {
  /** Node 环境桩 localStorage：缺省空库，可选抛错模拟隐私模式 */
  function stubStorage(
    initial: Record<string, string> = {},
    failing = false,
  ): {
    setItem: ReturnType<typeof vi.fn>;
    removeItem: ReturnType<typeof vi.fn>;
  } {
    const store = new Map(Object.entries(initial));
    const setItem = vi.fn((key: string, value: string) => {
      if (failing) throw new Error("denied");
      store.set(key, value);
    });
    const removeItem = vi.fn((key: string) => {
      store.delete(key);
    });
    vi.stubGlobal("localStorage", {
      getItem: vi.fn((key: string) => store.get(key) ?? null),
      setItem,
      removeItem,
    });
    return { setItem, removeItem };
  }

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("无存储时回落空", () => {
    expect(loadStoredSelection(SELECTED_MODEL_STORAGE_KEY)).toBeNull();
    expect(loadStoredSelection(SELECTED_DEVICE_STORAGE_KEY)).toBeNull();
  });

  it("空白与抛错回落空", () => {
    stubStorage({ [SELECTED_MODEL_STORAGE_KEY]: "  " });
    expect(loadStoredSelection(SELECTED_MODEL_STORAGE_KEY)).toBeNull();
    vi.unstubAllGlobals();
    stubStorage({}, true);
    expect(loadStoredSelection(SELECTED_MODEL_STORAGE_KEY)).toBeNull();
  });

  it("水合采用有效存量，无效存量回落首选项", async () => {
    stubStorage({ [SELECTED_MODEL_STORAGE_KEY]: model.id });
    await refreshServiceData();
    expect(serviceState.selectedModel).toBe(model.id);
    vi.unstubAllGlobals();

    stubStorage({ [SELECTED_MODEL_STORAGE_KEY]: "deleted-model" });
    await refreshServiceData();
    expect(serviceState.selectedModel).toBe(model.id);
  });

  it("水合采用有效存量设备", async () => {
    stubStorage({ [SELECTED_DEVICE_STORAGE_KEY]: device.id });
    await refreshServiceData();
    expect(serviceState.selectedDevice).toBe(device.id);
  });

  it("选模型与选设备即时落盘", async () => {
    const { setItem } = stubStorage();
    await refreshServiceData();
    await selectModel(model.id);
    expect(setItem).toHaveBeenCalledWith(SELECTED_MODEL_STORAGE_KEY, model.id);
    selectDevice(device.id);
    expect(setItem).toHaveBeenCalledWith(SELECTED_DEVICE_STORAGE_KEY, device.id);
  });

  it("切模型失败回滚落盘", async () => {
    const { setItem } = stubStorage();
    runningStatus();
    serviceState.status = { state: "running", modelId: model.id, loadMs: 800 };
    serviceState.selectedModel = model.id;
    mocked.startService.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );
    await selectModel("other-model");
    expect(serviceState.selectedModel).toBe(model.id);
    expect(setItem).toHaveBeenLastCalledWith(SELECTED_MODEL_STORAGE_KEY, model.id);
  });
});
