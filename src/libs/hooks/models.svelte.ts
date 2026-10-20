// 模型下载的全局状态：切页不丢失，模型页独占读写，未来识别页只读复用。
// 进度事件名与 Rust 侧 `features::models::MODEL_DOWNLOAD_PROGRESS_EVENT` 会合，改名需两端同步。
import commands from "#libs/commands";
import { reportCommandFailure } from "#libs/commands/cores";
import type { ModelEntry } from "#libs/commands/types";
import { m } from "#libs/i18n/paraglide/messages";
import { toast } from "#libs/utils/toast";

/** 下载进度事件名（见 Rust 侧同名常量） */
const PROGRESS_EVENT = "model-download-progress";

/** 跨页面共享的模型状态：条目由命令刷新，进度由事件推送 */
export const modelsState = $state<{
  entries: ModelEntry[];
  progress: Record<string, ModelDownloadProgress>;
}>({ entries: [], progress: {} });

/**
 * 下载进度事件载荷：与 Rust 侧 `features::models::DownloadProgress` 同构 （事件载荷不进命令签名，specta
 * 不导出，改名需两端同步）。
 */
export interface ModelDownloadProgress {
  /** 模型 id */
  id: string;
  /** 当前阶段 */
  phase: "download" | "extract";
  /** 累计字节数（下载阶段）或已解压条目数（解压阶段） */
  downloaded: number;
  /** 总字节数（解压阶段恒为空） */
  total: number | null;
  /** 当前处理文件名（仅解压阶段） */
  current: string | null;
}

let progressListening = false;

/** 订阅下载进度事件；浏览器 / 单测下动态导入失败则警告并允许下次重试 */
async function ensureProgressListener(): Promise<void> {
  if (progressListening) return;
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<ModelDownloadProgress>(PROGRESS_EVENT, (event) => {
      modelsState.progress[event.payload.id] = event.payload;
    });
    progressListening = true;
  } catch (error) {
    // 非 Tauri 环境无事件可订：经统一上报走 plugin-log，下次下载再试
    reportCommandFailure("[models] progress listener unavailable", error);
  }
}

/** 刷新模型清单；成功返回真，失败上报并返回假（页面据此渲染空态或重试） */
export async function refreshModels(): Promise<boolean> {
  const result = await commands.listModels().result();
  if (result.status === "error") {
    reportCommandFailure("[models] failed to load models", result.error);
    return false;
  }
  modelsState.entries = result.data;
  return true;
}

/** 开始下载：先订进度再发命令，避免首包事件丢失 */
export async function downloadModel(id: string): Promise<void> {
  await ensureProgressListener();
  await commands
    .downloadModel(id)
    .success(() => {
      delete modelsState.progress[id];
      toast.success(m.models_download_done());
      void refreshModels();
    })
    .failed((failure) => {
      delete modelsState.progress[id];
      toast.error(m.models_download_failed());
      reportCommandFailure("[models] download failed", failure);
      void refreshModels();
    });
}

/** 取消下载：命令结算即清进度并刷新（取消失败说明任务已结束，直接刷新） */
export async function cancelDownload(id: string): Promise<void> {
  await commands
    .cancelDownload(id)
    .success(() => {
      delete modelsState.progress[id];
      toast.success(m.models_download_cancelled());
      void refreshModels();
    })
    .failed((failure) => {
      delete modelsState.progress[id];
      reportCommandFailure("[models] cancel failed", failure);
      void refreshModels();
    });
}

/** 重新校验：手动放置文件后点此确认；失败提示用户补文件 */
export async function verifyModel(id: string): Promise<void> {
  await commands
    .verifyModel(id)
    .success((report) => {
      if (report.status === "ready") {
        toast.success(m.models_verify_done());
      } else {
        toast.error(m.models_verify_failed());
      }
      void refreshModels();
    })
    .failed((failure) => {
      toast.error(m.models_verify_failed());
      reportCommandFailure("[models] verify failed", failure);
    });
}

/** 模型文件节点：与后端 `ModelFileEntry` 同构，详情页目录树用 */
export interface ModelFileNode {
  /** 相对包顶层目录的路径，分隔符统一 `/` */
  path: string;
  /** 期望字节数（目录为所含之和） */
  size: number;
  /** 是否为目录 */
  is_dir: boolean;
  /** 核对结论 */
  status: ModelEntry["status"];
}

/** 拉取模型目录树：失败上报并回空，详情区直接隐藏该节 */
export async function fetchModelFiles(id: string): Promise<ModelFileNode[]> {
  const result = await commands.listModelFiles(id).result();
  if (result.status === "error") {
    reportCommandFailure("[models] list files failed", result.error);
    return [];
  }
  return result.data;
}

/** 删除模型目录；二次确认由调用方弹窗负责，此处只做落盘与提示 */
export async function deleteModel(id: string): Promise<boolean> {
  let deleted = false;
  await commands
    .deleteModel(id)
    .success(() => {
      deleted = true;
      toast.success(m.models_delete_done());
    })
    .failed((failure) => {
      toast.error(m.models_delete_failed());
      reportCommandFailure("[models] delete failed", failure);
    });
  if (deleted) await refreshModels();
  return deleted;
}

/** 打开模型根目录；失败提示用户手动前往 */
export async function openModelsRoot(): Promise<void> {
  await commands.openModelsRoot().failed((failure) => {
    toast.error(m.models_open_failed());
    reportCommandFailure("[models] open root failed", failure);
  });
}

/** 打开单个模型目录；失败提示用户手动前往 */
export async function openModelDir(id: string): Promise<void> {
  await commands.openModelDir(id).failed((failure) => {
    toast.error(m.models_open_failed());
    reportCommandFailure("[models] open dir failed", failure);
  });
}
