<script lang="ts">
  // 指令列表项：选中态按钮 + 启用徽标 + 图标操作（启用切换 / 编辑 / 更多）。
  // 更多菜单沿用词库分组的 DropdownMenu 写法（上移 / 下移 / 删除，触顶触底禁用）。
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import PlayIcon from "@lucide/svelte/icons/play";
  import CheckIcon from "@lucide/svelte/icons/check";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";
  import { cn } from "#libs/utils/shadcn-svelte";
  import type { Directive } from "#libs/commands/types";
  import { m } from "#libs/i18n/paraglide/messages";

  let {
    directive,
    selected,
    isFirst,
    isLast,
    onSelect,
    onToggle,
    onEdit,
    onMove,
    onDelete,
  }: {
    directive: Directive;
    selected: boolean;
    isFirst: boolean;
    isLast: boolean;
    onSelect: () => void;
    onToggle: () => void;
    onEdit: () => void;
    onMove: (direction: "up" | "down") => void;
    onDelete: () => void;
  } = $props();
</script>

<div class="rounded-lg px-1 py-1">
  <div class="flex items-center gap-1">
    <Button
      variant={selected ? "secondary" : "ghost"}
      class={cn("min-w-0 flex-1 justify-start gap-2 px-2 font-normal", selected && "font-medium")}
      onclick={onSelect}
      aria-current={selected ? "true" : undefined}
    >
      <span class="min-w-0 flex-1 truncate text-left text-sm">
        {directive.name}
        <span class="text-muted-foreground"> · {directive.trigger_text}</span>
        <span class="text-muted-foreground tabular-nums"> ({directive.actions.length})</span>
      </span>
      {#if directive.enabled}
        <Badge>{m.directives_enabled()}</Badge>
      {/if}
    </Button>
    <Button
      variant={directive.enabled ? "secondary" : "ghost"}
      size="icon-sm"
      onclick={onToggle}
      aria-pressed={directive.enabled}
      aria-label={directive.enabled ? m.directives_disable() : m.directives_enable()}
      title={directive.enabled ? m.directives_disable() : m.directives_enable()}
    >
      {#if directive.enabled}
        <CheckIcon />
      {:else}
        <PlayIcon />
      {/if}
    </Button>
    <Button
      variant="ghost"
      size="icon-sm"
      onclick={onEdit}
      aria-label={m.directives_edit()}
      title={m.directives_edit()}
    >
      <PencilIcon />
    </Button>
    <DropdownMenuPrimitive.Root>
      <DropdownMenuPrimitive.Trigger>
        {#snippet child({ props })}
          <Button
            variant="ghost"
            size="icon-sm"
            {...props}
            aria-label={m.directives_more_actions()}
            title={m.directives_more_actions()}
          >
            <EllipsisIcon />
          </Button>
        {/snippet}
      </DropdownMenuPrimitive.Trigger>
      <DropdownMenuPrimitive.Content
        align="end"
        sideOffset={4}
        class="z-50 min-w-32 overflow-hidden rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-md"
      >
        <DropdownMenuPrimitive.Item
          class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
          disabled={isFirst}
          onSelect={() => onMove("up")}
        >
          {m.directives_move_up()}
        </DropdownMenuPrimitive.Item>
        <DropdownMenuPrimitive.Item
          class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
          disabled={isLast}
          onSelect={() => onMove("down")}
        >
          {m.directives_move_down()}
        </DropdownMenuPrimitive.Item>
        <DropdownMenuPrimitive.Item
          class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
          onSelect={onDelete}
        >
          {m.directives_delete()}
        </DropdownMenuPrimitive.Item>
      </DropdownMenuPrimitive.Content>
    </DropdownMenuPrimitive.Root>
  </div>
</div>
