<script lang="ts">
  // 更新状态面板：检查可用 / 下载中 / 待重启 / 失败各阶段展示，状态走全局 hook，零 props。
  // 切页再回来保持原样；下载百分比服务端未给总量时回落不确定进度。
  import { downloadAndInstall, restartApp, updaterState } from "#libs/hooks/updater.svelte";
  import { Button } from "#components/shadcn-svelte/button";
  import { Progress } from "#components/shadcn-svelte/progress";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { m } from "#libs/i18n/paraglide/messages";

  /** 下载百分比；服务端未给总量时回落不确定进度 */
  const downloadPercent = $derived(
    updaterState.total ? Math.round((updaterState.downloaded / updaterState.total) * 100) : null,
  );
</script>

{#if updaterState.phase === "available" && updaterState.latest}
  <div class="flex flex-col gap-3 py-4">
    <p class="text-sm font-medium">
      {m.updater_update_available()}：{updaterState.latest.version}
    </p>
    {#if updaterState.latest.body}
      <!-- ScrollArea 需要确定高度：固定可视高度，避免 max-height 下 viewport 高度不收敛 -->
      <ScrollArea class="h-40 rounded-lg bg-muted">
        <pre class="p-3 text-xs whitespace-pre-wrap">{updaterState.latest.body}</pre>
      </ScrollArea>
    {/if}
    <div>
      <Button onclick={() => void downloadAndInstall()}>
        {m.updater_download_and_install()}
      </Button>
    </div>
  </div>
{:else if updaterState.phase === "downloading"}
  <div class="flex flex-col gap-3 py-4">
    <p class="text-sm font-medium">
      {m.updater_downloading()}{downloadPercent === null ? "…" : ` ${downloadPercent}%`}
    </p>
    <Progress value={downloadPercent} />
  </div>
{:else if updaterState.phase === "ready"}
  <div class="flex flex-col gap-3 py-4">
    <p class="text-sm font-medium">{m.updater_update_ready()}</p>
    <div>
      <Button onclick={() => restartApp()}>{m.updater_restart_now()}</Button>
    </div>
  </div>
{:else if updaterState.phase === "error" && updaterState.error}
  <div class="py-4">
    <p class="text-sm text-destructive">{updaterState.error}</p>
  </div>
{/if}
