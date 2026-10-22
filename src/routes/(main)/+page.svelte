<script lang="ts">
  // 首页工作台：状态驱动分支——无就绪模型时三步引导，否则四状态卡 + 最近识别。
  // 数据只读复用各域 hook，不新增命令；录制等完整操作仍在各功能页，首页只做入口。
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { resolve } from "$app/paths";
  import { Badge } from "#components/shadcn-svelte/badge";
  import { Card, CardContent, CardHeader, CardTitle } from "#components/shadcn-svelte/card";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from "#components/shadcn-svelte/select";
  import { m } from "#libs/i18n/paraglide/messages";
  import type { ShortcutOutput } from "#libs/commands/types";
  import { formatBytes } from "#libs/utils/format";
  import { toast } from "#libs/utils/toast";
  import { configValue, updateConfig } from "#libs/hooks/config.svelte";
  import { modelsState, refreshModels } from "#libs/hooks/models.svelte";
  import {
    deviceLabel,
    refreshServiceData,
    selectDevice,
    selectModel,
    serviceState,
    startService,
    stopService,
  } from "#libs/hooks/service.svelte";
  import { lexiconState, refreshLexicon } from "#libs/hooks/lexicon.svelte";
  import { directiveState, refreshDirectives } from "#libs/hooks/directives.svelte";
  import HomeOnboardingStep from "./_components/home-onboarding-step.svelte";
  import HomeStatusCard from "./_components/home-status-card.svelte";

  /** 首帧水合完成：四域快照到齐才分支渲染，避免闪烁 */
  let loaded = $state(false);

  /** 就绪模型：引导/工作台分支的唯一依据 */
  const readyModels = $derived(modelsState.entries.filter((entry) => entry.status === "ready"));
  const isReady = $derived(readyModels.length > 0);
  /** 服务运行态与切换中：启动/停止按钮共用 */
  const running = $derived(serviceState.status?.state === "running");
  const switching = $derived(serviceState.switching || serviceState.status?.state === "starting");

  /** 词库分组/词条计数：全局组恒生效，只计用户组；未知分组词条视为未生效 */
  const lexiconGroups = $derived(lexiconState.value.groups);
  const lexiconGroupTotal = $derived(lexiconGroups.length);
  const lexiconActiveIds = $derived(configValue("active_lexicon_groups", []));
  const lexiconGroupActive = $derived(
    lexiconGroups.filter((group) => group.is_global || lexiconActiveIds.includes(group.id)).length,
  );
  const lexiconEntries = $derived(lexiconState.value.entries);
  const lexiconEntryTotal = $derived(lexiconEntries.length);
  const lexiconEntryActive = $derived(
    lexiconEntries.filter((entry) => {
      if (!entry.enabled) return false;
      const group = lexiconGroups.find((item) => item.id === entry.group_id);
      if (!group) return false;
      return group.is_global || lexiconActiveIds.includes(group.id);
    }).length,
  );

  /** 模型本地总占用：已落盘条目实测之和（含未就绪残留） */
  const modelsDiskBytes = $derived(
    modelsState.entries.reduce((sum, entry) => sum + entry.local_bytes, 0),
  );

  /** 指令计数与启用数 */
  const directiveTotal = $derived(directiveState.value.length);
  const directiveEnabled = $derived(
    directiveState.value.filter((directive) => directive.enabled).length,
  );

  /** 最近识别前三条：会话内存，重启即空 */
  const recent = $derived(serviceState.history.slice(0, 3));

  /** 服务卡下拉禁用态：模型切换中禁用，设备录制/识别中禁用（与服务页一致） */
  const modelSelectDisabled = $derived(switching || serviceState.models.length === 0);
  const deviceSelectDisabled = $derived(
    serviceState.recording || serviceState.recognizing || serviceState.devices.length === 0,
  );

  /** 输出方式选项：值与后端 `ShortcutOutput` 落盘标签一一对应（与服务页同款） */
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

  /** 服务状态徽标文案：三态一一对应，未水合按停止渲染 */
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

  /** 记录时间展示：UTC 时间戳转本地时分秒 */
  function recordTime(at: number): string {
    return new Date(at).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  onMount(() => {
    void (async () => {
      await Promise.all([
        refreshModels(),
        refreshServiceData(),
        refreshLexicon(),
        refreshDirectives(),
      ]);
      loaded = true;
    })();
  });
</script>

<div class="h-full w-full" data-testid="home-page">
  {#if !loaded}
    <div class="flex h-full items-center justify-center">
      <p class="text-sm text-muted-foreground">{m.home_loading()}</p>
    </div>
  {:else if !isReady}
    <div class="flex h-full items-center justify-center p-4" data-testid="home-onboarding">
      <Card class="w-full max-w-lg">
        <CardHeader>
          <CardTitle>{m.home_onboarding_title()}</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="flex flex-col gap-4">
            <HomeOnboardingStep
              index={1}
              title={m.home_step1_title()}
              description={m.home_step1_description()}
              done={isReady}
              actionLabel={m.home_step1_action()}
              onAction={() => void goto(resolve("/models"))}
            />
            <HomeOnboardingStep
              index={2}
              title={m.home_step2_title()}
              description={m.home_step2_description()}
              done={running}
              actionLabel={m.service_start()}
              actionDisabled={switching || serviceState.selectedModel == null}
              onAction={() => void startService()}
            />
            <HomeOnboardingStep
              index={3}
              title={m.home_step3_title()}
              description={m.home_step3_description()}
              done={recent.length > 0}
              actionLabel={m.home_step3_action()}
              onAction={() => void goto(resolve("/service"))}
            />
          </div>
        </CardContent>
      </Card>
    </div>
  {:else}
    <ScrollArea class="h-full" data-testid="home-workspace">
      <div class="flex min-h-full flex-col gap-4 p-4">
        <h2 class="shrink-0 text-sm font-medium">{m.home_workspace_title()}</h2>
        <div class="grid shrink-0 grid-cols-1 gap-4">
          <HomeStatusCard
            title={m.nav_service()}
            actionLabel={running ? m.service_stop() : m.service_start()}
            actionDisabled={switching || (!running && serviceState.selectedModel == null)}
            onAction={() => void (running ? stopService() : startService())}
          >
            {#snippet headerExtra()}
              <Badge variant={running ? "default" : "secondary"}>{statusLabel()}</Badge>
            {/snippet}
            <div class="flex flex-wrap items-center gap-2">
              <div class="flex min-w-48 flex-1 items-center gap-2">
                <span class="shrink-0 text-xs text-muted-foreground">
                  {m.service_model_label()}
                </span>
                <Select
                  type="single"
                  value={serviceState.selectedModel ?? undefined}
                  items={serviceState.models.map((entry) => ({
                    value: entry.id,
                    label: entry.display_name,
                  }))}
                  onValueChange={(value) => void selectModel(value)}
                  disabled={modelSelectDisabled}
                >
                  <SelectTrigger class="min-w-0 flex-1" aria-label={m.service_model_label()}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {#each serviceState.models as entry (entry.id)}
                      <SelectItem value={entry.id}>{entry.display_name}</SelectItem>
                    {/each}
                  </SelectContent>
                </Select>
              </div>
              <div class="flex min-w-48 flex-1 items-center gap-2">
                <span class="shrink-0 text-xs text-muted-foreground">
                  {m.service_device_label()}
                </span>
                <Select
                  type="single"
                  value={serviceState.selectedDevice ?? undefined}
                  items={serviceState.devices.map((device) => ({
                    value: device.id,
                    label: deviceLabel(device),
                  }))}
                  onValueChange={(value) => selectDevice(value)}
                  disabled={deviceSelectDisabled}
                >
                  <SelectTrigger class="min-w-0 flex-1" aria-label={m.service_device_label()}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {#each serviceState.devices as device (device.id)}
                      <SelectItem value={device.id}>{deviceLabel(device)}</SelectItem>
                    {/each}
                  </SelectContent>
                </Select>
              </div>
              <div class="flex min-w-48 flex-1 items-center gap-2">
                <span class="shrink-0 text-xs text-muted-foreground">
                  {m.service_output_label()}
                </span>
                <Select
                  type="single"
                  value={configValue("shortcut_output", "paste")}
                  items={outputItems}
                  onValueChange={(value) => void handleOutputChange(value)}
                >
                  <SelectTrigger class="min-w-0 flex-1" aria-label={m.service_output_label()}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="paste">{m.service_output_option_paste()}</SelectItem>
                    <SelectItem value="clipboard">{m.service_output_option_clipboard()}</SelectItem>
                    <SelectItem value="ime">{m.service_output_option_ime()}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
          </HomeStatusCard>
        </div>
        <div class="grid shrink-0 grid-cols-1 gap-4 md:grid-cols-3">
          <HomeStatusCard
            title={m.nav_models()}
            actionLabel={m.home_models_action()}
            onAction={() => void goto(resolve("/models"))}
          >
            <div class="flex flex-col gap-1">
              <p class="text-sm tabular-nums">
                {#if readyModels.length > 0}
                  {m.home_models_ready({ count: readyModels.length })}
                {:else}
                  {m.home_models_none()}
                {/if}
              </p>
              {#if modelsDiskBytes > 0}
                <p class="text-xs text-muted-foreground tabular-nums">
                  {m.home_models_disk({ size: formatBytes(modelsDiskBytes) })}
                </p>
              {/if}
            </div>
          </HomeStatusCard>
          <HomeStatusCard
            title={m.nav_lexicon()}
            actionLabel={m.home_lexicon_action()}
            onAction={() => void goto(resolve("/lexicon"))}
          >
            <div class="flex flex-col gap-1">
              <p class="text-sm tabular-nums">
                {m.home_lexicon_groups({
                  total: lexiconGroupTotal,
                  active: lexiconGroupActive,
                  inactive: lexiconGroupTotal - lexiconGroupActive,
                })}
              </p>
              <p class="text-sm tabular-nums">
                {m.home_lexicon_entries({
                  total: lexiconEntryTotal,
                  active: lexiconEntryActive,
                  inactive: lexiconEntryTotal - lexiconEntryActive,
                })}
              </p>
            </div>
          </HomeStatusCard>
          <HomeStatusCard
            title={m.nav_directives()}
            actionLabel={m.home_directives_action()}
            onAction={() => void goto(resolve("/directives"))}
          >
            <p class="text-sm tabular-nums">
              {m.home_directives_summary({ enabled: directiveEnabled, total: directiveTotal })}
            </p>
          </HomeStatusCard>
        </div>
        <Card class="min-h-0 flex-1">
          <CardHeader>
            <CardTitle>{m.home_recent_title()}</CardTitle>
          </CardHeader>
          <CardContent>
            {#if recent.length === 0}
              <p class="text-sm text-muted-foreground">{m.home_recent_empty()}</p>
            {:else}
              <ul class="flex flex-col gap-1.5">
                {#each recent as record (record.id)}
                  <li class="min-w-0">
                    <p class="truncate text-sm">{record.text}</p>
                    <p class="text-xs text-muted-foreground tabular-nums">
                      {recordTime(record.at)}
                    </p>
                  </li>
                {/each}
              </ul>
            {/if}
            <p class="mt-2 text-xs text-muted-foreground">{m.home_session_hint()}</p>
          </CardContent>
        </Card>
      </div>
    </ScrollArea>
  {/if}
</div>
