// 模型页专用的展示 helpers：状态文案，列表行与详情共用。
// 纯展示逻辑放此处， hooks 只做状态与命令。
import { m } from "#libs/i18n/paraglide/messages";
import type { ModelEntry } from "#libs/commands/types";

// 字节格式化见 `#libs/utils/format`（通用工具，首页磁盘统计复用）。
export { formatBytes } from "#libs/utils/format";

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
