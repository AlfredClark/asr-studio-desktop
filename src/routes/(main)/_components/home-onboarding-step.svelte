<script lang="ts">
  // 引导步骤行：序号 + 文案 + 右侧操作；完成态收起按钮改挂徽标。
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import { m } from "#libs/i18n/paraglide/messages";

  let {
    index,
    title,
    description,
    done,
    actionLabel,
    onAction,
    actionDisabled = false,
  }: {
    /** 步骤序号（1 起） */
    index: number;
    /** 步骤标题 */
    title: string;
    /** 步骤说明 */
    description: string;
    /** 是否已完成 */
    done: boolean;
    /** 操作按钮文案 */
    actionLabel: string;
    /** 按钮点击回调 */
    onAction: () => void;
    /** 按钮禁用态 */
    actionDisabled?: boolean;
  } = $props();
</script>

<div class="flex items-center gap-3">
  <span
    class="flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-medium tabular-nums"
    class:bg-primary={done}
    class:text-primary-foreground={done}
    class:bg-muted={!done}
    class:text-muted-foreground={!done}
    aria-hidden="true"
  >
    {index}
  </span>
  <div class="min-w-0 flex-1">
    <p class="flex items-center gap-2 text-sm font-medium">
      <span class="truncate">{title}</span>
      {#if done}
        <Badge>{m.home_step_done()}</Badge>
      {/if}
    </p>
    <p class="text-xs text-muted-foreground">{description}</p>
  </div>
  {#if !done}
    <Button size="sm" onclick={onAction} disabled={actionDisabled} class="shrink-0">
      {actionLabel}
    </Button>
  {/if}
</div>
