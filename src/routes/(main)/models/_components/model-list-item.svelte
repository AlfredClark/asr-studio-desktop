<script lang="ts">
  // 模型列表行：名称 + 体积 + 状态短文案，选中态 Steel 底加中字重（沿用侧栏导航项语言）。
  // 下载进度只读全局表，不自己订事件。
  import { Button } from "#components/shadcn-svelte/button";
  import { cn } from "#libs/utils/shadcn-svelte";
  import type { ModelEntry } from "#libs/commands/types";
  import { m } from "#libs/i18n/paraglide/messages";
  import { modelsState } from "#libs/hooks/models.svelte";
  import { formatBytes, modelStatusLabel } from "./model-status";

  /** 行绑定的模型条目 */
  let {
    entry,
    selected,
    onSelect,
  }: { entry: ModelEntry; selected: boolean; onSelect: () => void } = $props();

  /** 下载中：后端事件驱动，前端不做本地推断 */
  const downloading = $derived(modelsState.progress[entry.id] !== undefined);
</script>

<Button
  variant={selected ? "secondary" : "ghost"}
  class={cn("w-full justify-start gap-2 font-normal", selected && "font-medium")}
  aria-current={selected ? "true" : undefined}
  onclick={onSelect}
>
  <span class="min-w-0 flex-1 truncate text-left">{entry.display_name}</span>
  <span class="shrink-0 text-xs text-muted-foreground tabular-nums">
    {#if entry.local_bytes > 0}
      {formatBytes(entry.local_bytes)}
    {:else if entry.tarball_bytes > 0}
      {formatBytes(entry.tarball_bytes)}
    {:else}
      {m.models_manual_size()}
    {/if}
  </span>
  <span class="shrink-0 text-xs text-muted-foreground">
    {modelStatusLabel(entry.status, downloading)}
  </span>
</Button>
