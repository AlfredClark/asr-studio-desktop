<script lang="ts">
  // 模型详情：标题 + 徽标 + 外链、简介、结构化元信息（体积 / 位置可复制），上方滚动，操作按钮沉底。
  // 操作按状态收敛：下载中禁用互斥操作；missing 只有下载；partial 可删残留；ready 主行动去服务页。
  // 删除走二次确认弹窗；前往下载页经网关。目录树默认只展顶层，非就绪节点才挂徽标。
  import { SvelteSet } from "svelte/reactivity";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import ConfirmDialog from "#components/common/confirm-dialog.svelte";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import ExternalLinkIcon from "@lucide/svelte/icons/external-link";
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
    setCustomModelDir,
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
  const categoryIntros: Record<ModelCategory, () => string> = {
    funasr_nano: () => m.models_intro_funasr_nano(),
    qwen3_asr: () => m.models_intro_qwen3_asr(),
    sense_voice: () => m.models_intro_sense_voice(),
  };

  /** 按模型 id 的简介：同分类多版本在此区分并标注标点支持，未命中回落分类简介 */
  function modelIntro(): string {
    switch (entry.id) {
      case "funasr-nano-fp16":
        return m.models_intro_funasr_nano_fp16();
      case "funasr-nano-fp32":
        return m.models_intro_funasr_nano_fp32();
      case "qwen3-1.7b-int8":
        return m.models_intro_qwen3_1p7b();
      case "sense-voice-yue-int8":
        return m.models_intro_sense_voice_yue_int8();
      case "sense-voice-fp32":
        return m.models_intro_sense_voice_fp32();
      case "sense-voice-yue-fp32":
        return m.models_intro_sense_voice_yue_fp32();
      default:
        return categoryIntros[entry.category]();
    }
  }

  /** 删除确认弹窗开关 */
  let confirmOpen = $state(false);

  /** 删除在途时禁用按钮，避免重复提交 */
  let deleting = $state(false);

  /** 目录树：随条目与状态刷新，失败回空（该节隐藏） */
  let files = $state<ModelFileNode[]>([]);

  /** 收起的目录路径集合：本身响应式，深层目录默认收起，切模型即重算 */
  let collapsed = new SvelteSet<string>();

  // 下载进度（本模型）：事件推送写全局表，此处只读
  const progress = $derived(modelsState.progress[entry.id]);
  // 下载中：后端事件驱动，前端不做本地推断
  const busy = $derived(entry.status === "downloading" || progress !== undefined);

  /** 是否命中用户指定目录：隐藏删除与下载，按钮矩阵据此分支 */
  const isCustom = $derived(entry.custom_dir != null);

  /** 路径行展示值：指定目录优先（缺失态 `local_path` 为空时仍有处可看） */
  const shownPath = $derived(entry.custom_dir ?? entry.local_path);

  /** 未就绪文件数：目录树组头一次说清，避免逐行徽标 */
  const pendingCount = $derived(files.filter((file) => file.status !== "ready").length);

  /** 进度百分比：解压阶段总数未知，转不定值条展示 */
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

  // 条目或状态变化即重拉：竞态用取消标记拦截，旧请求不覆盖新结果；
  // 深层目录默认收起，顶层保持展开（渐进披露）
  $effect(() => {
    const id = entry.id;
    void entry.status;
    collapsed.clear();
    let cancelled = false;
    void fetchModelFiles(id).then((rows) => {
      if (cancelled) return;
      collapsed.clear();
      for (const row of rows) {
        if (row.is_dir && row.path.includes("/")) collapsed.add(row.path);
      }
      files = rows;
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

  /** 复制安装路径：剪贴板不可用（如非安全上下文）即提示失败 */
  async function handleCopyPath(): Promise<void> {
    const path = shownPath;
    if (!path) return;
    try {
      await navigator.clipboard.writeText(path);
      toast.success(m.models_path_copied());
    } catch {
      toast.error(m.models_path_copy_failed());
    }
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

  /** 指定本地目录：弹系统目录选择器，取消则静默返回；落盘与提示由 hook 负责 */
  async function handlePickCustomDir(): Promise<void> {
    let selected: string | string[] | null;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      selected = await open({ directory: true, multiple: false });
    } catch {
      toast.error(m.models_pick_dir_failed());
      return;
    }
    if (typeof selected !== "string") return;
    await setCustomModelDir(entry.id, selected);
  }

  /** 取消指定：仅解绑映射、回归应用管理，不碰用户文件 */
  async function handleClearCustomDir(): Promise<void> {
    await setCustomModelDir(entry.id, null);
  }
</script>

<div data-testid="model-detail" class="flex h-full flex-col gap-4 p-4">
  <ScrollArea class="min-h-0 flex-1">
    <div class="flex flex-col gap-3 pr-1">
      <div class="flex items-center gap-2">
        <h2 class="min-w-0 flex-1 truncate text-xl font-medium">{entry.display_name}</h2>
        <Badge variant={entry.status === "ready" ? "default" : "secondary"}>
          {modelStatusLabel(entry.status, progress !== undefined)}
        </Badge>
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={() => void handleOpenPage()}
          aria-label={m.models_open_page()}
          title={m.models_open_page()}
        >
          <ExternalLinkIcon />
        </Button>
      </div>
      <p class="text-sm text-muted-foreground">{modelIntro()}</p>
      <div class="flex flex-col gap-1.5 rounded-lg bg-muted/50 p-2.5 text-xs">
        <div class="flex items-center justify-between gap-2">
          <span class="shrink-0 text-muted-foreground">{m.models_detail_size_label()}</span>
          {#if entry.manual}
            <span>{m.models_manual_size()}</span>
          {:else}
            <span class="tabular-nums">{formatBytes(entry.tarball_bytes)}</span>
          {/if}
        </div>
        {#if shownPath}
          <div class="flex items-center gap-2">
            <span class="shrink-0 text-muted-foreground">{m.models_detail_path_label()}</span>
            <span class="min-w-0 flex-1 truncate text-right" title={shownPath}>
              {shownPath}
            </span>
            <Button
              variant="ghost"
              size="icon-xs"
              onclick={() => void handleCopyPath()}
              aria-label={m.models_copy_path()}
              title={m.models_copy_path()}
            >
              <CopyIcon />
            </Button>
          </div>
        {/if}
        {#if entry.status === "partial"}
          <p class="text-muted-foreground">{m.models_partial_hint()}</p>
        {/if}
        {#if entry.manual && entry.status !== "ready"}
          <p class="text-muted-foreground">{m.models_manual_hint()}</p>
        {/if}
        {#if isCustom && entry.status !== "ready"}
          <p class="text-muted-foreground">{m.models_custom_hint()}</p>
        {/if}
      </div>
      {#if files.length > 0}
        <div>
          <div class="flex items-baseline gap-2">
            <p class="text-xs font-medium text-muted-foreground">{m.models_detail_tree()}</p>
            {#if pendingCount > 0}
              <span class="text-xs text-muted-foreground tabular-nums">
                {m.models_tree_pending({ count: pendingCount })}
              </span>
            {/if}
          </div>
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
                      class="flex h-8 w-full items-center gap-2 rounded-md pr-1 text-left text-sm hover:bg-accent"
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
                      {#if file.status !== "ready"}
                        <Badge variant={badgeVariant(file.status)}>
                          {modelStatusLabel(file.status, false)}
                        </Badge>
                      {/if}
                    </button>
                  {:else}
                    <div
                      class="flex h-8 items-center gap-2 pr-1 text-sm"
                      style={`padding-left: ${depth * 16 + 24}px`}
                    >
                      <FileIcon class="size-4 shrink-0 text-muted-foreground" />
                      <span class="min-w-0 flex-1 truncate">{name}</span>
                      <span class="shrink-0 text-xs text-muted-foreground tabular-nums">
                        {formatBytes(file.size)}
                      </span>
                      {#if file.status !== "ready"}
                        <Badge variant={badgeVariant(file.status)}>
                          {modelStatusLabel(file.status, false)}
                        </Badge>
                      {/if}
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
        {:else}
          <div
            class="h-1 w-full overflow-hidden rounded-full bg-muted"
            role="progressbar"
            aria-label={phaseText ?? m.models_phase_extract()}
          >
            <div class="h-full w-1/3 animate-pulse rounded-full bg-primary"></div>
          </div>
        {/if}
        {#if phaseText}
          <p class="text-xs text-muted-foreground">{phaseText}</p>
        {/if}
        {#if progress.phase === "extract"}
          <p class="text-xs text-muted-foreground">{m.models_extract_wait()}</p>
        {/if}
      </div>
    {/if}
    <div class="flex flex-wrap items-center gap-2">
      {#if busy}
        <Button variant="outline" size="sm" onclick={() => void cancelDownload(entry.id)}>
          {m.models_cancel()}
        </Button>
      {:else if entry.status === "ready"}
        <Button
          variant="outline"
          size="sm"
          title={m.models_verify_hint()}
          onclick={() => void verifyModel(entry.id)}
        >
          {m.models_verify()}
        </Button>
        <Button variant="outline" size="sm" onclick={() => void openModelDir(entry.id)}>
          {m.models_open_dir()}
        </Button>
        <Button variant="outline" size="sm" onclick={() => void handlePickCustomDir()}>
          {isCustom ? m.models_change_custom_dir() : m.models_set_custom_dir()}
        </Button>
        {#if isCustom}
          <Button variant="outline" size="sm" onclick={() => void handleClearCustomDir()}>
            {m.models_clear_custom_dir()}
          </Button>
        {:else}
          <Button
            variant="destructive"
            size="sm"
            disabled={deleting}
            onclick={() => (confirmOpen = true)}
          >
            {m.models_delete()}
          </Button>
        {/if}
      {:else if entry.status === "partial"}
        {#if entry.manual || isCustom}
          <Button variant="outline" size="sm" onclick={() => void openModelDir(entry.id)}>
            {m.models_open_dir()}
          </Button>
        {:else}
          <Button size="sm" onclick={() => void downloadModel(entry.id)}>
            {m.models_download()}
          </Button>
        {/if}
        <Button
          variant="outline"
          size="sm"
          title={m.models_verify_hint()}
          onclick={() => void verifyModel(entry.id)}
        >
          {m.models_verify()}
        </Button>
        {#if !entry.manual && !isCustom}
          <Button variant="outline" size="sm" onclick={() => void openModelDir(entry.id)}>
            {m.models_open_dir()}
          </Button>
        {/if}
        <Button variant="outline" size="sm" onclick={() => void handlePickCustomDir()}>
          {isCustom ? m.models_change_custom_dir() : m.models_set_custom_dir()}
        </Button>
        {#if isCustom}
          <Button variant="outline" size="sm" onclick={() => void handleClearCustomDir()}>
            {m.models_clear_custom_dir()}
          </Button>
        {:else}
          <Button
            variant="destructive"
            size="sm"
            disabled={deleting}
            onclick={() => (confirmOpen = true)}
          >
            {m.models_delete()}
          </Button>
        {/if}
      {:else if entry.manual || isCustom}
        <Button variant="outline" size="sm" onclick={() => void openModelDir(entry.id)}>
          {m.models_open_dir()}
        </Button>
        <Button
          variant="outline"
          size="sm"
          title={m.models_verify_hint()}
          onclick={() => void verifyModel(entry.id)}
        >
          {m.models_verify()}
        </Button>
        {#if isCustom}
          <Button variant="outline" size="sm" onclick={() => void handlePickCustomDir()}>
            {m.models_change_custom_dir()}
          </Button>
          <Button variant="outline" size="sm" onclick={() => void handleClearCustomDir()}>
            {m.models_clear_custom_dir()}
          </Button>
        {/if}
      {:else}
        <Button size="sm" onclick={() => void downloadModel(entry.id)}>
          {m.models_download()}
        </Button>
        <Button variant="outline" size="sm" onclick={() => void handlePickCustomDir()}>
          {m.models_set_custom_dir()}
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
