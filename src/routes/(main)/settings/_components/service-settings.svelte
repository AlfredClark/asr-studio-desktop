<script lang="ts">
  // 服务设置分组：服务模式（内部/本机）与本机外部服务的程序路径、端口、线程数、推理后端，
  // 零 props，直接读写配置 hook。内部模式只展示模式选择，其余四项仅外部模式可见。
  import { onDestroy } from "svelte";
  import type { ServiceMode, ServiceProvider } from "#libs/commands/types";
  import { configValue, updateConfig } from "#libs/hooks/config.svelte";
  import { Button } from "#components/shadcn-svelte/button";
  import { Input } from "#components/shadcn-svelte/input";
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
  import { toast } from "#libs/utils/toast";

  /** 服务模式候选项：值与后端 `ServiceMode` 落盘标签一一对应 */
  const modeItems: { value: ServiceMode; label: string }[] = [
    { value: "internal", label: m.settings_service_mode_option_internal() },
    { value: "external", label: m.settings_service_mode_option_external() },
  ];

  /** 推理后端候选项：值与后端 `ServiceProvider` 落盘标签一一对应 */
  const providerItems: { value: ServiceProvider; label: string }[] = [
    { value: "cpu", label: m.settings_service_provider_option_cpu() },
    { value: "cuda", label: m.settings_service_provider_option_cuda() },
    { value: "coreml", label: m.settings_service_provider_option_coreml() },
  ];

  /** 服务模式以后端配置为真值；水合前按内部服务渲染 */
  const serviceMode = $derived<ServiceMode>(configValue("service_mode", "internal"));
  /** 是否展示外部服务四项：仅本机服务模式可见 */
  const isExternal = $derived(serviceMode === "external");
  /** 推理后端以后端配置为真值；水合前按 CPU 渲染 */
  const provider = $derived<ServiceProvider>(configValue("service_provider", "cpu"));

  /** 文本/数字输入草稿：输入态本地持有，失焦/回车落盘；`null` 表示跟随后端值 */
  let pathDraft = $state<string | null>(null);
  let portDraft = $state<string | null>(null);
  let threadsDraft = $state<string | null>(null);

  /** 服务模式经命令落盘后端；失败不乐观更新，下拉自动回滚 */
  // `bits-ui` 给 `string`，经 `modeItems` 收窄为 `ServiceMode` 后才提交，无类型断言
  async function handleModeChange(value: string): Promise<void> {
    const mode = modeItems.find((item) => item.value === value)?.value;
    if (!mode || mode === serviceMode) return;
    if (await updateConfig({ service_mode: mode })) {
      toast.success(m.settings_service_updated());
    } else {
      toast.error(m.settings_service_update_failed());
    }
  }

  /** 推理后端经命令落盘后端；失败不乐观更新，下拉自动回滚 */
  async function handleProviderChange(value: string): Promise<void> {
    const next = providerItems.find((item) => item.value === value)?.value;
    if (!next || next === provider) return;
    if (await updateConfig({ service_provider: next })) {
      toast.success(m.settings_service_updated());
    } else {
      toast.error(m.settings_service_update_failed());
    }
  }

  /** 服务程序路径经文件选择器落盘后端；取消选择静默返回 */
  async function handleBrowseServerPath(): Promise<void> {
    let selected: string | string[] | null;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      selected = await open({ directory: false, multiple: false });
    } catch {
      toast.error(m.settings_service_server_path_pick_failed());
      return;
    }
    if (typeof selected !== "string") return;
    if (await updateConfig({ service_server_path: selected })) {
      pathDraft = null;
      toast.success(m.settings_service_updated());
    } else {
      toast.error(m.settings_service_update_failed());
    }
  }

  /** 服务程序路径经命令落盘后端（后端 trim 入库）；失败保留草稿，输入框不回滚 */
  async function handleServerPathSave(): Promise<void> {
    const raw = pathDraft;
    if (raw == null) return;
    if (await updateConfig({ service_server_path: raw })) {
      pathDraft = null;
      toast.success(m.settings_service_updated());
    } else {
      toast.error(m.settings_service_update_failed());
    }
  }

  /** 服务端口经命令落盘后端；非法输入提示并保留草稿 */
  async function handlePortSave(): Promise<void> {
    const raw = portDraft;
    if (raw == null) return;
    const port = Number.parseInt(raw, 10);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      toast.error(m.settings_service_port_invalid());
      return;
    }
    if (await updateConfig({ service_port: port })) {
      portDraft = null;
      toast.success(m.settings_service_updated());
    } else {
      toast.error(m.settings_service_update_failed());
    }
  }

  /** 线程数经命令落盘后端（`0` 表示自动）；非法输入提示并保留草稿 */
  async function handleThreadsSave(): Promise<void> {
    const raw = threadsDraft;
    if (raw == null) return;
    const threads = Number.parseInt(raw, 10);
    if (!Number.isInteger(threads) || threads < 0) {
      toast.error(m.settings_service_threads_invalid());
      return;
    }
    if (await updateConfig({ service_threads: threads })) {
      threadsDraft = null;
      toast.success(m.settings_service_updated());
    } else {
      toast.error(m.settings_service_update_failed());
    }
  }

  /** 切页销毁时刷一次草稿：用户输一半直接点左侧导航，失焦落盘不会触发 */
  onDestroy(() => {
    if (pathDraft != null) void handleServerPathSave();
    if (portDraft != null) void handlePortSave();
    if (threadsDraft != null) void handleThreadsSave();
  });
