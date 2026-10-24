import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import Component from "../../../../../../routes/(main)/directives/_components/directive-demo.svelte";
import { m } from "#libs/i18n/paraglide/messages";
import type { DirectiveAction } from "#libs/commands/types";

// jsdom 缺失的浏览器 API，就地补齐（同其它组件测试）。
if (typeof ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  } as unknown as typeof ResizeObserver;
}

// C 按住、V 按住、V 松开、C 松开：覆盖按下无描边与松开独占描边
const actions: DirectiveAction[] = [
  { key: "C", direction: "press" },
  { key: "V", direction: "press" },
  { key: "V", direction: "release" },
  { key: "C", direction: "release" },
];

/** 当前带描边环的键帽标签 */
function ringedLabels(): (string | null)[] {
  return [...document.querySelectorAll("button.ring-2")].map((button) =>
    button.getAttribute("aria-label"),
  );
}

/** 渲染后立刻暂停并步进到指定步（0 起），自动播放来不及触发 */
async function renderPausedAt(step: number): Promise<void> {
  render(Component, { props: { actions, intervalMs: 50 } });
  await fireEvent.click(screen.getByRole("button", { name: m.directives_demo_pause() }));
  for (let i = 0; i <= step; i += 1) {
    await fireEvent.click(screen.getByRole("button", { name: m.directives_demo_step_forward() }));
  }
}

afterEach(() => {
  cleanup();
});

describe("指令效果演示", () => {
  it("按下步骤无描边，松开步骤仅被松开的键描边", async () => {
    await renderPausedAt(0);
    // 第 1 步 C 按下：按住填充、无描边
    expect(ringedLabels()).toEqual([]);

    await fireEvent.click(screen.getByRole("button", { name: m.directives_demo_step_forward() }));
    // 第 2 步 V 按下：两键按住、无描边
    expect(ringedLabels()).toEqual([]);

    await fireEvent.click(screen.getByRole("button", { name: m.directives_demo_step_forward() }));
    // 第 3 步 V 松开：仅 V 描边，C 只剩填充
    const vLabel = document.querySelector("button.ring-2")?.getAttribute("aria-label");
    expect(vLabel).toBe("V");
    expect(ringedLabels()).toHaveLength(1);

    await fireEvent.click(screen.getByRole("button", { name: m.directives_demo_step_forward() }));
    // 第 4 步 C 松开：描边跟到 C
    expect(ringedLabels()).toHaveLength(1);
    expect(document.querySelector("button.ring-2")?.getAttribute("aria-label")).toBe("C");
  });

  it("步退回按下步骤后描边消失", async () => {
    await renderPausedAt(2);

    // 停在 V 松开步：仅 V 描边
    expect(ringedLabels()).toHaveLength(1);

    await fireEvent.click(screen.getByRole("button", { name: m.directives_demo_step_back() }));
    // 回到 V 按下步：描边消失
    expect(ringedLabels()).toEqual([]);
  });
});
