<script lang="ts">
  // 指令列表项：选中态按钮 + 开关 / 上移 / 下移 / 编辑 / 删除，全常驻。
  // 启用状态只由开关表达，避免与详情页徽标重复。
  import { Button } from "#components/shadcn-svelte/button";
  import { Switch } from "#components/shadcn-svelte/switch";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import { cn } from "#libs/utils/shadcn-svelte";
  import type { Directive } from "#libs/commands/types";
  import { m } from "#libs/i18n/paraglide/messages";

  let {
    directive,
    selected,
    onSelect,
    onToggle,
    onEdit,
    canMoveUp,
    canMoveDown,
    moving,
    onMove,
    onDelete,
  }: {
    directive: Directive;
    selected: boolean;
    onSelect: () => void;
    onToggle: () => void;
    onEdit: () => void;
    /** 是否可上移：首行禁用 */
    canMoveUp: boolean;
    /** 是否可下移：末行禁用 */
    canMoveDown: boolean;
    /** 移动请求进行中：两个移动按钮同时禁用 */
    moving: boolean;
    onMove: (direction: "up" | "down") => void;
    onDelete: () => void;
  } = $props();
</script>

<div class="group rounded-lg px-1 py-1">
  <div class="flex items-center gap-1">
    <Button
      variant={selected ? "secondary" : "ghost"}
      class={cn("min-w-0 flex-1 justify-start gap-2 px-2 font-normal", selected && "font-medium")}
      onclick={onSelect}
      aria-current={selected ? "true" : undefined}
    >
      <span class="min-w-0 flex-1 truncate text-left text-sm">
        {directive.name}
      </span>
    </Button>
    <Switch
      checked={directive.enabled}
      onCheckedChange={onToggle}
      aria-label={directive.enabled ? m.directives_disable() : m.directives_enable()}
    />
    <div class="flex shrink-0 items-center gap-0.5">
      <Button
        variant="ghost"
        size="icon-xs"
        disabled={!canMoveUp || moving}
        onclick={() => onMove("up")}
        aria-label={m.directives_move_up()}
        title={m.directives_move_up()}
      >
        <ChevronUpIcon />
      </Button>
      <Button
        variant="ghost"
        size="icon-xs"
        disabled={!canMoveDown || moving}
        onclick={() => onMove("down")}
        aria-label={m.directives_move_down()}
        title={m.directives_move_down()}
      >
        <ChevronDownIcon />
      </Button>
    </div>
    <Button
      variant="ghost"
      size="icon-sm"
      onclick={onEdit}
      aria-label={m.directives_edit()}
      title={m.directives_edit()}
    >
      <PencilIcon />
    </Button>
    <Button
      variant="ghost"
      size="icon-sm"
      onclick={onDelete}
      aria-label={m.directives_delete()}
      title={m.directives_delete()}
    >
      <TrashIcon />
    </Button>
  </div>
</div>
