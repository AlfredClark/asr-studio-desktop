<script lang="ts">
  // 指令编辑弹窗：新建与编辑双模式，`editing == null` 即新建。
  // 触发拼音自动建议逻辑照抄词条编辑器（防抖 + 手改锁定 + 重新生成）；
  // 动作序列只读预览 + 清空，平衡校验在保存时一次性拦截。
  import { Badge } from "#components/shadcn-svelte/badge";
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
  import type { Directive } from "#libs/commands/types";
  import { isKeyHeld, keyLabel } from "#libs/directives/key-registry";
  import {
    DIRECTIVE_INTERVAL_MAX,
    DIRECTIVE_INTERVAL_MIN,
    areActionsBalanced,
    createDirective,
    directiveState,
    isTriggerDuplicate,
    suggestDirectivePinyin,
    updateDirective,
  } from "#libs/hooks/directives.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import { toast } from "#libs/utils/toast";
  import type { DirectiveAction } from "#libs/commands/types";
  import KeyboardRecorder from "./keyboard-recorder.svelte";

  /** 弹窗开关（父组件双向绑定）与编辑目标（空即新建） */
  let { open = $bindable(false), editing }: { open: boolean; editing: Directive | null } = $props();

  /** 表单草稿：打开时从编辑目标回填（编辑视为已手改拼音，不再自动覆盖） */
  let draftName = $state("");
  let draftTrigger = $state("");
  let draftPinyin = $state("");
  let pinyinTouched = $state(false);
  let draftEnabled = $state(true);
  let draftInterval = $state("50");
  let draftActions = $state<DirectiveAction[]>([]);
  let saving = $state(false);

  $effect(() => {
    if (open) {
      draftName = editing?.name ?? "";
      draftTrigger = editing?.trigger_text ?? "";
      draftPinyin = editing?.trigger_pinyin ?? "";
      pinyinTouched = editing != null;
      draftEnabled = editing?.enabled ?? true;
      draftInterval = String(editing?.interval_ms ?? 50);
      draftActions = editing != null ? [...editing.actions] : [];
      saving = false;
    }
  });

  // 新建时输入触发文本即自动生成拼音：防抖等停顿，未手改、弹窗仍开、文本未变才回填
  $effect(() => {
    const text = draftTrigger.trim();
    if (!open || editing != null || pinyinTouched || saving || text === "") {
      return;
    }
    const timer = setTimeout(() => {
      void (async () => {
        const suggested = await suggestDirectivePinyin(text);
        if (suggested !== "" && open && !pinyinTouched && draftTrigger.trim() === text) {
          draftPinyin = suggested;
        }
      })();
    }, 350);
    return () => clearTimeout(timer);
  });

  /** 按当前触发文本重新生成拼音建议 */
  async function handleRegeneratePinyin(): Promise<void> {
    if (saving || draftTrigger.trim() === "") return;
    saving = true;
    try {
      const suggested = await suggestDirectivePinyin(draftTrigger);
      if (suggested !== "") draftPinyin = suggested;
    } finally {
      saving = false;
    }
  }

  /** 点击键帽录制事件：按下↔松开交替，跨键可同时多按 */
  function handleKeyRecord(key: string): void {
    const direction = isKeyHeld(draftActions, key) ? "release" : "press";
    draftActions = [...draftActions, { key, direction }];
  }

  /** 保存：非空 → 拼音回填 → 判重（文本/拼音） → 序列平衡 → 落盘，成功关弹窗 */
  async function handleSave(): Promise<void> {
    if (saving) return;
    const name = draftName.trim();
    const trigger = draftTrigger.trim();
    if (name === "" || trigger === "") return;
    let pinyin = draftPinyin;
    if (pinyin.trim() === "" && trigger !== "") {
      pinyin = await suggestDirectivePinyin(trigger);
    }
    if (isTriggerDuplicate(directiveState.value, trigger, editing?.id, pinyin)) {
      toast.info(m.directives_trigger_exists());
      return;
    }
    if (!areActionsBalanced(draftActions)) {
      toast.error(m.directives_unbalanced());
      return;
    }
    const parsed = Number.parseInt(draftInterval, 10);
    const intervalMs = Number.isNaN(parsed)
      ? DIRECTIVE_INTERVAL_MIN
      : Math.min(DIRECTIVE_INTERVAL_MAX, Math.max(DIRECTIVE_INTERVAL_MIN, parsed));
    saving = true;
    try {
      const input = {
        name,
        triggerText: trigger,
        triggerPinyin: pinyin === "" ? undefined : pinyin,
        enabled: draftEnabled,
        intervalMs,
        actions: draftActions,
      };
      const ok =
        editing == null ? await createDirective(input) : await updateDirective(editing.id, input);
      if (ok) open = false;
    } finally {
      saving = false;
    }
  }
