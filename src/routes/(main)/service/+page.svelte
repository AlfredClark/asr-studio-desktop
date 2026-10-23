<script lang="ts">
  // 服务页组装：左右分栏，左侧识别记录面板、右侧服务控制面板，各自独立滚动。
  // 状态机收敛在 service/config/lexicon hooks，本页只做渲染与接线。
  import { onMount } from "svelte";
  import {
    ResizableHandle,
    ResizablePane,
    ResizablePaneGroup,
  } from "#components/shadcn-svelte/resizable";
  import { refreshLexicon } from "#libs/hooks/lexicon.svelte";
  import { refreshServiceData } from "#libs/hooks/service.svelte";
  import HistoryPanel from "./_components/history-panel.svelte";
  import ControlPanel from "./_components/control-panel.svelte";

  onMount(() => {
    void refreshServiceData();
    void refreshLexicon();
  });
</script>

<!-- 整页左右分栏并占满页面：左记录右设置，各自独立滚动；记录更宽，右栏可窄到 35% -->
<div class="h-full w-full" data-testid="service-page">
  <ResizablePaneGroup direction="horizontal" class="h-full gap-2">
    <ResizablePane defaultSize={60} minSize={35}>
      <HistoryPanel />
    </ResizablePane>
    <ResizableHandle withHandle />
    <ResizablePane defaultSize={40} minSize={30}>
      <ControlPanel />
    </ResizablePane>
  </ResizablePaneGroup>
</div>
