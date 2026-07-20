import { describe, expect, it } from "vitest";
import { formatBytes, formatParamCount, formatPercent, formatTokensPerSecond, shortHash } from "./format";

describe("formatBytes", () => {
  it("renders an em dash for missing values instead of 0 or NaN", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(undefined)).toBe("—");
  });

  it("formats zero explicitly rather than falling through to the dash", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("scales to the nearest unit", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(3 * 1024 * 1024 * 1024)).toBe("3.0 GB");
  });
});

describe("formatParamCount", () => {
  it("renders an em dash for missing values", () => {
    expect(formatParamCount(null)).toBe("—");
    expect(formatParamCount(undefined)).toBe("—");
  });

  it("abbreviates millions and billions", () => {
    expect(formatParamCount(5_000_000)).toBe("5.0M");
    expect(formatParamCount(630_000_000)).toBe("630M");
    expect(formatParamCount(7_000_000_000)).toBe("7.0B");
    expect(formatParamCount(70_000_000_000)).toBe("70B");
  });

  it("leaves small counts as a plain grouped number", () => {
    expect(formatParamCount(1234)).toBe("1,234");
  });
});

describe("formatTokensPerSecond", () => {
  it("never fabricates a rate for a null measurement", () => {
    expect(formatTokensPerSecond(null)).toBe("—");
  });

  it("formats a real measurement to one decimal place", () => {
    expect(formatTokensPerSecond(12.345)).toBe("12.3 tok/s");
  });
});

describe("formatPercent", () => {
  it("returns a dash for unknown values rather than 0%", () => {
    expect(formatPercent(null)).toBe("—");
  });

  it("converts a 0..1 ratio to a percentage string", () => {
    expect(formatPercent(0.5)).toBe("50%");
  });
});

describe("shortHash", () => {
  it("returns a dash for a missing hash", () => {
    expect(shortHash(null)).toBe("—");
  });

  it("truncates a long hash with an ellipsis", () => {
    const hash = "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db";
    expect(shortHash(hash, 12)).toBe("74a4da8c9fdb…");
  });

  it("leaves a short value untouched", () => {
    expect(shortHash("abcd", 12)).toBe("abcd");
  });
});
