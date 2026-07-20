import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
});

// jsdom doesn't implement scrollIntoView (a real browser API gap, not a
// bug in the code under test) - Chat's auto-scroll-to-latest-message
// effect calls it on every render, so every test needs a harmless stub.
// Guarded: this setup file also loads for node-environment test files
// (e.g. structuralGuards.test.ts), where `window` doesn't exist at all.
if (typeof window !== "undefined" && !window.HTMLElement.prototype.scrollIntoView) {
  window.HTMLElement.prototype.scrollIntoView = () => {};
}

// jsdom also doesn't implement scrollTo - Run's auto-scroll-output effect
// calls it on every output update, same reasoning as scrollIntoView above.
if (typeof window !== "undefined" && !window.HTMLElement.prototype.scrollTo) {
  window.HTMLElement.prototype.scrollTo = () => {};
}

// The real Tauri IPC bridge only exists inside the webview; unit tests
// run in jsdom, so every command call is mocked at the module boundary.
// Individual tests override these with vi.mocked(...).mockResolvedValue
// as needed — this file only provides safe, inert defaults so a test
// that never touches IPC doesn't crash on import.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
  save: vi.fn().mockResolvedValue(null),
}));
