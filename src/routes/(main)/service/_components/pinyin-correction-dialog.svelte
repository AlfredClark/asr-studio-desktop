<script lang="ts">
  // 拼音纠正弹窗：分组复选框 + 底部识别/指令双开关；全程草稿态，点保存才一次落盘。
  // 打开时从后端配置回填，取消直接丢弃；总开关由两应用开关派生（任一开即启用）。
  import { SvelteSet } from "svelte/reactivity";
  import { Button } from "#components/shadcn-svelte/button";
  import { Switch } from "#components/shadcn-svelte/switch";
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
  } from "#components/shadcn-svelte/dialog";
  import { m } from "#libs/i18n/paraglide/messages";
  import { configValue, updateConfig } from "#libs/hooks/config.svelte";
  import { toast } from "#libs/utils/toast";
  import {
    PINYIN_CORRECTION_GROUPS,
    PINYIN_CORRECTION_RULE_IDS,
    PINYIN_CORRECTION_RULE_META,
    type PinyinCorrectionGroup,
  } from "#libs/pinyin-correction";

  /** 弹窗开关（父组件双向绑定） */
  let { open = $bindable(false) }: { open: boolean } = $props();

  /** 草稿：识别 / 指令应用开关，默认全勾，仅在打开时从配置回填 */
  let draftApplyRecognition = $state(true);
  let draftApplyDirective = $state(true);
  /** 草稿：选中规则 id 集合（`SvelteSet` 自带响应式，无需包 `$state`） */
  const draftRules = new SvelteSet<string>();
  /** 保存中禁用，避免重复提交 */
  let saving = $state(false);

  /** 打开时从后端配置回填草稿；水合前按默认值渲染 */
  $effect(() => {
    if (open) {
      draftApplyRecognition = configValue("pinyin_correction_apply_recognition", true);
      draftApplyDirective = configValue("pinyin_correction_apply_directive", true);
      draftRules.clear();
      for (const rule of configValue("pinyin_correction_rules", [])) draftRules.add(rule);
      saving = false;
    }
  });

  /** 分组标题：id 到本地化文案的唯一映射 */
  function groupLabel(group: PinyinCorrectionGroup): string {
    switch (group.id) {
      case "flat":
        return m.service_pinyin_group_flat();
      case "nasal":
        return m.service_pinyin_group_nasal();
      case "lateral":
        return m.service_pinyin_group_lateral();
      case "labial":
        return m.service_pinyin_group_labial();
      case "rhyme":
        return m.service_pinyin_group_rhyme();
    }
  }

  /** 组内是否全选 */
  function isGroupChecked(group: PinyinCorrectionGroup): boolean {
    return group.rules.every((rule) => draftRules.has(rule));
  }

  /** 切换单条规则 */
  function toggleRule(rule: string): void {
    if (draftRules.has(rule)) {
      draftRules.delete(rule);
    } else {
      draftRules.add(rule);
    }
  }

  /** 切换整组：全选时清空本组，否则补齐本组 */
  function toggleGroup(group: PinyinCorrectionGroup): void {
    if (isGroupChecked(group)) {
      for (const rule of group.rules) draftRules.delete(rule);
    } else {
      for (const rule of group.rules) draftRules.add(rule);
    }
  }

  /** 保存：四键一次落盘，保序为注册表顺序；总开关由两应用开关派生；失败不关窗，成功提示并关闭 */
  async function handleSave(): Promise<void> {
    if (saving) return;
    saving = true;
    try {
      const rules = PINYIN_CORRECTION_RULE_IDS.filter((rule) => draftRules.has(rule));
      if (
        await updateConfig({
          pinyin_correction_enabled: draftApplyRecognition || draftApplyDirective,
          pinyin_correction_rules: [...rules],
          pinyin_correction_apply_recognition: draftApplyRecognition,
          pinyin_correction_apply_directive: draftApplyDirective,
        })
      ) {
        toast.success(m.service_pinyin_updated());
        open = false;
      } else {
        toast.error(m.service_pinyin_update_failed());
      }
    } finally {
      saving = false;
    }
  }
</script>

<Dialog bind:open>
  <DialogContent aria-label={m.service_pinyin_dialog_title()} class="sm:max-w-4xl">
    <DialogHeader>
      <DialogTitle>{m.service_pinyin_dialog_title()}</DialogTitle>
      <DialogDescription>{m.service_pinyin_dialog_description()}</DialogDescription>
    </DialogHeader>
    <div class="flex max-h-[70vh] min-h-0 flex-col gap-4 overflow-x-hidden overflow-y-auto py-1">
      {#each PINYIN_CORRECTION_GROUPS as group (group.id)}
        <section class="flex flex-col gap-1.5">
          <div class="flex items-center justify-between gap-2">
            <h3 class="text-sm font-medium">{groupLabel(group)}</h3>
            <Button variant="ghost" size="sm" onclick={() => toggleGroup(group)}>
              {m.service_pinyin_select_all()}
            </Button>
          </div>
          <div class="grid grid-cols-2 gap-1.5 md:grid-cols-4">
            {#each group.rules as rule (rule)}
              <label
                class="flex min-w-0 cursor-pointer items-center gap-2 rounded-lg bg-muted/50 px-2.5 py-2 text-sm"
              >
                <input
                  type="checkbox"
                  class="size-4 shrink-0 accent-primary"
                  checked={draftRules.has(rule)}
                  onchange={() => toggleRule(rule)}
                  aria-label={PINYIN_CORRECTION_RULE_META[rule]}
                />
                <span class="min-w-0 flex-1 break-words">{PINYIN_CORRECTION_RULE_META[rule]}</span>
              </label>
            {/each}
          </div>
        </section>
      {/each}
      <div class="grid grid-cols-2 gap-1.5">
        <label
          class="flex items-center justify-between gap-2 rounded-lg bg-muted/50 px-2.5 py-2 text-sm"
        >
          <span class="min-w-0 flex-1 font-medium">{m.service_pinyin_apply_recognition()}</span>
          <Switch
            bind:checked={draftApplyRecognition}
            aria-label={m.service_pinyin_apply_recognition()}
          />
        </label>
        <label
          class="flex items-center justify-between gap-2 rounded-lg bg-muted/50 px-2.5 py-2 text-sm"
        >
          <span class="min-w-0 flex-1 font-medium">{m.service_pinyin_apply_directive()}</span>
          <Switch
            bind:checked={draftApplyDirective}
            aria-label={m.service_pinyin_apply_directive()}
          />
        </label>
      </div>
    </div>
    <DialogFooter>
      <Button variant="outline" onclick={() => (open = false)} disabled={saving}>
        {m.service_pinyin_cancel()}
      </Button>
      <Button onclick={() => void handleSave()} disabled={saving}>
        {m.service_pinyin_save()}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
