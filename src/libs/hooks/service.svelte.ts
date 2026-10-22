// 识别服务的全局状态：模型切换自动停启、录制与识别互斥，切页不丢失。
// 服务启停与模型切换走命令结算分支；状态以 `serviceStatus` 查询为准，
// 前端不做本地推断（后端是唯一真值，与配置中心的约定一致）。
import commands from "#libs/commands";
import { reportCommandFailure } from "#libs/commands/cores";
import type { AudioDevice, ModelEntry } from "#libs/commands/types";
import { configValue } from "#libs/hooks/config.svelte";
import { m } from "#libs/i18n/paraglide/messages";
import { toast } from "#libs/utils/toast";

/** 服务运行快照（后端 `ServiceStatus` 的前端镜像，`null` 表示尚未查询） */
export interface ServiceSnapshot {
  /** Stopped | starting | running */
  state: "stopped" | "starting" | "running";
  /** 已加载（或正在加载）的模型 id */
  modelId: string | null;
  /** 加载耗时毫秒 */
  loadMs: number | null;
}

/** 单次识别测试的结果回显 */
export interface TestResult {
  /** 识别文本 */
  text: string;
  /** 音频时长秒 */
  audioSec: number;
  /** 推理耗时毫秒 */
  inferMs: number;
  /** 存档文件名 */
  fileName: string;
}

/** 会话级识别记录：`TestResult` 加 id 与完成时间戳；只放会话内存，重启即失效 */
export interface RecognitionRecord extends TestResult {
  /** 会话内自增 id */
  id: number;
  /** 完成时间戳（UTC 毫秒，展示层转本地时区） */
  at: number;
}

/** 历史上限：最新在前，超限截尾，避免长会话内存膨胀 */
export const HISTORY_LIMIT = 100;

/** 会话内记录 id 计数器：模块级，重启即清零 */
let recordSeq = 0;

/** 历史统计：条数 / 字符总数（`text.length` 求和）/ 推理耗时均值毫秒（空历史为 `null`） */
export function summarizeHistory(records: RecognitionRecord[]): {
  count: number;
  totalChars: number;
  avgInferMs: number | null;
} {
  const count = records.length;
  if (count === 0) return { count: 0, totalChars: 0, avgInferMs: null };
  let totalChars = 0;
  let totalInferMs = 0;
  for (const record of records) {
    totalChars += record.text.length;
    totalInferMs += record.inferMs;
  }
  return { count, totalChars, avgInferMs: totalInferMs / count };
}

/** 跨页面共享的服务状态；选择项只放会话内存，不落盘 */
export const serviceState = $state<{
  /** 后端状态快照 */
  status: ServiceSnapshot | null;
  /** 可选模型（仅就绪） */
  models: ModelEntry[];
  /** 录音设备 */
  devices: AudioDevice[];
  /** 当前选中的模型 id */
  selectedModel: string | null;
  /** 当前选中的设备 id */
  selectedDevice: string | null;
  /** 录制中 */
  recording: boolean;
  /** 识别中（转写命令结算前） */
  recognizing: boolean;
  /** 模型切换中（停旧启新串行段） */
  switching: boolean;
  /** 最近一次识别结果 */
  result: TestResult | null;
  /** 会话级识别历史：最新在前，只放会话内存，重启即失效 */
  history: RecognitionRecord[];
}>({
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
});

/** “系统默认”输入选项 id：与后端 `features::audio::DEFAULT_DEVICE_ID` 同字面量， 改名需两端同步 */
export const DEFAULT_DEVICE_ID = "default";

/** 设备下拉展示名：合成项走本地化文案，否则用系统上报的原名 */
export function deviceLabel(device: AudioDevice): string {
  return device.id === DEFAULT_DEVICE_ID ? m.service_device_system_default() : device.name;
}

/** 同步后端状态快照到内存；失败仅上报（调用方决定是否提示） */
async function syncStatus(): Promise<void> {
  await commands
    .serviceStatus()
    .success((status) => {
      serviceState.status = {
        state: status.state,
        modelId: status.model_id,
        loadMs: status.load_ms,
      };
    })
    .failed((failure) => {
      reportCommandFailure("[service] failed to sync status", failure);
    });
}

/** 仅同步后端状态快照：快捷键触发等热路径用，不拉模型/设备清单 */
export async function refreshServiceStatus(): Promise<void> {
  await syncStatus();
}

