import { describe, expect, it } from "vitest";

import {
  clampPercent,
  escapeRegExp,
  formatBytes,
  formatDuration,
  formatUptime,
  humanize,
  truncatePath,
} from "./format";

describe("formatBytes", () => {
  it("renders binary units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 * 1024 * 3)).toBe("3.0 MB");
    expect(formatBytes(1024 * 1024 * 1024 * 2)).toBe("2.0 GB");
  });

  it("handles missing values", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(undefined)).toBe("—");
  });
});

describe("formatDuration", () => {
  it("switches units", () => {
    expect(formatDuration(250)).toBe("250 ms");
    expect(formatDuration(1500)).toBe("1.50 s");
    expect(formatDuration(90_000)).toBe("1 m 30 s");
  });
});

describe("formatUptime", () => {
  it("renders days, hours and minutes", () => {
    expect(formatUptime(90)).toBe("1m");
    expect(formatUptime(3700)).toBe("1h 1m");
    expect(formatUptime(90_000)).toBe("1d 1h");
  });
});

describe("humanize", () => {
  it("converts snake_case identifiers", () => {
    expect(humanize("registry_key")).toBe("Registry Key");
    expect(humanize("scheduled_task")).toBe("Scheduled Task");
  });
});

describe("truncatePath", () => {
  it("keeps the head and the tail", () => {
    const path = `C:\\Program Files\\${"x".repeat(80)}\\agent.exe`;
    const result = truncatePath(path, 40);
    expect(result.length).toBeLessThanOrEqual(41);
    expect(result).toContain("…");
  });

  it("leaves short paths untouched", () => {
    expect(truncatePath("C:\\app", 40)).toBe("C:\\app");
  });
});

describe("escapeRegExp", () => {
  it("escapes metacharacters", () => {
    expect(escapeRegExp("a.b*c")).toBe("a\\.b\\*c");
  });
});

describe("clampPercent", () => {
  it("clamps into the 0..100 range", () => {
    expect(clampPercent(-10)).toBe(0);
    expect(clampPercent(150)).toBe(100);
    expect(clampPercent(42.5)).toBe(42.5);
    expect(clampPercent(Number.NaN)).toBe(0);
  });
});
