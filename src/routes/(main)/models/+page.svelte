<script lang="ts">
  // 模型页组装：整页左右分栏，左侧标题 + 在途胶囊 + 列表（分类可折叠）+ 打开根目录，
  // 右侧详情。条目与进度来自跨页面共享的 modelsState；挂载即加载一次。
  // 选中态放页面局部：数据回来默认选首个，选中被删回落，下载完成刷新不抢选中。
  import { onMount } from "svelte";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { Button } from "#components/shadcn-svelte/button";
  import {
    ResizableHandle,
    ResizablePane,
    ResizablePaneGroup,
  } from "#components/shadcn-svelte/resizable";
  import { m } from "#libs/i18n/paraglide/messages";
  import type { ModelCategory } from "#libs/commands/types";
  import { modelsState, openModelsRoot, refreshModels } from "#libs/hooks/models.svelte";
  import ModelDetail from "./_components/model-detail.svelte";
  import ModelGroup from "./_components/model-group.svelte";
  import ModelListItem from "./_components/model-list-item.svelte";

  /** 分组顺序即列表渲染顺序，与 `ModelCategory` 的注册顺序无关 */
  const categories: { key: ModelCategory; title: () => string }[] = [
    { key: "funasr_nano", title: () => m.models_category_funasr_nano() },
    { key: "qwen3_asr", title: () => m.models_category_qwen3_asr() },
    { key: "sense_voice", title: () => m.models_category_sense_voice() },
  ];

  /** 当前选中的模型 id：只放会话内存，不落盘 */
  let selectedId = $state<string | null>(null);

  /** 清单加载态：失败时左栏给重试入口，右栏区分“未选中”与“加载失败” */
  let loadState = $state<"loading" | "ready" | "error">("loading");

  /** 当前选中的条目：被删即为空，交回落逻辑处理 */
  const selectedEntry = $derived(
    modelsState.entries.find((entry) => entry.id === selectedId) ?? null,
  );

  /** 在途下载（全局进度表首个）：切页不中断，切回本页经胶囊一眼找回 */
  const activeId = $derived(Object.keys(modelsState.progress)[0]);
  const activeEntry = $derived(modelsState.entries.find((entry) => entry.id === activeId));
  const activeProgress = $derived(activeId ? modelsState.progress[activeId] : undefined);
  const activePercent = $derived.by(() => {
    if (!activeProgress || activeProgress.total == null || activeProgress.total === 0) return null;
    return Math.min(100, Math.round((activeProgress.downloaded / activeProgress.total) * 100));
  });

  // 选中兜底：读条目写选中，两者不是同一批状态，不形成回路
  $effect(() => {
    const entries = modelsState.entries;
    if (selectedId != null && entries.some((entry) => entry.id === selectedId)) return;
    for (const { key } of categories) {
      const hit = entries.find((entry) => entry.category === key);
      if (hit !== undefined) {
        selectedId = hit.id;
        return;
      }
    }
    selectedId = null;
  });

  /** 首屏与重试共用：`refreshModels` 成功判真，失败左栏给重试而非静默空态 */
  async function load(): Promise<void> {
    loadState = "loading";
    loadState = (await refreshModels()) ? "ready" : "error";
  }

  onMount(() => {
    void load();
  });
</script>

