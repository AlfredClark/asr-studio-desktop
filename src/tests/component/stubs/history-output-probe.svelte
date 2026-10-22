<script lang="ts">
  // 历史回填响应式回归探针：订阅首条记录的输出通道，静默改值不断言
  import { serviceState } from "#libs/hooks/service.svelte";
</script>

<p data-testid="probe-output">{serviceState.history[0]?.output ?? "empty"}</p>
