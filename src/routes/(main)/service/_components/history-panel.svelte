<script lang="ts">
  // 识别记录面板（左栏）：会话级统计三格 + 来源筛选 + 记录列表（点击展开全文）。
  // 展开集合与筛选是纯前端局部态（筛选落盘记忆），数据来自跨页面共享的 serviceState；
  // 统计口径始终是全量历史，筛选只影响列表展示。
  import { SvelteSet } from "svelte/reactivity";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from "#components/shadcn-svelte/select";
  import RecognitionRecordItem from "#components/common/recognition-record-item.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import {
    filterHistoryByKind,
    isServiceHistoryFilter,
    loadServiceHistoryFilter,
    saveServiceHistoryFilter,
    serviceState,
    summarizeHistory,
    type ServiceHistoryFilter,
  } from "#libs/hooks/service.svelte";

  /** 左栏统计：识别条数 / 字符总数 / 推理耗时均值，会话级，重启即清零 */
  const stats = $derived(summarizeHistory(serviceState.history));

  /** 推理耗时均值展示（秒，1 位小数；空历史显示 0） */
  const avgText = $derived(stats.avgInferMs == null ? "0" : (stats.avgInferMs / 1000).toFixed(1));

  /** 展开全文的记录 id 集合：纯前端局部态 */
  const expanded = new SvelteSet<number>();

  /** 来源筛选：页面私有视图偏好，挂载时从存量恢复、脏数据回落默认值，切换后落盘记忆 */
  let filterMode = $state<ServiceHistoryFilter>(loadServiceHistoryFilter());

  /** 筛选后的可见记录：统计口径不受筛选影响 */
  const visibleRecords = $derived(filterHistoryByKind(serviceState.history, filterMode));

  /** 筛选下拉项：全部 + 两种来源（复用行徽标文案） */
  const filterItems = $derived([
    { value: "all", label: m.service_history_filter_all() },
    { value: "voice", label: m.service_history_kind_text() },
    { value: "directive", label: m.service_history_kind_directive() },
  ]);

  /** 切换筛选：非法取值直接忽略，合法即切内存态并落盘 */
  function handleFilterChange(value: string): void {
    if (!isServiceHistoryFilter(value)) return;
    filterMode = value;
    saveServiceHistoryFilter(value);
  }

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
  <div class="flex shrink-0 items-center justify-between gap-2">
    <h2 class="text-sm font-medium">{m.service_history_title()}</h2>
    <Select
      type="single"
      value={filterMode}
      items={filterItems}
      onValueChange={(value) => handleFilterChange(value)}
    >
      <SelectTrigger class="w-28" aria-label={m.service_history_filter_label()}>
        <SelectValue placeholder={m.service_history_filter_label()} />
      </SelectTrigger>
      <SelectContent>
        {#each filterItems as item (item.value)}
          <SelectItem value={item.value}>{item.label}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
  </div>
  <p class="shrink-0 text-xs text-muted-foreground">
    {m.service_history_session_hint()}
  </p>
  {#if serviceState.history.length === 0}
    <div class="flex min-h-0 flex-1 items-center justify-center">
      <p class="text-sm text-muted-foreground">{m.service_history_empty()}</p>
    </div>
  {:else if visibleRecords.length === 0}
    <div class="flex min-h-0 flex-1 items-center justify-center">
      <p class="text-sm text-muted-foreground">{m.service_history_empty_filter()}</p>
    </div>
  {:else}
    <ScrollArea class="min-h-0 flex-1">
      <div class="flex flex-col gap-1 pr-1">
        {#each visibleRecords as record (record.id)}
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
