<script lang="ts">
  // 键盘网格纯展示：三区定宽 + 固定键单位，列对齐真机 104 布局。
  // 录制（交互）与演示（只读）共用；占位只占位不渲染按钮，不进 Tab、不上报点击；
  // 当前平台不支持的键仅录制时禁用（平台经既有 `getSystemInfo` 获取，失败回落全可用）。
  import { Button } from "#components/shadcn-svelte/button";
  import commands from "#libs/commands";
  import { cn } from "#libs/utils/shadcn-svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import {
    KEYBOARD_ROWS,
    keyLabel,
    layoutSpanningCells,
    type DesktopPlatform,
    type KeyCap,
  } from "#libs/directives/key-registry";

  let {
    heldKeys,
    markedKeys = new Set<string>(),
    currentKey = null,
    interactive = false,
    onRecord,
    onPressStart,
    onPressEnd,
  }: {
    /** 填充高亮的键集合（父组件按需重建实例以触发更新） */
    heldKeys: Set<string>;
    /** 描边标记的键集合（演示轮内累积，录制器传空） */
    markedKeys?: Set<string>;
    /** 当前事件键：描边环，不分方向 */
    currentKey?: string | null;
    /** 交互模式才可点击，未传回调一律只读 */
    interactive?: boolean;
    /** 点击键帽回调，仅交互模式调用 */
    onRecord?: (key: string) => void;
    /** 按下键帽回调（按住录制用），仅交互模式调用 */
    onPressStart?: (key: string) => void;
    /** 松开键帽回调（按住录制用），仅交互模式调用 */
    onPressEnd?: (key: string) => void;
  } = $props();

  /** 当前平台（小写）：仅交互模式拉取，用于禁用本平台不支持的键 */
  let platform = $state<string | null>(null);

  $effect(() => {
    if (!interactive) return;
    void commands
      .getSystemInfo()
      .success((info) => {
        platform = info.platform.toLowerCase();
      })
      .failed(() => {
        platform = null;
      });
  });

  /** 主区定宽取最宽行，窄行尾部留白，区左沿行行对齐 */
  const mainItemsMax = Math.max(...KEYBOARD_ROWS.map((row) => row[0].length));
  const mainZoneWidth = `calc(var(--kbd-key) * 15 + var(--kbd-gap) * ${mainItemsMax - 1})`;
  const navZoneWidth = "calc(var(--kbd-key) * 3 + var(--kbd-gap) * 2)";
  const numpadZoneWidth = "calc(var(--kbd-key) * 4 + var(--kbd-gap) * 3)";

  /** 数字区跨行摆位（第 2–6 行）：`+` 与小键盘回车纵跨两行 */
  const numpadCells = layoutSpanningCells(KEYBOARD_ROWS.slice(1).map((row) => row[2]));

  /** 当前平台是否不支持该键：占位不渲染按钮，无需判断 */
  function isUnsupported(cap: KeyCap): boolean {
    if (cap.placeholder === true || platform == null) return false;
    return cap.unavailableOn?.includes(platform as DesktopPlatform) ?? false;
  }

  /** 点击守卫：非交互、占位、不支持的键一律不上报 */
  function handleCap(cap: KeyCap): void {
    if (!interactive || cap.placeholder === true || isUnsupported(cap)) return;
    onRecord?.(cap.id);
  }
</script>

{#snippet capButton(cap: KeyCap, style: string, stretched: boolean)}
  {@const pressed = heldKeys.has(cap.id)}
  {@const current = currentKey === cap.id || markedKeys.has(cap.id)}
  {@const unsupported = interactive && isUnsupported(cap)}
  <Button
    variant={pressed ? "default" : "outline"}
    size="sm"
    class={cn(
      "min-w-0 shrink-0 overflow-hidden px-1 text-xs",
      stretched ? "h-auto" : "h-9",
      current && "ring-2 ring-primary",
      !interactive && "disabled:cursor-default disabled:opacity-100",
    )}
    {style}
    onclick={() => handleCap(cap)}
    onmousedown={() => {
      if (interactive) onPressStart?.(cap.id);
    }}
    onmouseup={() => {
      if (interactive) onPressEnd?.(cap.id);
    }}
    onkeydown={(event) => {
      if (!interactive || event.repeat) return;
      if (event.key === "Enter" || event.key === " ") onPressStart?.(cap.id);
    }}
    onkeyup={(event) => {
      if (!interactive) return;
      if (event.key === "Enter" || event.key === " ") onPressEnd?.(cap.id);
    }}
    disabled={!interactive || unsupported}
    aria-pressed={pressed}
    aria-label={keyLabel(cap.id)}
    title={unsupported ? m.directives_key_unsupported() : keyLabel(cap.id)}
  >
    <span class="min-w-0 flex-1 truncate">{keyLabel(cap.id)}</span>
  </Button>
{/snippet}

{#snippet zoneRow(caps: KeyCap[])}
  <!-- 行内键总宽恒定（主区 15u），`justify-between` 让间隙吸收键数差，区左右沿行行对齐 -->
  <div class="flex justify-between">
    {#each caps as cap, capIndex (capIndex)}
      {#if cap.placeholder === true}
        <div
          aria-hidden="true"
          class="h-9 shrink-0"
          style="width: calc(var(--kbd-key) * {cap.span ?? 1})"
        ></div>
      {:else}
        {@render capButton(cap, `width: calc(var(--kbd-key) * ${cap.span ?? 1})`, false)}
      {/if}
    {/each}
  </div>
{/snippet}

<div
  class="flex w-max min-w-160 gap-3"
  data-testid="key-grid"
  style="--kbd-key: 2.25rem; --kbd-gap: 0.375rem;"
>
  <div class="flex flex-col gap-1.5" style="width: {mainZoneWidth}">
    {#each KEYBOARD_ROWS as row, rowIndex (rowIndex)}
      {@render zoneRow(row[0])}
    {/each}
  </div>
  <div class="w-3 shrink-0" aria-hidden="true"></div>
  <div class="flex flex-col gap-1.5" style="width: {navZoneWidth}">
    {#each KEYBOARD_ROWS as row, rowIndex (rowIndex)}
      {@render zoneRow(row[1])}
    {/each}
  </div>
  <div class="w-3 shrink-0" aria-hidden="true"></div>
  <div class="flex flex-col gap-1.5" style="width: {numpadZoneWidth}">
    {@render zoneRow(KEYBOARD_ROWS[0][2])}
    <div
      class="grid"
      style="grid-template-columns: repeat(4, var(--kbd-key)); grid-auto-rows: var(--kbd-key); gap: var(--kbd-gap);"
    >
      {#each numpadCells as cell (cell.row * 10 + cell.col)}
        {@render capButton(
          cell.cap,
          `grid-column: ${cell.col} / span ${cell.colSpan}; grid-row: ${cell.row} / span ${cell.rowSpan};`,
          cell.rowSpan > 1,
        )}
      {/each}
    </div>
  </div>
</div>
