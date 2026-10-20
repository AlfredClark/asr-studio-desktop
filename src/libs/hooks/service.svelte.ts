// 识别服务的全局状态：模型切换自动停启、录制与识别互斥，切页不丢失。
// 服务启停与模型切换走命令结算分支；状态以 `serviceStatus` 查询为准，
// 前端不做本地推断（后端是唯一真值，与配置中心的约定一致）。
import commands from "#libs/commands";
import { reportCommandFailure } from "#libs/commands/cores";
import type { AudioDevice, ModelEntry } from "#libs/commands/types";
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
});

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
      serviceState.devices = devices;
      if (
        serviceState.selectedDevice == null ||
        !devices.some((device) => device.id === serviceState.selectedDevice)
      ) {
        serviceState.selectedDevice =
          devices.find((device) => device.is_default)?.id ?? devices[0]?.id ?? null;
      }
    })
    .failed((failure) => {
      toast.error(m.service_load_devices_failed());
      reportCommandFailure("[service] failed to load devices", failure);
    });
  await syncStatus();
}

/** 启动服务：只接受已选中的就绪模型，成功后同步快照 */
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
  serviceState.selectedModel = id;
  if (serviceState.status?.state !== "running" || serviceState.switching) return;
  serviceState.switching = true;
  try {
    await commands.stopService().result();
    await commands
      .startService(id)
      .success(() => {
        toast.success(m.service_started());
      })
      .failed((failure) => {
        toast.error(m.service_start_failed());
        reportCommandFailure("[service] switch failed", failure);
      });
    await syncStatus();
  } finally {
    serviceState.switching = false;
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

/** 结束录制并识别：存档落盘后调转写，结果回显，用时一并展示 */
export async function stopTestRecording(): Promise<void> {
  if (!serviceState.recording || serviceState.recognizing) return;
  serviceState.recording = false;
  serviceState.recognizing = true;
  try {
    const record = await commands.stopRecording().result();
    if (record.status === "error") {
      toast.error(m.service_record_failed());
      reportCommandFailure("[service] stop record failed", record.error);
      return;
    }
    const fileName = record.data.path.split("/").pop() ?? record.data.path;
    const transcribed = await commands.transcribeFile(record.data.path).result();
    if (transcribed.status === "error") {
      toast.error(m.service_recognize_failed());
      reportCommandFailure("[service] transcribe failed", transcribed.error);
      return;
    }
    serviceState.result = {
      text: transcribed.data.text,
      audioSec: transcribed.data.audio_sec ?? 0,
      inferMs: transcribed.data.infer_ms,
      fileName,
    };
  } finally {
    serviceState.recognizing = false;
  }
}
