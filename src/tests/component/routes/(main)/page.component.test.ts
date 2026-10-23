import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import type { Directive, LexiconEntry, LexiconGroup, ModelEntry } from "#libs/commands/types";
import { m } from "#libs/i18n/paraglide/messages";
import Page from "../../../../routes/(main)/+page.svelte";

// bits-ui 组件在 jsdom 下缺失的浏览器 API，就地补齐（同服务页测试）。
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

// 纯替身：四域 hook 全部 mock，只验证首页的分支、卡片计数与接线。
const refreshModelsMock = vi.hoisted(() => vi.fn());
const refreshServiceDataMock = vi.hoisted(() => vi.fn());
const startServiceMock = vi.hoisted(() => vi.fn());
const stopServiceMock = vi.hoisted(() => vi.fn());
const selectModelMock = vi.hoisted(() => vi.fn());
const selectDeviceMock = vi.hoisted(() => vi.fn());
const refreshLexiconMock = vi.hoisted(() => vi.fn());
const refreshDirectivesMock = vi.hoisted(() => vi.fn());
const updateConfigMock = vi.hoisted(() => vi.fn());
const gotoMock = vi.hoisted(() => vi.fn());
const modelsStateMock = vi.hoisted(
  (): { entries: ModelEntry[]; progress: Record<string, unknown> } => ({
    entries: [],
    progress: {},
  }),
);
const serviceStateMock = vi.hoisted(
  (): {
    status: { state: string; modelId: string | null; loadMs: number | null } | null;
    models: ModelEntry[];
    devices: { id: string; name: string; is_default: boolean }[];
    selectedModel: string | null;
    selectedDevice: string | null;
    switching: boolean;
    recording: boolean;
    recognizing: boolean;
    history: {
      id: number;
      text: string;
      audioSec: number;
      inferMs: number;
      fileName: string;
      at: number;
      kind: "voice";
      output: "paste" | "clipboard" | "ime" | null;
    }[];
  } => ({
    status: null,
    models: [],
    devices: [],
    selectedModel: null,
    selectedDevice: null,
    switching: false,
    recording: false,
    recognizing: false,
    history: [],
  }),
);
const lexiconStateMock = vi.hoisted(
  (): {
    value: { groups: LexiconGroup[]; entries: LexiconEntry[]; active_group_ids: string[] };
  } => ({
    value: { groups: [], entries: [], active_group_ids: [] },
  }),
);
const directiveStateMock = vi.hoisted((): { value: Directive[] } => ({ value: [] }));

vi.mock("#libs/hooks/models.svelte", () => ({
  modelsState: modelsStateMock,
  refreshModels: refreshModelsMock,
}));
vi.mock("#libs/hooks/service.svelte", async (importOriginal) => {
  const actual = await importOriginal<typeof import("#libs/hooks/service.svelte")>();
  return {
    ...actual,
    serviceState: serviceStateMock,
    refreshServiceData: refreshServiceDataMock,
    startService: startServiceMock,
    stopService: stopServiceMock,
    selectModel: selectModelMock,
    selectDevice: selectDeviceMock,
  };
});
vi.mock("#libs/hooks/lexicon.svelte", async (importOriginal) => {
  const actual = await importOriginal<typeof import("#libs/hooks/lexicon.svelte")>();
  return {
    ...actual,
    lexiconState: lexiconStateMock,
    refreshLexicon: refreshLexiconMock,
  };
});
vi.mock("#libs/hooks/directives.svelte", () => ({
  directiveState: directiveStateMock,
  refreshDirectives: refreshDirectivesMock,
}));
vi.mock("#libs/hooks/config.svelte", () => ({
  configValue: (_key: string, fallback: unknown) => fallback,
  updateConfig: updateConfigMock,
}));
vi.mock("$app/navigation", () => ({ goto: gotoMock }));

const readyEntry: ModelEntry = {
  id: "sense-voice-int8",
  category: "sense_voice",
  display_name: "SenseVoice int8",
  tarball_bytes: 163_002_883,
  required_bytes: 239_549_735,
  local_bytes: 239_549_735,
  status: "ready",
  local_path: "/models",
  custom_dir: null,
  manual: false,
  page_url: "https://example.invalid/models",
};

