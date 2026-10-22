<script lang="ts">
  // 录制键盘：交互 wrapper，状态归父组件所有，点击即上报。
  // 渲染与演示共用 `key-grid`，此处只做全量折叠的按下态派生。
  import { SvelteSet } from "svelte/reactivity";
  import type { DirectiveAction } from "#libs/commands/types";
  import KeyGrid from "./key-grid.svelte";

  /** 已录制事件序列（父组件所有，清空即复位键盘） */
  let { actions, onRecord }: { actions: DirectiveAction[]; onRecord: (key: string) => void } =
    $props();

  /** 按下态：序列中该键最后一个事件为按下即高亮 */
  const heldKeys = $derived.by(() => {
    const held = new SvelteSet<string>();
    const seen = new SvelteSet<string>();
    for (let index = actions.length - 1; index >= 0; index -= 1) {
      const action = actions[index];
      if (seen.has(action.key)) continue;
      seen.add(action.key);
      if (action.direction === "press") held.add(action.key);
    }
    return held;
  });
</script>

<div data-testid="keyboard-recorder">
  <KeyGrid {heldKeys} interactive {onRecord} />
</div>
