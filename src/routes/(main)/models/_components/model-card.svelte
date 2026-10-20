<script lang="ts">
  // 单个模型卡片：状态徽标 + 大小 + 操作按钮组 + 下载进度条。
  // 下载中禁用互斥操作；删除走二次确认弹窗；前往下载页经 openExternal 网关。
  import ConfirmDialog from "#components/common/confirm-dialog.svelte";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
  } from "#components/shadcn-svelte/card";
  import { Progress } from "#components/shadcn-svelte/progress";
  import type { ModelEntry } from "#libs/commands/types";
  import {
    cancelDownload,
    deleteModel,
    downloadModel,
    modelsState,
    openModelDir,
    verifyModel,
  } from "#libs/hooks/models.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import { openExternal } from "#libs/utils/opener";

  /** 卡片绑定的模型条目（后端 `list_models` 的写后值为准） */
  let { entry }: { entry: ModelEntry } = $props();

  /** 删除确认弹窗开关 */
  let confirmOpen = $state(false);

  /** 删除在途时禁用按钮，避免重复提交 */
  let deleting = $state(false);

  // 下载进度（本卡片）：事件推送写全局表，此处只读
  const progress = $derived(modelsState.progress[entry.id]);
  // 下载中：后端事件驱动，前端不做本地推断
  const busy = $derived(entry.status === "downloading" || progress !== undefined);

  /** 状态徽标文案：四态一一对应，无回落 */
  function statusLabel(status: ModelEntry["status"]): string {
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
  function formatBytes(bytes: number): string {
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    return `${Math.max(1, Math.round(bytes / 1000))} KB`;
  }

  /** 进度百分比：解压阶段总数未知，固定转圈不定值展示 */
  const percent = $derived.by(() => {
    if (!progress || progress.total == null) return null;
    if (progress.total === 0) return 0;
    return Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
  });

  /** 阶段文案：下载给百分比，解压给当前文件名 */
  const phaseText = $derived.by(() => {
    if (!progress) return null;
    if (progress.phase === "extract") {
      const current = progress.current ? ` · ${progress.current}` : "";
      return `${m.models_phase_extract()}${current}`;
    }
    const suffix =
      progress.total != null && percent != null
        ? ` ${percent}% · ${formatBytes(progress.downloaded)} / ${formatBytes(progress.total)}`
        : ` ${formatBytes(progress.downloaded)}`;
    return `${m.models_phase_download()}${suffix}`;
  });

  /** 前往下载页：经网关校验 scheme，失败静默（用户已在当前页） */
  async function handleOpenPage(): Promise<void> {
    await openExternal(entry.page_url);
  }

  /** 确认删除：弹窗已由通用组件关闭，失败不刷新（后端没动） */
  async function handleDelete(): Promise<void> {
    if (deleting) return;
    deleting = true;
    try {
      await deleteModel(entry.id);
    } finally {
      deleting = false;
    }
  }
</script>

<Card>
  <CardHeader>
    <div class="flex items-center justify-between gap-2">
      <CardTitle>{entry.display_name}</CardTitle>
      <Badge variant={entry.status === "ready" ? "default" : "secondary"}>
        {statusLabel(progress !== undefined ? "downloading" : entry.status)}
      </Badge>
    </div>
    <CardDescription>
      {formatBytes(entry.tarball_bytes)}
    </CardDescription>
  </CardHeader>
  <CardContent>
    <div class="flex flex-wrap items-center gap-2">
      {#if busy}
        <Button variant="outline" size="sm" onclick={() => void cancelDownload(entry.id)}>
          {m.models_cancel()}
        </Button>
      {:else if entry.status === "ready"}
        <Button variant="outline" size="sm" onclick={() => void verifyModel(entry.id)}>
          {m.models_verify()}
        </Button>
        <Button variant="outline" size="sm" onclick={() => void openModelDir(entry.id)}>
          {m.models_open_dir()}
        </Button>
        <Button
          variant="destructive"
          size="sm"
          disabled={deleting}
          onclick={() => (confirmOpen = true)}
        >
          {m.models_delete()}
        </Button>
      {:else}
        <Button size="sm" onclick={() => void downloadModel(entry.id)}>
          {m.models_download()}
        </Button>
        {#if entry.status === "partial"}
          <Button variant="outline" size="sm" onclick={() => void verifyModel(entry.id)}>
            {m.models_verify()}
          </Button>
        {/if}
        <Button variant="outline" size="sm" onclick={() => void openModelDir(entry.id)}>
          {m.models_open_dir()}
        </Button>
        <Button variant="link" size="sm" onclick={() => void handleOpenPage()}>
          {m.models_open_page()}
        </Button>
      {/if}
    </div>
    {#if progress}
      <div class="mt-3 space-y-1">
        {#if percent != null}
          <Progress value={percent} max={100} aria-label={phaseText ?? ""} />
        {/if}
        {#if phaseText}
          <p class="text-xs text-muted-foreground">{phaseText}</p>
        {/if}
      </div>
    {/if}
  </CardContent>
</Card>

<ConfirmDialog
  bind:open={confirmOpen}
  title={m.models_delete_confirm_title()}
  description={m.models_delete_confirm_description()}
  cancelLabel={m.settings_close_confirm_cancel()}
  confirmLabel={m.models_delete_confirm_ok()}
  variant="destructive"
  onConfirm={() => void handleDelete()}
/>