<!-- 整页左右分栏并占满页面：左列表右详情，各自独立滚动；详情更宽，左栏可窄到 30% -->
<div class="h-full w-full">
  <ResizablePaneGroup direction="horizontal" class="h-full gap-2">
    <ResizablePane defaultSize={40} minSize={30}>
      <div class="flex h-full flex-col gap-2 p-4">
        <div class="flex shrink-0 items-baseline justify-between gap-2">
          <h2 class="text-sm font-medium">{m.models_list_title()}</h2>
          <span class="text-xs text-muted-foreground tabular-nums">
            {m.models_list_count({ count: modelsState.entries.length })}
          </span>
        </div>
        <p class="shrink-0 text-xs text-muted-foreground">{m.models_description()}</p>
        {#if activeProgress && activeId}
          <button
            type="button"
            class="flex shrink-0 items-center gap-2 rounded-lg bg-muted/60 px-2.5 py-1.5 text-left text-xs hover:bg-muted"
            onclick={() => (selectedId = activeId)}
            aria-label={m.models_active_title()}
          >
            <span class="size-1.5 shrink-0 animate-pulse rounded-full bg-primary" aria-hidden="true"
            ></span>
            <span class="font-medium">{m.models_active_title()}</span>
            <span class="min-w-0 flex-1 truncate text-muted-foreground">
              {activeEntry?.display_name ?? activeId}
            </span>
            {#if activePercent != null}
              <span class="shrink-0 tabular-nums">{activePercent}%</span>
            {/if}
            <span class="shrink-0 underline underline-offset-4">{m.models_active_view()}</span>
          </button>
        {/if}
        {#if loadState === "loading" && modelsState.entries.length === 0}
          <div
            class="flex min-h-0 flex-1 flex-col gap-2"
            role="status"
            aria-label={m.models_list_loading()}
          >
            <div class="h-9 animate-pulse rounded-md bg-muted/60"></div>
            <div class="h-9 animate-pulse rounded-md bg-muted/60"></div>
            <div class="h-9 animate-pulse rounded-md bg-muted/60"></div>
            <span class="sr-only">{m.models_list_loading()}</span>
          </div>
        {:else if loadState === "error" && modelsState.entries.length === 0}
          <div class="flex min-h-0 flex-1 flex-col items-start justify-center gap-2" role="alert">
            <p class="text-sm text-muted-foreground">{m.models_list_load_failed()}</p>
            <Button variant="outline" size="sm" onclick={() => void load()}>
              {m.models_retry()}
            </Button>
          </div>
        {:else}
          <ScrollArea class="min-h-0 flex-1">
            <div class="flex flex-col gap-4 pr-1">
              {#each categories as { key, title } (key)}
                {@const entries = modelsState.entries.filter((entry) => entry.category === key)}
                {#if entries.length > 0}
                  <ModelGroup title={title()} count={entries.length}>
                    {#each entries as entry (entry.id)}
                      <ModelListItem
                        {entry}
                        selected={entry.id === selectedId}
                        onSelect={() => (selectedId = entry.id)}
                      />
                    {/each}
                  </ModelGroup>
                {/if}
              {/each}
            </div>
          </ScrollArea>
        {/if}
        <Button
          variant="outline"
          size="sm"
          class="w-full shrink-0"
          onclick={() => void openModelsRoot()}
        >
          {m.models_open_root()}
        </Button>
      </div>
    </ResizablePane>
    <ResizableHandle withHandle />
    <ResizablePane defaultSize={60} minSize={35}>
      {#if selectedEntry}
        {#key selectedEntry.id}
          <ModelDetail entry={selectedEntry} />
        {/key}
      {:else if loadState === "loading"}
        <div class="flex h-full items-center justify-center p-4" role="status">
          <p class="text-sm text-muted-foreground">{m.models_list_loading()}</p>
        </div>
      {:else if loadState === "error"}
        <div class="flex h-full flex-col items-center justify-center gap-2 p-4" role="alert">
          <p class="text-sm text-muted-foreground">{m.models_list_load_failed()}</p>
          <Button variant="outline" size="sm" onclick={() => void load()}>
            {m.models_retry()}
          </Button>
        </div>
      {:else}
        <div class="flex h-full flex-col items-center justify-center gap-1 p-4 text-center">
          <p class="text-sm font-medium">{m.models_detail_empty_title()}</p>
          <p class="max-w-64 text-xs text-muted-foreground">{m.models_detail_empty_hint()}</p>
        </div>
      {/if}
    </ResizablePane>
  </ResizablePaneGroup>
</div>
