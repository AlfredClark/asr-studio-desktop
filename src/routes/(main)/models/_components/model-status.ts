// 模型页专用的展示 helpers：状态文案与字节格式化，列表行与详情共用。
// 纯展示逻辑放此处， hooks 只做状态与命令。
import { m } from "#libs/i18n/paraglide/messages";
import type { ModelEntry } from "#libs/commands/types";

/** 状态短文案：下载中优先于条目态（事件推送 arrival 先于命令结算刷新） */
export function modelStatusLabel(status: ModelEntry["status"], downloading: boolean): string {
  if (downloading) return m.models_status_downloading();
  switch (status) {
    case "ready":
      return m.models_status_ready();
    case "partial":
      return m.models_status_partial();
    case "downloading":
      return m.models_status_downloading();
    default:
      return m.models_status_missing();
  }
}

/** 字节数展示：MB 保留一位小数，GB 同理，小于 1MB 显示 KB */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1000))} KB`;
}
