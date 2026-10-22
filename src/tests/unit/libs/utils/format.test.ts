import { describe, expect, it } from "vitest";
import { formatBytes } from "#libs/utils/format";

describe("formatBytes", () => {
  it("按 GB/MB/KB 分档展示", () => {
    expect(formatBytes(1_047_870_769)).toBe("1.0 GB");
    expect(formatBytes(239_549_735)).toBe("239.5 MB");
    expect(formatBytes(1500)).toBe("2 KB");
  });

  it("零字节回落 1KB，避免误读为空", () => {
    expect(formatBytes(0)).toBe("1 KB");
  });
});
