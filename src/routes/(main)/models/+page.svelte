<script lang="ts">
  // 模型页组装：标题 + 打开根目录入口 + 三类分组（卡片列表），无业务逻辑。
  // 条目与进度来自跨页面共享的 modelsState；挂载即刷新一次，切页回来状态仍在。
  import { onMount } from "svelte";
  import { ScrollArea } from "#components/shadcn-svelte/scroll-area";
  import { Button } from "#components/shadcn-svelte/button";
  import CardSection from "#components/common/card-section.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import type { ModelCategory } from "#libs/commands/types";
  import { modelsState, openModelsRoot, refreshModels } from "#libs/hooks/models.svelte";
  import ModelCard from "./_components/model-card.svelte";

  /** 分组顺序即页面渲染顺序，与 `ModelCategory` 的注册顺序无关 */
  const categories: { key: ModelCategory; title: () => string }[] = [
    { key: "funasr_nano", title: () => m.models_category_funasr_nano() },
    { key: "qwen3_asr", title: () => m.models_category_qwen3_asr() },
    { key: "sense_voice", title: () => m.models_category_sense_voice() },
  ];

  onMount(() => {
    void refreshModels();
  });
</script>

<!-- 页面级纵向滚动：内容超高时在视口内滚动；内层保持水平居中、垂直置顶 -->
<ScrollArea class="h-full w-full">
  <div class="mx-auto flex w-full max-w-2xl flex-col gap-6 px-4 py-6">
    <div class="flex items-center justify-between gap-2">
      <h1 class="text-2xl font-medium">{m.page_models_title()}</h1>
      <Button variant="outline" size="sm" onclick={() => void openModelsRoot()}>
        {m.models_open_root()}
      </Button>
    </div>
    <p class="-mt-3 text-sm text-muted-foreground">{m.models_description()}</p>

    {#each categories as { key, title } (key)}
      {@const entries = modelsState.entries.filter((entry) => entry.category === key)}
      {#if entries.length > 0}
        <CardSection title={title()}>
          <div class="flex flex-col gap-3 py-1">
            {#each entries as entry (entry.id)}
              <ModelCard {entry} />
            {/each}
          </div>
        </CardSection>
      {/if}
    {/each}
  </div>
</ScrollArea>
