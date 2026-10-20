<script lang="ts">
  // 模型详情：标题 + 徽标、简介、大小与目录、目录树，上方滚动，操作按钮沉底。
  // 操作行为与旧卡片一致：下载中禁用互斥操作；删除走二次确认弹窗；前往下载页经网关。
  import { SvelteSet } from "svelte/reactivity";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import ConfirmDialog from "#components/common/confirm-dialog.svelte";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import FileIcon from "@lucide/svelte/icons/file";
  import { Progress } from "#components/shadcn-svelte/progress";
  import { cn } from "#libs/utils/shadcn-svelte";
  import type { ModelCategory, ModelEntry } from "#libs/commands/types";
  import {
    cancelDownload,
    deleteModel,
    downloadModel,
    fetchModelFiles,
    modelsState,
    openModelDir,
    verifyModel,
    type ModelFileNode,
  } from "#libs/hooks/models.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import { openExternal } from "#libs/utils/opener";
  import { toast } from "#libs/utils/toast";
  import { formatBytes, modelStatusLabel } from "./model-status";

  /** 详情绑定的模型条目（后端 `list_models` 的写后值为准） */
  let { entry }: { entry: ModelEntry } = $props();

  /** 分类到简介文案的映射：新增模型同步加条目与两份文案 */
  const intros: Record<ModelCategory, () => string> = {
    funasr_nano: () => m.models_intro_funasr_nano(),
    qwen3_asr: () => m.models_intro_qwen3_asr(),
    sense_voice: () => m.models_intro_sense_voice(),
  };

  /** 删除确认弹窗开关 */
  let confirmOpen = $state(false);

  /** 删除在途时禁用按钮，避免重复提交 */
  let deleting = $state(false);

  /** 目录树：随条目与状态刷新，失败回空（该节隐藏） */
  let files = $state<ModelFileNode[]>([]);

  /** 收起的目录路径集合：本身响应式，默认全展开，切模型即清空 */
  let collapsed = new SvelteSet<string>();

  // 下载进度（本模型）：事件推送写全局表，此处只读
  const progress = $derived(modelsState.progress[entry.id]);
  // 下载中：后端事件驱动，前端不做本地推断
  const busy = $derived(entry.status === "downloading" || progress !== undefined);

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

  // 条目或状态变化即重拉：竞态用取消标记拦截，旧请求不覆盖新结果
  $effect(() => {
    const id = entry.id;
    void entry.status;
    collapsed.clear();
    let cancelled = false;
    void fetchModelFiles(id).then((rows) => {
      if (!cancelled) files = rows;
    });
    return () => {
      cancelled = true;
    };
  });

  /** 目录收起切换 */
  function toggleDir(path: string): void {
    if (collapsed.has(path)) collapsed.delete(path);
    else collapsed.add(path);
  }

  /** 行是否被某层收起的祖先隐藏 */
  function isHidden(path: string): boolean {
    let index = path.indexOf("/");
    while (index !== -1) {
      if (collapsed.has(path.slice(0, index))) return true;
      index = path.indexOf("/", index + 1);
    }
    return false;
  }

  /** 状态徽标样式：就绪实心，其余静音（沿用列表徽标语言） */
  function badgeVariant(status: ModelEntry["status"]): "default" | "secondary" {
    return status === "ready" ? "default" : "secondary";
  }

  /** 前往下载页：打不开弹提示，不再静默（网关在浏览器预览等环境下不可用） */
  async function handleOpenPage(): Promise<void> {
    if (!(await openExternal(entry.page_url))) toast.error(m.models_open_page_failed());
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

<div data-testid="model-detail" class="flex h-full flex-col gap-4 p-4">
  <ScrollArea class="min-h-0 flex-1">
    <div class="flex flex-col gap-3 pr-1">
      <div class="flex items-center justify-between gap-2">
        <h2 class="text-2xl font-medium">{entry.display_name}</h2>
        <Badge variant={entry.status === "ready" ? "default" : "secondary"}>
          {modelStatusLabel(entry.status, progress !== undefined)}
        </Badge>
      </div>
      <p class="text-sm text-muted-foreground">{intros[entry.category]()}</p>
      {#if entry.local_path}
        <p class="text-xs break-all text-muted-foreground">{entry.local_path}</p>
      {/if}
      <p class="text-xs text-muted-foreground">{formatBytes(entry.tarball_bytes)}</p>
      {#if files.length > 0}
        <div>
          <p class="text-xs font-medium text-muted-foreground">{m.models_detail_tree()}</p>
          <ul class="mt-1 flex flex-col">
            {#each files as file (file.path)}
              {@const segments = file.path.split("/")}
              {@const depth = segments.length - 1}
              {@const name = segments[segments.length - 1] ?? file.path}
              {#if !isHidden(file.path)}
                <li>
                  {#if file.is_dir}
                    <button
                      type="button"
                      class="flex h-7 w-full items-center gap-2 rounded-md pr-1 text-left text-sm hover:bg-accent"
                      style={`padding-left: ${depth * 16}px`}
                      aria-expanded={!collapsed.has(file.path)}
                      onclick={() => toggleDir(file.path)}
                    >
                      <ChevronDownIcon
                        class={cn(
                          "size-4 shrink-0 text-muted-foreground transition-transform duration-100",
                          collapsed.has(file.path) && "-rotate-90",
                        )}
                      />
                      <span class="min-w-0 flex-1 truncate font-medium">{name}</span>
                      <span class="shrink-0 text-xs text-muted-foreground tabular-nums">
                        {formatBytes(file.size)}
                      </span>
                      <Badge variant={badgeVariant(file.status)}>
                        {modelStatusLabel(file.status, false)}
                      </Badge>
                    </button>
                  {:else}
                    <div
                      class="flex h-7 items-center gap-2 pr-1 text-sm"
                      style={`padding-left: ${depth * 16 + 24}px`}
                    >
                      <FileIcon class="size-4 shrink-0 text-muted-foreground" />
                      <span class="min-w-0 flex-1 truncate">{name}</span>
                      <span class="shrink-0 text-xs text-muted-foreground tabular-nums">
                        {formatBytes(file.size)}
                      </span>
                      <Badge variant={badgeVariant(file.status)}>
                        {modelStatusLabel(file.status, false)}
                      </Badge>
                    </div>
                  {/if}
                </li>
              {/if}
            {/each}
          </ul>
        </div>
      {/if}
    </div>
  </ScrollArea>
  <div class="shrink-0 border-t pt-3">
    {#if progress}
      <div class="mb-2 space-y-1">
        {#if percent != null}
          <Progress value={percent} max={100} aria-label={phaseText ?? ""} />
        {/if}
        {#if phaseText}
          <p class="text-xs text-muted-foreground">{phaseText}</p>
        {/if}
      </div>
    {/if}
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
        <Button variant="link" size="sm" class="ml-auto" onclick={() => void handleOpenPage()}>
          {m.models_open_page()}
        </Button>
      {/if}
    </div>
  </div>
</div>

<ConfirmDialog
  bind:open={confirmOpen}
  title={m.models_delete_confirm_title()}
  description={m.models_delete_confirm_description()}
  cancelLabel={m.settings_close_confirm_cancel()}
  confirmLabel={m.models_delete_confirm_ok()}
  variant="destructive"
  onConfirm={() => void handleDelete()}
/>
