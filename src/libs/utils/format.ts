// 通用格式化工具：不含业务与应用知识，纯函数可单测。

/** 字节数展示：GB/MB 保留一位小数，小于 1MB 显示 KB（最小 1KB，避免 0 字节误读为空） */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1000))} KB`;
}
