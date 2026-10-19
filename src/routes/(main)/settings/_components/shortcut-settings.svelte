<script lang="ts">
  // 快捷键设置分组：录制“快速打开应用”的全局快捷键，经后端命令落盘并即时注册。
  // 录制是“点击开始、按键结束”的单次交互；保存成功后查询注册状态，未生效提示换组合。
  import CardRow from "#components/common/card-row.svelte";
  import CardSection from "#components/common/card-section.svelte";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import XIcon from "@lucide/svelte/icons/x";
  import commands from "#libs/commands";
  import { reportCommandFailure } from "#libs/commands/cores";
  import { configState, updateConfig } from "#libs/hooks/config.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import {
    formatAccelerator,
    hasAnyModifier,
    isModifierKey,
    isValidAccelerator,
    type ShortcutModifiers,
  } from "#libs/utils/shortcut";
  import { toast } from "#libs/utils/toast";

  /** 录制中：按下主键后自动结束，`Esc` 取消 */
  let recording = $state(false);

  /** 保存中：禁用录制按钮与清除按钮，避免重复提交 */
  let saving = $state(false);

  // 后端配置是唯一真值；水合前回落空字符串，渲染占位
  const stored = $derived(configState.value?.shortcut_quick_open ?? "");

  /** 待机展示的按键分段：`Ctrl+Shift+O` 拆成三段，每段一个徽标 */
  const parts = $derived(stored === "" ? [] : stored.split("+"));

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
      toast.error(m.settings_quick_open_invalid());
      return;
    }
    recording = false;
    await saveShortcut(combo);
  }

  /** 落盘并即时注册；注册失败保留新值，提示换组合 */
  async function saveShortcut(value: string): Promise<void> {
    if (saving) return;
    saving = true;
    try {
      if (!(await updateConfig({ shortcut_quick_open: value }))) {
        toast.error(m.settings_quick_open_update_failed());
        return;
      }
      if (value === "") {
        toast.success(m.settings_quick_open_updated());
        return;
      }
      const result = await commands.isShortcutRegistered(value).result();
      if (result.status === "ok" && result.data) {
        toast.success(m.settings_quick_open_updated());
        return;
      }
      if (result.status === "error") {
        reportCommandFailure("[shortcut] failed to query registration", result.error);
      }
      toast.warning(m.settings_quick_open_not_registered());
    } finally {
      saving = false;
    }
  }
</script>

<CardSection title={m.settings_shortcut_title()} description={m.settings_shortcut_description()}>
  <CardRow label={m.settings_quick_open_label()} description={m.settings_quick_open_description()}>
    {#snippet control()}
      {#if recording}
        <button
          type="button"
          class="w-(--control-column-width) rounded-md border px-3 py-2 text-left text-sm"
          onblur={() => {
            recording = false;
          }}
          disabled={saving}
          aria-label={m.settings_quick_open_label()}
          title={m.settings_quick_open_hint()}
        >
          {m.settings_quick_open_recording()}
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
            <span class="text-sm text-muted-foreground">{m.settings_quick_open_placeholder()}</span>
          {/if}
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={m.settings_quick_open_record()}
            title={m.settings_quick_open_record()}
            onclick={startRecording}
            disabled={saving}
          >
            <PencilIcon />
          </Button>
          {#if parts.length > 0}
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={m.settings_quick_open_clear()}
              title={m.settings_quick_open_clear()}
              onclick={() => void saveShortcut("")}
              disabled={saving}
            >
              <XIcon />
            </Button>
          {/if}
        </div>
      {/if}
    {/snippet}
  </CardRow>
</CardSection>
