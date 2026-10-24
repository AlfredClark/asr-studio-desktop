<script lang="ts">
  // 指令效果演示：只读键盘，按动作序列与触发间隔循环播放。
  // 播完停 3 秒再重播；选中切换或卸载自动取消。`prefers-reduced-motion` 下给步骤列表。
  // 版型自适应：量容器宽与键盘自然尺寸做双向等比缩放，缩放不占布局，
  // 外层按缩放后高度预留空间，窗口 resize（含最大化）与分栏拖拽都经 `bind:clientWidth` 跟进。
  import { SvelteSet } from "svelte/reactivity";
  import PlayIcon from "@lucide/svelte/icons/play";
  import PauseIcon from "@lucide/svelte/icons/pause";
  import ChevronLeftIcon from "@lucide/svelte/icons/chevron-left";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import { Button } from "#components/shadcn-svelte/button";
  import { m } from "#libs/i18n/paraglide/messages";
  import type { DirectiveAction } from "#libs/commands/types";
  import { fitScale, foldHeldKeys, keyLabel } from "#libs/directives/key-registry";
  import KeyGrid from "./key-grid.svelte";

  /** 演示用的动作序列与触发间隔（毫秒） */
  let { actions, intervalMs }: { actions: DirectiveAction[]; intervalMs: number } = $props();

  /** 视觉步进下限：间隔再小也按此播放，与真实回放无关 */
  const DEMO_STEP_MIN_MS = 150;
  /** 单轮播完后的停顿：3 秒后重播 */
  const DEMO_REPLAY_PAUSE_MS = 3000;

  /** 当前点亮的按下集合（就地变异，`SvelteSet` 本身即响应式） */
  let heldKeys = new SvelteSet<string>();
  /** 当前松开的键（0~1 个）：仅松开事件描边环，按下事件无描边，借此凸显松开步骤 */
  let releasedKeys = new SvelteSet<string>();

  /** 播放中：暂停冻结当前高亮，不断开序列 */
  let playing = $state(true);
  /** 当前步进序号（-1 即尚未开始/已重置），驱动步进指示 */
  let stepIndex = $state(-1);

  /** 容器宽与键盘自然尺寸：双向等比缩放 */
  let containerWidth = $state(0);
  let naturalWidth = $state(0);
  let naturalHeight = $state(0);

  /** 内边距（`p-1`）：键帽描边环向外扩 2px，留 4px 呼吸空间避免被裁剪 */
  const PAD = 4;

  const scale = $derived(fitScale(containerWidth - PAD * 2, naturalWidth));

  /** 动作方向文案：读屏列表用，符号箭头读屏无意义 */
  function directionLabel(direction: DirectiveAction["direction"]): string {
    return direction === "press" ? m.directives_action_press() : m.directives_action_release();
  }

  /** 应用指定步的可视态：按住集重算 + 仅松开事件描边；越界即清空调零 */
  function applyStepVisual(index: number): void {
    heldKeys.clear();
    releasedKeys.clear();
    if (index < 0 || index >= actions.length) return;
    for (const key of foldHeldKeys(actions, index)) heldKeys.add(key);
    const action = actions[index];
    if (action.direction === "release") releasedKeys.add(action.key);
  }

  /** 手动跳到指定步：暂停时步进/步退用，重算按住集与松开描边 */
  function showStep(index: number): void {
    stepIndex = index;
    applyStepVisual(index);
  }

  /** 步退：到开头回绕到末尾，形成首尾衔接循环 */
  function stepBack(): void {
    if (actions.length === 0) return;
    showStep(stepIndex <= -1 ? actions.length - 1 : stepIndex - 1);
  }

  /** 步进：到末尾回绕到开头，形成首尾衔接循环 */
  function stepForward(): void {
    if (actions.length === 0) return;
    showStep(stepIndex >= actions.length - 1 ? -1 : stepIndex + 1);
  }

  /** 是否允许动画播放：减弱动态效果偏好下给静态步骤列表 */
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
      releasedKeys.clear();
      stepIndex = -1;
      return;
    }
    // 暂停只停计时器，冻结当前高亮不断开序列
    if (!playing) return;
    stepIndex = -1;
    let cancelled = false;
    const timers: number[] = [];
    const later = (ms: number): Promise<void> =>
      new Promise((resolve) => {
        timers.push(window.setTimeout(resolve, ms));
      });
    void (async () => {
      while (!cancelled) {
        releasedKeys.clear();
        for (let index = 0; index < sequence.length; index += 1) {
          if (cancelled) return;
          await later(step);
          if (cancelled) return;
          heldKeys.clear();
          for (const key of foldHeldKeys(sequence, index)) heldKeys.add(key);
          releasedKeys.clear();
          if (sequence[index].direction === "release") {
            releasedKeys.add(sequence[index].key);
          }
          stepIndex = index;
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

<div data-testid="directive-demo">
  {#if motionAllowed()}
    <div class="flex shrink-0 items-center gap-2">
      <Button
        variant="ghost"
        size="icon-sm"
        onclick={() => (playing = !playing)}
        aria-label={playing ? m.directives_demo_pause() : m.directives_demo_play()}
        title={playing ? m.directives_demo_pause() : m.directives_demo_play()}
        aria-pressed={playing}
      >
        {#if playing}
          <PauseIcon />
        {:else}
          <PlayIcon />
        {/if}
      </Button>
      {#if actions.length > 0}
        <span class="text-xs text-muted-foreground tabular-nums">
          {m.directives_demo_step({ current: stepIndex + 1, total: actions.length })}
        </span>
      {/if}
      {#if !playing}
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={stepBack}
          disabled={actions.length === 0}
          aria-label={m.directives_demo_step_back()}
          title={m.directives_demo_step_back()}
        >
          <ChevronLeftIcon />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={stepForward}
          disabled={actions.length === 0}
          aria-label={m.directives_demo_step_forward()}
          title={m.directives_demo_step_forward()}
        >
          <ChevronRightIcon />
        </Button>
      {/if}
      <span class="text-xs text-muted-foreground">
        {m.directives_demo_slowed({ ms: intervalMs })}
      </span>
    </div>
    <ol class="sr-only">
      {#each actions as action, index (index)}
        <li>{index + 1}. {keyLabel(action.key)} {directionLabel(action.direction)}</li>
      {/each}
    </ol>
    <div class="overflow-hidden p-1" aria-hidden="true" bind:clientWidth={containerWidth}>
      <div style:height={naturalHeight > 0 ? `${naturalHeight * scale}px` : null}>
        <div
          class="w-max origin-top-left"
          style:transform={`scale(${scale})`}
          bind:clientWidth={naturalWidth}
          bind:clientHeight={naturalHeight}
        >
          <KeyGrid {heldKeys} markedKeys={releasedKeys} />
        </div>
      </div>
    </div>
  {:else}
    <ol class="flex flex-col gap-1 text-sm">
      {#each actions as action, index (index)}
        <li class="text-muted-foreground tabular-nums">
          {index + 1}. {keyLabel(action.key)}
          {directionLabel(action.direction)}
        </li>
      {/each}
    </ol>
  {/if}
</div>
