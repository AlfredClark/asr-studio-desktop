import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import Component from "../../../../components/common/recognition-record-item.svelte";
import type { RecognitionRecord } from "#libs/hooks/service.svelte";

/** 记录行固件：调用方按场景覆盖字段 */
function record(partial: Partial<RecognitionRecord> = {}): RecognitionRecord {
  return {
    text: "",
    audioSec: 1,
    inferMs: 100,
    segments: [],
    fileName: "a.wav",
    id: 1,
    at: 0,
    kind: "voice",
    output: null,
    spokenPinyin: "",
    directiveHit: false,
    directiveName: "",
    ...partial,
  };
}

/** 渲染单行记录：展开态便于断言全文 */
function renderRecord(entry: RecognitionRecord) {
  render(Component, { props: { record: entry, open: true, onToggle: vi.fn() } });
}

afterEach(() => {
  cleanup();
});

describe("识别记录行", () => {
  it("语音无片段时纯文本展示", () => {
    renderRecord(record({ text: "你好世界", kind: "voice" }));

    expect(screen.getByText("你好世界")).not.toBeNull();
    expect(document.querySelector("s")).toBeNull();
    expect(document.querySelector("strong")).toBeNull();
  });

  it("语音有片段时原文删除线、替换加粗", () => {
    renderRecord(
      record({
        text: "权力以赴",
        kind: "voice",
        segments: [
          { kind: "original", text: "全" },
          { kind: "replacement", text: "权力" },
          { kind: "unchanged", text: "以赴" },
        ],
      }),
    );

    const struck = screen.getByText("全");
    expect(struck.closest("s")).not.toBeNull();
    const bold = screen.getByText("权力");
    expect(bold.closest("strong")).not.toBeNull();
    expect(screen.getByText("以赴")).not.toBeNull();
  });

  it("指令命中时展示文本拼音指令名", () => {
    renderRecord(
      record({
        text: "打开记事本",
        kind: "directive",
        spokenPinyin: "da kai ji shi ben",
        directiveHit: true,
        directiveName: "记事本",
      }),
    );

    expect(screen.getByText("打开记事本[da kai ji shi ben][记事本]")).not.toBeNull();
  });

  it("指令未命中时追加未命中标记", () => {
    renderRecord(
      record({
        text: "打开记事本",
        kind: "directive",
        spokenPinyin: "da kai ji shi ben",
        directiveHit: false,
        directiveName: "",
      }),
    );

    expect(screen.getByText("打开记事本[da kai ji shi ben]")).not.toBeNull();
    expect(screen.getByText("[Missed]")).not.toBeNull();
  });

  it("指令拼音缺失时省略中括号段", () => {
    renderRecord(
      record({
        text: "打开记事本",
        kind: "directive",
        spokenPinyin: "",
        directiveHit: true,
        directiveName: "记事本",
      }),
    );

    expect(screen.getByText("打开记事本[记事本]")).not.toBeNull();
  });
});
