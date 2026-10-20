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
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from "#components/shadcn-svelte/select";
  import { m } from "#libs/i18n/paraglide/messages";
  import { configState, updateConfig } from "#libs/hooks/config.svelte";
  import type { ShortcutOutput } from "#libs/commands/types";
  import { toast } from "#libs/utils/toast";
  import {
    deviceLabel,
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

  /** 记录时间展示：UTC 时间戳转本地时分秒 */
  function recordTime(at: number): string {
    return new Date(at).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
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

  onMount(() => {
    void refreshServiceData();
  });
</script>

<!-- 整页左右分栏并占满页面：左记录右设置，各自独立滚动；记录更宽，右栏可窄到 35% -->
<div class="h-full w-full" data-testid="service-page">
  <ResizablePaneGroup direction="horizontal" class="h-full gap-2">
    <ResizablePane defaultSize={60} minSize={35}>
      <div class="flex h-full flex-col gap-2 p-4">
        <div class="grid shrink-0 grid-cols-3 gap-2" data-testid="service-stats">
          <div>
            <p class="text-2xl font-medium tabular-nums">{stats.count}</p>
            <p class="text-xs text-muted-foreground">{m.service_stats_count()}</p>
          </div>
          <div>
            <p class="text-2xl font-medium tabular-nums">{stats.totalChars}</p>
            <p class="text-xs text-muted-foreground">{m.service_stats_chars()}</p>
          </div>
          <div>
            <p class="text-2xl font-medium tabular-nums">{avgText} s</p>
            <p class="text-xs text-muted-foreground">{m.service_stats_avg_time()}</p>
          </div>
        </div>
        <h2 class="shrink-0 text-sm font-medium">{m.service_history_title()}</h2>
        {#if serviceState.history.length === 0}
          <div class="flex min-h-0 flex-1 items-center justify-center">
            <p class="text-sm text-muted-foreground">{m.service_history_empty()}</p>
          </div>
        {:else}
          <ScrollArea class="min-h-0 flex-1">
            <div class="flex flex-col gap-1 pr-1">
              {#each serviceState.history as record (record.id)}
                {@const open = expanded.has(record.id)}
                <button
                  type="button"
                  class="rounded-lg bg-muted/50 px-2.5 py-2 text-left hover:bg-muted"
                  aria-expanded={open}
                  onclick={() => toggleRecord(record.id)}
                >
                  <p class={open ? "text-sm whitespace-pre-wrap" : "line-clamp-2 text-sm"}>
                    {record.text}
                  </p>
                  <p class="mt-1 text-xs text-muted-foreground tabular-nums">
                    {recordTime(record.at)} · {m.service_result_audio()}
                    {record.audioSec.toFixed(1)}s · {m.service_result_time()}
                    {(record.inferMs / 1000).toFixed(1)}s
                  </p>
                </button>
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
              <Button size="sm" disabled>
                {m.service_recognizing()}
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
          <CardRow label={m.settings_voice_label()} description={m.settings_voice_description()}>
            {#snippet control()}
              <ShortcutRecordRow kind="voice" />
            {/snippet}
          </CardRow>
          <CardRow label={m.service_output_label()} description={m.service_output_description()}>
            {#snippet control()}
              <Select
                type="single"
                value={configState.value?.shortcut_output ?? "paste"}
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
          <!-- 下方留空：将来新增设置直接往下加行 -->
          <div class="min-h-4 flex-1"></div>
        </div>
      </ScrollArea>
    </ResizablePane>
  </ResizablePaneGroup>
</div>
