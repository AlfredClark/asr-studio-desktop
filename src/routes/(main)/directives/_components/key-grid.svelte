<script lang="ts">
  // 键盘网格纯展示：行列布局来自按键注册表，状态全由父组件传入。
  // 录制（交互）与演示（只读）共用；渲染 key 用行列序号。
  import { Button } from "#components/shadcn-svelte/button";
  import { cn } from "#libs/utils/shadcn-svelte";
  import { KEYBOARD_ROWS, keyLabel } from "#libs/directives/key-registry";

  let {
    heldKeys,
    markedKeys = new Set<string>(),
    currentKey = null,
    interactive = false,
    onRecord,
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
  } = $props();
</script>

<div class="flex min-w-160 flex-col gap-1.5" data-testid="key-grid">
  {#each KEYBOARD_ROWS as row, rowIndex (rowIndex)}
    <div class="flex gap-1.5">
      {#each row as cluster, clusterIndex (clusterIndex)}
        {#if clusterIndex > 0}
          <div class="w-3 shrink-0" aria-hidden="true"></div>
        {/if}
        {#each cluster as cap, capIndex (capIndex)}
          {@const pressed = heldKeys.has(cap.id)}
          {@const current = currentKey === cap.id || markedKeys.has(cap.id)}
          <Button
            variant={pressed ? "default" : "outline"}
            size="sm"
            class={cn(
              "h-9 min-w-0 flex-1 overflow-hidden px-1 text-xs",
              current && "ring-2 ring-primary",
              !interactive && "disabled:cursor-default disabled:opacity-100",
            )}
            style={`flex-grow: ${cap.span ?? 1};`}
            onclick={interactive && onRecord ? () => onRecord(cap.id) : undefined}
            disabled={!interactive}
            aria-pressed={pressed}
            aria-label={keyLabel(cap.id)}
            title={keyLabel(cap.id)}
          >
            <span class="min-w-0 flex-1 truncate">{keyLabel(cap.id)}</span>
          </Button>
        {/each}
      {/each}
    </div>
  {/each}
</div>
