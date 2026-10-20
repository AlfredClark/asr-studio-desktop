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
// 标题、分组折叠、选中切换、详情渲染与按钮接线。
const refreshModelsMock = vi.hoisted(() => vi.fn());
const downloadModelMock = vi.hoisted(() => vi.fn());
const cancelDownloadMock = vi.hoisted(() => vi.fn());
const verifyModelMock = vi.hoisted(() => vi.fn());
const deleteModelMock = vi.hoisted(() => vi.fn());
const openModelsRootMock = vi.hoisted(() => vi.fn());
const openModelDirMock = vi.hoisted(() => vi.fn());
const fetchModelFilesMock = vi.hoisted(() => vi.fn());
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
  fetchModelFiles: fetchModelFilesMock,
}));

vi.mock("#libs/utils/opener", () => ({ openExternal: openExternalMock }));

vi.mock("#libs/utils/toast", () => ({ toast: toastMocks }));

beforeEach(() => {
  vi.clearAllMocks();
  // 纯对象替身无响应式：渲染前预填，组件首帧即读到（同设置页测试模式）
  modelsStateMock.entries = [nanoEntry, qwenEntry, senseEntry];
  modelsStateMock.progress = {};
  deleteModelMock.mockResolvedValue(true);
  fetchModelFilesMock.mockResolvedValue([]);
  openExternalMock.mockResolvedValue(true);
});

afterEach(() => {
  cleanup();
  document.body.removeAttribute("style");
});

