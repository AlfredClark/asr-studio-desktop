<script lang="ts">
  // 输出方式下拉（首页服务卡与服务页共用）：只渲染控件本体，行容器由调用方决定；
  // 选项值与后端 `ShortcutOutput` 落盘标签一一对应，落盘失败不乐观更新。
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

  /** 触发器宽度由调用方传入：首页用弹性宽，设置行用固定控件列宽 */
  let { triggerClass = "min-w-0 flex-1" }: { triggerClass?: string } = $props();

  /** 输出方式选项：粘贴优先、输入法置末 */
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
</script>

<Select
  type="single"
  value={configValue("shortcut_output", "paste")}
  items={outputItems}
  onValueChange={(value) => void handleOutputChange(value)}
>
  <SelectTrigger class={triggerClass} aria-label={m.service_output_label()}>
    <SelectValue />
  </SelectTrigger>
  <SelectContent>
    <SelectItem value="paste">{m.service_output_option_paste()}</SelectItem>
    <SelectItem value="clipboard">{m.service_output_option_clipboard()}</SelectItem>
    <SelectItem value="ime">{m.service_output_option_ime()}</SelectItem>
  </SelectContent>
</Select>
