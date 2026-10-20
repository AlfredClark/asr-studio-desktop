<script lang="ts">
  // 模型分组合：分类标题 + 数量 + 折叠开关，默认全展开。
  // 折叠态放组件内部，页面不存；空分组由调用方直接不渲染。
  // 刻意不用按钮面（无 Steel 底、无 hover 底）：选中模型的 Steel 底是唯一的视觉重心。
  import type { Snippet } from "svelte";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import { cn } from "#libs/utils/shadcn-svelte";

  /** 分组标题与条目数，下方行经 children 传入 */
  let { title, count, children }: { title: string; count: number; children: Snippet } = $props();

  /** 展开态：组件内部状态，切页重建即回到全展开 */
  let expanded = $state(true);
</script>

<div>
  <button
    type="button"
    class="flex h-7 w-full items-center gap-1 rounded-md px-2 text-left text-sm focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
    aria-expanded={expanded}
    onclick={() => (expanded = !expanded)}
  >
    <ChevronDownIcon
      class={cn("size-4 shrink-0 transition-transform duration-100", !expanded && "-rotate-90")}
    />
    <span class="font-medium">{title}</span>
    <span class="ml-auto text-xs tabular-nums">{count}</span>
  </button>
  {#if expanded}
    <div class="mt-1 flex flex-col gap-0.5">
      {@render children()}
    </div>
  {/if}
</div>
