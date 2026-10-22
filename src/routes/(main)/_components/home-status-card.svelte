<script lang="ts">
  // 首页状态卡：标题 + 内容区 + 底部操作按钮，四卡布局统一，行为由调用方持有。
  import type { Snippet } from "svelte";
  import { Button } from "#components/shadcn-svelte/button";
  import {
    Card,
    CardContent,
    CardFooter,
    CardHeader,
    CardTitle,
  } from "#components/shadcn-svelte/card";

  let {
    title,
    actionLabel,
    onAction,
    actionDisabled = false,
    headerExtra,
    secondaryLabel,
    onSecondaryAction,
    children,
  }: {
    /** 卡片标题（导航名复用） */
    title: string;
    /** 底部主操作按钮文案（右对齐） */
    actionLabel: string;
    /** 主按钮点击回调 */
    onAction: () => void;
    /** 主按钮禁用态 */
    actionDisabled?: boolean;
    /** 标题栏右侧附加内容（如状态徽标），缺省不渲染 */
    headerExtra?: Snippet;
    /** 底部次操作按钮文案（左对齐），缺省不渲染 */
    secondaryLabel?: string;
    /** 次按钮点击回调 */
    onSecondaryAction?: () => void;
    /** 卡片内容（徽标 + 摘要行） */
    children: Snippet;
  } = $props();
</script>

<Card>
  <CardHeader>
    <div class="flex items-center justify-between gap-2">
      <CardTitle>{title}</CardTitle>
      {#if headerExtra}
        <div class="shrink-0">{@render headerExtra()}</div>
      {/if}
    </div>
  </CardHeader>
  <CardContent class="flex-1">
    {@render children()}
  </CardContent>
  <CardFooter class={secondaryLabel ? "justify-between" : undefined}>
    {#if secondaryLabel}
      <Button size="sm" variant="ghost" onclick={onSecondaryAction}>
        {secondaryLabel}
      </Button>
    {/if}
    <Button size="sm" variant="outline" onclick={onAction} disabled={actionDisabled}>
      {actionLabel}
    </Button>
  </CardFooter>
</Card>
