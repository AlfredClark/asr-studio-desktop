<script lang="ts">
  // 快捷键录制行：徽标展示 + 录制按钮 + 清除按钮，三种快捷键（快速显示 / 语音识别 / 快捷词库）共用。
  // 文案与配置键按 `kind` 内聚在此，调用方只传 kind；落盘与注册查询走同一链路。
  // 语音与快捷词库注册均与服务运行态同生命周期：停止态只落盘不查注册（启动成功后自动接管并核验）。
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import XIcon from "@lucide/svelte/icons/x";
  import commands from "#libs/commands";
  import { reportCommandFailure } from "#libs/commands/cores";
  import { configValue, updateConfig } from "#libs/hooks/config.svelte";
  import { serviceState } from "#libs/hooks/service.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import {
    formatAccelerator,
    hasAnyModifier,
    isModifierKey,
    isValidAccelerator,
    type ShortcutModifiers,
  } from "#libs/utils/shortcut";
  import { toast } from "#libs/utils/toast";

  /** 行种类：决定配置键与文案 */
  type Kind = "quick_open" | "voice" | "quick_lexicon";

  let { kind }: { kind: Kind } = $props();

  /** 录制中：按下主键后自动结束，`Esc` 取消 */
  let recording = $state(false);

  /** 保存中：禁用录制按钮与清除按钮，避免重复提交 */
  let saving = $state(false);

  /** 配置键：`quick_open` 走快速显示，`voice` 走语音识别，`quick_lexicon` 走快捷词库 */
  const configKey = $derived(
    kind === "voice"
      ? "shortcut_voice"
      : kind === "quick_lexicon"
        ? "shortcut_quick_lexicon"
        : "shortcut_quick_open",
  );

  // 后端配置是唯一真值；水合前回落空字符串，渲染占位
  const stored = $derived(configValue(configKey, ""));

  /** 待机展示的按键分段：`Ctrl+Shift+O` 拆成三段，每段一个徽标 */
  const parts = $derived(stored === "" ? [] : stored.split("+"));

  /** 行文案：三行各一套，互不串扰 */
  const texts = $derived(
    kind === "voice"
      ? {
          label: m.settings_voice_label(),
          description: m.settings_voice_description(),
          placeholder: m.settings_voice_placeholder(),
          recording: m.settings_voice_recording(),
          hint: m.settings_voice_hint(),
          record: m.settings_voice_record(),
          clear: m.settings_voice_clear(),
          invalid: m.settings_voice_invalid(),
          updated: m.settings_voice_updated(),
          updateFailed: m.settings_voice_update_failed(),
          notRegistered: m.settings_voice_not_registered(),
        }
      : kind === "quick_lexicon"
        ? {
            label: m.settings_quick_lexicon_label(),
            description: m.settings_quick_lexicon_description(),
            placeholder: m.settings_quick_lexicon_placeholder(),
            recording: m.settings_quick_lexicon_recording(),
            hint: m.settings_quick_lexicon_hint(),
            record: m.settings_quick_lexicon_record(),
            clear: m.settings_quick_lexicon_clear(),
            invalid: m.settings_quick_lexicon_invalid(),
            updated: m.settings_quick_lexicon_updated(),
            updateFailed: m.settings_quick_lexicon_update_failed(),
            notRegistered: m.settings_quick_lexicon_not_registered(),
          }
        : {
            label: m.settings_quick_open_label(),
            description: m.settings_quick_open_description(),
            placeholder: m.settings_quick_open_placeholder(),
            recording: m.settings_quick_open_recording(),
            hint: m.settings_quick_open_hint(),
            record: m.settings_quick_open_record(),
            clear: m.settings_quick_open_clear(),
            invalid: m.settings_quick_open_invalid(),
            updated: m.settings_quick_open_updated(),
            updateFailed: m.settings_quick_open_update_failed(),
            notRegistered: m.settings_quick_open_not_registered(),
          },
  );

  /** 录制态才监听全局按键，捕获阶段拦截，避免触发页面其它快捷键 */
  $effect(() => {
    if (!recording) return;
    const onKeyDown = (event: KeyboardEvent): void => {
      void handleRecordKey(event);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
    };
  });

  /** 开始录制；保存中不进入，避免并发提交 */
  function startRecording(): void {
    if (saving || recording) return;
    recording = true;
  }

  /** 从键盘事件提取修饰键快照 */
  function modifiersOf(event: KeyboardEvent): ShortcutModifiers {
    return {
      ctrl: event.ctrlKey,
      alt: event.altKey,
      shift: event.shiftKey,
      super: event.metaKey,
    };
  }

  /** 录制按键：纯修饰键等待主键，单键忽略，`Esc` 取消，`Backspace` 清空 */
  async function handleRecordKey(event: KeyboardEvent): Promise<void> {
    event.preventDefault();
    event.stopPropagation();
    const modifiers = modifiersOf(event);
    if (event.key === "Escape") {
      recording = false;
      return;
    }
    if ((event.key === "Backspace" || event.key === "Delete") && !hasAnyModifier(modifiers)) {
      recording = false;
      await saveShortcut("");
      return;
    }
    if (isModifierKey(event.key)) return;
    if (!hasAnyModifier(modifiers)) return;
    const combo = formatAccelerator(modifiers, event.key);
    if (!isValidAccelerator(combo)) {
      toast.error(texts.invalid);
      return;
    }
    recording = false;
    await saveShortcut(combo);
  }

  /**
   * 落盘并即时注册；注册失败保留新值，提示换组合。语音与快捷词库停止态只落盘：注册查询恒为假
   * （注册与服务运行态同生命周期），查了反而误报，启动成功后由服务状态机统一核验。
   */
  async function saveShortcut(value: string): Promise<void> {
    if (saving) return;
    saving = true;
    try {
      const patch =
        kind === "voice"
          ? { shortcut_voice: value }
          : kind === "quick_lexicon"
            ? { shortcut_quick_lexicon: value }
            : { shortcut_quick_open: value };
      if (!(await updateConfig(patch))) {
        toast.error(texts.updateFailed);
        return;
      }
      if (value === "") {
        toast.success(texts.updated);
        return;
      }
      const live = kind === "quick_open" || serviceState.status?.state === "running";
      if (!live) {
        toast.success(texts.updated);
        return;
      }
      const result = await commands.isShortcutRegistered(value).result();
      if (result.status === "ok" && result.data) {
        toast.success(texts.updated);
        return;
      }
      if (result.status === "error") {
        reportCommandFailure("[shortcut] failed to query registration", result.error);
      }
      toast.warning(texts.notRegistered);
    } finally {
      saving = false;
    }
  }
</script>

{#if recording}
  <button
    type="button"
    class="w-(--control-column-width) rounded-md border px-3 py-2 text-left text-sm"
    onblur={() => {
      recording = false;
    }}
    disabled={saving}
    aria-label={texts.label}
    title={texts.hint}
  >
    {texts.recording}
  </button>
{:else}
  <div class="flex items-center gap-1.5">
    {#if parts.length > 0}
      {#each parts as part, index (part)}
        {#if index > 0}
          <span class="text-xs text-muted-foreground" aria-hidden="true">+</span>
        {/if}
        <Badge variant="secondary">{part}</Badge>
      {/each}
    {:else}
      <span class="text-sm text-muted-foreground">{texts.placeholder}</span>
    {/if}
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={texts.record}
      title={texts.record}
      onclick={startRecording}
      disabled={saving}
    >
      <PencilIcon />
    </Button>
    {#if parts.length > 0}
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={texts.clear}
        title={texts.clear}
        onclick={() => void saveShortcut("")}
        disabled={saving}
      >
        <XIcon />
      </Button>
    {/if}
  </div>
{/if}