describe("模型页", () => {
  /** 右侧详情区：标题在列表与详情各出现一次，此处圈定详情范围 */
  function detail(): ReturnType<typeof within> {
    return within(screen.getByTestId("model-detail"));
  }

  /** 左侧列表行：按钮名由名称加状态组成，用名称子串匹配 */
  function listRow(name: string): HTMLElement {
    return screen.getByRole("button", { name: new RegExp(name) });
  }

  it("挂载即刷新，分组渲染且默认选中首个", async () => {
    render(Page);

    await waitFor(() => {
      expect(refreshModelsMock).toHaveBeenCalledOnce();
    });
    expect(screen.getByText("FunASR Nano family")).not.toBeNull();
    expect(screen.getByText("Model list")).not.toBeNull();
    expect(screen.getByText("SenseVoice family")).not.toBeNull();
    expect(screen.getByText("Qwen3-ASR family")).not.toBeNull();
    // 默认选中首个分类的首个模型，详情区展示其标题与简介
    expect(detail().getByText("FunASR Nano int8")).not.toBeNull();
    expect(detail().getByText(/Qwen3-0.6B LLM/)).not.toBeNull();
  });

  it("空态无分组，右侧显示空提示", async () => {
    modelsStateMock.entries = [];
    render(Page);

    await waitFor(() => {
      expect(refreshModelsMock).toHaveBeenCalledOnce();
    });
    expect(screen.queryByText("SenseVoice int8")).toBeNull();
    expect(screen.getByText("Select a model on the left to see details")).not.toBeNull();
  });

  it("点击列表行切换右侧详情", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(listRow("SenseVoice int8")).not.toBeNull();
    });
    await user.click(listRow("SenseVoice int8"));

    expect(detail().getByText("SenseVoice int8")).not.toBeNull();
    expect(detail().getByText(/inverse text normalization/)).not.toBeNull();
  });

  it("折叠分组隐藏行，展开恢复", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(listRow("SenseVoice int8")).not.toBeNull();
    });
    await user.click(screen.getByRole("button", { name: /SenseVoice family/ }));
    expect(screen.queryByRole("button", { name: /SenseVoice int8/ })).toBeNull();

    await user.click(screen.getByRole("button", { name: /SenseVoice family/ }));
    expect(listRow("SenseVoice int8")).not.toBeNull();
  });

  it("缺失态在详情点下载调命令", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(listRow("SenseVoice int8")).not.toBeNull();
    });
    await user.click(listRow("SenseVoice int8"));
    await user.click(detail().getByRole("button", { name: "Download" }));

    expect(downloadModelMock).toHaveBeenCalledWith("sense-voice-int8");
  });

  it("就绪态在详情提供校验与打开目录入口", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(detail().getByText("FunASR Nano int8")).not.toBeNull();
    });
    await user.click(detail().getByRole("button", { name: "Re-verify" }));
    expect(verifyModelMock).toHaveBeenCalledWith("funasr-nano-int8");

    await user.click(detail().getByRole("button", { name: "Open folder" }));
    expect(openModelDirMock).toHaveBeenCalledWith("funasr-nano-int8");
  });

  it("前往下载页经外部网关打开", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(listRow("SenseVoice int8")).not.toBeNull();
    });
    await user.click(listRow("SenseVoice int8"));
    const pageButton = detail().getByRole("button", { name: "Download page" });
    // 未下载态的前往下载页靠右对齐
    expect(pageButton.classList.contains("ml-auto")).toBe(true);
    await user.click(pageButton);

    expect(openExternalMock).toHaveBeenCalledWith(
      "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models",
    );
  });

  it("下载页打不开时提示", async () => {
    openExternalMock.mockResolvedValue(false);
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(listRow("SenseVoice int8")).not.toBeNull();
    });
    await user.click(listRow("SenseVoice int8"));
    await user.click(detail().getByRole("button", { name: "Download page" }));

    await waitFor(() => {
      expect(toastMocks.error).toHaveBeenCalled();
    });
  });

  it("目录树在简介下方展示文件核对", async () => {
    fetchModelFilesMock.mockResolvedValue([
      { path: "model.int8.onnx", size: 239_233_841, is_dir: false, status: "missing" },
      { path: "tokenizer", size: 2_776_833, is_dir: true, status: "partial" },
      { path: "tokenizer/vocab.json", size: 2_776_833, is_dir: false, status: "ready" },
    ]);
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(listRow("SenseVoice int8")).not.toBeNull();
    });
    await user.click(listRow("SenseVoice int8"));

    await waitFor(() => {
      expect(detail().getByText("Folder contents")).not.toBeNull();
    });
    expect(detail().getByText("model.int8.onnx")).not.toBeNull();
    expect(detail().getByText("vocab.json")).not.toBeNull();
    // 大小后方徽标展示核对结论：缺失、部分、就绪各一（标题徽标另计，不在此断言）
    expect(detail().getByText("Ready")).not.toBeNull();
    expect(detail().getByText("Incomplete")).not.toBeNull();
    expect(detail().getAllByText("Not downloaded").length).toBeGreaterThan(0);
    expect(fetchModelFilesMock).toHaveBeenCalledWith("sense-voice-int8");
  });

  it("目录树文件夹可折叠", async () => {
    fetchModelFilesMock.mockResolvedValue([
      { path: "tokenizer", size: 2_776_833, is_dir: true, status: "ready" },
      { path: "tokenizer/vocab.json", size: 2_776_833, is_dir: false, status: "ready" },
    ]);
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(listRow("SenseVoice int8")).not.toBeNull();
    });
    await user.click(listRow("SenseVoice int8"));
    await waitFor(() => {
      expect(detail().getByText("vocab.json")).not.toBeNull();
    });

    const folder = detail().getByRole("button", { name: /tokenizer/ });
    expect(folder.getAttribute("aria-expanded")).toBe("true");
    await user.click(folder);
    expect(detail().queryByText("vocab.json")).toBeNull();
    expect(folder.getAttribute("aria-expanded")).toBe("false");
    await user.click(folder);
    expect(detail().getByText("vocab.json")).not.toBeNull();
  });

  it("删除走二次确认", async () => {
    const user = userEvent.setup();
    render(Page);

    await waitFor(() => {
      expect(detail().getByText("FunASR Nano int8")).not.toBeNull();
    });
    await user.click(detail().getByRole("button", { name: "Delete" }));
    // 确认弹窗经 portal 挂载，排在详情按钮之后
    const confirmButtons = screen.getAllByRole("button", { name: "Delete" });
    expect(confirmButtons.length).toBe(2);
    await user.click(confirmButtons[1]);

    await waitFor(() => {
      expect(deleteModelMock).toHaveBeenCalledWith("funasr-nano-int8");
    });
  });
});
