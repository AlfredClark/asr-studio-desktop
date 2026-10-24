<script lang="ts">
  // 识别记录行：文本 + 元信息（时间 · 音频 · 用时 · 输出方式）+ 最右来源徽标，点击展开全文。
  // 服务页与首页共用同一款式；展开态由调用方持有（页面局部 `SvelteSet`）。
  import { Badge } from "#components/shadcn-svelte/badge";
  import { m } from "#libs/i18n/paraglide/messages";
  import type { RecognitionRecord } from "#libs/hooks/service.svelte";

  let {
    record,
    open,
    onToggle,
  }: {
    /** 识别记录 */
    record: RecognitionRecord;
    /** 是否展开全文 */
    open: boolean;
    /** 点击切换回调 */
    onToggle: () => void;
  } = $props();

  /** 记录时间展示：UTC 时间戳转本地时分秒 */
  function recordTime(at: number): string {
    return new Date(at).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  /** 指令记录展示文案：命中为 `文本[拼音][指令名]`，未命中为 `文本[拼音]`；拼音为空时省略中括号段 */
  function directiveDisplay(record: RecognitionRecord): string {
    const pinyinPart = record.spokenPinyin === "" ? "" : `[${record.spokenPinyin}]`;
    if (record.directiveHit) {
      return `${record.text}${pinyinPart}[${record.directiveName}]`;
    }
    return `${record.text}${pinyinPart}`;
  }

  /** 记录来源徽标文案 */
  function kindLabel(kind: RecognitionRecord["kind"]): string {
    return kind === "directive"
      ? m.service_history_kind_directive()
      : m.service_history_kind_text();
  }

  /** 投递通道文案：快捷键输出的实际去向；未输出（测试按钮/指令回放）显示占位 */
  function outputLabel(output: RecognitionRecord["output"]): string {
    switch (output) {
      case "paste":
        return m.service_output_option_paste();
      case "clipboard":
        return m.service_output_option_clipboard();
      case "ime":
        return m.service_output_option_ime();
      default:
        return m.service_history_no_output();
    }
  }
</script>

<button
  type="button"
  class="flex w-full items-start gap-2 rounded-lg bg-muted/50 px-2.5 py-2 text-left hover:bg-muted"
  aria-expanded={open}
  onclick={onToggle}
>
  <div class="min-w-0 flex-1">
    {#if record.kind === "directive"}
      <p class={open ? "text-sm whitespace-pre-wrap" : "line-clamp-2 text-sm"}>
        {directiveDisplay(record)}{#if !record.directiveHit}
          <span class="text-muted-foreground">[{m.service_history_directive_missed()}]</span>
        {/if}
      </p>
    {:else if record.segments.length > 0}
      <p class={open ? "text-sm whitespace-pre-wrap" : "line-clamp-2 text-sm"}>
        {#each record.segments as segment, index (index)}
          {#if segment.kind === "original"}<s class="text-muted-foreground">{segment.text}</s
            >{:else if segment.kind === "replacement"}<strong>{segment.text}</strong
            >{:else}{segment.text}{/if}
        {/each}
      </p>
    {:else}
      <p class={open ? "text-sm whitespace-pre-wrap" : "line-clamp-2 text-sm"}>
        {record.text}
      </p>
    {/if}
    <p class="mt-1 text-xs text-muted-foreground tabular-nums">
      {recordTime(record.at)} · {m.service_result_audio()}
      {record.audioSec.toFixed(1)}s · {m.service_result_time()}
      {(record.inferMs / 1000).toFixed(1)}s · {outputLabel(record.output)}
    </p>
  </div>
  <Badge variant="secondary" class="shrink-0">
    {kindLabel(record.kind)}
  </Badge>
</button>
