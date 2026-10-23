<script lang="ts">
  // 词条表（右栏）：所查分组的词条筛选/排序/批量操作/行内开关编辑删除。
  // 选中集合、排序筛选偏好、批量在途态全在内部；新建/编辑/批量添加弹窗与删除确认框自带。
  import { SvelteSet } from "svelte/reactivity";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { Button } from "#components/shadcn-svelte/button";
  import { Switch } from "#components/shadcn-svelte/switch";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from "#components/shadcn-svelte/select";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import MenuIcon from "@lucide/svelte/icons/menu";
  import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";
  import ConfirmDialog from "#components/common/confirm-dialog.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import type { LexiconEntry } from "#libs/commands/types";
  import {
    DEFAULT_LEXICON_FILTER,
    DEFAULT_LEXICON_SORT,
    LEXICON_BATCH_ADD_LIMIT,
    batchCopyLexiconEntries,
    batchDeleteLexiconEntries,
    batchMoveLexiconEntries,
    batchSetLexiconEntriesEnabled,
    deleteLexiconEntry,
    entriesOfGroup,
    filterLexiconEntries,
    groupLabel,
    isLexiconFilterMode,
    isLexiconSortMode,
    lexiconState,
    loadLexiconFilterMode,
    loadLexiconSortMode,
    saveLexiconFilterMode,
    saveLexiconSortMode,
    sortLexiconEntries,
    updateLexiconEntry,
  } from "#libs/hooks/lexicon.svelte";
  import type { LexiconFilterMode, LexiconSortMode } from "#libs/hooks/lexicon.svelte";
  import EntryEditorDialog from "./entry-editor-dialog.svelte";
  import BatchAddDialog from "./batch-add-dialog.svelte";

  /** 当前查看的分组 id（页算好的有效值） */
  let { groupId }: { groupId: string } = $props();

  /** 词条排序与筛选：页面私有视图偏好，落盘记忆保证会话间一致 */
  let sortMode = $state<LexiconSortMode>(DEFAULT_LEXICON_SORT);
  let filterMode = $state<LexiconFilterMode>(DEFAULT_LEXICON_FILTER);

  /** 首挂载时从存量恢复排序与筛选，脏数据回落默认值 */
  $effect.pre(() => {
    sortMode = loadLexiconSortMode();
    filterMode = loadLexiconFilterMode();
  });

  /** 所查分组的词条：先按启用状态过滤，再按排序模式重排，添加顺序即后端顺序 */
  const baseEntries = $derived(entriesOfGroup(lexiconState.value.entries, groupId));
  const visibleEntries = $derived(
    sortLexiconEntries(filterLexiconEntries(baseEntries, filterMode), sortMode),
  );

  /** 选中的词条 id 集合：驱动批量操作入口显隐 */
  const checked = new SvelteSet<string>();

  /** 批量操作下拉的重置键：每次执行后递增以清空已选项展示 */
  let batchKey = $state(0);

  /** 批量删除确认弹窗开关 */
  let confirmDeleteBatch = $state(false);

  /** 清空全组词条确认弹窗开关 */
  let confirmClearAll = $state(false);

  /** 待确认删除的词条 id */
  let deletingEntryId = $state<string | null>(null);
  /** 删除词条确认弹窗开关 */
  let confirmDeleteEntry = $state(false);

  /** 词条编辑弹窗开关与编辑目标：`editingEntry == null` 即新建 */
  let editorOpen = $state(false);
  let editingEntry = $state<LexiconEntry | null>(null);

  /** 批量添加弹窗开关与目标快照：落盘与判重以它为准，切组不影响已开弹窗 */
  let batchOpen = $state(false);
  let batchTargetGroupId = $state("");
  let batchTargetGroupName = $state("");

  /** 切换单条选中 */
  function toggleCheck(id: string): void {
    if (checked.has(id)) checked.delete(id);
    else checked.add(id);
  }

  /** 批量复制 / 转移的目标分组：除当前查看分组外的全部分组（含全局组） */
  const copyTargets = $derived(lexiconState.value.groups.filter((group) => group.id !== groupId));

  /** 复制 / 转移在途中禁用，避免重复提交 */
  let copyMoveBusy = $state(false);

  /** 执行批量删除 / 启用 / 禁用：删除走二次确认，启用 / 禁用直接落盘 */
  async function handleBatchAction(value: string): Promise<void> {
    if (value === "delete") {
      confirmDeleteBatch = true;
      return;
    }
    const ids = [...checked];
    if (ids.length === 0) return;
    if (await batchSetLexiconEntriesEnabled(ids, value === "enable")) {
      batchKey += 1;
    }
  }

  /** 批量复制到目标分组：源词条原样保留，选中保持 */
  async function handleBatchCopy(targetGroupId: string): Promise<void> {
    if (copyMoveBusy) return;
    const ids = [...checked];
    if (ids.length === 0) return;
    copyMoveBusy = true;
    try {
      if ((await batchCopyLexiconEntries(ids, targetGroupId)).ok) {
        batchKey += 1;
      }
    } finally {
      copyMoveBusy = false;
    }
  }

  /** 批量转移到目标分组：成功后清空已转移选中（源词条已不存在） */
  async function handleBatchMove(targetGroupId: string): Promise<void> {
    if (copyMoveBusy) return;
    const ids = [...checked];
    if (ids.length === 0) return;
    copyMoveBusy = true;
    try {
      if ((await batchMoveLexiconEntries(ids, targetGroupId)).ok) {
        for (const id of ids) checked.delete(id);
        batchKey += 1;
      }
    } finally {
      copyMoveBusy = false;
    }
  }

  /** 确认批量删除：成功后清空选中（条目已不存在） */
  async function handleConfirmBatchDelete(): Promise<void> {
    const ids = [...checked];
    if (await batchDeleteLexiconEntries(ids)) {
      for (const id of ids) checked.delete(id);
      batchKey += 1;
    }
  }

  /** 排序下拉项：添加顺序即后端落盘顺序 */
  const sortItems = $derived([
    { value: "added", label: m.lexicon_sort_added() },
    { value: "pinyin-asc", label: m.lexicon_sort_pinyin_asc() },
    { value: "pinyin-desc", label: m.lexicon_sort_pinyin_desc() },
    { value: "weight-asc", label: m.lexicon_sort_weight_asc() },
    { value: "weight-desc", label: m.lexicon_sort_weight_desc() },
  ]);

  /** 筛选下拉项：按启用状态过滤 */
  const filterItems = $derived([
    { value: "all", label: m.lexicon_filter_all() },
    { value: "enabled", label: m.lexicon_filter_enabled() },
    { value: "disabled", label: m.lexicon_filter_disabled() },
  ]);

  /** 切换排序：非法取值直接忽略，合法即切内存态并落盘 */
  function handleSortChange(value: string): void {
    if (!isLexiconSortMode(value)) return;
    sortMode = value;
    saveLexiconSortMode(value);
  }

  /** 切换筛选：非法取值直接忽略，合法即切内存态并落盘 */
  function handleFilterChange(value: string): void {
    if (!isLexiconFilterMode(value)) return;
    filterMode = value;
    saveLexiconFilterMode(value);
  }

  /** 选中当前筛选下的全部可见词条 */
  function selectAllVisible(): void {
    for (const entry of visibleEntries) checked.add(entry.id);
    batchKey += 1;
  }

  /** 确认清空全组词条：成功后清理选中（条目已不存在） */
  async function handleConfirmClearAll(): Promise<void> {
    const ids = baseEntries.map((entry) => entry.id);
    if (await batchDeleteLexiconEntries(ids)) {
      for (const id of ids) checked.delete(id);
      batchKey += 1;
    }
  }

  /** 当前查看分组的展示名：清空确认框点名分组用 */
  const currentGroupLabel = $derived.by(() => {
    const target = lexiconState.value.groups.find((group) => group.id === groupId);
    return target == null ? "" : groupLabel(target);
  });

  /** 打开批量添加弹窗：输入清空，目标锁定当前查看分组 */
  function openBatchAdd(): void {
    batchTargetGroupId = groupId;
    const target = lexiconState.value.groups.find((group) => group.id === groupId);
    batchTargetGroupName = target == null ? "" : groupLabel(target);
    batchOpen = true;
  }

  // 切组或条目变化时裁剪选中：跨组选中会误删，删掉的条目残留也一并清理
  $effect(() => {
    const alive = new Set(baseEntries.map((entry) => entry.id));
    for (const id of [...checked]) {
      if (!alive.has(id)) checked.delete(id);
    }
  });

  /** 打开新建词条弹窗：默认落到当前查看分组 */
  function openCreateEntry(): void {
    editingEntry = null;
    editorOpen = true;
  }

  /** 打开编辑弹窗：回填当前值 */
  function openEditEntry(entry: LexiconEntry): void {
    editingEntry = entry;
    editorOpen = true;
  }
