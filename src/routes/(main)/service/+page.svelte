<script lang="ts">
  // 服务页组装：左右分栏，左侧会话级识别统计 + 记录列表（点击展开全文），
  // 右侧服务启停、录制测试与设置直铺（无卡片），下方留空待将来扩展。
  // 状态机收敛在 `libs/hooks/service.svelte.ts`，本页只做渲染与接线；按钮可用性见各派生值。
  import { SvelteSet } from "svelte/reactivity";
  import { onMount } from "svelte";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import {
    ResizableHandle,
    ResizablePane,
    ResizablePaneGroup,
  } from "#components/shadcn-svelte/resizable";
  import CardRow from "#components/common/card-row.svelte";
  import ShortcutRecordRow from "#components/common/shortcut-record-row.svelte";
  import { Switch } from "#components/shadcn-svelte/switch";
  import RecognitionRecordItem from "#components/common/recognition-record-item.svelte";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from "#components/shadcn-svelte/select";
  import { m } from "#libs/i18n/paraglide/messages";
  import { configValue, updateConfig } from "#libs/hooks/config.svelte";
  import type { ShortcutOutput } from "#libs/commands/types";
  import { toast } from "#libs/utils/toast";
  import { groupLabel, lexiconState, refreshLexicon } from "#libs/hooks/lexicon.svelte";
  import {
    deviceLabel,
    cancelTranscription,
    refreshDevices,
    refreshServiceData,
    selectDevice,
    selectModel,
    serviceState,
    startService,
    startTestRecording,
    stopService,
    stopTestRecording,
    summarizeHistory,
  } from "#libs/hooks/service.svelte";

  /** 服务运行中：录制按钮可用，模型下拉可切（自动停旧启新） */
  const running = $derived(serviceState.status?.state === "running");
  /** 启动中或切换中：整组控件禁用 */
  const transitioning = $derived(
    serviceState.switching || serviceState.status?.state === "starting",
  );

  /** 左栏统计：识别条数 / 字符总数 / 推理耗时均值，会话级，重启即清零 */
  const stats = $derived(summarizeHistory(serviceState.history));

  /** 推理耗时均值展示（秒，1 位小数；空历史显示 0） */
  const avgText = $derived(stats.avgInferMs == null ? "0" : (stats.avgInferMs / 1000).toFixed(1));

  /** 展开全文的记录 id 集合：纯前端局部态 */
  const expanded = new SvelteSet<number>();

  /** 记录展开切换 */
  function toggleRecord(id: number): void {
    if (expanded.has(id)) expanded.delete(id);
    else expanded.add(id);
  }

  /** 状态徽标文案：四态一一对应，未水合按停止渲染 */
  function statusLabel(): string {
    switch (serviceState.status?.state) {
      case "running":
        return m.service_status_running();
      case "starting":
        return m.service_status_starting();
      default:
        return m.service_status_stopped();
    }
  }

  /** 录制按钮是否可点：运行中、未录制、未识别、未切换且已选设备 */
  const canRecord = $derived(
    running &&
      !serviceState.recording &&
      !serviceState.recognizing &&
      !serviceState.switching &&
      serviceState.selectedDevice != null,
  );

  /** 输出方式选项：值与后端 `ShortcutOutput` 落盘标签一一对应，粘贴优先、输入法置末 */
  const outputItems: { value: ShortcutOutput; label: string }[] = [
    { value: "paste", label: m.service_output_option_paste() },
    { value: "clipboard", label: m.service_output_option_clipboard() },
    { value: "ime", label: m.service_output_option_ime() },
  ];

  /** 输出方式经命令落盘后端；失败不乐观更新，下拉自动回滚 */
  async function handleOutputChange(value: string): Promise<void> {
    const output = outputItems.find((item) => item.value === value)?.value;
    if (!output) return;
    if (await updateConfig({ shortcut_output: output })) {
      toast.success(m.service_output_updated());
    } else {
      toast.error(m.service_output_update_failed());
    }
  }

  /** 粘贴模式才展示恢复开关：水合前按粘贴默认渲染，避免首帧闪烁 */
  const isPasteMode = $derived(configValue("shortcut_output", "paste") === "paste");

  /** 恢复开关以后端配置为真值；水合前按关闭渲染 */
  const pasteRestore = $derived(configValue("shortcut_paste_restore", false));

  /** 恢复开关切换中禁用，避免重复提交 */
  let switchingRestore = $state(false);

  /** 恢复开关经命令落盘后端；失败不乐观更新，开关自动回滚 */
  async function handleRestoreChange(checked: boolean): Promise<void> {
    if (checked === pasteRestore || switchingRestore) return;
    switchingRestore = true;
    try {
      if (await updateConfig({ shortcut_paste_restore: checked })) {
        toast.success(m.service_output_restore_updated());
      } else {
        toast.error(m.service_output_restore_update_failed());
      }
    } finally {
      switchingRestore = false;
    }
  }

  /** 快捷词库已设快捷键：非空才展示目标分组选择 */
  const quickLexiconSet = $derived(configValue("shortcut_quick_lexicon", "") !== "");

  /** 快捷词库目标分组：存量分组被删后回滚全局分组 */
  const quickLexiconGroup = $derived.by(() => {
    const stored = configValue("quick_lexicon_group", "global");
    return lexiconState.value.groups.some((group) => group.id === stored) ? stored : "global";
  });

  /** 目标分组经命令落盘后端；失败不乐观更新，下拉自动回滚 */
  async function handleQuickLexiconGroupChange(value: string): Promise<void> {
    const target = lexiconState.value.groups.some((group) => group.id === value) ? value : "global";
    if (await updateConfig({ quick_lexicon_group: target })) {
      toast.success(m.settings_quick_lexicon_group_updated());
    } else {
      toast.error(m.settings_quick_lexicon_group_failed());
    }
  }

  onMount(() => {
    void refreshServiceData();
    void refreshLexicon();
  });
