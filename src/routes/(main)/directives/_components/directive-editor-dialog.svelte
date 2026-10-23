<script lang="ts">
  // 指令编辑弹窗：新建与编辑双模式，`editing == null` 即新建，分三步走完。
  // 触发词 → 按键 → 确认：拼音自动建议照抄词条编辑器（防抖 + 手改锁定 + 重新生成）；
  // 校验全部内联（字段下错误 + 平衡计），保存前复核并跳回出错步骤。
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import { Input } from "#components/shadcn-svelte/input";
  import { Switch } from "#components/shadcn-svelte/switch";
  import XIcon from "@lucide/svelte/icons/x";
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
  } from "#components/shadcn-svelte/dialog";
  import type { Directive } from "#libs/commands/types";
  import { isKeyHeld, keyLabel, unclosedKeys } from "#libs/directives/key-registry";
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
  import type { DirectiveAction } from "#libs/commands/types";
  import KeyboardRecorder from "./keyboard-recorder.svelte";
  import DirectiveDemo from "./directive-demo.svelte";

  /** 弹窗开关（父组件双向绑定）与编辑目标（空即新建） */
  let { open = $bindable(false), editing }: { open: boolean; editing: Directive | null } = $props();

  /** 当前步骤：1 触发词 → 2 按键 → 3 确认 */
  let step = $state(1);
  /** 录制模式：按住录制默认，点按切换兼容旧行为 */
  let recordMode = $state<"hold" | "toggle">("hold");

  /** 表单草稿：打开时从编辑目标回填（编辑视为已手改拼音，不再自动覆盖） */
  let draftName = $state("");
  let draftTrigger = $state("");
  let draftPinyin = $state("");
  let pinyinTouched = $state(false);
  let draftEnabled = $state(true);
  let draftInterval = $state("50");
  let draftActions = $state<DirectiveAction[]>([]);
  let saving = $state(false);

  /** 内联校验错误：下一步/保存时设置，编辑对应字段即清除 */
  let errors = $state<{ name?: string; trigger?: string; balance?: string }>({});

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
      step = 1;
      recordMode = "hold";
      errors = {};
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

  /** 解析落盘用拼音：为空且触发文本非空时先自动建议 */
  async function resolvePinyin(): Promise<string> {
    let pinyin = draftPinyin;
    if (pinyin.trim() === "" && draftTrigger.trim() !== "") {
      pinyin = await suggestDirectivePinyin(draftTrigger);
    }
    return pinyin;
  }

  /** 解析落盘用间隔：非法钳到后端同口径 */
  function parseIntervalMs(): number {
    const parsed = Number.parseInt(draftInterval, 10);
    if (Number.isNaN(parsed)) return DIRECTIVE_INTERVAL_MIN;
    return Math.min(DIRECTIVE_INTERVAL_MAX, Math.max(DIRECTIVE_INTERVAL_MIN, parsed));
  }

  /** 点击键帽（点按模式）：按下↔松开交替，跨键可同时多按 */
  function handleKeyToggle(key: string): void {
    const direction = isKeyHeld(draftActions, key) ? "release" : "press";
    draftActions = [...draftActions, { key, direction }];
  }

  /** 按下键帽（按住模式）：追加按下事件 */
  function handleKeyPress(key: string): void {
    draftActions = [...draftActions, { key, direction: "press" }];
  }

  /** 松开键帽（按住模式）：无对应按下则忽略，避免产生孤立松开 */
  function handleKeyRelease(key: string): void {
    if (!isKeyHeld(draftActions, key)) return;
    draftActions = [...draftActions, { key, direction: "release" }];
  }

  /** 移除单个事件：成对中的一个被删后由平衡校验拦截，保存时跳回按键步 */
  function removeAction(index: number): void {
    draftActions = draftActions.filter((_, i) => i !== index);
  }

  /** 未闭合的按键（按按下顺序）：有残留才展示按住条与自动闭合按钮 */
  const unclosed = $derived(unclosedKeys(draftActions));

  /** 自动闭合：把未松开的键按按下顺序依次松开 */
  function handleAutoClose(): void {
    draftActions = [
      ...draftActions,
      ...unclosed.map((key): DirectiveAction => ({ key, direction: "release" })),
    ];
  }

  // 序列快照：变化即清除平衡错误。提醒只针对点下一步那一刻的快照，
  // 快照比对保证报错写入本身不会触发清除。
  let actionsSnapshot = $state("");
  $effect(() => {
    const snapshot = draftActions.map((action) => `${action.key}:${action.direction}`).join("|");
    if (snapshot !== actionsSnapshot) {
      actionsSnapshot = snapshot;
      if (errors.balance != null) errors = { ...errors, balance: undefined };
    }
  });

  /** 第一步校验：非空 + 判重（文本/拼音），失败留内联错误 */
  async function validateTrigger(): Promise<boolean> {
    const next: typeof errors = {};
    if (draftName.trim() === "") next.name = m.directives_name_empty();
    if (draftTrigger.trim() === "") next.trigger = m.directives_trigger_empty();
    if (next.name == null && next.trigger == null) {
      const pinyin = await resolvePinyin();
      if (isTriggerDuplicate(directiveState.value, draftTrigger.trim(), editing?.id, pinyin)) {
        next.trigger = m.directives_trigger_exists();
      }
    }
    errors = { ...errors, name: next.name, trigger: next.trigger };
    return next.name == null && next.trigger == null;
  }

  /** 第二步校验：非空 + 收尾平衡，失败留内联错误 */
  function validateActions(): boolean {
    let balance: string | undefined;
    if (draftActions.length === 0) {
      balance = m.directives_actions_required();
    } else if (!areActionsBalanced(draftActions)) {
      balance = m.directives_balance_unclosed({ count: unclosed.length });
    }
    errors = { ...errors, balance };
    return balance == null;
  }

  /** 下一步：按步校验，失败停留并展示内联错误 */
  async function handleNext(): Promise<void> {
    if (saving) return;
    if (step === 1) {
      saving = true;
      try {
        if (await validateTrigger()) {
          errors = {};
          step = 2;
        }
      } finally {
        saving = false;
      }
      return;
    }
    if (step === 2 && validateActions()) {
      errors = { ...errors, balance: undefined };
      step = 3;
    }
  }

  /** 保存：复核全部校验，失败跳回出错步骤；成功关弹窗 */
  async function handleSave(): Promise<void> {
    if (saving) return;
    saving = true;
    try {
      if (!(await validateTrigger())) {
        step = 1;
        return;
      }
      if (!validateActions()) {
        step = 2;
        return;
      }
      const pinyin = await resolvePinyin();
      const ok =
        editing == null
          ? await createDirective({
              name: draftName.trim(),
              triggerText: draftTrigger.trim(),
              triggerPinyin: pinyin === "" ? undefined : pinyin,
              enabled: draftEnabled,
              intervalMs: parseIntervalMs(),
              actions: draftActions,
            })
          : await updateDirective(editing.id, {
              name: draftName.trim(),
              triggerText: draftTrigger.trim(),
              triggerPinyin: pinyin === "" ? undefined : pinyin,
              enabled: draftEnabled,
              intervalMs: parseIntervalMs(),
              actions: draftActions,
            });
      if (ok) open = false;
    } finally {
      saving = false;
    }
  }

  /** 步骤指示：当前步高亮，其余置灰 */
  const stepLabel = $derived(
    step === 1
      ? m.directives_step_trigger()
      : step === 2
        ? m.directives_step_keys()
        : m.directives_step_review(),
  );