</script>

<ScrollArea class="h-full">
  <div class="flex h-full flex-col gap-2 p-4">
    <div class="flex shrink-0 items-center justify-between gap-2">
      <h2 class="text-sm font-medium">{m.lexicon_entries_title()}</h2>
      <div class="flex shrink-0 items-center gap-2">
        {#if checked.size > 0}
          {#key batchKey}
            <DropdownMenuPrimitive.Root>
              <DropdownMenuPrimitive.Trigger>
                {#snippet child({ props })}
                  <Button
                    variant="outline"
                    size="sm"
                    {...props}
                    aria-label={`${m.lexicon_batch_actions()} (${checked.size})`}
                    disabled={copyMoveBusy}
                  >
                    {m.lexicon_batch_actions()} ({checked.size})
                  </Button>
                {/snippet}
              </DropdownMenuPrimitive.Trigger>
              <DropdownMenuPrimitive.Content
                align="end"
                sideOffset={4}
                class="z-50 min-w-32 overflow-hidden rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-md"
              >
                <DropdownMenuPrimitive.Item
                  class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                  onSelect={() => void handleBatchAction("delete")}
                >
                  {m.lexicon_batch_delete()}
                </DropdownMenuPrimitive.Item>
                <DropdownMenuPrimitive.Item
                  class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                  onSelect={() => void handleBatchAction("enable")}
                >
                  {m.lexicon_batch_enable()}
                </DropdownMenuPrimitive.Item>
                <DropdownMenuPrimitive.Item
                  class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                  onSelect={() => void handleBatchAction("disable")}
                >
                  {m.lexicon_batch_disable()}
                </DropdownMenuPrimitive.Item>
                <DropdownMenuPrimitive.Sub>
                  <DropdownMenuPrimitive.SubTrigger
                    class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                    disabled={copyTargets.length === 0 || copyMoveBusy}
                  >
                    <span>{m.lexicon_batch_copy()}</span>
                    <ChevronRightIcon class="ml-auto size-4" />
                  </DropdownMenuPrimitive.SubTrigger>
                  <DropdownMenuPrimitive.SubContent
                    sideOffset={8}
                    class="z-50 min-w-32 overflow-hidden rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-md"
                  >
                    {#each copyTargets as target (target.id)}
                      <DropdownMenuPrimitive.Item
                        class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                        disabled={copyMoveBusy}
                        onSelect={() => void handleBatchCopy(target.id)}
                      >
                        {groupLabel(target)} ({entriesOfGroup(lexiconState.value.entries, target.id)
                          .length})
                      </DropdownMenuPrimitive.Item>
                    {/each}
                  </DropdownMenuPrimitive.SubContent>
                </DropdownMenuPrimitive.Sub>
                <DropdownMenuPrimitive.Sub>
                  <DropdownMenuPrimitive.SubTrigger
                    class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                    disabled={copyTargets.length === 0 || copyMoveBusy}
                  >
                    <span>{m.lexicon_batch_move()}</span>
                    <ChevronRightIcon class="ml-auto size-4" />
                  </DropdownMenuPrimitive.SubTrigger>
                  <DropdownMenuPrimitive.SubContent
                    sideOffset={8}
                    class="z-50 min-w-32 overflow-hidden rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-md"
                  >
                    {#each copyTargets as target (target.id)}
                      <DropdownMenuPrimitive.Item
                        class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                        disabled={copyMoveBusy}
                        onSelect={() => void handleBatchMove(target.id)}
                      >
                        {groupLabel(target)} ({entriesOfGroup(lexiconState.value.entries, target.id)
                          .length})
                      </DropdownMenuPrimitive.Item>
                    {/each}
                  </DropdownMenuPrimitive.SubContent>
                </DropdownMenuPrimitive.Sub>
              </DropdownMenuPrimitive.Content>
            </DropdownMenuPrimitive.Root>
          {/key}
        {/if}
        <Select
          type="single"
          value={sortMode}
          items={sortItems}
          onValueChange={(value) => handleSortChange(value)}
        >
          <SelectTrigger class="w-(--control-column-width)" aria-label={m.lexicon_sort_label()}>
            <SelectValue placeholder={m.lexicon_sort_label()} />
          </SelectTrigger>
          <SelectContent>
            {#each sortItems as item (item.value)}
              <SelectItem value={item.value}>{item.label}</SelectItem>
            {/each}
          </SelectContent>
        </Select>
        <Select
          type="single"
          value={filterMode}
          items={filterItems}
          onValueChange={(value) => handleFilterChange(value)}
        >
          <SelectTrigger class="w-(--control-column-width)" aria-label={m.lexicon_filter_label()}>
            <SelectValue placeholder={m.lexicon_filter_label()} />
          </SelectTrigger>
          <SelectContent>
            {#each filterItems as item (item.value)}
              <SelectItem value={item.value}>{item.label}</SelectItem>
            {/each}
          </SelectContent>
        </Select>
        <Button variant="outline" size="sm" onclick={openBatchAdd}>
          {m.lexicon_batch_add_limited({ limit: LEXICON_BATCH_ADD_LIMIT })}
        </Button>
        <Button size="sm" onclick={openCreateEntry}>
          {m.lexicon_new_entry()}
        </Button>
        <DropdownMenuPrimitive.Root>
          <DropdownMenuPrimitive.Trigger>
            {#snippet child({ props })}
              <Button
                variant="ghost"
                size="icon-sm"
                {...props}
                aria-label={m.lexicon_selection_menu()}
                title={m.lexicon_selection_menu()}
              >
                <MenuIcon />
              </Button>
            {/snippet}
          </DropdownMenuPrimitive.Trigger>
          <DropdownMenuPrimitive.Content
            align="end"
            sideOffset={4}
            class="z-50 min-w-32 overflow-hidden rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-md"
          >
            <DropdownMenuPrimitive.Item
              class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
              disabled={visibleEntries.length === 0}
              onSelect={() => selectAllVisible()}
            >
              {m.lexicon_select_all()}
            </DropdownMenuPrimitive.Item>
            <DropdownMenuPrimitive.Item
              class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
              disabled={baseEntries.length === 0}
              onSelect={() => {
                confirmClearAll = true;
              }}
            >
              {m.lexicon_clear_all()}
            </DropdownMenuPrimitive.Item>
          </DropdownMenuPrimitive.Content>
        </DropdownMenuPrimitive.Root>
      </div>
    </div>
    <p class="shrink-0 text-xs text-muted-foreground">{m.lexicon_rule_hint()}</p>
    {#if visibleEntries.length === 0}
      <div class="flex min-h-0 flex-1 items-center justify-center">
        <p class="text-sm text-muted-foreground">
          {baseEntries.length === 0 ? m.lexicon_empty_entries() : m.lexicon_empty_filter()}
        </p>
      </div>
    {:else}
      <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
        {#each visibleEntries as entry (entry.id)}
          <div class="flex items-center gap-2 rounded-lg bg-muted/50 px-2.5 py-2">
            <input
              type="checkbox"
              class="size-4 shrink-0 accent-primary"
              checked={checked.has(entry.id)}
              onchange={() => toggleCheck(entry.id)}
              aria-label={entry.text}
            />
            <p class="min-w-0 flex-1 truncate text-sm">
              <span class="font-medium">{entry.text}</span>
              <span class="text-muted-foreground"> [{entry.pinyin}]</span>
            </p>
            <span class="shrink-0 text-xs text-muted-foreground tabular-nums">
              {entry.weight}
            </span>
            <Switch
              checked={entry.enabled}
              onCheckedChange={(checked) => void updateLexiconEntry(entry.id, { enabled: checked })}
              aria-label={`${entry.text} - ${m.lexicon_entry_enabled()}`}
            />
            <Button
              variant="ghost"
              size="sm"
              onclick={() => openEditEntry(entry)}
              aria-label={`${entry.text} - ${m.lexicon_edit()}`}
              title={`${entry.text} - ${m.lexicon_edit()}`}
            >
              {m.lexicon_edit()}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onclick={() => {
                deletingEntryId = entry.id;
                confirmDeleteEntry = true;
              }}
              aria-label={`${entry.text} - ${m.lexicon_delete()}`}
              title={`${entry.text} - ${m.lexicon_delete()}`}
            >
              {m.lexicon_delete()}
            </Button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</ScrollArea>

<EntryEditorDialog bind:open={editorOpen} editing={editingEntry} defaultGroupId={groupId} />

<BatchAddDialog
  bind:open={batchOpen}
  targetGroupId={batchTargetGroupId}
  targetGroupName={batchTargetGroupName}
/>

<ConfirmDialog
  bind:open={confirmDeleteBatch}
  title={m.lexicon_delete_entries_confirm_title()}
  description={m.lexicon_delete_entries_confirm_description({ count: checked.size })}
  cancelLabel={m.lexicon_cancel()}
  confirmLabel={m.lexicon_delete()}
  variant="destructive"
  onConfirm={() => {
    confirmDeleteBatch = false;
    void handleConfirmBatchDelete();
  }}
/>

<ConfirmDialog
  bind:open={confirmClearAll}
  title={m.lexicon_clear_entries_confirm_title()}
  description={m.lexicon_clear_all_consequence({
    group: currentGroupLabel,
    count: baseEntries.length,
  })}
  cancelLabel={m.lexicon_cancel()}
  confirmLabel={m.lexicon_delete_all_entries({ count: baseEntries.length })}
  variant="destructive"
  onConfirm={() => {
    confirmClearAll = false;
    void handleConfirmClearAll();
  }}
/>

<ConfirmDialog
  bind:open={confirmDeleteEntry}
  title={m.lexicon_delete_entry_confirm_title()}
  description={m.lexicon_delete_entry_confirm_description()}
  cancelLabel={m.lexicon_cancel()}
  confirmLabel={m.lexicon_delete()}
  variant="destructive"
  onConfirm={() => {
    const id = deletingEntryId;
    deletingEntryId = null;
    if (id != null) void deleteLexiconEntry(id);
  }}
/>