</script>

<CardSection title={m.settings_service_title()} description={m.settings_service_description()}>
  <CardRow
    label={m.settings_service_mode_label()}
    description={m.settings_service_mode_description()}
  >
    {#snippet control()}
      <Select
        type="single"
        value={serviceMode}
        items={modeItems}
        onValueChange={(value) => void handleModeChange(value)}
      >
        <SelectTrigger
          class="w-(--control-column-width)"
          aria-label={m.settings_service_mode_label()}
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="internal">{m.settings_service_mode_option_internal()}</SelectItem>
          <SelectItem value="external">{m.settings_service_mode_option_external()}</SelectItem>
        </SelectContent>
      </Select>
    {/snippet}
  </CardRow>
  {#if isExternal}
    <div>
      <CardRow
        label={m.settings_service_server_path_label()}
        description={m.settings_service_server_path_description()}
      />
      <div class="flex items-center gap-2 pb-3">
        <Input
          value={pathDraft ?? configValue("service_server_path", "")}
          placeholder={m.settings_service_server_path_placeholder()}
          aria-label={m.settings_service_server_path_label()}
          class="min-w-0 flex-1"
          oninput={(event) => (pathDraft = event.currentTarget.value)}
          onblur={() => void handleServerPathSave()}
          onkeydown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
        />
        <Button
          variant="outline"
          size="sm"
          class="shrink-0"
          onclick={() => void handleBrowseServerPath()}
        >
          {m.settings_service_server_path_browse()}
        </Button>
      </div>
    </div>
    <CardRow
      label={m.settings_service_port_label()}
      description={m.settings_service_port_description()}
    >
      {#snippet control()}
        <Input
          type="number"
          value={portDraft ?? String(configValue("service_port", 6006))}
          aria-label={m.settings_service_port_label()}
          class="w-(--control-column-width)"
          min={1}
          max={65535}
          oninput={(event) => (portDraft = event.currentTarget.value)}
          onblur={() => void handlePortSave()}
          onkeydown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
        />
      {/snippet}
    </CardRow>
    <CardRow
      label={m.settings_service_threads_label()}
      description={m.settings_service_threads_description()}
    >
      {#snippet control()}
        <Input
          type="number"
          value={threadsDraft ?? String(configValue("service_threads", 0))}
          aria-label={m.settings_service_threads_label()}
          class="w-(--control-column-width)"
          min={0}
          oninput={(event) => (threadsDraft = event.currentTarget.value)}
          onblur={() => void handleThreadsSave()}
          onkeydown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
        />
      {/snippet}
    </CardRow>
    <CardRow
      label={m.settings_service_provider_label()}
      description={m.settings_service_provider_description()}
    >
      {#snippet control()}
        <Select
          type="single"
          value={provider}
          items={providerItems}
          onValueChange={(value) => void handleProviderChange(value)}
        >
          <SelectTrigger
            class="w-(--control-column-width)"
            aria-label={m.settings_service_provider_label()}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="cpu">{m.settings_service_provider_option_cpu()}</SelectItem>
            <SelectItem value="cuda">{m.settings_service_provider_option_cuda()}</SelectItem>
            <SelectItem value="coreml">{m.settings_service_provider_option_coreml()}</SelectItem>
          </SelectContent>
        </Select>
      {/snippet}
    </CardRow>
  {/if}
</CardSection>