</script>

<Dialog bind:open>
  <DialogContent
    aria-label={editing == null ? m.directives_new_title() : m.directives_edit_title()}
    class={step === 1
      ? "max-h-[90vh] overflow-y-auto sm:max-w-2xl"
      : "max-h-[90vh] overflow-y-auto sm:max-w-5xl"}
  >
    <DialogHeader>
      <DialogTitle>
        {editing == null ? m.directives_new_title() : m.directives_edit_title()}
      </DialogTitle>
      <DialogDescription>{m.directives_editor_hint()}</DialogDescription>
    </DialogHeader>
    <ol class="flex shrink-0 items-center gap-2 text-xs" aria-label={stepLabel}>
      <li
        aria-current={step === 1 ? "step" : undefined}
        class={step === 1 ? "font-medium text-foreground" : "text-muted-foreground"}
      >
        1. {m.directives_step_trigger()}
      </li>
      <li aria-hidden="true" class="text-muted-foreground">·</li>
      <li
        aria-current={step === 2 ? "step" : undefined}
        class={step === 2 ? "font-medium text-foreground" : "text-muted-foreground"}
      >
        2. {m.directives_step_keys()}
      </li>
      <li aria-hidden="true" class="text-muted-foreground">·</li>
      <li
        aria-current={step === 3 ? "step" : undefined}
        class={step === 3 ? "font-medium text-foreground" : "text-muted-foreground"}
      >
        3. {m.directives_step_review()}
      </li>
    </ol>
    {#if step === 1}
      <div class="flex flex-col gap-3">
        <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label class="flex flex-col gap-1 text-sm">
            <span class="font-medium">{m.directives_name()}</span>
            <Input
              value={draftName}
              oninput={(event) => {
                draftName = event.currentTarget.value;
                errors = { ...errors, name: undefined };
              }}
              aria-invalid={errors.name != null}
            />
            {#if errors.name != null}
              <span class="text-xs text-destructive">{errors.name}</span>
            {/if}
          </label>
          <div class="flex flex-col gap-1 text-sm">
            <span class="font-medium">{m.directives_trigger_text()}</span>
            <Input
              value={draftTrigger}
              oninput={(event) => {
                draftTrigger = event.currentTarget.value;
                errors = { ...errors, trigger: undefined };
              }}
              aria-label={m.directives_trigger_text()}
              aria-invalid={errors.trigger != null}
            />
            {#if errors.trigger != null}
              <span class="text-xs text-destructive">{errors.trigger}</span>
            {/if}
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
            <p class="text-xs text-muted-foreground">{m.directives_interval_hint()}</p>
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
      </div>
    {:else if step === 2}
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between gap-2">
          <span class="min-w-0 flex-1 truncate text-sm font-medium">
            {m.directives_actions()} ({draftActions.length}){#if unclosed.length > 0}<span
                class="font-normal text-muted-foreground"
              >
                · {m.directives_holding()}：{unclosed.map((key) => keyLabel(key)).join(", ")}
              </span>{/if}{#if errors.balance != null}<span class="font-normal text-destructive">
                · {errors.balance}
              </span>{/if}
          </span>
          <div class="flex items-center gap-2">
            {#if unclosed.length > 0}
              <Button variant="outline" size="sm" onclick={handleAutoClose}>
                {m.directives_auto_close()}
              </Button>
            {/if}
            <Button
              variant="ghost"
              size="sm"
              onclick={() => (draftActions = [])}
              disabled={draftActions.length === 0}
            >
              {m.directives_clear_actions()}
            </Button>
          </div>
        </div>
        {#if draftActions.length === 0}
          <p class="text-sm text-muted-foreground">{m.directives_actions_empty()}</p>
        {:else}
          <div class="flex flex-wrap items-center gap-1.5">
            {#each draftActions as action, index (index)}
              <span class="inline-flex items-center">
                <Badge variant={action.direction === "press" ? "default" : "secondary"} class="h-7">
                  {index + 1}. {keyLabel(action.key)}
                  {action.direction === "press" ? "↓" : "↑"}
                </Badge>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  onclick={() => removeAction(index)}
                  aria-label={m.directives_remove_action()}
                  title={m.directives_remove_action()}
                >
                  <XIcon />
                </Button>
              </span>
            {/each}
          </div>
        {/if}
        <div class="flex items-center gap-1" role="group" aria-label={m.directives_actions()}>
          <Button
            variant={recordMode === "hold" ? "default" : "outline"}
            size="sm"
            onclick={() => (recordMode = "hold")}
          >
            {m.directives_record_hold()}
          </Button>
          <Button
            variant={recordMode === "toggle" ? "default" : "outline"}
            size="sm"
            onclick={() => (recordMode = "toggle")}
          >
            {m.directives_record_toggle()}
          </Button>
        </div>
        <div class="overflow-x-auto">
          <KeyboardRecorder
            actions={draftActions}
            mode={recordMode}
            onPress={handleKeyPress}
            onRelease={handleKeyRelease}
            onToggle={handleKeyToggle}
          />
        </div>
      </div>
    {:else}
      <div class="flex flex-col gap-3">
        <dl class="grid shrink-0 grid-cols-2 gap-2 text-sm">
          <div class="flex min-w-0 gap-2">
            <dt class="shrink-0 font-medium">{m.directives_directive_name()}</dt>
            <dd class="min-w-0 flex-1 truncate">{draftName.trim()}</dd>
          </div>
          <div class="flex min-w-0 gap-2">
            <dt class="shrink-0 font-medium">{m.directives_trigger_text()}</dt>
            <dd class="min-w-0 flex-1 truncate">{draftTrigger.trim()}</dd>
          </div>
          <div class="flex min-w-0 gap-2">
            <dt class="shrink-0 font-medium">{m.directives_interval_label()}</dt>
            <dd class="shrink-0 tabular-nums">{parseIntervalMs()} ms</dd>
          </div>
          <div class="flex min-w-0 gap-2">
            <dt class="shrink-0 font-medium">{m.directives_trigger_pinyin_label()}</dt>
            <dd class="min-w-0 flex-1 truncate text-muted-foreground">{draftPinyin}</dd>
          </div>
        </dl>
        <div class="flex flex-col gap-1.5">
          <p class="text-sm font-medium">
            {m.directives_actions()} ({draftActions.length})
          </p>
          <div class="flex flex-wrap gap-1.5">
            {#each draftActions as action, index (index)}
              <Badge variant={action.direction === "press" ? "default" : "secondary"} class="h-7">
                {index + 1}. {keyLabel(action.key)}
                {action.direction === "press" ? "↓" : "↑"}
              </Badge>
            {/each}
          </div>
        </div>
        <div class="flex flex-col gap-1.5">
          <p class="text-sm font-medium">{m.directives_demo_title()}</p>
          <DirectiveDemo actions={draftActions} intervalMs={parseIntervalMs()} />
        </div>
      </div>
    {/if}
    <DialogFooter>
      <Button variant="outline" onclick={() => (open = false)}>
        {m.directives_cancel()}
      </Button>
      {#if step > 1}
        <Button variant="outline" onclick={() => (step -= 1)}>
          {m.directives_prev()}
        </Button>
      {/if}
      {#if step < 3}
        <Button onclick={() => void handleNext()} disabled={saving}>
          {m.directives_next()}
        </Button>
      {:else}
        <Button onclick={() => void handleSave()} disabled={saving}>
          {m.directives_save()}
        </Button>
      {/if}
    </DialogFooter>
  </DialogContent>
</Dialog>
