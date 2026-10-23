<script lang="ts">
  // 识别记录面板（左栏）：会话级统计三格 + 记录列表（点击展开全文）。
  // 展开集合是纯前端局部态；数据来自跨页面共享的 serviceState。
  import { SvelteSet } from "svelte/reactivity";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import RecognitionRecordItem from "#components/common/recognition-record-item.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import { serviceState, summarizeHistory } from "#libs/hooks/service.svelte";

  /** 左栏统计：识别条数 / 字符总数 / 推理耗时均值，会话级，重启即清零 */
  const stats = $derived(summarizeHistory(serviceState.history));

  /** 推理耗时均值展示（秒，1 位小数；空历史显示 0） */
  const avgText = $derived(stats.avgInferMs == null ? "0" : (stats.avgInferMs / 1000).toFixed(1));

  /** 展开全文的记录 id 集合：纯前端局部态 */
  const expanded = new SvelteSet<number>();

  /** 记录展开切换 */
  function toggleRecord(id: number): void {
    if (expanded.has(id)) expanded.delete(id);
    else expanded.add(id);
  }
</script>

<div class="flex h-full flex-col gap-2 p-4">
  <div class="grid shrink-0 grid-cols-3 gap-2" data-testid="service-stats">
    <div class="text-center">
      <p class="text-2xl font-medium tabular-nums">{stats.count}</p>
      <p class="text-xs text-muted-foreground">{m.service_stats_count()}</p>
    </div>
    <div class="text-center">
      <p class="text-2xl font-medium tabular-nums">{stats.totalChars}</p>
      <p class="text-xs text-muted-foreground">{m.service_stats_chars()}</p>
    </div>
    <div class="text-center">
      <p class="text-2xl font-medium tabular-nums">{avgText} s</p>
      <p class="text-xs text-muted-foreground">{m.service_stats_avg_time()}</p>
    </div>
  </div>
  <h2 class="shrink-0 text-sm font-medium">{m.service_history_title()}</h2>
  <p class="shrink-0 text-xs text-muted-foreground">
    {m.service_history_session_hint()}
  </p>
  {#if serviceState.history.length === 0}
    <div class="flex min-h-0 flex-1 items-center justify-center">
      <p class="text-sm text-muted-foreground">{m.service_history_empty()}</p>
    </div>
  {:else}
    <ScrollArea class="min-h-0 flex-1">
      <div class="flex flex-col gap-1 pr-1">
        {#each serviceState.history as record (record.id)}
          <RecognitionRecordItem
            {record}
            open={expanded.has(record.id)}
            onToggle={() => toggleRecord(record.id)}
          />
        {/each}
      </div>
    </ScrollArea>
  {/if}
</div>
