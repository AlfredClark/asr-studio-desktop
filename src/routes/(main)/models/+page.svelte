<script lang="ts">
  // 模型页组装：整页左右分栏，左侧标题 + 列表（分类可折叠）+ 打开根目录，
  // 右侧详情。条目与进度来自跨页面共享的 modelsState；挂载即刷新一次。
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

  /** 当前选中的条目：被删即为空，交回落逻辑处理 */
  const selectedEntry = $derived(
    modelsState.entries.find((entry) => entry.id === selectedId) ?? null,
  );

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

  onMount(() => {
    void refreshModels();
  });
</script>

<!-- 整页左右分栏并占满页面：左列表右详情，各自独立滚动；默认 1:1，可拖拽分隔条 -->
<div class="h-full w-full">
  <ResizablePaneGroup direction="horizontal" class="h-full gap-2">
    <ResizablePane defaultSize={50} minSize={25}>
      <div class="flex h-full flex-col gap-2 p-4">
        <h2 class="shrink-0 text-2xl font-medium">{m.models_list_title()}</h2>
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
    <ResizablePane defaultSize={50} minSize={25}>
      {#if selectedEntry}
        {#key selectedEntry.id}
          <ModelDetail entry={selectedEntry} />
        {/key}
      {:else}
        <p class="py-6 text-center text-sm text-muted-foreground">
          {m.models_detail_empty()}
        </p>
      {/if}
    </ResizablePane>
  </ResizablePaneGroup>
</div>
