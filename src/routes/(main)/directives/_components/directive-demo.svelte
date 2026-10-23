<script lang="ts">
  // 指令效果演示：只读键盘，按动作序列与触发间隔循环播放。
  // 播完停 3 秒再重播；选中切换或卸载自动取消。`prefers-reduced-motion` 下静态展示。
  // 版型自适应：量容器宽与键盘自然尺寸做双向等比缩放，缩放不占布局，
  // 外层按缩放后高度预留空间，窗口 resize（含最大化）与分栏拖拽都经 `bind:clientWidth` 跟进。
  import { SvelteSet } from "svelte/reactivity";
  import type { DirectiveAction } from "#libs/commands/types";
  import { fitScale, foldHeldKeys } from "#libs/directives/key-registry";
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
  /** 本轮触发过的键：逐事件累积，轮内保留描边，下一轮开始清空 */
  let visitedKeys = new SvelteSet<string>();

  /** 容器宽与键盘自然尺寸：双向等比缩放 */
  let containerWidth = $state(0);
  let naturalWidth = $state(0);
  let naturalHeight = $state(0);

  /** 内边距（`p-1`）：键帽描边环向外扩 2px，留 4px 呼吸空间避免被裁剪 */
  const PAD = 4;

  const scale = $derived(fitScale(containerWidth - PAD * 2, naturalWidth));

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
      visitedKeys.clear();
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
        visitedKeys.clear();
        for (let index = 0; index < sequence.length; index += 1) {
          if (cancelled) return;
          await later(step);
          if (cancelled) return;
          heldKeys.clear();
          for (const key of foldHeldKeys(sequence, index)) heldKeys.add(key);
          currentKey = sequence[index].key;
          visitedKeys.add(sequence[index].key);
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

<div
  class="overflow-hidden p-1"
  data-testid="directive-demo"
  aria-hidden="true"
  bind:clientWidth={containerWidth}
>
  <div style:height={naturalHeight > 0 ? `${naturalHeight * scale}px` : null}>
    <div
      class="w-max origin-top-left"
      style:transform={`scale(${scale})`}
      bind:clientWidth={naturalWidth}
      bind:clientHeight={naturalHeight}
    >
      <KeyGrid {heldKeys} markedKeys={visitedKeys} {currentKey} />
    </div>
  </div>
</div>