/** 刷新模型与设备清单，并补齐默认选中（首个就绪模型、默认设备优先） */
export async function refreshServiceData(): Promise<void> {
  await commands
    .listModels()
    .success((entries) => {
      serviceState.models = entries.filter((entry) => entry.status === "ready");
      if (
        serviceState.selectedModel == null ||
        !serviceState.models.some((entry) => entry.id === serviceState.selectedModel)
      ) {
        serviceState.selectedModel = serviceState.models[0]?.id ?? null;
      }
    })
    .failed((failure) => {
      reportCommandFailure("[service] failed to load models", failure);
    });
  await commands
    .listInputDevices()
    .success((devices) => {
      // 设备列表会话内只取一次：枚举会打开硬件，进页就刷又慢又刷屏；
      // 热插拔暂不支持，需要时重启应用重取
      if (serviceState.devices.length > 0) return;
      serviceState.devices = devices;
      if (
        serviceState.selectedDevice == null ||
        !devices.some((device) => device.id === serviceState.selectedDevice)
      ) {
        // 默认采用系统默认输入：合成项 → 物理默认标记 → 第 1 个
        serviceState.selectedDevice =
          devices.find((device) => device.id === DEFAULT_DEVICE_ID)?.id ??
          devices.find((device) => device.is_default)?.id ??
          devices[0]?.id ??
          null;
      }
    })
    .failed((failure) => {
      toast.error(m.service_load_devices_failed());
      reportCommandFailure("[service] failed to load devices", failure);
    });
  await syncStatus();
}

/** 启动服务：只接受已选中的就绪模型，成功后同步快照并核验语音键注册 */
export async function startService(): Promise<void> {
  const id = serviceState.selectedModel;
  if (id == null || serviceState.switching) return;
  serviceState.switching = true;
  try {
    await commands
      .startService(id)
      .success(() => {
        toast.success(m.service_started());
      })
      .failed((failure) => {
        toast.error(m.service_start_failed());
        reportCommandFailure("[service] start failed", failure);
      });
    await syncStatus();
  } finally {
    serviceState.switching = false;
  }
  await warnIfVoiceShortcutInactive();
}

/** 停止服务：幂等成功，后端未运行也视为成功 */
export async function stopService(): Promise<void> {
  await commands
    .stopService()
    .success(() => {
      toast.success(m.service_stopped());
    })
    .failed((failure) => {
      toast.error(m.service_stop_failed());
      reportCommandFailure("[service] stop failed", failure);
    });
  await syncStatus();
}

/** 选择模型：运行中切模型自动走“停旧启新”串行段，期间整组禁用； 停止态只记录选择，由启动按钮生效。 */
export async function selectModel(id: string): Promise<void> {
  // 切换中重复点选直接忽略：先判后写，避免选中态与实际运行长期不一致
  if (serviceState.switching) return;
  const previous = serviceState.selectedModel;
  serviceState.selectedModel = id;
  if (serviceState.status?.state !== "running") return;
  serviceState.switching = true;
  try {
    await commands.stopService().result();
    await commands
      .startService(id)
      .success(() => {
        toast.success(m.service_started());
      })
      .failed((failure) => {
        // 切模型失败回滚选中：状态以实际运行为准，不留“选了但没跑起来”的假象
        serviceState.selectedModel = previous;
        toast.error(m.service_start_failed());
        reportCommandFailure("[service] switch failed", failure);
      });
    await syncStatus();
  } finally {
    serviceState.switching = false;
  }
  await warnIfVoiceShortcutInactive();
}

/** 启动成功后核验语音快捷键：存量组合未注册（被占用或非法）即提示换组合。 停止态与未配置直接跳过——注册与服务运行态同生命周期，查了也是假阴性。 */
async function warnIfVoiceShortcutInactive(): Promise<void> {
  if (serviceState.status?.state !== "running") return;
  const combo = configValue("shortcut_voice", "").trim();
  if (!combo) return;
  const result = await commands.isShortcutRegistered(combo).result();
  if (result.status === "error") {
    reportCommandFailure("[service] voice shortcut check failed", result.error);
    return;
  }
  if (!result.data) {
    toast.warning(m.settings_voice_not_registered());
  }
}

/** 选择设备：录制中禁止切换（页面按钮已禁用，此处二次拦截） */
export function selectDevice(id: string): void {
  if (serviceState.recording) return;
  serviceState.selectedDevice = id;
}

/** 开始测试录制：服务必须运行中，重复点击直接忽略 */
export async function startTestRecording(): Promise<void> {
  const deviceId = serviceState.selectedDevice;
  if (
    deviceId == null ||
    serviceState.status?.state !== "running" ||
    serviceState.recording ||
    serviceState.recognizing ||
    serviceState.switching
  ) {
    return;
  }
  await commands
    .startRecording(deviceId)
    .success(() => {
      serviceState.recording = true;
      serviceState.result = null;
    })
    .failed((failure) => {
      toast.error(m.service_record_failed());
      reportCommandFailure("[service] record failed", failure);
    });
}

