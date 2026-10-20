<script lang="ts">
  // 服务页组装：识别服务控制（模型与设备选择、启停、状态）+ 识别测试（录制启停、结果回显）。
  // 状态机收敛在 `libs/hooks/service.svelte.ts`，本页只做渲染与接线；按钮可用性见各派生值。
  import { onMount } from "svelte";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Button } from "#components/shadcn-svelte/button";
  import CardRow from "#components/common/card-row.svelte";
  import CardSection from "#components/common/card-section.svelte";
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
    refreshServiceData,
    selectDevice,
    selectModel,
    serviceState,
    startService,
    startTestRecording,
    stopService,
    stopTestRecording,
  } from "#libs/hooks/service.svelte";

  /** 服务运行中：录制按钮可用，模型下拉可切（自动停旧启新） */
  const running = $derived(serviceState.status?.state === "running");
  /** 启动中或切换中：整组控件禁用 */
  const transitioning = $derived(
    serviceState.switching || serviceState.status?.state === "starting",
  );

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

  /** 输出方式选项：值与后端 `ShortcutOutput` 落盘标签一一对应 */
  const outputItems: { value: ShortcutOutput; label: string }[] = [
    { value: "ime", label: m.service_output_option_ime() },
    { value: "clipboard", label: m.service_output_option_clipboard() },
    { value: "paste", label: m.service_output_option_paste() },
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

<!-- 页面级纵向滚动：内容超高时在视口内滚动；内层保持水平居中、垂直置顶 -->
<ScrollArea class="h-full w-full">
  <div class="mx-auto flex w-full max-w-2xl flex-col gap-6 px-4 py-6">
    <div class="flex items-center justify-between gap-2">
      <h1 class="text-2xl font-medium">{m.page_service_title()}</h1>
      <Badge variant={running ? "default" : "secondary"}>
        {serviceState.switching ? m.service_switching() : statusLabel()}
      </Badge>
    </div>

    <CardSection title={m.service_control_title()} description={m.service_control_description()}>
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
                label: device.name,
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
                  <SelectItem value={device.id}>{device.name}</SelectItem>
                {/each}
              </SelectContent>
            </Select>
          {:else}
            <p class="text-sm text-muted-foreground">{m.service_device_empty()}</p>
          {/if}
        {/snippet}
      </CardRow>
      <div class="py-3">
        {#if running}
          <Button
            variant="outline"
            class="w-full"
            onclick={() => void stopService()}
            disabled={serviceState.switching}
          >
            {m.service_stop()}
          </Button>
        {:else}
          <Button
            class="w-full"
            onclick={() => void startService()}
            disabled={transitioning || serviceState.selectedModel == null}
          >
            {m.service_start()}
          </Button>
        {/if}
      </div>
    </CardSection>

    <CardSection title={m.service_output_title()} description={m.service_output_description()}>
      <CardRow label={m.service_output_label()} description="">
        {#snippet control()}
          <Select
            type="single"
            value={configState.value?.shortcut_output ?? "clipboard"}
            items={outputItems}
            onValueChange={(value) => void handleOutputChange(value)}
          >
            <SelectTrigger class="w-(--control-column-width)" aria-label={m.service_output_label()}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="ime">{m.service_output_option_ime()}</SelectItem>
              <SelectItem value="clipboard">{m.service_output_option_clipboard()}</SelectItem>
              <SelectItem value="paste">{m.service_output_option_paste()}</SelectItem>
            </SelectContent>
          </Select>
        {/snippet}
      </CardRow>
    </CardSection>

    <CardSection title={m.service_test_title()} description={m.service_test_description()}>
      <div class="flex flex-col gap-3 py-1">
        <div>
          {#if serviceState.recording}
            <Button variant="destructive" onclick={() => void stopTestRecording()}>
              {m.service_record_stop()}
            </Button>
          {:else if serviceState.recognizing}
            <Button disabled>
              {m.service_recognizing()}
            </Button>
          {:else}
            <Button onclick={() => void startTestRecording()} disabled={!canRecord}>
              {m.service_record_start()}
            </Button>
          {/if}
        </div>
        <div>
          {#if serviceState.result}
            <p class="text-sm whitespace-pre-wrap">{serviceState.result.text}</p>
            <p class="mt-1 text-xs text-muted-foreground">
              {m.service_result_audio()}
              {serviceState.result.audioSec.toFixed(1)}s ·
              {m.service_result_time()}
              {(serviceState.result.inferMs / 1000).toFixed(1)}s ·
              {serviceState.result.fileName}
            </p>
          {:else}
            <p class="text-sm text-muted-foreground">{m.service_result_placeholder()}</p>
          {/if}
        </div>
      </div>
    </CardSection>
  </div>
</ScrollArea>
