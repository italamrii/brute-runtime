import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
});

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
