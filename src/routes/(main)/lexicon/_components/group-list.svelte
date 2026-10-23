<script lang="ts">
  // 词库分组列表（左栏）：分组选择/计数/徽标、启用开关、行内重命名、
  // 更多菜单（上下移/导出/删除）、底部新建与导入；删组确认框自带。
  // 选中态由页传入（选中不存在回落逻辑在页），新建/重命名等草稿全在内部。
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import { Input } from "#components/shadcn-svelte/input";
  import { Switch } from "#components/shadcn-svelte/switch";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";
  import ConfirmDialog from "#components/common/confirm-dialog.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import { cn } from "#libs/utils/shadcn-svelte";
  import {
    createLexiconGroup,
    deleteLexiconGroup,
    entriesOfGroup,
    exportLexiconGroup,
    groupLabel,
    importLexiconGroup,
    lexiconState,
    moveLexiconGroup,
    renameLexiconGroup,
    setLexiconGroupEnabled,
  } from "#libs/hooks/lexicon.svelte";

  /** 当前选中的分组 id（页算好的有效值）与选中回调（含导入成功后切换） */
  let { selectedId, onSelect }: { selectedId: string; onSelect: (id: string | null) => void } =
    $props();

  /** 生效中的用户分组 id 集合 */
  const activeIds = $derived(lexiconState.value.active_group_ids);

  /** 是否启用 */
  function isActive(id: string): boolean {
    return activeIds.includes(id);
  }

  /** 新建分组输入 */
  let newGroupName = $state("");
  /** 新建分组在途中禁用，避免重复提交 */
  let creatingGroup = $state(false);

  /** 重命名中的分组 id 与草稿 */
  let renamingId = $state<string | null>(null);
  let renameDraft = $state("");

  /** 待确认删除的分组 id */
  let deletingGroupId = $state<string | null>(null);
  /** 删除确认弹窗开关 */
  let confirmDeleteGroup = $state(false);

  /** 导入分组：成功后切到新分组查看 */
  async function handleImportGroup(): Promise<void> {
    const summary = await importLexiconGroup();
    if (summary != null) onSelect(summary.id);
  }

  /** 新建分组：空名直接忽略 */
  async function handleCreateGroup(): Promise<void> {
    if (newGroupName.trim() === "" || creatingGroup) return;
    creatingGroup = true;
    try {
      if (await createLexiconGroup(newGroupName)) newGroupName = "";
    } finally {
      creatingGroup = false;
    }
  }

  /** 开始重命名：草稿回填存量名（全局组无入口） */
  function startRename(id: string, name: string): void {
    renamingId = id;
    renameDraft = name;
  }

  /** 提交重命名：空名直接忽略 */
  async function handleRenameGroup(id: string): Promise<void> {
    if (renameDraft.trim() === "") return;
    if (await renameLexiconGroup(id, renameDraft)) renamingId = null;
  }
</script>