function directive(id: string, enabled: boolean): Directive {
  return {
    id,
    name: id,
    trigger_text: "打开记事本",
    trigger_pinyin: "da kai ji shi ben",
    enabled,
    interval_ms: 50,
    actions: [],
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  refreshModelsMock.mockResolvedValue(true);
  refreshServiceDataMock.mockResolvedValue(undefined);
  refreshLexiconMock.mockResolvedValue(true);
  refreshDirectivesMock.mockResolvedValue(true);
  updateConfigMock.mockResolvedValue(true);
  modelsStateMock.entries = [];
  modelsStateMock.progress = {};
  serviceStateMock.status = null;
  serviceStateMock.models = [];
  serviceStateMock.devices = [];
  serviceStateMock.selectedModel = null;
  serviceStateMock.selectedDevice = null;
  serviceStateMock.switching = false;
  serviceStateMock.recording = false;
  serviceStateMock.recognizing = false;
  serviceStateMock.history = [];
  lexiconStateMock.value = { groups: [], entries: [], active_group_ids: [] };
  directiveStateMock.value = [];
});

afterEach(() => {
  cleanup();
});

describe("首页工作台", () => {
  it("首帧先显示加载态", () => {
    render(Page);
    expect(screen.getByText(m.home_loading())).not.toBeNull();
  });

  it("无就绪模型时显示三步引导，点第一步跳模型页", async () => {
    const user = userEvent.setup();
    render(Page);
    const onboarding = await screen.findByTestId("home-onboarding");
    expect(within(onboarding).getByText(m.home_step1_title())).not.toBeNull();
    await user.click(within(onboarding).getByRole("button", { name: m.home_step1_action() }));
    expect(gotoMock).toHaveBeenCalledWith("/models");
  });

  it("就绪后显示四状态卡与计数", async () => {
    const user = userEvent.setup();
    modelsStateMock.entries = [readyEntry];
    serviceStateMock.models = [readyEntry];
    serviceStateMock.selectedModel = readyEntry.id;
    serviceStateMock.status = { state: "stopped", modelId: null, loadMs: null };
    lexiconStateMock.value = {
      groups: [
        { id: "global", name: "", is_global: true },
        { id: "g-work", name: "工作", is_global: false },
      ],
      entries: [
        {
          id: "w1",
          group_id: "global",
          text: "权力",
          pinyin: "quan li",
          weight: 0,
          enabled: true,
        },
        {
          id: "w2",
          group_id: "g-work",
          text: "成长",
          pinyin: "cheng zhang",
          weight: 0,
          enabled: false,
        },
      ],
      active_group_ids: ["g-work"],
    };
    directiveStateMock.value = [directive("d-1", true), directive("d-2", false)];
    render(Page);
    const workspace = await screen.findByTestId("home-workspace");
    // 模型卡一行：计数与占用同行展示（同节点多文本，用正则匹配）
    expect(within(workspace).getByText(/1 ready/)).not.toBeNull();
    // 模型卡一行：计数与占用同行展示
    expect(within(workspace).getByText(/239\.5 MB/)).not.toBeNull();
    // 词库卡一行：分组与词条同行展示（测试环境为英文文案）
    expect(within(workspace).getByText(/2 groups/)).not.toBeNull();
    expect(within(workspace).getByText(/2 entries/)).not.toBeNull();
    expect(
      within(workspace).getByText(m.home_directives_summary({ enabled: 1, total: 2 })),
    ).not.toBeNull();
    // 新增第二行：生效分组名、首个启用指令名、选中模型名（下拉选中值同名出现两次）
    expect(within(workspace).getByText(/工作/)).not.toBeNull();
    expect(within(workspace).getByText("d-1")).not.toBeNull();
    expect(within(workspace).getAllByText(new RegExp(readyEntry.display_name))).toHaveLength(2);
    expect(within(workspace).getByText(m.home_recent_empty())).not.toBeNull();
    expect(within(workspace).getByText(m.home_recent_session())).not.toBeNull();
    await user.click(within(workspace).getByRole("button", { name: m.home_step3_action() }));
    expect(gotoMock).toHaveBeenCalledWith("/service");
  });

  it("停止态点启动调服务启动", async () => {
    const user = userEvent.setup();
    modelsStateMock.entries = [readyEntry];
    serviceStateMock.models = [readyEntry];
    serviceStateMock.selectedModel = readyEntry.id;
    serviceStateMock.status = { state: "stopped", modelId: null, loadMs: null };
    render(Page);
    const workspace = await screen.findByTestId("home-workspace");
    await user.click(within(workspace).getByRole("button", { name: m.service_start() }));
    expect(startServiceMock).toHaveBeenCalledOnce();
  });

  it("服务卡管理按钮跳服务页", async () => {
    const user = userEvent.setup();
    modelsStateMock.entries = [readyEntry];
    render(Page);
    const workspace = await screen.findByTestId("home-workspace");
    await user.click(within(workspace).getByRole("button", { name: m.home_service_manage() }));
    expect(gotoMock).toHaveBeenCalledWith("/service");
  });

  it("运行态显示停止按钮与运行中模型名", async () => {
    const user = userEvent.setup();
    modelsStateMock.entries = [readyEntry];
    serviceStateMock.models = [readyEntry];
    serviceStateMock.selectedModel = readyEntry.id;
    serviceStateMock.status = { state: "running", modelId: readyEntry.id, loadMs: 800 };
    render(Page);
    const workspace = await screen.findByTestId("home-workspace");
    expect(within(workspace).getAllByText(readyEntry.display_name).length).toBeGreaterThanOrEqual(
      1,
    );
    await user.click(within(workspace).getByRole("button", { name: m.service_stop() }));
    expect(stopServiceMock).toHaveBeenCalledOnce();
  });

  it("识别记录不截断，全部展示", async () => {
    modelsStateMock.entries = [readyEntry];
    serviceStateMock.history = [1, 2, 3, 4].map((id) => ({
      id,
      text: `记录${id}`,
      audioSec: 1,
      inferMs: 100,
      fileName: "a.wav",
      at: id,
      kind: "voice" as const,
      output: "paste" as const,
    }));
    render(Page);
    const workspace = await screen.findByTestId("home-workspace");
    for (const id of [1, 2, 3, 4]) {
      expect(within(workspace).getByText(`记录${id}`)).not.toBeNull();
    }
    // 来源徽标居右，输出方式跟在时间后方（带“·”前缀以区别输出下拉的选中值）
    expect(within(workspace).getAllByText(m.service_history_kind_text())).not.toHaveLength(0);
    expect(within(workspace).getAllByText(/· Simulate copy-paste$/)).not.toHaveLength(0);
  });

  it("服务卡渲染模型、设备与输出三个下拉", async () => {
    modelsStateMock.entries = [readyEntry];
    serviceStateMock.models = [readyEntry];
    serviceStateMock.devices = [{ id: "default", name: "System default", is_default: true }];
    serviceStateMock.selectedModel = readyEntry.id;
    serviceStateMock.selectedDevice = "default";
    serviceStateMock.status = { state: "stopped", modelId: null, loadMs: null };
    render(Page);
    const workspace = await screen.findByTestId("home-workspace");
    // 模型 + 设备 + 输出三个下拉（bits-ui 触发器为 button 角色，名取各自标签）
    expect(
      within(workspace).getByRole("combobox", { name: m.service_model_label() }),
    ).not.toBeNull();
    expect(
      within(workspace).getByRole("combobox", { name: m.service_device_label() }),
    ).not.toBeNull();
    expect(
      within(workspace).getByRole("combobox", { name: m.service_output_label() }),
    ).not.toBeNull();
    // 下拉选中值与模型卡第二行同名
    expect(within(workspace).getAllByText(readyEntry.display_name).length).toBeGreaterThanOrEqual(
      1,
    );
  });

  it("输出下拉切换调更新配置", async () => {
    const user = userEvent.setup();
    modelsStateMock.entries = [readyEntry];
    render(Page);
    const workspace = await screen.findByTestId("home-workspace");
    await user.click(within(workspace).getByRole("combobox", { name: m.service_output_label() }));
    // bits-ui 选项在 jsdom 下隐藏渲染，按文本匹配（勾选图标干扰可访问名计算）
    const option = await waitFor(() => {
      const el =
        screen
          .queryAllByRole("option", { hidden: true })
          .find((item) => item.textContent?.trim() === m.service_output_option_clipboard()) ?? null;
      expect(el).not.toBeNull();
      return el as HTMLElement;
    });
    await user.click(option);
    expect(updateConfigMock).toHaveBeenCalledWith({ shortcut_output: "clipboard" });
  });
});
