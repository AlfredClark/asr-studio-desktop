<script lang="ts">
  // 指令效果演示：只读键盘，按动作序列与触发间隔循环播放。
  // 播完停 3 秒再重播；选中切换或卸载自动取消。`prefers-reduced-motion` 下静态展示。
  import { SvelteSet } from "svelte/reactivity";
  import type { DirectiveAction } from "#libs/commands/types";
  import { foldHeldKeys } from "#libs/directives/key-registry";
  import KeyGrid from "./key-grid.svelte";

  /** 演示用的动作序列与触发间隔（毫秒） */
  let { actions, intervalMs }: { actions: DirectiveAction[]; intervalMs: number } = $props();

  /** 视觉步进下限：间隔再小也按此播放，与真实回放无关 */
  const DEMO_STEP_MIN_MS = 150;
  /** 单轮播完后的停顿：3 秒后重播 */
  const DEMO_REPLAY_PAUSE_MS = 3000;

  /** 当前点亮的按下集合（就地变异，`SvelteSet` 本身即响应式） */
  let heldKeys = new SvelteSet<string>();
  /** 当前事件键：描边环，不分方向 */
  let currentKey = $state<string | null>(null);

  /** 是否允许自动播放：减弱动态效果偏好下静态展示 */
  function motionAllowed(): boolean {
    return (
      typeof window === "undefined" ||
      typeof window.matchMedia !== "function" ||
      !window.matchMedia("(prefers-reduced-motion: reduce)").matches
    );
  }

  $effect(() => {
    const sequence = actions;
    const step = Math.max(intervalMs, DEMO_STEP_MIN_MS);
    if (sequence.length === 0 || !motionAllowed()) {
      heldKeys.clear();
      currentKey = null;
      return;
    }
    let cancelled = false;
    const timers: number[] = [];
    const later = (ms: number): Promise<void> =>
      new Promise((resolve) => {
        timers.push(window.setTimeout(resolve, ms));
      });
    void (async () => {
      while (!cancelled) {
        for (let index = 0; index < sequence.length; index += 1) {
          if (cancelled) return;
          await later(step);
          if (cancelled) return;
          heldKeys.clear();
          for (const key of foldHeldKeys(sequence, index)) heldKeys.add(key);
          currentKey = sequence[index].key;
        }
        if (cancelled) return;
        await later(DEMO_REPLAY_PAUSE_MS);
      }
    })();
    return () => {
      cancelled = true;
      for (const timer of timers) window.clearTimeout(timer);
    };
  });
</script>

<div class="overflow-x-auto" data-testid="directive-demo" aria-hidden="true">
  <KeyGrid {heldKeys} {currentKey} />
</div>
