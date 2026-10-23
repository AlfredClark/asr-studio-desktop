<script lang="ts">
  // 批量添加弹窗：输入多行词面 → 防抖逐词解析拼音 → 预览新增/跳过数 → 落盘。
  // 打开时输入清空、目标锁定父组件传入的分组快照，切组不影响已开弹窗。
  import { Button } from "#components/shadcn-svelte/button";
  import { Textarea } from "#components/shadcn-svelte/textarea";
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
  } from "#components/shadcn-svelte/dialog";
  import { SvelteSet } from "svelte/reactivity";
  import { m } from "#libs/i18n/paraglide/messages";
  import {
    LEXICON_BATCH_ADD_LIMIT,
    batchCreateLexiconEntries,
    lexiconState,
    normalizeLexiconPinyin,
    parseBatchEntryTexts,
    suggestLexiconPinyin,
  } from "#libs/hooks/lexicon.svelte";

  /** 弹窗开关（父组件双向绑定）与目标分组快照（落盘与判重都以它为准） */
  let {
    open = $bindable(false),
    targetGroupId,
    targetGroupName,
  }: { open: boolean; targetGroupId: string; targetGroupName: string } = $props();

  /** 批量添加输入草稿 */
  let batchInput = $state("");
  /** 批量确认在途中禁用，避免重复提交 */
  let batchBusy = $state(false);

  /** 打开时重置：输入清空，解析清空 */
  $effect(() => {
    if (open) {
      batchInput = "";
      batchResolved = [];
      batchBusy = false;
    }
  });

  /** 批量输入解析出的候选词：输入内去重保序，未截断 */
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
    if (!open || batchBusy || candidates.length === 0) {
      batchResolved = [];
      return;
    }
    const snapshot = [...candidates];
    const timer = setTimeout(() => {
      void (async () => {
        const pairs = await Promise.all(
          snapshot.map(async (text) => ({ text, pinyin: await suggestLexiconPinyin(text) })),
        );
        if (open && batchCapped.join("\n") === snapshot.join("\n")) {
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
      if (entry.group_id === targetGroupId) {
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
      const result = await batchCreateLexiconEntries(targetGroupId, batchResolved);
      if (result.ok) {
        open = false;
        batchInput = "";
      }
    } finally {
      batchBusy = false;
    }
  }
</script>

<Dialog bind:open>
  <DialogContent aria-label={m.lexicon_batch_add()} class="sm:max-w-xl">
    <DialogHeader>
      <DialogTitle>{m.lexicon_batch_add()}</DialogTitle>
      <DialogDescription>{m.lexicon_batch_add_hint()}</DialogDescription>
    </DialogHeader>
    <div class="flex flex-col gap-3">
      <p class="text-sm text-muted-foreground">
        {m.lexicon_move_group()}：{targetGroupName}
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
      <Button variant="outline" onclick={() => (open = false)} disabled={batchBusy}>
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
