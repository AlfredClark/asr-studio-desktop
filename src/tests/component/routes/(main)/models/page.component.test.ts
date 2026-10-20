import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import type { ModelEntry } from "#libs/commands/types";
import Page from "../../../../../routes/(main)/models/+page.svelte";

// bits-ui 组件在 jsdom 下缺失的浏览器 API，就地补齐（同设置页测试）。
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}
if (typeof ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  } as unknown as typeof ResizeObserver;
}

// 纯替身：模型状态 hook 与外部打开网关全部 mock，只验证模型页的组装——
// 标题、分类分组、卡片渲染与按钮接线。
const refreshModelsMock = vi.hoisted(() => vi.fn());
const downloadModelMock = vi.hoisted(() => vi.fn());
const cancelDownloadMock = vi.hoisted(() => vi.fn());
const verifyModelMock = vi.hoisted(() => vi.fn());
const deleteModelMock = vi.hoisted(() => vi.fn());
const openModelsRootMock = vi.hoisted(() => vi.fn());
const openModelDirMock = vi.hoisted(() => vi.fn());
const openExternalMock = vi.hoisted(() => vi.fn());
const toastMocks = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn() }));
const modelsStateMock = vi.hoisted(
  (): { entries: ModelEntry[]; progress: Record<string, never> } => ({
    entries: [],
    progress: {},
  }),
);

const senseEntry: ModelEntry = {
  id: "sense-voice-int8",
  category: "sense_voice",
  display_name: "SenseVoice int8",
  tarball_bytes: 1_047_870_769,
  required_bytes: 239_549_735,
  status: "missing",
  local_path: null,
  page_url: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models",
};

const qwenEntry: ModelEntry = {
  id: "qwen3-0.6b-int8",
  category: "qwen3_asr",
  display_name: "Qwen3-ASR 0.6B int8",
  tarball_bytes: 878_702_423,
  required_bytes: 986_012_257,
  status: "partial",
  local_path: "/models/qwen3-asr/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25",
  page_url: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models",
};

const nanoEntry: ModelEntry = {
  id: "funasr-nano-int8",
  category: "funasr_nano",
  display_name: "FunASR Nano int8",
  tarball_bytes: 841_730_611,
  required_bytes: 1_009_071_962,
  status: "ready",
  local_path: "/models/funasr-nano/sherpa-onnx-funasr-nano-int8-2025-12-30",
  page_url: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models",
};

vi.mock("#libs/hooks/models.svelte", () => ({
  modelsState: modelsStateMock,
  refreshModels: refreshModelsMock,
  downloadModel: downloadModelMock,
  cancelDownload: cancelDownloadMock,
  verifyModel: verifyModelMock,
  deleteModel: deleteModelMock,
  openModelsRoot: openModelsRootMock,
  openModelDir: openModelDirMock,
}));

vi.mock("#libs/utils/opener", () => ({ openExternal: openExternalMock }));

vi.mock("#libs/utils/toast", () => ({ toast: toastMocks }));

beforeEach(() => {
  vi.clearAllMocks();
  // 纯对象替身无响应式：渲染前预填，组件首帧即读到（同设置页测试模式）
  modelsStateMock.entries = [nanoEntry, qwenEntry, senseEntry];
  modelsStateMock.progress = {};
  deleteModelMock.mockResolvedValue(true);
});

afterEach(() => {
  cleanup();
  document.body.removeAttribute("style");
});

describe("模型页", () => {
  /** 按卡片标题圈定查询范围：同名按钮多卡并存，全局查询会歧义 */
  function cardFor(title: string): ReturnType<typeof within> {
    const titleEl = screen.getByText(title);
    const card = titleEl.closest('[data-slot="card"]');
    if (!card) throw new Error(`card not found for ${title}`);
    return within(card as HTMLElement);
  }

  it("挂载即刷新并渲染标题与分类分组", async () => {
    render(Page);

    await waitFor(() => {
      expect(refreshModelsMock).toHaveBeenCalledOnce();
    });
    expect(screen.getByText("Models")).not.toBeNull();
    expect(screen.getByText("FunASR Nano family")).not.toBeNull();
    expect(screen.getByText("SenseVoice family")).not.toBeNull();
    expect(screen.getByText("Qwen3-ASR family")).not.toBeNull();
  });

  it("空态不渲染任何卡片", async () => {
    modelsStateMock.entries = [];
    render(Page);

    await waitFor(() => {
      expect(refreshModelsMock).toHaveBeenCalledOnce();
    });
    expect(screen.queryByText("SenseVoice int8")).toBeNull();
  });

  it("缺失态点下载调命令", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(screen.getByText("SenseVoice int8")).not.toBeNull();
    });
    await user.click(cardFor("SenseVoice int8").getByRole("button", { name: "Download" }));

    expect(downloadModelMock).toHaveBeenCalledWith("sense-voice-int8");
  });

  it("就绪态提供校验与打开目录入口", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(screen.getByText("FunASR Nano int8")).not.toBeNull();
    });
    const nanoCard = cardFor("FunASR Nano int8");
    await user.click(nanoCard.getByRole("button", { name: "Re-verify" }));
    expect(verifyModelMock).toHaveBeenCalledWith("funasr-nano-int8");

    await user.click(nanoCard.getByRole("button", { name: "Open folder" }));
    expect(openModelDirMock).toHaveBeenCalledWith("funasr-nano-int8");
  });

  it("前往下载页经外部网关打开", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(screen.getByText("SenseVoice int8")).not.toBeNull();
    });
    await user.click(cardFor("SenseVoice int8").getByRole("button", { name: "Download page" }));

    expect(openExternalMock).toHaveBeenCalledWith(
      "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models",
    );
  });

  it("删除走二次确认", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(screen.getByText("FunASR Nano int8")).not.toBeNull();
    });
    const nanoCard = cardFor("FunASR Nano int8");
    await user.click(nanoCard.getByRole("button", { name: "Delete" }));
    // 确认弹窗经 portal 挂载，排在卡片按钮之后
    const confirmButtons = screen.getAllByRole("button", { name: "Delete" });
    expect(confirmButtons.length).toBe(2);
    await user.click(confirmButtons[1]);

    await waitFor(() => {
      expect(deleteModelMock).toHaveBeenCalledWith("funasr-nano-int8");
    });
  });
});
