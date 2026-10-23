<script lang="ts">
  // 指令列表项：选中态按钮 + 启用徽标 + 开关 / 编辑 / 拖拽手柄 / 删除，全常驻。
  // 排序走拖拽（手柄 mousedown 后行可拖）；键盘聚焦手柄按上下键单步移动。
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import { Switch } from "#components/shadcn-svelte/switch";
  import GripVerticalIcon from "@lucide/svelte/icons/grip-vertical";
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
    onMove,
    onDelete,
    onGripReady,
    onGripCancel,
  }: {
    directive: Directive;
    selected: boolean;
    onSelect: () => void;
    onToggle: () => void;
    onEdit: () => void;
    onMove: (direction: "up" | "down") => void;
    onDelete: () => void;
    /** 手柄按下：通知页武装本行拖拽（`draggable` 由页按行开关） */
    onGripReady: () => void;
    /** 手柄松开（未拖拽）：解除武装，避免整行可拖 */
    onGripCancel: () => void;
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
      {#if directive.enabled}
        <Badge>{m.directives_enabled()}</Badge>
      {/if}
    </Button>
    <Switch
      checked={directive.enabled}
      onCheckedChange={onToggle}
      aria-label={directive.enabled ? m.directives_disable() : m.directives_enable()}
    />
    <Button
      variant="ghost"
      size="icon-sm"
      class="cursor-grab active:cursor-grabbing"
      onmousedown={onGripReady}
      onmouseup={onGripCancel}
      onkeydown={(event) => {
        if (event.key === "ArrowUp") {
          event.preventDefault();
          onMove("up");
        } else if (event.key === "ArrowDown") {
          event.preventDefault();
          onMove("down");
        }
      }}
      aria-label={m.directives_reorder()}
      title={m.directives_reorder()}
    >
      <GripVerticalIcon />
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
