<script lang="ts">
  // 应用图标：本地语音波形（单色 + `currentColor`，明暗主题自适应）；
  // 内在尺寸 1024×1024，具体显示大小由调用方经 `class` 控制；
  // 纯装饰，`aria-hidden` 不进入可访问名称。
  import { cn } from "#libs/utils/shadcn-svelte";

  let { class: className = "" }: { class?: string } = $props();
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  width="1024"
  height="1024"
  viewBox="0 0 24 24"
  fill="none"
  aria-hidden="true"
  class={cn(className)}
>
  <rect x="3" y="9" width="2" height="6" rx="1" fill="currentColor" />
  <rect x="7" y="7" width="2" height="10" rx="1" fill="currentColor" />
  <rect x="11" y="4" width="2" height="16" rx="1" fill="currentColor" />
  <rect x="15" y="7" width="2" height="10" rx="1" fill="currentColor" />
  <rect x="19" y="9" width="2" height="6" rx="1" fill="currentColor" />
</svg>
