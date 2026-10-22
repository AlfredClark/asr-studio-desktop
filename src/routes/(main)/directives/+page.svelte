<script lang="ts">
  // 指令页组装：左列表右详情，选中态放页面局部；数据来自跨页面共享的 directiveState。
  // 左栏列表项复用词库分组行的图标操作（启用切换/编辑/更多），右栏详情只读展示。
  import { onMount } from "svelte";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { Button } from "#components/shadcn-svelte/button";
  import {
    ResizableHandle,
    ResizablePane,
    ResizablePaneGroup,
  } from "#components/shadcn-svelte/resizable";
  import ConfirmDialog from "#components/common/confirm-dialog.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import type { Directive } from "#libs/commands/types";
  import {
    deleteDirective,
    directiveState,
    moveDirective,
    refreshDirectives,
    setDirectiveEnabled,
  } from "#libs/hooks/directives.svelte";
  import DirectiveDetail from "./_components/directive-detail.svelte";
  import DirectiveEditorDialog from "./_components/directive-editor-dialog.svelte";
  import DirectiveListItem from "./_components/directive-list-item.svelte";

  /** 当前选中的指令 id：只放会话内存，不落盘 */
  let selectedId = $state<string | null>(null);

  /** 加载中 / 加载失败；失败给重试入口 */
  let loading = $state(true);
  let loadFailed = $state(false);

  /** 编辑弹窗开关：`editing == null` 即新建 */
  let editorOpen = $state(false);
  let editing = $state<Directive | null>(null);

  /** 删除确认弹窗开关与待删 id */
  let confirmDelete = $state(false);
  let deletingId = $state<string | null>(null);

  /** 当前选中的指令：被删即为空，交回落逻辑处理 */
  const selected = $derived(
    directiveState.value.find((directive) => directive.id === selectedId) ?? null,
  );

  // 选中兜底：读条目写选中，两者不是同一批状态，不形成回路
  $effect(() => {
    const directives = directiveState.value;
    if (selectedId != null && directives.some((directive) => directive.id === selectedId)) return;
    selectedId = directives[0]?.id ?? null;
  });

  /** 打开新建弹窗 */
  function openCreate(): void {
    editing = null;
    editorOpen = true;
  }

  /** 打开编辑弹窗：回填当前值 */
  function openEdit(directive: Directive): void {
    editing = directive;
    editorOpen = true;
  }

  /** 确认删除：成功后若删的是选中项，回落逻辑自动切首项 */
  async function handleConfirmDelete(): Promise<void> {
    const id = deletingId;
    deletingId = null;
    if (id == null) return;
    if (selectedId === id) selectedId = null;
    await deleteDirective(id);
  }

  /** 拉取指令表：挂载与重试共用，失败切错误态 */
  async function load(): Promise<void> {
    loading = true;
    loadFailed = false;
    try {
      loadFailed = !(await refreshDirectives());
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<!-- 整页左右分栏并占满页面：左列表右详情，各自独立滚动 -->
<div class="h-full w-full" data-testid="directives-page">
  {#if loading}
    <div class="flex h-full items-center justify-center">
      <p class="text-sm text-muted-foreground">{m.directives_loading()}</p>
    </div>
  {:else if loadFailed}
    <div class="flex h-full flex-col items-center justify-center gap-2">
      <p class="text-sm text-muted-foreground">{m.directives_load_failed()}</p>
      <Button size="sm" onclick={() => void load()}>{m.directives_retry()}</Button>
    </div>
  {:else}
    <ResizablePaneGroup direction="horizontal" class="h-full gap-2">
      <ResizablePane defaultSize={30} minSize={22}>
        <div class="flex h-full flex-col gap-2 p-4">
          <div class="flex shrink-0 items-baseline justify-between gap-2">
            <h2 class="text-sm font-medium">{m.directives_list_title()}</h2>
            <span class="text-xs text-muted-foreground tabular-nums">
              {m.directives_list_count({ count: directiveState.value.length })}
            </span>
          </div>
          <p class="shrink-0 text-xs text-muted-foreground">{m.directives_description()}</p>
          <div class="shrink-0">
            <Button size="sm" onclick={openCreate}>{m.directives_new()}</Button>
          </div>
          {#if directiveState.value.length === 0}
            <div class="flex min-h-0 flex-1 items-center justify-center">
              <p class="text-sm text-muted-foreground">{m.directives_empty()}</p>
            </div>
          {:else}
            <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
              {#each directiveState.value as directive, directiveIndex (directive.id)}
                <DirectiveListItem
                  {directive}
                  selected={directive.id === selected?.id}
                  isFirst={directiveIndex === 0}
                  isLast={directiveIndex === directiveState.value.length - 1}
                  onSelect={() => (selectedId = directive.id)}
                  onToggle={() => void setDirectiveEnabled(directive.id, !directive.enabled)}
                  onEdit={() => openEdit(directive)}
                  onMove={(direction) => void moveDirective(directive.id, direction)}
                  onDelete={() => {
                    deletingId = directive.id;
                    confirmDelete = true;
                  }}
                />
              {/each}
            </div>
          {/if}
        </div>
      </ResizablePane>
      <ResizableHandle withHandle />
      <ResizablePane defaultSize={70} minSize={45}>
        <ScrollArea class="h-full">
          <div class="flex h-full flex-col gap-2 p-4">
            <DirectiveDetail directive={selected} onEdit={() => selected && openEdit(selected)} />
          </div>
        </ScrollArea>
      </ResizablePane>
    </ResizablePaneGroup>
  {/if}
</div>

<DirectiveEditorDialog bind:open={editorOpen} {editing} />

<ConfirmDialog
  bind:open={confirmDelete}
  title={m.directives_delete_confirm_title()}
  description={m.directives_delete_confirm_description()}
  cancelLabel={m.directives_cancel()}
  confirmLabel={m.directives_delete()}
  variant="destructive"
  onConfirm={() => {
    confirmDelete = false;
    void handleConfirmDelete();
  }}
/>
