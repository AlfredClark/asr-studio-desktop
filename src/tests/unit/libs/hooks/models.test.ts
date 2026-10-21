import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelDownload,
  deleteModel,
  downloadModel,
  fetchModelFiles,
  modelsState,
  openModelDir,
  openModelsRoot,
  refreshModels,
  verifyModel,
} from "#libs/hooks/models.svelte";
import type { ModelEntry } from "#libs/commands/types";

vi.mock("#libs/commands", () => ({
  default: {
    listModels: vi.fn(),
    downloadModel: vi.fn(),
    cancelDownload: vi.fn(),
    verifyModel: vi.fn(),
    deleteModel: vi.fn(),
    openModelsRoot: vi.fn(),
    openModelDir: vi.fn(),
    listModelFiles: vi.fn(),
  },
}));

vi.mock("#libs/utils/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import commands from "#libs/commands";
import { toast } from "#libs/utils/toast";

const mocked = vi.mocked(commands);
const mockedToast = vi.mocked(toast);

const entry: ModelEntry = {
  id: "sense-voice-int8",
  category: "sense_voice",
  display_name: "SenseVoice int8",
  tarball_bytes: 1_047_870_769,
  required_bytes: 239_549_735,
  status: "missing",
  local_path: null,
  page_url: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models",
  manual: false,
};

type FakeResult =
  { status: "ok"; data: unknown } | { status: "error"; error: { kind: string; message: string } };

/** 与 `EnhancedCommand` 行为一致的替身：按分支跑回调后结算结果 */
function fakeCommand(result: FakeResult): {
  success: (handler: (data: never) => unknown) => unknown;
  failed: (failure: (never: never) => unknown) => unknown;
  result: () => Promise<FakeResult>;
} {
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

beforeEach(() => {
  vi.clearAllMocks();
  modelsState.entries = [];
  modelsState.progress = {};
  mocked.listModels.mockReturnValue(fakeCommand({ status: "ok", data: [entry] }) as never);
  mocked.downloadModel.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.cancelDownload.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.verifyModel.mockReturnValue(
    fakeCommand({
      status: "ok",
      data: { id: entry.id, status: "ready", missing: [], mismatched: [] },
    }) as never,
  );
  mocked.deleteModel.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.openModelsRoot.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.openModelDir.mockReturnValue(fakeCommand({ status: "ok", data: null }) as never);
  mocked.listModelFiles.mockReturnValue(fakeCommand({ status: "ok", data: [] }) as never);
});

describe("模型全局状态", () => {
  it("刷新写入条目并返回真", async () => {
    await expect(refreshModels()).resolves.toBe(true);

    expect(mocked.listModels).toHaveBeenCalledOnce();
    expect(modelsState.entries).toEqual([entry]);
  });

  it("刷新失败时上报、返回假且不抛错", async () => {
    mocked.listModels.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await expect(refreshModels()).resolves.toBe(false);

    expect(modelsState.entries).toEqual([]);
  });

  it("下载成功清进度刷清单并提示", async () => {
    modelsState.progress[entry.id] = {
      id: entry.id,
      phase: "download",
      downloaded: 10,
      total: 100,
      current: null,
    };

    await downloadModel(entry.id);

    expect(mocked.downloadModel).toHaveBeenCalledWith(entry.id);
    expect(modelsState.progress[entry.id]).toBeUndefined();
    expect(mockedToast.success).toHaveBeenCalled();
    expect(modelsState.entries).toEqual([entry]);
  });

  it("下载失败清进度报错并刷新", async () => {
    mocked.downloadModel.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await downloadModel(entry.id);

    expect(mockedToast.error).toHaveBeenCalled();
    expect(modelsState.progress[entry.id]).toBeUndefined();
  });

  it("取消后清进度刷新并提示", async () => {
    await cancelDownload(entry.id);

    expect(mocked.cancelDownload).toHaveBeenCalledWith(entry.id);
    expect(mockedToast.success).toHaveBeenCalled();
  });

  it("校验就绪提示成功，否则报错", async () => {
    await verifyModel(entry.id);
    expect(mockedToast.success).toHaveBeenCalled();

    mocked.verifyModel.mockReturnValue(
      fakeCommand({
        status: "ok",
        data: { id: entry.id, status: "partial", missing: ["a.onnx"], mismatched: [] },
      }) as never,
    );
    await verifyModel(entry.id);
    expect(mockedToast.error).toHaveBeenCalled();
  });

  it("删除成功返回真并刷新，失败返回假", async () => {
    await expect(deleteModel(entry.id)).resolves.toBe(true);

    mocked.deleteModel.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );
    await expect(deleteModel(entry.id)).resolves.toBe(false);
  });

  it("打开目录失败时提示", async () => {
    mocked.openModelDir.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await openModelDir(entry.id);

    expect(mockedToast.error).toHaveBeenCalled();
  });

  it("打开根目录失败时提示", async () => {
    mocked.openModelsRoot.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await openModelsRoot();

    expect(mockedToast.error).toHaveBeenCalled();
  });

  it("目录树成功时原样返回", async () => {
    const rows = [{ path: "model.int8.onnx", size: 10, is_dir: false, status: "ready" }];
    mocked.listModelFiles.mockReturnValue(fakeCommand({ status: "ok", data: rows }) as never);

    await expect(fetchModelFiles(entry.id)).resolves.toEqual(rows);
    expect(mocked.listModelFiles).toHaveBeenCalledWith(entry.id);
  });

  it("目录树失败时回空", async () => {
    mocked.listModelFiles.mockReturnValue(
      fakeCommand({ status: "error", error: { kind: "Internal", message: "boom" } }) as never,
    );

    await expect(fetchModelFiles(entry.id)).resolves.toEqual([]);
  });
});
