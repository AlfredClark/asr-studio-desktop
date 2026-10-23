<script lang="ts">
  // 词条编辑弹窗：新建与编辑双模式，`editing == null` 即新建。
  // 触发拼音自动建议照抄词条编辑器（防抖 + 手改锁定 + 重新生成）；
  // 打开时从编辑目标回填（编辑视为已手改拼音），新建默认落到传入分组。
  import { Button } from "#components/shadcn-svelte/button";
  import { Input } from "#components/shadcn-svelte/input";
  import { Switch } from "#components/shadcn-svelte/switch";
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
  } from "#components/shadcn-svelte/dialog";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from "#components/shadcn-svelte/select";
  import { m } from "#libs/i18n/paraglide/messages";
  import { toast } from "#libs/utils/toast";
  import type { LexiconEntry } from "#libs/commands/types";
  import {
    createLexiconEntry,
    entriesOfGroup,
    groupLabel,
    lexiconState,
    normalizeLexiconPinyin,
    suggestLexiconPinyin,
    updateLexiconEntry,
  } from "#libs/hooks/lexicon.svelte";

  /** 弹窗开关（父组件双向绑定）、编辑目标（空即新建）、新建默认分组 */
  let {
    open = $bindable(false),
    editing,
    defaultGroupId,
  }: { open: boolean; editing: LexiconEntry | null; defaultGroupId: string } = $props();

  /** 表单草稿：打开时从编辑目标回填（编辑视为已手改拼音，不再自动覆盖） */
  let draftText = $state("");
  let draftPinyin = $state("");
  /** 拼音是否被亲手改过：改过即不再随词面自动覆盖（重新生成按钮不受此限） */
  let pinyinTouched = $state(false);
  let draftWeightText = $state("0");
  let draftEnabled = $state(true);
  let draftGroupId = $state("");
  /** 保存或拼音生成在途中禁用，避免重复提交 */
  let editorBusy = $state(false);

  /** 拼音建议失败标记：显式重新生成无结果时内联提示，编辑词面即清除 */
  let suggestFailed = $state(false);

  $effect(() => {
    if (open) {
      draftText = editing?.text ?? "";
      draftPinyin = editing?.pinyin ?? "";
      pinyinTouched = editing != null;
      draftWeightText = editing == null ? "0" : String(editing.weight);
      draftEnabled = editing?.enabled ?? true;
      draftGroupId = editing?.group_id ?? defaultGroupId;
      editorBusy = false;
      suggestFailed = false;
    }
  });

  // 新建时输入词面即自动生成拼音：防抖等停顿，未手改、弹窗仍开、文本未变才回填
  $effect(() => {
    const text = draftText.trim();
    if (!open || editing != null || pinyinTouched || editorBusy || text === "") {
      return;
    }
    const timer = setTimeout(() => {
      void (async () => {
        const suggested = await suggestLexiconPinyin(text);
        if (suggested !== "" && open && !pinyinTouched && draftText.trim() === text) {
          draftPinyin = suggested;
        }
      })();
    }, 350);
    return () => clearTimeout(timer);
  });

  /** 按当前词面重新生成拼音建议：失败留内联提示，不吞错 */
  async function handleRegeneratePinyin(): Promise<void> {
    if (editorBusy || draftText.trim() === "") return;
    editorBusy = true;
    try {
      const suggested = await suggestLexiconPinyin(draftText);
      suggestFailed = suggested === "";
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
      if (editing == null) {
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
        editing == null
          ? await createLexiconEntry({
              groupId: draftGroupId,
              text: draftText,
              pinyin: pinyin === "" ? undefined : pinyin,
              weight,
              enabled: draftEnabled,
            })
          : await updateLexiconEntry(editing.id, {
              groupId: draftGroupId,
              text: draftText,
              pinyin: pinyin === "" ? undefined : pinyin,
              weight,
              enabled: draftEnabled,
            });
      if (ok) open = false;
    } finally {
      editorBusy = false;
    }
  }
</script>

<Dialog bind:open>
  <DialogContent aria-label={editing == null ? m.lexicon_new_entry() : m.lexicon_edit()}>
    <DialogHeader>
      <DialogTitle>
        {editing == null ? m.lexicon_new_entry() : m.lexicon_edit()}
      </DialogTitle>
      <DialogDescription>{m.lexicon_editor_hint()}</DialogDescription>
    </DialogHeader>
    <div class="flex flex-col gap-3">
      <label class="flex flex-col gap-1 text-sm">
        <span class="font-medium">{m.lexicon_entry_text()}</span>
        <Input
          value={draftText}
          oninput={(event) => {
            draftText = event.currentTarget.value;
            suggestFailed = false;
          }}
        />
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
        {#if suggestFailed}
          <p class="text-xs text-destructive">{m.lexicon_pinyin_suggest_failed()}</p>
        {/if}
      </label>
      <div class="flex items-center gap-3">
        <div class="flex min-w-0 flex-1 flex-col gap-1 text-sm">
          <label class="flex items-center gap-2">
            <span class="shrink-0 font-medium">{m.lexicon_entry_weight()}</span>
            <Input
              type="number"
              min={-100}
              max={100}
              value={draftWeightText}
              oninput={(event) => (draftWeightText = event.currentTarget.value)}
              class="min-w-0 flex-1"
            />
          </label>
          <p class="text-xs text-muted-foreground">{m.lexicon_weight_scale()}</p>
        </div>
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
      <Button variant="outline" onclick={() => (open = false)}>
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