</script>

<Dialog bind:open>
  <DialogContent
    aria-label={editing == null ? m.directives_new_title() : m.directives_edit_title()}
    class="max-h-[90vh] overflow-y-auto sm:max-w-5xl"
  >
    <DialogHeader>
      <DialogTitle>
        {editing == null ? m.directives_new_title() : m.directives_edit_title()}
      </DialogTitle>
      <DialogDescription>{m.directives_editor_hint()}</DialogDescription>
    </DialogHeader>
    <div class="flex flex-col gap-3">
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label class="flex flex-col gap-1 text-sm">
          <span class="font-medium">{m.directives_name()}</span>
          <Input value={draftName} oninput={(event) => (draftName = event.currentTarget.value)} />
        </label>
        <div class="flex flex-col gap-1 text-sm">
          <span class="font-medium">{m.directives_trigger_text()}</span>
          <Input
            value={draftTrigger}
            oninput={(event) => (draftTrigger = event.currentTarget.value)}
            aria-label={m.directives_trigger_text()}
          />
        </div>
        <div class="flex flex-col gap-1 text-sm">
          <span class="font-medium">{m.directives_interval()}</span>
          <div class="flex items-center gap-2">
            <Input
              type="number"
              min={DIRECTIVE_INTERVAL_MIN}
              max={DIRECTIVE_INTERVAL_MAX}
              value={draftInterval}
              oninput={(event) => (draftInterval = event.currentTarget.value)}
              class="min-w-0 flex-1"
              aria-label={m.directives_interval()}
            />
            <label class="flex shrink-0 items-center gap-2 text-sm">
              <span class="font-medium">{m.directives_enabled_label()}</span>
              <Switch
                checked={draftEnabled}
                onCheckedChange={(checked) => (draftEnabled = checked)}
              />
            </label>
          </div>
        </div>
        <div class="flex flex-col gap-1 text-sm">
          <span class="font-medium">{m.directives_trigger_pinyin()}</span>
          <div class="flex items-center gap-2">
            <Input
              value={draftPinyin}
              oninput={(event) => {
                pinyinTouched = true;
                draftPinyin = event.currentTarget.value;
              }}
              class="min-w-0 flex-1"
              aria-label={m.directives_trigger_pinyin()}
            />
            <Button
              variant="outline"
              size="sm"
              onclick={() => void handleRegeneratePinyin()}
              disabled={saving || draftTrigger.trim() === ""}
            >
              {m.directives_regenerate_pinyin()}
            </Button>
          </div>
        </div>
      </div>
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between gap-2">
          <span class="text-sm font-medium">
            {m.directives_actions()} ({draftActions.length})
          </span>
          <Button
            variant="ghost"
            size="sm"
            onclick={() => (draftActions = [])}
            disabled={draftActions.length === 0}
          >
            {m.directives_clear_actions()}
          </Button>
        </div>
        {#if draftActions.length === 0}
          <p class="text-sm text-muted-foreground">{m.directives_actions_empty()}</p>
        {:else}
          <div class="flex flex-wrap gap-1.5">
            {#each draftActions as action, index (index)}
              <Badge variant={action.direction === "press" ? "default" : "secondary"}>
                {index + 1}. {keyLabel(action.key)}
                {action.direction === "press" ? "↓" : "↑"}
              </Badge>
            {/each}
          </div>
        {/if}
      </div>
      <KeyboardRecorder actions={draftActions} onRecord={handleKeyRecord} />
      <p class="text-xs text-muted-foreground">{m.directives_interval_hint()}</p>
    </div>
    <DialogFooter>
      <Button variant="outline" onclick={() => (open = false)}>
        {m.directives_cancel()}
      </Button>
      <Button
        onclick={() => void handleSave()}
        disabled={saving || draftName.trim() === "" || draftTrigger.trim() === ""}
      >
        {m.directives_save()}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