<ScrollArea class="h-full">
  <div class="flex h-full flex-col gap-2 p-4">
    <h2 class="shrink-0 text-sm font-medium">{m.lexicon_groups_title()}</h2>
    <p class="shrink-0 text-xs text-muted-foreground">{m.lexicon_global_hint()}</p>
    <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
      {#each lexiconState.value.groups as group, groupIndex (group.id)}
        {@const selected = group.id === selectedId}
        {@const count = entriesOfGroup(lexiconState.value.entries, group.id).length}
        <div class="rounded-lg px-1 py-1">
          {#if renamingId === group.id}
            <div class="flex items-center gap-2">
              <Input
                value={renameDraft}
                oninput={(event) => (renameDraft = event.currentTarget.value)}
                onkeydown={(event) => {
                  if (event.key === "Enter") void handleRenameGroup(group.id);
                  else if (event.key === "Escape") renamingId = null;
                }}
                aria-label={m.lexicon_group_name()}
              />
              <Button
                size="sm"
                onclick={() => void handleRenameGroup(group.id)}
                disabled={renameDraft.trim() === ""}
              >
                {m.lexicon_save()}
              </Button>
              <Button variant="ghost" size="sm" onclick={() => (renamingId = null)}>
                {m.lexicon_cancel()}
              </Button>
            </div>
          {:else}
            <div class="flex items-center gap-1">
              <Button
                variant={selected ? "secondary" : "ghost"}
                class={cn(
                  "min-w-0 flex-1 justify-start gap-2 px-2 font-normal",
                  selected && "font-medium",
                )}
                onclick={() => onSelect(group.id)}
                aria-current={selected ? "true" : undefined}
              >
                <span class="min-w-0 flex-1 truncate text-left text-sm">
                  {groupLabel(group)}
                  <span class="text-muted-foreground tabular-nums"> ({count})</span>
                </span>
                {#if group.is_global}
                  <Badge>{m.lexicon_always_on()}</Badge>
                {/if}
              </Button>
              {#if !group.is_global}
                <Switch
                  checked={isActive(group.id)}
                  onCheckedChange={() => void setLexiconGroupEnabled(group.id, !isActive(group.id))}
                  aria-label={`${groupLabel(group)} - ${isActive(group.id) ? m.lexicon_disable() : m.lexicon_enable()}`}
                />
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onclick={() => startRename(group.id, group.name)}
                  aria-label={m.lexicon_rename_group()}
                  title={m.lexicon_rename_group()}
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
                        aria-label={m.lexicon_more_actions()}
                        title={m.lexicon_more_actions()}
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
                      disabled={groupIndex <= 1}
                      onSelect={() => void moveLexiconGroup(group.id, "up")}
                    >
                      {m.lexicon_move_up()}
                    </DropdownMenuPrimitive.Item>
                    <DropdownMenuPrimitive.Item
                      class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                      disabled={groupIndex === lexiconState.value.groups.length - 1}
                      onSelect={() => void moveLexiconGroup(group.id, "down")}
                    >
                      {m.lexicon_move_down()}
                    </DropdownMenuPrimitive.Item>
                    <DropdownMenuPrimitive.Item
                      class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:bg-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                      onSelect={() => void exportLexiconGroup(group.id)}
                    >
                      {m.lexicon_export_group()}
                    </DropdownMenuPrimitive.Item>
                    <DropdownMenuPrimitive.Item
                      class="flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:bg-accent-foreground data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50"
                      onSelect={() => {
                        deletingGroupId = group.id;
                        confirmDeleteGroup = true;
                      }}
                    >
                      {m.lexicon_delete_group()}
                    </DropdownMenuPrimitive.Item>
                  </DropdownMenuPrimitive.Content>
                </DropdownMenuPrimitive.Root>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
    <form
      class="flex shrink-0 items-center gap-2"
      onsubmit={(event) => {
        event.preventDefault();
        void handleCreateGroup();
      }}
    >
      <Button type="button" variant="outline" size="sm" onclick={() => void handleImportGroup()}>
        {m.lexicon_import_group()}
      </Button>
      <Input
        value={newGroupName}
        oninput={(event) => (newGroupName = event.currentTarget.value)}
        aria-label={m.lexicon_group_name()}
        placeholder={m.lexicon_group_name()}
      />
      <Button type="submit" size="sm" disabled={newGroupName.trim() === "" || creatingGroup}>
        {m.lexicon_new_group()}
      </Button>
    </form>
  </div>
</ScrollArea>

<ConfirmDialog
  bind:open={confirmDeleteGroup}
  title={m.lexicon_delete_group_confirm_title()}
  description={m.lexicon_delete_group_confirm_description()}
  cancelLabel={m.lexicon_cancel()}
  confirmLabel={m.lexicon_delete()}
  variant="destructive"
  onConfirm={() => {
    const id = deletingGroupId;
    deletingGroupId = null;
    if (id != null) {
      if (selectedId === id) onSelect(null);
      void deleteLexiconGroup(id);
    }
  }}
/>
