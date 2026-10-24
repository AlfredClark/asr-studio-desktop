import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import { handleShortcutTrigger, serviceState } from "#libs/hooks/service.svelte";
import Probe from "../../stubs/history-output-probe.svelte";

// 通知级回归：用真实 hook 模块 + 模拟命令，走一遍快捷键输出流程，
// 探针订阅首条记录的输出通道——值回填必须触发重渲染（曾出现静默改值导致页面恒显示“未输出”）。
vi.mock("#libs/commands", () => {
  type FakeResult =
    { status: "ok"; data: unknown } | { status: "error"; error: { kind: string; message: string } };
  function fake(result: FakeResult) {
    const chain = {
      success(handler: (data: never) => unknown) {
        if (result.status === "ok") handler(result.data as never);
        return chain;
      },
      failed(handler: (failure: never) => unknown) {
        if (result.status === "error") handler(result.error as never);
        return chain;
      },
      result() {
        return Promise.resolve(result);
      },
    };
    return chain;
  }
  return {
    default: {
      stopRecording: () =>
        fake({ status: "ok", data: { path: "/records/a.wav", file_name: "a.wav", seconds: 1 } }),
      transcribeFile: () =>
        fake({ status: "ok", data: { text: "你好", audio_sec: 1, infer_ms: 100, segments: [] } }),
      outputText: () =>
        fake({
          status: "ok",
          data: { via: "clipboard", fallback: true, reason: "injection_fallback" },
        }),
      serviceStatus: () =>
        fake({
          status: "ok",
          data: { state: "running", model_id: "m", display_name: "M", load_ms: 1 },
        }),
    },
  };
});
vi.mock("#libs/utils/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), message: vi.fn() },
}));
vi.mock("$app/navigation", () => ({ goto: vi.fn() }));

beforeEach(() => {
  vi.clearAllMocks();
  cleanup();
  serviceState.status = { state: "running", modelId: "m", loadMs: 1 };
  serviceState.selectedModel = "m";
  serviceState.selectedDevice = "d";
  serviceState.recording = true;
  serviceState.recognizing = false;
  serviceState.switching = false;
  serviceState.history = [];
  serviceState.result = null;
});

describe("历史输出通道回填", () => {
  it("快捷键输出成功后探针从空变为实际通道", async () => {
    render(Probe);
    expect(screen.getByTestId("probe-output").textContent).toBe("empty");

    await handleShortcutTrigger();

    await waitFor(() => {
      expect(screen.getByTestId("probe-output").textContent).toBe("clipboard");
    });
    expect(serviceState.history).toHaveLength(1);
  });
});
