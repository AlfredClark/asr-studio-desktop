<script lang="ts">
  // 录制键盘：交互 wrapper，状态归父组件所有，按模式上报按下/松开或点按切换。
  // 按住模式默认：`mousedown` 按下、`mouseup` 松开（窗口级兜底拖出松开）；
  // 点按模式兼容旧行为：点击按 `isKeyHeld` 交替。渲染与演示共用 `key-grid`，
  // 此处只做全量折叠的按下态派生。
  import { SvelteSet } from "svelte/reactivity";
  import type { DirectiveAction } from "#libs/commands/types";
  import KeyGrid from "./key-grid.svelte";

  /** 已录制事件序列（父组件所有，清空即复位键盘）与录制模式及上报回调 */
  let {
    actions,
    mode,
    onPress,
    onRelease,
    onToggle,
  }: {
    actions: DirectiveAction[];
    /** 按住录制（默认）或点按切换 */
    mode: "hold" | "toggle";
    /** 按下键帽：追加按下事件 */
    onPress: (key: string) => void;
    /** 松开键帽：追加松开事件 */
    onRelease: (key: string) => void;
    /** 点按切换：按下↔松开交替 */
    onToggle: (key: string) => void;
  } = $props();

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

  /** 按住中的键：单指针同一时间只按住一个键帽 */
  let pressingKey = $state<string | null>(null);

  /** 按下：重复触发直接忽略，在途清空不影响指针态 */
  function handlePressStart(key: string): void {
    if (pressingKey === key) return;
    pressingKey = key;
    onPress(key);
  }

  /** 松开：只认当前按住的键，拖出键帽由窗口级兜底 */
  function handlePressEnd(key: string): void {
    if (pressingKey !== key) return;
    pressingKey = null;
    onRelease(key);
  }

  // 按住模式下窗口级松开兜底：按住拖出键帽再松开，指针仍能闭合
  $effect(() => {
    if (mode !== "hold") return;
    const release = (): void => {
      if (pressingKey != null) handlePressEnd(pressingKey);
    };
    window.addEventListener("mouseup", release);
    return () => window.removeEventListener("mouseup", release);
  });
</script>

<div data-testid="keyboard-recorder">
  {#if mode === "hold"}
    <KeyGrid {heldKeys} interactive onPressStart={handlePressStart} onPressEnd={handlePressEnd} />
  {:else}
    <KeyGrid {heldKeys} interactive onRecord={onToggle} />
  {/if}
</div>
