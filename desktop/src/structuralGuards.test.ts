// @vitest-environment node
//
// Enforces the singleton-window requirement structurally: BRUTE must use
// exactly one native Tauri window (see tauri.conf.json's single "main"
// entry and capabilities/default.json, which grants only core:default -
// not core:window:allow-create). A source file that starts creating
// windows, opening popups, or navigating away would silently reintroduce
// the class of bug this guard exists to catch, regardless of what any
// individual page's own tests happen to cover.
import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname, sep } from "node:path";

const SRC_DIR = join(__dirname);

const FORBIDDEN_PATTERNS: { pattern: RegExp; reason: string }[] = [
  { pattern: /new\s+WebviewWindow\s*\(/, reason: "creates a second native Tauri window" },
  { pattern: /createWebviewWindow/, reason: "creates a second native Tauri window" },
  { pattern: /\bWindowBuilder\b/, reason: "creates a second native window" },
  { pattern: /window\.open\s*\(/, reason: "browser-native popup window" },
  { pattern: /target\s*=\s*["']_blank["']/, reason: "anchor target=_blank can spawn a new window" },
  { pattern: /@tauri-apps\/api\/window/, reason: "the window-management API is not needed - this app has exactly one window" },
  { pattern: /\bappWindow\b/, reason: "multi-window helper not used by a single-window app" },
  { pattern: /\bgetCurrentWindow\s*\(/, reason: "multi-window helper not used by a single-window app" },
];

function collectSourceFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === "test") continue;
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      collectSourceFiles(full, out);
    } else if ([".ts", ".tsx"].includes(extname(full)) && !full.endsWith(".test.ts") && !full.endsWith(".test.tsx")) {
      out.push(full);
    }
  }
  return out;
}

describe("no native window-creation or popup APIs anywhere in the frontend", () => {
  const files = collectSourceFiles(SRC_DIR);

  it("scans a non-trivial number of source files (sanity check the scan itself works)", () => {
    expect(files.length).toBeGreaterThan(10);
  });

  for (const { pattern, reason } of FORBIDDEN_PATTERNS) {
    it(`no file matches ${pattern} (${reason})`, () => {
      const offenders = files.filter((f) => pattern.test(readFileSync(f, "utf-8")));
      expect(offenders).toEqual([]);
    });
  }

  it("the only window.location usage is the documented local-state reload in Settings", () => {
    const offenders = files.filter((f) => {
      if (f.endsWith(`${sep}Settings.tsx`)) return false;
      return /window\.location/.test(readFileSync(f, "utf-8"));
    });
    expect(offenders).toEqual([]);
  });
});