</script>

<!-- 整页左右分栏并占满页面：左记录右设置，各自独立滚动；记录更宽，右栏可窄到 35% -->
<div class="h-full w-full" data-testid="service-page">
  <ResizablePaneGroup direction="horizontal" class="h-full gap-2">
    <ResizablePane defaultSize={60} minSize={35}>
      <div class="flex h-full flex-col gap-2 p-4">
        <div class="grid shrink-0 grid-cols-3 gap-2" data-testid="service-stats">
          <div class="text-center">
            <p class="text-2xl font-medium tabular-nums">{stats.count}</p>
            <p class="text-xs text-muted-foreground">{m.service_stats_count()}</p>
          </div>
          <div class="text-center">
            <p class="text-2xl font-medium tabular-nums">{stats.totalChars}</p>
            <p class="text-xs text-muted-foreground">{m.service_stats_chars()}</p>
          </div>
          <div class="text-center">
            <p class="text-2xl font-medium tabular-nums">{avgText} s</p>
            <p class="text-xs text-muted-foreground">{m.service_stats_avg_time()}</p>
          </div>
        </div>
        <h2 class="shrink-0 text-sm font-medium">{m.service_history_title()}</h2>
        <p class="shrink-0 text-xs text-muted-foreground">
          {m.service_history_session_hint()}
        </p>
        {#if serviceState.history.length === 0}
          <div class="flex min-h-0 flex-1 items-center justify-center">
            <p class="text-sm text-muted-foreground">{m.service_history_empty()}</p>
          </div>
        {:else}
          <ScrollArea class="min-h-0 flex-1">
            <div class="flex flex-col gap-1 pr-1">
              {#each serviceState.history as record (record.id)}
                <RecognitionRecordItem
                  {record}
                  open={expanded.has(record.id)}
                  onToggle={() => toggleRecord(record.id)}
                />
              {/each}
            </div>
          </ScrollArea>
        {/if}
      </div>
    </ResizablePane>
    <ResizableHandle withHandle />
    <ResizablePane defaultSize={40} minSize={30}>
      <ScrollArea class="h-full">
        <div class="flex flex-col gap-2 p-4">
          <div class="flex shrink-0 items-center justify-between gap-2">
            <Badge variant={running ? "default" : "secondary"}>
              {serviceState.switching ? m.service_switching() : statusLabel()}
            </Badge>
            {#if running}
              <Button
                variant="outline"
                size="sm"
                onclick={() => void stopService()}
                disabled={serviceState.switching}
              >
                {m.service_stop()}
              </Button>
            {:else}
              <Button
                size="sm"
                onclick={() => void startService()}
                disabled={transitioning || serviceState.selectedModel == null}
              >
                {m.service_start()}
              </Button>
            {/if}
          </div>
          <p class="shrink-0 text-xs font-medium text-muted-foreground">
            {m.service_test_title()}
          </p>
          <div class="flex shrink-0 items-center gap-2">
            {#if serviceState.recording}
              <Button variant="destructive" size="sm" onclick={() => void stopTestRecording()}>
                {m.service_record_stop()}
              </Button>
            {:else if serviceState.recognizing}
              <Button variant="outline" size="sm" onclick={() => void cancelTranscription()}>
                {m.service_cancel_recognition()}
              </Button>
            {:else}
              <Button
                variant="outline"
                size="sm"
                onclick={() => void startTestRecording()}
                disabled={!canRecord}
              >
                {m.service_record_start()}
              </Button>
            {/if}
          </div>
          <CardRow label={m.service_model_label()} description={m.service_model_description()}>
            {#snippet control()}
              {#if serviceState.models.length > 0}
                <Select
                  type="single"
                  value={serviceState.selectedModel ?? undefined}
                  items={serviceState.models.map((entry) => ({
                    value: entry.id,
                    label: entry.display_name,
                  }))}
                  onValueChange={(value) => void selectModel(value)}
                  disabled={transitioning}
                >
                  <SelectTrigger
                    class="w-(--control-column-width)"
                    aria-label={m.service_model_label()}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {#each serviceState.models as entry (entry.id)}
                      <SelectItem value={entry.id}>{entry.display_name}</SelectItem>
                    {/each}
                  </SelectContent>
                </Select>
              {:else}
                <p class="text-sm text-muted-foreground">{m.service_model_empty()}</p>
              {/if}
            {/snippet}
          </CardRow>
          <CardRow label={m.service_device_label()} description={m.service_device_description()}>
            {#snippet control()}
              {#if serviceState.devices.length > 0}
                <Select
                  type="single"
                  value={serviceState.selectedDevice ?? undefined}
                  items={serviceState.devices.map((device) => ({
                    value: device.id,
                    label: deviceLabel(device),
                  }))}
                  onValueChange={(value) => selectDevice(value)}
                  disabled={serviceState.recording || serviceState.recognizing}
                >
                  <SelectTrigger
                    class="w-(--control-column-width)"
                    aria-label={m.service_device_label()}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {#each serviceState.devices as device (device.id)}
                      <SelectItem value={device.id}>{deviceLabel(device)}</SelectItem>
                    {/each}
                  </SelectContent>
                </Select>
              {:else}
                <p class="text-sm text-muted-foreground">{m.service_device_empty()}</p>
              {/if}
            {/snippet}
          </CardRow>
          <div class="flex shrink-0 justify-end">
            <Button
              variant="ghost"
              size="sm"
              onclick={() => void refreshDevices()}
              disabled={serviceState.recording || serviceState.recognizing}
            >
              {m.service_device_refresh()}
            </Button>
          </div>
          <CardRow label={m.settings_voice_label()} description={m.settings_voice_description()}>
            {#snippet control()}
              <ShortcutRecordRow kind="voice" />
            {/snippet}
          </CardRow>
          <CardRow
            label={m.settings_directive_label()}
            description={m.settings_directive_description()}
          >
            {#snippet control()}
              <ShortcutRecordRow kind="directive" />
            {/snippet}
          </CardRow>
          <CardRow
            label={m.settings_quick_lexicon_label()}
            description={m.settings_quick_lexicon_description()}
          >
            {#snippet control()}
              <ShortcutRecordRow kind="quick_lexicon" />
            {/snippet}
          </CardRow>
          {#if quickLexiconSet}
            <CardRow
              label={m.settings_quick_lexicon_group_label()}
              description={m.settings_quick_lexicon_group_description()}
            >
              {#snippet control()}
                <Select
                  type="single"
                  value={quickLexiconGroup}
                  items={lexiconState.value.groups.map((group) => ({
                    value: group.id,
                    label: groupLabel(group),
                  }))}
                  onValueChange={(value) => void handleQuickLexiconGroupChange(value)}
                >
                  <SelectTrigger
                    class="w-(--control-column-width)"
                    aria-label={m.settings_quick_lexicon_group_label()}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {#each lexiconState.value.groups as group (group.id)}
                      <SelectItem value={group.id}>{groupLabel(group)}</SelectItem>
                    {/each}
                  </SelectContent>
                </Select>
              {/snippet}
            </CardRow>
          {/if}
          <CardRow label={m.service_output_label()} description={m.service_output_description()}>
            {#snippet control()}
              <Select
                type="single"
                value={configValue("shortcut_output", "paste")}
                items={outputItems}
                onValueChange={(value) => void handleOutputChange(value)}
              >
                <SelectTrigger
                  class="w-(--control-column-width)"
                  aria-label={m.service_output_label()}
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="paste">{m.service_output_option_paste()}</SelectItem>
                  <SelectItem value="clipboard">{m.service_output_option_clipboard()}</SelectItem>
                  <SelectItem value="ime">{m.service_output_option_ime()}</SelectItem>
                </SelectContent>
              </Select>
            {/snippet}
          </CardRow>
          {#if isPasteMode}
            <CardRow
              label={m.service_output_restore_label()}
              description={m.service_output_restore_description()}
            >
              {#snippet control()}
                <Switch
                  checked={pasteRestore}
                  onCheckedChange={handleRestoreChange}
                  disabled={switchingRestore}
                  aria-label={m.service_output_restore_label()}
                />
              {/snippet}
            </CardRow>
          {/if}
          <!-- 下方留空：将来新增设置直接往下加行 -->
          <div class="min-h-4 flex-1"></div>
        </div>
      </ScrollArea>
    </ResizablePane>
  </ResizablePaneGroup>
</div>
