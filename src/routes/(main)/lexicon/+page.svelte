<script lang="ts">
  // 词库管理页组装：左分组列表右词条表，选中态放页面局部；数据来自跨页面共享的 lexiconState。
  // 左栏分组操作与右栏词条批量/弹窗全下沉到各自组件，本页只做渲染与接线。
  import { onMount } from "svelte";
  import { Button } from "#components/shadcn-svelte/button";
  import {
    ResizableHandle,
    ResizablePane,
    ResizablePaneGroup,
  } from "#components/shadcn-svelte/resizable";
  import { m } from "#libs/i18n/paraglide/messages";
  import { lexiconState, refreshLexicon } from "#libs/hooks/lexicon.svelte";
  import GroupList from "./_components/group-list.svelte";
  import EntryTable from "./_components/entry-table.svelte";

  /** 查看中的分组 id：空则跟随首个生效分组（无则全局）；分组被删后自动回落 */
  let selectedGroupId = $state<string | null>(null);

  /** 实际展示的分组 id：查看态存在即用，否则跟随首个生效分组，无则全局 */
  const effectiveGroupId = $derived.by(() => {
    const groups = lexiconState.value.groups;
    const activeIds = lexiconState.value.active_group_ids;
    if (selectedGroupId != null && groups.some((group) => group.id === selectedGroupId)) {
      return selectedGroupId;
    }
    return (
      groups.find((group) => !group.is_global && activeIds.includes(group.id))?.id ??
      groups.find((group) => group.is_global)?.id ??
      ""
    );
  });

  /** 首次水合中；完成前展示加载态而非空态，避免误导 */
  let loading = $state(true);
  /** 首次水合失败；展示错误与重试入口 */
  let loadFailed = $state(false);

  /** 拉取词库快照：挂载与重试共用，失败切错误态 */
  async function load(): Promise<void> {
    loading = true;
    loadFailed = false;
    try {
      loadFailed = !(await refreshLexicon());
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<!-- 整页左右分栏：左分组右词条，各自独立滚动；加载中与失败态优先于内容 -->
<div class="h-full w-full" data-testid="lexicon-page">
  {#if loading}
    <div class="flex h-full items-center justify-center">
      <p class="text-sm text-muted-foreground">{m.lexicon_loading()}</p>
    </div>
  {:else if loadFailed}
    <div class="flex h-full flex-col items-center justify-center gap-2">
      <p class="text-sm text-muted-foreground">{m.lexicon_load_failed()}</p>
      <Button size="sm" onclick={() => void load()}>{m.lexicon_retry()}</Button>
    </div>
  {:else}
    <ResizablePaneGroup direction="horizontal" class="h-full gap-2">
      <ResizablePane defaultSize={32} minSize={25}>
        <GroupList selectedId={effectiveGroupId} onSelect={(id) => (selectedGroupId = id)} />
      </ResizablePane>
      <ResizableHandle withHandle />
      <ResizablePane defaultSize={68} minSize={40}>
        <EntryTable groupId={effectiveGroupId} />
      </ResizablePane>
    </ResizablePaneGroup>
  {/if}
</div>
