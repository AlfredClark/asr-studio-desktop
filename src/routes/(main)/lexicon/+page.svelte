<script lang="ts">
  // 词库管理页：左右分栏，左侧分组列表（新建/重命名/删除/设为生效），右侧所选分组的词条表。
  // 状态机收敛在 `libs/hooks/lexicon.svelte.ts`，本页只做渲染与接线；词条编辑走弹窗表单。
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import { Input } from "#components/shadcn-svelte/input";
  import { Textarea } from "#components/shadcn-svelte/textarea";
  import { Switch } from "#components/shadcn-svelte/switch";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from "#components/shadcn-svelte/select";
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
  } from "#components/shadcn-svelte/dialog";
  import PlayIcon from "@lucide/svelte/icons/play";
  import CheckIcon from "@lucide/svelte/icons/check";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import MenuIcon from "@lucide/svelte/icons/menu";
  import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";
  import ConfirmDialog from "#components/common/confirm-dialog.svelte";
  import {
    ResizableHandle,
    ResizablePane,
    ResizablePaneGroup,
  } from "#components/shadcn-svelte/resizable";
  import { m } from "#libs/i18n/paraglide/messages";
  import { cn } from "#libs/utils/shadcn-svelte";
  import { toast } from "#libs/utils/toast";
  import type { LexiconEntry } from "#libs/commands/types";
  import {
    DEFAULT_LEXICON_FILTER,
    DEFAULT_LEXICON_SORT,
    LEXICON_BATCH_ADD_LIMIT,
    batchCopyLexiconEntries,
    batchCreateLexiconEntries,
    batchDeleteLexiconEntries,
    batchMoveLexiconEntries,
    batchSetLexiconEntriesEnabled,
    createLexiconEntry,
    createLexiconGroup,
    deleteLexiconEntry,
    deleteLexiconGroup,
    entriesOfGroup,
    exportLexiconGroup,
    filterLexiconEntries,
    groupLabel,
    importLexiconGroup,
    isLexiconFilterMode,
    isLexiconSortMode,
    lexiconState,
    loadLexiconFilterMode,
    loadLexiconSortMode,
    moveLexiconGroup,
    normalizeLexiconPinyin,
    parseBatchEntryTexts,
    refreshLexicon,
    renameLexiconGroup,
    saveLexiconFilterMode,
    saveLexiconSortMode,
    setLexiconGroupEnabled,
    sortLexiconEntries,
    suggestLexiconPinyin,
    updateLexiconEntry,
  } from "#libs/hooks/lexicon.svelte";
  import type { LexiconFilterMode, LexiconSortMode } from "#libs/hooks/lexicon.svelte";

  /** 查看中的分组 id：空则跟随首个生效分组（无则全局）；分组被删后自动回落 */
  let selectedGroupId = $state<string | null>(null);

  /** 生效中的用户分组 id 集合 */
  const activeIds = $derived(lexiconState.value.active_group_ids);

  /** 是否启用 */
  function isActive(id: string): boolean {
    return activeIds.includes(id);
  }

  /** 实际展示的分组 id：查看态存在即用，否则跟随首个生效分组，无则全局 */
  const effectiveGroupId = $derived.by(() => {
    const groups = lexiconState.value.groups;
    if (selectedGroupId != null && groups.some((group) => group.id === selectedGroupId)) {
      return selectedGroupId;
    }
    return (
      groups.find((group) => !group.is_global && activeIds.includes(group.id))?.id ??
      groups.find((group) => group.is_global)?.id ??
      ""
    );
  });

  /** 词条排序与筛选：页面私有视图偏好，落盘记忆保证会话间一致 */
  let sortMode = $state<LexiconSortMode>(DEFAULT_LEXICON_SORT);
  let filterMode = $state<LexiconFilterMode>(DEFAULT_LEXICON_FILTER);

  /** 首挂载时从存量恢复排序与筛选，脏数据回落默认值 */
  $effect.pre(() => {
    sortMode = loadLexiconSortMode();
    filterMode = loadLexiconFilterMode();
  });

  /** 所查分组的词条：先按启用状态过滤，再按排序模式重排，添加顺序即后端顺序 */
  const baseEntries = $derived(entriesOfGroup(lexiconState.value.entries, effectiveGroupId));
  const visibleEntries = $derived(
    sortLexiconEntries(filterLexiconEntries(baseEntries, filterMode), sortMode),
  );

  /** 首次水合中；完成前展示加载态而非空态，避免误导 */
  let loading = $state(true);
  /** 首次水合失败；展示错误与重试入口 */
  let loadFailed = $state(false);

  /** 新建分组输入 */
  let newGroupName = $state("");
  /** 新建分组在途中禁用，避免重复提交 */
  let creatingGroup = $state(false);

  /** 重命名中的分组 id 与草稿 */
  let renamingId = $state<string | null>(null);
  let renameDraft = $state("");

  /** 选中的词条 id 集合：驱动批量操作入口显隐 */
  const checked = new SvelteSet<string>();

  /** 批量操作下拉的重置键：每次执行后递增以清空已选项展示 */
  let batchKey = $state(0);

  /** 批量删除确认弹窗开关 */
  let confirmDeleteBatch = $state(false);

  /** 清空全组词条确认弹窗开关 */
  let confirmClearAll = $state(false);

  /** 待确认删除的分组 id */
  let deletingGroupId = $state<string | null>(null);

  /** 切换单条选中 */
  function toggleCheck(id: string): void {
    if (checked.has(id)) checked.delete(id);
    else checked.add(id);
  }

  /** 批量复制 / 转移的目标分组：除当前查看分组外的全部分组（含全局组） */
  const copyTargets = $derived(
    lexiconState.value.groups.filter((group) => group.id !== effectiveGroupId),
  );

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

  /** 批量添加弹窗开关与输入草稿 */
  let batchOpen = $state(false);
  let batchInput = $state("");
  /** 批量确认在途中禁用，避免重复提交 */
  let batchBusy = $state(false);
  /** 打开弹窗时快照的目标分组：落盘与判重都以它为准，切组不影响已开弹窗 */
  let batchTargetGroupId = $state("");
  let batchTargetGroupName = $state("");

  /** 打开批量添加弹窗：输入清空，目标锁定当前查看分组 */
  function openBatchAdd(): void {
    batchInput = "";
    batchResolved = [];
    batchTargetGroupId = effectiveGroupId;
    const target = lexiconState.value.groups.find((group) => group.id === effectiveGroupId);
    batchTargetGroupName = target == null ? "" : groupLabel(target);
    batchOpen = true;
  }

  /** 批量输入解析出的候选词：输入内已去重保序，未截断 */
  const batchParsed = $derived(parseBatchEntryTexts(batchInput));
  /** 截断到上限后的待添加候选 */
  const batchCapped = $derived(batchParsed.slice(0, LEXICON_BATCH_ADD_LIMIT));
  /** 是否因上限被截断 */
  const batchTruncated = $derived(batchParsed.length > batchCapped.length);

  /** 批量候选的拼音解析结果：防抖后逐词建议，供预览精确判重 */
  let batchResolved = $state<Array<{ text: string; pinyin: string }>>([]);

  // 批量输入防抖解析拼音：快照比对通过才回填，避免后一次输入被先一次覆盖
  $effect(() => {
    const candidates = batchCapped;
    if (!batchOpen || batchBusy || candidates.length === 0) {
      batchResolved = [];
      return;
    }
    const snapshot = [...candidates];
    const timer = setTimeout(() => {
      void (async () => {
        const pairs = await Promise.all(
          snapshot.map(async (text) => ({ text, pinyin: await suggestLexiconPinyin(text) })),
        );
        if (batchOpen && batchCapped.join("\n") === snapshot.join("\n")) {
          batchResolved = pairs;
        }
      })();
    }, 350);
    return () => clearTimeout(timer);
  });

  /** 目标分组内存词条的（词面 + 归一化拼音）集合 */
  const batchExistingKeys = $derived.by(() => {
    const keys = new SvelteSet<string>();
    for (const entry of lexiconState.value.entries) {
      if (entry.group_id === batchTargetGroupId) {
        keys.add(`${entry.text}\n${normalizeLexiconPinyin(entry.pinyin)}`);
      }
    }
    return keys;
  });

  /** 批量解析是否就绪：解析条数追上候选条数 */
  const batchReady = $derived(
    batchCapped.length > 0 && batchResolved.length === batchCapped.length,
  );

  /** 预览：解析完成且未命中已存在的候选数 */
  const batchNewCount = $derived(
    batchResolved.filter(
      (pair) => !batchExistingKeys.has(`${pair.text}\n${normalizeLexiconPinyin(pair.pinyin)}`),
    ).length,
  );

  /** 预览：解析完成且命中已存在的候选数 */
  const batchSkipCount = $derived(batchResolved.length - batchNewCount);

  /** 确认批量添加：用已解析的词音对落盘，成功关弹窗 */
  async function handleConfirmBatchAdd(): Promise<void> {
    if (batchBusy || !batchReady) return;
    batchBusy = true;
    try {
      const result = await batchCreateLexiconEntries(batchTargetGroupId, batchResolved);
      if (result.ok) {
        batchOpen = false;
        batchInput = "";
      }
    } finally {
      batchBusy = false;
    }
  }

  // 切组或条目变化时裁剪选中：跨组选中会误删，删掉的条目残留也一并清理
  $effect(() => {
    const alive = new Set(baseEntries.map((entry) => entry.id));
    for (const id of [...checked]) {
      if (!alive.has(id)) checked.delete(id);
    }
  });
  /** 待确认删除的词条 id */
  let deletingEntryId = $state<string | null>(null);
  /** 删除确认弹窗开关（分组 / 词条各一，id 另存） */
  let confirmDeleteGroup = $state(false);
  let confirmDeleteEntry = $state(false);

  /** 词条编辑弹窗开关：`editingId == null` 即新建 */
  let editorOpen = $state(false);
  let editingId = $state<string | null>(null);
  let draftText = $state("");
  let draftPinyin = $state("");
  /** 拼音是否被亲手改过：改过即不再随词面自动覆盖（重新生成按钮不受此限） */
  let pinyinTouched = $state(false);
  let draftWeightText = $state("0");
  let draftEnabled = $state(true);
  let draftGroupId = $state("");
  /** 保存或拼音生成在途中禁用，避免重复提交 */
  let editorBusy = $state(false);

  /** 打开新建词条弹窗：默认落到当前查看分组 */
  function openCreateEntry(): void {
    editingId = null;
    draftText = "";
    draftPinyin = "";
    pinyinTouched = false;
    draftWeightText = "0";
    draftEnabled = true;
    draftGroupId = effectiveGroupId;
    editorOpen = true;
  }

  /** 打开编辑弹窗：草稿回填当前值，视为已手改（不再自动覆盖） */
  function openEditEntry(entry: LexiconEntry): void {
    editingId = entry.id;
    draftText = entry.text;
    draftPinyin = entry.pinyin;
    pinyinTouched = true;
    draftWeightText = String(entry.weight);
    draftEnabled = entry.enabled;
    draftGroupId = entry.group_id;
    editorOpen = true;
  }

  // 新建时输入词面即自动生成拼音：防抖等停顿，未手改、弹窗仍开、文本未变才回填
  $effect(() => {
    const text = draftText.trim();
    if (!editorOpen || editingId != null || pinyinTouched || editorBusy || text === "") {
      return;
    }
    const timer = setTimeout(() => {
      void (async () => {
        const suggested = await suggestLexiconPinyin(text);
        if (suggested !== "" && editorOpen && !pinyinTouched && draftText.trim() === text) {
          draftPinyin = suggested;
        }
      })();
    }, 350);
    return () => clearTimeout(timer);
  });

  /** 按当前词面重新生成拼音建议 */
  async function handleRegeneratePinyin(): Promise<void> {
    if (editorBusy || draftText.trim() === "") return;
    editorBusy = true;
    try {
      const suggested = await suggestLexiconPinyin(draftText);
      if (suggested !== "") draftPinyin = suggested;
    } finally {
      editorBusy = false;
    }
  }

  /** 保存词条：拼音为空且词面非空时先自动建议再落盘；权重钳到后端同口径 */
  async function handleSaveEntry(): Promise<void> {
    if (editorBusy) return;
    const parsed = Number.parseInt(draftWeightText, 10);
    const weight = Number.isNaN(parsed) ? undefined : Math.min(100, Math.max(-100, parsed));
    editorBusy = true;
    try {
      let pinyin = draftPinyin;
      if (pinyin.trim() === "" && draftText.trim() !== "") {
        pinyin = await suggestLexiconPinyin(draftText);
      }
      if (editingId == null) {
        const duplicate = entriesOfGroup(lexiconState.value.entries, draftGroupId).some(
          (entry) =>
            entry.text === draftText.trim() &&
            normalizeLexiconPinyin(entry.pinyin) === normalizeLexiconPinyin(pinyin),
        );
        if (duplicate) {
          toast.info(m.lexicon_entry_exists());
          return;
        }
      }
      const ok =
        editingId == null
          ? await createLexiconEntry({
              groupId: draftGroupId,
              text: draftText,
              pinyin: pinyin === "" ? undefined : pinyin,
              weight,
              enabled: draftEnabled,
            })
          : await updateLexiconEntry(editingId, {
              groupId: draftGroupId,
              text: draftText,
              pinyin: pinyin === "" ? undefined : pinyin,
              weight,
              enabled: draftEnabled,
            });
      if (ok) editorOpen = false;
    } finally {
      editorBusy = false;
    }
  }

  /** 导入分组：成功后切到新分组查看 */
  async function handleImportGroup(): Promise<void> {
    const summary = await importLexiconGroup();
    if (summary != null) selectedGroupId = summary.id;
  }

  /** 新建分组：空名直接忽略 */
  async function handleCreateGroup(): Promise<void> {
    if (newGroupName.trim() === "" || creatingGroup) return;
    creatingGroup = true;
    try {
      if (await createLexiconGroup(newGroupName)) newGroupName = "";
    } finally {
      creatingGroup = false;
    }
  }

  /** 开始重命名：草稿回填存量名（全局组无入口） */
  function startRename(id: string, name: string): void {
    renamingId = id;
    renameDraft = name;
  }

  /** 提交重命名：空名直接忽略 */
  async function handleRenameGroup(id: string): Promise<void> {
    if (renameDraft.trim() === "") return;
    if (await renameLexiconGroup(id, renameDraft)) renamingId = null;
  }

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
        <ScrollArea class="h-full">
          <div class="flex h-full flex-col gap-2 p-4">
            <h2 class="shrink-0 text-sm font-medium">{m.lexicon_groups_title()}</h2>
            <p class="shrink-0 text-xs text-muted-foreground">{m.lexicon_global_hint()}</p>
            <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
              {#each lexiconState.value.groups as group, groupIndex (group.id)}
                {@const selected = group.id === effectiveGroupId}
                {@const count = entriesOfGroup(lexiconState.value.entries, group.id).length}
                <div class="rounded-lg px-1 py-1">
                  {#if renamingId === group.id}
                    <div class="flex items-center gap-2">
                      <Input
                        value={renameDraft}
                        oninput={(event) => (renameDraft = event.currentTarget.value)}
                        onkeydown={(event) => {
                          if (event.key === "Enter") void handleRenameGroup(group.id);
                          else if (event.key === "Escape") renamingId = null;
                        }}
                        aria-label={m.lexicon_group_name()}
                      />
                      <Button
                        size="sm"
                        onclick={() => void handleRenameGroup(group.id)}
                        disabled={renameDraft.trim() === ""}
                      >
                        {m.lexicon_save()}
                      </Button>
                      <Button variant="ghost" size="sm" onclick={() => (renamingId = null)}>
                        {m.lexicon_cancel()}
                      </Button>
                    </div>
                  {:else}
                    <div class="flex items-center gap-1">
                      <Button
                        variant={selected ? "secondary" : "ghost"}
                        class={cn(
                          "min-w-0 flex-1 justify-start gap-2 px-2 font-normal",
                          selected && "font-medium",
                        )}
                        onclick={() => (selectedGroupId = group.id)}
                        aria-current={selected ? "true" : undefined}
                      >
                        <span class="min-w-0 flex-1 truncate text-left text-sm">
                          {groupLabel(group)}
                          <span class="text-muted-foreground tabular-nums"> ({count})</span>
                        </span>
                        {#if group.is_global}
                          <Badge>{m.lexicon_always_on()}</Badge>
                        {:else if isActive(group.id)}
                          <Badge>{m.lexicon_enabled()}</Badge>
                        {/if}
                      </Button>
                      {#if !group.is_global}
                        <Button
                          variant={isActive(group.id) ? "secondary" : "ghost"}
                          size="icon-sm"
                          onclick={() => void setLexiconGroupEnabled(group.id, !isActive(group.id))}
                          aria-pressed={isActive(group.id)}
                          aria-label={isActive(group.id) ? m.lexicon_disable() : m.lexicon_enable()}
                          title={isActive(group.id) ? m.lexicon_disable() : m.lexicon_enable()}
                        >
                          {#if isActive(group.id)}
                            <CheckIcon />
                          {:else}
                            <PlayIcon />
                          {/if}
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          onclick={() => startRename(group.id, group.name)}
                          aria-label={m.lexicon_rename_group()}
                          title={m.lexicon_rename_group()}
                        >
                          <PencilIcon />
                        </Button>
                        <DropdownMenuPrimitive.Root>
                          <DropdownMenuPrimitive.Trigger>
                            {#snippet child({ props })}
                              <Button
                                variant="ghost"
                                size="icon-sm"
                                {...props}
                                aria-label={m.lexicon_more_actions()}
                                title={m.lexicon_more_actions()}
                              >
                                <EllipsisIcon />
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
                              disabled={groupIndex <= 1}
                              onSelect={() => void moveLexiconGroup(group.id, "up")}
                            >
                              {m.lexicon_move_up()}
                            </DropdownMenuPrimitive.Item>
                            <DropdownMenuPrimitive.Item
                              class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                              disabled={groupIndex === lexiconState.value.groups.length - 1}
                              onSelect={() => void moveLexiconGroup(group.id, "down")}
                            >
                              {m.lexicon_move_down()}
                            </DropdownMenuPrimitive.Item>
                            <DropdownMenuPrimitive.Item
                              class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:bg-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                              onSelect={() => void exportLexiconGroup(group.id)}
                            >
                              {m.lexicon_export_group()}
                            </DropdownMenuPrimitive.Item>
                            <DropdownMenuPrimitive.Item
                              class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                              onSelect={() => {
                                deletingGroupId = group.id;
                                confirmDeleteGroup = true;
                              }}
                            >
                              {m.lexicon_delete_group()}
                            </DropdownMenuPrimitive.Item>
                          </DropdownMenuPrimitive.Content>
                        </DropdownMenuPrimitive.Root>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
            <form
              class="flex shrink-0 items-center gap-2"
              onsubmit={(event) => {
                event.preventDefault();
                void handleCreateGroup();
              }}
            >
              <Button
                type="button"
                variant="outline"
                size="sm"
                onclick={() => void handleImportGroup()}
              >
                {m.lexicon_import_group()}
              </Button>
              <Input
                value={newGroupName}
                oninput={(event) => (newGroupName = event.currentTarget.value)}
                aria-label={m.lexicon_group_name()}
                placeholder={m.lexicon_group_name()}
              />
              <Button
                type="submit"
                size="sm"
                disabled={newGroupName.trim() === "" || creatingGroup}
              >
                {m.lexicon_new_group()}
              </Button>
            </form>
          </div>
        </ScrollArea>
      </ResizablePane>
      <ResizableHandle withHandle />
      <ResizablePane defaultSize={68} minSize={40}>
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
                                {groupLabel(target)} ({entriesOfGroup(
                                  lexiconState.value.entries,
                                  target.id,
                                ).length})
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
                                {groupLabel(target)} ({entriesOfGroup(
                                  lexiconState.value.entries,
                                  target.id,
                                ).length})
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
                  <SelectTrigger
                    class="w-(--control-column-width)"
                    aria-label={m.lexicon_sort_label()}
                  >
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
                  <SelectTrigger
                    class="w-(--control-column-width)"
                    aria-label={m.lexicon_filter_label()}
                  >
                    <SelectValue placeholder={m.lexicon_filter_label()} />
                  </SelectTrigger>
                  <SelectContent>
                    {#each filterItems as item (item.value)}
                      <SelectItem value={item.value}>{item.label}</SelectItem>
                    {/each}
                  </SelectContent>
                </Select>
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
                      class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:bg-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                      disabled={visibleEntries.length === 0}
                      onSelect={() => selectAllVisible()}
                    >
                      {m.lexicon_select_all()}
                    </DropdownMenuPrimitive.Item>
                    <DropdownMenuPrimitive.Item
                      class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:bg-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                      onSelect={() => openBatchAdd()}
                    >
                      {m.lexicon_batch_add()}
                    </DropdownMenuPrimitive.Item>
                    <DropdownMenuPrimitive.Item
                      class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:bg-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
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
                      onCheckedChange={(checked) =>
                        void updateLexiconEntry(entry.id, { enabled: checked })}
                      aria-label={m.lexicon_entry_enabled()}
                    />
                    <Button variant="ghost" size="sm" onclick={() => openEditEntry(entry)}>
                      {m.lexicon_edit()}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onclick={() => {
                        deletingEntryId = entry.id;
                        confirmDeleteEntry = true;
                      }}
                    >
                      {m.lexicon_delete()}
                    </Button>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        </ScrollArea>
      </ResizablePane>
    </ResizablePaneGroup>
  {/if}
</div>

<Dialog bind:open={editorOpen}>
  <DialogContent aria-label={editingId == null ? m.lexicon_new_entry() : m.lexicon_edit()}>
    <DialogHeader>
      <DialogTitle>
        {editingId == null ? m.lexicon_new_entry() : m.lexicon_edit()}
      </DialogTitle>
      <DialogDescription>{m.lexicon_editor_hint()}</DialogDescription>
    </DialogHeader>
    <div class="flex flex-col gap-3">
      <label class="flex flex-col gap-1 text-sm">
        <span class="font-medium">{m.lexicon_entry_text()}</span>
        <Input value={draftText} oninput={(event) => (draftText = event.currentTarget.value)} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="font-medium">{m.lexicon_entry_pinyin()}</span>
        <div class="flex items-center gap-2">
          <Input
            value={draftPinyin}
            oninput={(event) => {
              pinyinTouched = true;
              draftPinyin = event.currentTarget.value;
            }}
            class="min-w-0 flex-1"
          />
          <Button
            variant="outline"
            size="sm"
            onclick={() => void handleRegeneratePinyin()}
            disabled={editorBusy || draftText.trim() === ""}
          >
            {m.lexicon_regenerate_pinyin()}
          </Button>
        </div>
      </label>
      <div class="flex items-center gap-3">
        <label class="flex min-w-0 flex-1 items-center gap-2 text-sm">
          <span class="shrink-0 font-medium">{m.lexicon_entry_weight()}</span>
          <Input
            type="number"
            min={-100}
            max={100}
            value={draftWeightText}
            oninput={(event) => (draftWeightText = event.currentTarget.value)}
          />
        </label>
        <label class="flex shrink-0 items-center gap-2 text-sm">
          <span class="font-medium">{m.lexicon_entry_enabled()}</span>
          <Switch checked={draftEnabled} onCheckedChange={(checked) => (draftEnabled = checked)} />
        </label>
      </div>
      <div class="flex items-center gap-2 text-sm">
        <span class="shrink-0 font-medium">{m.lexicon_move_group()}</span>
        <Select
          type="single"
          value={draftGroupId}
          items={lexiconState.value.groups.map((group) => ({
            value: group.id,
            label: groupLabel(group),
          }))}
          onValueChange={(value) => (draftGroupId = value)}
        >
          <SelectTrigger class="min-w-0 flex-1" aria-label={m.lexicon_move_group()}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {#each lexiconState.value.groups as group (group.id)}
              <SelectItem value={group.id}>{groupLabel(group)}</SelectItem>
            {/each}
          </SelectContent>
        </Select>
      </div>
    </div>
    <DialogFooter>
      <Button variant="outline" onclick={() => (editorOpen = false)}>
        {m.lexicon_cancel()}
      </Button>
      <Button
        onclick={() => void handleSaveEntry()}
        disabled={editorBusy || draftText.trim() === ""}
      >
        {m.lexicon_save()}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>

<Dialog bind:open={batchOpen}>
  <DialogContent aria-label={m.lexicon_batch_add()} class="sm:max-w-xl">
    <DialogHeader>
      <DialogTitle>{m.lexicon_batch_add()}</DialogTitle>
      <DialogDescription>{m.lexicon_batch_add_hint()}</DialogDescription>
    </DialogHeader>
    <div class="flex flex-col gap-3">
      <p class="text-sm text-muted-foreground">
        {m.lexicon_move_group()}：{batchTargetGroupName}
      </p>
      <label class="flex flex-col gap-1 text-sm">
        <span class="font-medium">{m.lexicon_batch_add_label()}</span>
        <Textarea
          bind:value={batchInput}
          rows={10}
          disabled={batchBusy}
          placeholder={m.lexicon_batch_add_placeholder()}
          aria-label={m.lexicon_batch_add_label()}
        />
      </label>
      {#if batchCapped.length > 0 && !batchReady}
        <p class="text-sm text-muted-foreground">{m.lexicon_batch_add_resolving()}</p>
      {/if}
      {#if batchReady}
        <p class="text-sm text-muted-foreground">
          {m.lexicon_batch_add_preview({ count: batchNewCount, skipped: batchSkipCount })}
        </p>
      {/if}
      {#if batchTruncated}
        <p class="text-sm text-muted-foreground">{m.lexicon_batch_add_truncated()}</p>
      {/if}
    </div>
    <DialogFooter>
      <Button variant="outline" onclick={() => (batchOpen = false)} disabled={batchBusy}>
        {m.lexicon_cancel()}
      </Button>
      <Button
        onclick={() => void handleConfirmBatchAdd()}
        disabled={batchBusy || !batchReady || batchNewCount === 0}
      >
        {m.lexicon_save()}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>

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
  description={m.lexicon_clear_entries_confirm_description({ count: baseEntries.length })}
  cancelLabel={m.lexicon_cancel()}
  confirmLabel={m.lexicon_delete()}
  variant="destructive"
  onConfirm={() => {
    confirmClearAll = false;
    void handleConfirmClearAll();
  }}
/>

<ConfirmDialog
  bind:open={confirmDeleteGroup}
  title={m.lexicon_delete_group_confirm_title()}
  description={m.lexicon_delete_group_confirm_description()}
  cancelLabel={m.lexicon_cancel()}
  confirmLabel={m.lexicon_delete()}
  variant="destructive"
  onConfirm={() => {
    const id = deletingGroupId;
    deletingGroupId = null;
    if (id != null) {
      if (selectedGroupId === id) selectedGroupId = null;
      void deleteLexiconGroup(id);
    }
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
