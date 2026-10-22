<script lang="ts">
  // 指令详情：只读展示，空选中给引导；序列 chips 与编辑器预览同口径（`keyLabel` + ↓/↑）。
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import type { Directive } from "#libs/commands/types";
  import { keyLabel } from "#libs/directives/key-registry";
  import { m } from "#libs/i18n/paraglide/messages";
  import DirectiveDemo from "./directive-demo.svelte";

  let { directive, onEdit }: { directive: Directive | null; onEdit: () => void } = $props();
</script>

{#if directive == null}
  <div class="flex min-h-0 flex-1 items-center justify-center">
    <p class="text-sm text-muted-foreground">{m.directives_pick_hint()}</p>
  </div>
{:else}
  <div class="flex min-h-0 flex-1 flex-col gap-3" data-testid="directive-detail">
    <div class="flex shrink-0 items-center justify-between gap-2">
      <h3 class="min-w-0 flex-1 truncate text-sm font-medium">{directive.name}</h3>
      <div class="flex shrink-0 items-center gap-2">
        {#if directive.enabled}
          <Badge>{m.directives_enabled()}</Badge>
        {/if}
        <Button variant="outline" size="sm" onclick={onEdit}>{m.directives_edit()}</Button>
      </div>
    </div>
    <dl class="flex shrink-0 flex-col gap-2 text-sm">
      <div class="flex gap-2">
        <dt class="shrink-0 font-medium">{m.directives_trigger_text()}</dt>
        <dd class="min-w-0 flex-1 truncate">{directive.trigger_text}</dd>
      </div>
      <div class="flex gap-2">
        <dt class="shrink-0 font-medium">{m.directives_trigger_pinyin_label()}</dt>
        <dd class="min-w-0 flex-1 truncate text-muted-foreground">{directive.trigger_pinyin}</dd>
      </div>
      <div class="flex gap-2">
        <dt class="shrink-0 font-medium">{m.directives_interval_label()}</dt>
        <dd class="shrink-0 tabular-nums">{directive.interval_ms} ms</dd>
      </div>
    </dl>
    <div class="flex min-h-0 flex-1 flex-col gap-1.5">
      <p class="shrink-0 text-sm font-medium">
        {m.directives_actions()} ({directive.actions.length})
      </p>
      <div class="flex flex-wrap gap-1.5 overflow-y-auto">
        {#each directive.actions as action, index (index)}
          <Badge variant={action.direction === "press" ? "default" : "secondary"} class="h-7">
            {index + 1}. {keyLabel(action.key)}
            {action.direction === "press" ? "↓" : "↑"}
          </Badge>
        {/each}
      </div>
    </div>
    <div class="flex shrink-0 flex-col gap-1.5">
      <p class="text-sm font-medium">{m.directives_demo_title()}</p>
      <p class="text-xs text-muted-foreground">{m.directives_demo_hint()}</p>
      <DirectiveDemo actions={directive.actions} intervalMs={directive.interval_ms} />
    </div>
  </div>
{/if}