/** 结束录制并识别：存档落盘后调转写，结果回显，用时一并展示；仅回显不输出 */
export async function stopTestRecording(): Promise<void> {
  const result = await stopAndTranscribe();
  if (result) serviceState.result = result;
}

/**
 * 停止录制并转写（共用内核）：测试按钮只取回显，快捷键流程拿到结果再按配置输出。 失败已在内部提示并上报，调用方按 `null`
 * 收尾即可；成功结果同时记入会话历史。
 */
async function stopAndTranscribe(): Promise<TestResult | null> {
  if (!serviceState.recording || serviceState.recognizing) return null;
  serviceState.recording = false;
  serviceState.recognizing = true;
  try {
    const record = await commands.stopRecording().result();
    if (record.status === "error") {
      toast.error(m.service_record_failed());
      reportCommandFailure("[service] stop record failed", record.error);
      return null;
    }
    const fileName = record.data.path.split(/[/\\]/).pop() ?? record.data.path;
    const transcribed = await commands.transcribeFile(record.data.path).result();
    if (transcribed.status === "error") {
      if (isTranscribeBusyFailure(transcribed.error.message)) {
        toast.warning(m.service_recognize_busy());
      } else {
        toast.error(m.service_recognize_failed());
      }
      reportCommandFailure("[service] transcribe failed", transcribed.error);
      return null;
    }
    const entry: RecognitionRecord = {
      text: transcribed.data.text,
      audioSec: transcribed.data.audio_sec ?? 0,
      inferMs: transcribed.data.infer_ms,
      fileName,
      id: (recordSeq += 1),
      at: Date.now(),
    };
    // 会话历史：测试按钮与快捷键两路共用同一内核，此处一次记入，最新在前、超限截尾
    serviceState.history.unshift(entry);
    if (serviceState.history.length > HISTORY_LIMIT) {
      serviceState.history.length = HISTORY_LIMIT;
    }
    return entry;
  } finally {
    serviceState.recognizing = false;
  }
}

/** 快捷键语音触发事件名（与 Rust 侧同名字面量会合，改名需两端同步） */
const SHORTCUT_VOICE_TRIGGER_EVENT = "shortcut-voice-trigger";

/** 后端转写忙错误字面量（与 Rust 侧 `TRANSCRIBE_BUSY_MESSAGE` 会合，改名需两端同步） */
export const TRANSCRIBE_BUSY_MESSAGE = "transcription already in progress";

/** 是否为“已有转写占用”失败：命中则提示稍候而非报失败；纯函数，可单测 */
export function isTranscribeBusyFailure(message: unknown): boolean {
  return typeof message === "string" && message.includes(TRANSCRIBE_BUSY_MESSAGE);
}

let shortcutVoiceListening = false;

/** 订阅快捷键语音触发（全局单例，根布局挂载一次）； 非 Tauri 环境无事件可订，失败仅上报不抛错。 */
export async function listenShortcutVoice(): Promise<void> {
  if (shortcutVoiceListening) return;
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen(SHORTCUT_VOICE_TRIGGER_EVENT, () => {
      void handleShortcutTrigger();
    });
    shortcutVoiceListening = true;
  } catch (error) {
    reportCommandFailure("[service] shortcut voice listener unavailable", error);
  }
}

/**
 * 快捷键触发处理：录制中则停止、转写并按配置输出；空闲且服务运行时直接开始录制。 全程不显示本窗口、不切换页面，焦点留在用户当前应用；结果进回显供事后查看。
 * 切换中或识别中直接忽略（防重入）。
 */
export async function handleShortcutTrigger(): Promise<void> {
  if (serviceState.switching || serviceState.recognizing) return;
  if (serviceState.recording) {
    const result = await stopAndTranscribe();
    if (!result) return;
    serviceState.result = result;
    await outputResult(result.text);
    return;
  }
  // 热路径：已有选中与状态只同步快照（模型清单走全盘 stat，枚举设备碰硬件）；
  // 冷启动（未水合）才全量刷新，补齐默认模型与设备
  if (
    serviceState.selectedDevice != null &&
    serviceState.selectedModel != null &&
    serviceState.status != null
  ) {
    await refreshServiceStatus();
  } else {
    await refreshServiceData();
  }
  if (serviceState.status?.state !== "running") return;
  await startTestRecording();
}

/** 按配置输出识别文本；降级或失败时提示用户（文本仍保留在回显区） */
async function outputResult(text: string): Promise<void> {
  const delivered = await commands.outputText(text).result();
  if (delivered.status === "error") {
    toast.error(m.service_output_failed());
    reportCommandFailure("[service] output failed", delivered.error);
    return;
  }
  if (delivered.data.fallback) {
    if (delivered.data.reason === "no_editable_focus") {
      toast.warning(m.service_output_no_focus());
    } else {
      toast.warning(m.service_output_fallback());
    }
  }
}
