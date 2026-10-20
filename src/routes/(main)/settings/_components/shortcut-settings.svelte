<script lang="ts">
  // 快捷键设置分组：只剩快速显示一行；语音识别一行已移到服务页的识别服务分组，
  // 与服务运行态同页管理。本文件只做分组装配，无业务逻辑。
  import CardRow from "#components/common/card-row.svelte";
  import CardSection from "#components/common/card-section.svelte";
  import { m } from "#libs/i18n/paraglide/messages";
  import ShortcutRecordRow from "#components/common/shortcut-record-row.svelte";
</script>

<CardSection title={m.settings_shortcut_title()} description={m.settings_shortcut_description()}>
  <CardRow label={m.settings_quick_open_label()} description={m.settings_quick_open_description()}>
    {#snippet control()}
      <ShortcutRecordRow kind="quick_open" />
    {/snippet}
  </CardRow>
</CardSection>
