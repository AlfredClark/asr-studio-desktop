<script lang="ts">
  // 快捷键设置分组：快速显示与语音识别两行，行内录制逻辑收敛在行组件内。
  // 本文件只做分组装配，无业务逻辑。
  import CardRow from "#components/common/card-row.svelte";
  import CardSection from "#components/common/card-section.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import ShortcutRecordRow from "./shortcut-record-row.svelte";
</script>

<CardSection title={m.settings_shortcut_title()} description={m.settings_shortcut_description()}>
  <CardRow label={m.settings_quick_open_label()} description={m.settings_quick_open_description()}>
    {#snippet control()}
      <ShortcutRecordRow kind="quick_open" />
    {/snippet}
  </CardRow>
  <CardRow label={m.settings_voice_label()} description={m.settings_voice_description()}>
    {#snippet control()}
      <ShortcutRecordRow kind="voice" />
    {/snippet}
  </CardRow>
</CardSection>
