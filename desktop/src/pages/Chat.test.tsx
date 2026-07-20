import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup, within } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { AppStatusProvider } from "../lib/AppStatusContext";
import { Chat } from "./Chat";
import type { Conversation, ConversationSummary, LibraryEntry, RuntimeProfile } from "../lib/types";

const listenCallbacks: Record<string, ((event: { payload: unknown }) => void)[]> = {};
function emit(eventName: string, payload: unknown) {
  (listenCallbacks[eventName] ?? []).forEach((cb) => cb({ payload }));
}
function captureListen(eventName: string, cb: (event: { payload: unknown }) => void) {
  listenCallbacks[eventName] = listenCallbacks[eventName] ?? [];
  listenCallbacks[eventName].push(cb);
  return Promise.resolve(() => undefined);
}
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(captureListen),
}));

vi.mock("../lib/api", () => ({
  listConversations: vi.fn(),
  showConversation: vi.fn(),
  createConversation: vi.fn(),
  saveConversation: vi.fn(),
  deleteConversation: vi.fn(),
  clearAllConversations: vi.fn(),
  listLibrary: vi.fn(),
  getAssociations: vi.fn(),
  localRunGenerate: vi.fn(),
  localRunCancel: vi.fn(),
  resolveRuntime: vi.fn(),
}));

import {
  clearAllConversations,
  createConversation,
  deleteConversation,
  getAssociations,
  listConversations,
  listLibrary,
  localRunCancel,
  localRunGenerate,
  resolveRuntime,
  saveConversation,
  showConversation,
} from "../lib/api";
import { listen } from "@tauri-apps/api/event";

function libraryEntry(): LibraryEntry {
  return {
    library_id: "lib-1",
    schema_version: "v1",
    sha256: "a".repeat(64),
    file_size_bytes: 500_000_000,
    gguf_version: 3,
    tensor_count: 10,
    kv_count: 5,
    architecture: "qwen2",
    quantization: "Q4_K_M",
    parameter_count: 500_000_000,
    current_path: "C:\\Models\\qwen.gguf",
    original_import_path: "C:\\Models\\qwen.gguf",
    imported_at_rfc3339: "2026-01-01T00:00:00Z",
    last_verified_at_rfc3339: "2026-01-01T00:00:00Z",
    file_modified_at_rfc3339: null,
    file_status: "unchanged",
    trust: "local_unverified_source",
    last_verification: null,
    catalog_match: { catalog_id: null, confidence: "none", notes: [] },
    alias: "Qwen2.5 0.5B",
    notes: null,
    quarantine: null,
    managed_copy: false,
  };
}

function runtimeProfile(): RuntimeProfile {
  return {
    schema_version: "v1",
    profile_id: "profile-1",
    tuning_date: "2026-01-01T00:00:00Z",
    model_sha256: "a".repeat(64),
    model_architecture: "qwen2",
    model_quantization: "Q4_K_M",
    model_parameter_count: 500_000_000,
    machine_profile_schema_version: "v1",
    machine_id: "machine-1",
    backend: "cpu",
    llama_cli_sha256: "b".repeat(64),
    llama_bench_sha256: "b".repeat(64),
    threads: 8,
    gpu_layers: 0,
    context_size: 4096,
    batch_size: 512,
    mean_generation_tokens_per_second: 40,
    mean_prompt_tokens_per_second: 300,
    predicted_ram_bytes: 1_000_000_000,
    predicted_vram_bytes: null,
    stability: "stable",
    stability_formula_version: "v1",
    ranking_formula_version: "v1",
    tuning_formula_version: "v1",
    confidence: "high",
  } as RuntimeProfile;
}

function conversationSummary(overrides: Partial<ConversationSummary> = {}): ConversationSummary {
  return {
    conversation_id: "conv-1",
    title: "Existing chat",
    created_at_rfc3339: "2026-01-01T00:00:00Z",
    updated_at_rfc3339: "2026-01-01T00:01:00Z",
    message_count: 2,
    library_id: "lib-1",
    ...overrides,
  };
}

function conversation(overrides: Partial<Conversation> = {}): Conversation {
  return {
    conversation_id: "conv-1",
    schema_version: "chat-conversation-v1",
    title: "Existing chat",
    created_at_rfc3339: "2026-01-01T00:00:00Z",
    updated_at_rfc3339: "2026-01-01T00:01:00Z",
    library_id: "lib-1",
    profile_id: "profile-1",
    language: "en",
    messages: [
      { message_id: "m1", role: "user", content: "hello", created_at_rfc3339: "2026-01-01T00:00:00Z" },
      { message_id: "m2", role: "assistant", content: "hi there", created_at_rfc3339: "2026-01-01T00:00:05Z" },
    ],
    ...overrides,
  };
}

function renderChat() {
  return render(
    <I18nProvider>
      <AppStatusProvider>
        <Chat />
      </AppStatusProvider>
    </I18nProvider>,
  );
}

describe("Chat page", () => {
  beforeEach(() => {
    // A prior test may have set a one-off mockImplementation (a hanging
    // promise for "duplicate sends", a chunk-emitting implementation for
    // streaming, etc.) - reset every mock to a clean slate before
    // re-establishing the standard fixtures below, so no test can leak
    // its custom behavior into the next one.
    vi.resetAllMocks();
    for (const key of Object.keys(listenCallbacks)) delete listenCallbacks[key];
    vi.mocked(listen).mockImplementation(captureListen as never);
    vi.mocked(resolveRuntime).mockResolvedValue({
      source: "bundled",
      binary_dir: "C:\\Program Files\\BRUTE Runtime\\runtime\\cpu",
      cli_verified: true,
      bench_verified: true,
      detail: "ok",
    });
    vi.mocked(listConversations).mockResolvedValue([conversationSummary()]);
    vi.mocked(showConversation).mockResolvedValue(conversation());
    vi.mocked(createConversation).mockResolvedValue(
      conversation({ conversation_id: "conv-new", title: "New chat", messages: [] }),
    );
    vi.mocked(saveConversation).mockResolvedValue(undefined);
    vi.mocked(deleteConversation).mockResolvedValue(undefined);
    vi.mocked(clearAllConversations).mockResolvedValue(1);
    vi.mocked(listLibrary).mockResolvedValue([libraryEntry()]);
    vi.mocked(getAssociations).mockResolvedValue({ runtime_profiles: [runtimeProfile()], calibration_record_count: 0 });
    vi.mocked(localRunGenerate).mockResolvedValue({
      succeeded: true,
      timed_out: false,
      cancelled: false,
      generation_tokens_per_second: 40,
      prompt_tokens_per_second: 300,
      elapsed_secs: 1.2,
      error: null,
    });
    vi.mocked(localRunCancel).mockResolvedValue(undefined);
    vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.spyOn(window, "prompt").mockReturnValue("Renamed chat");
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows the empty state with suggestions when there is no active conversation", async () => {
    vi.mocked(listConversations).mockResolvedValue([]);
    renderChat();
    expect(await screen.findByText("How can I help?")).toBeInTheDocument();
    expect(screen.getByText("Summarize this text for me")).toBeInTheDocument();
  });

  it("lists saved conversations in the sidebar", async () => {
    renderChat();
    expect(await screen.findByText("Existing chat")).toBeInTheDocument();
  });

  it("search filters the conversation list by title", async () => {
    vi.mocked(listConversations).mockResolvedValue([
      conversationSummary({ conversation_id: "c1", title: "Arabic recipes" }),
      conversationSummary({ conversation_id: "c2", title: "Coding help" }),
    ]);
    renderChat();
    await screen.findByText("Arabic recipes");
    fireEvent.change(screen.getByPlaceholderText("Search conversations"), { target: { value: "coding" } });
    expect(screen.queryByText("Arabic recipes")).not.toBeInTheDocument();
    expect(screen.getByText("Coding help")).toBeInTheDocument();
  });

  it("New Chat creates a fresh conversation and clears the message view", async () => {
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("New Chat"));
    await waitFor(() => expect(createConversation).toHaveBeenCalled());
    expect(await screen.findByText("How can I help?")).toBeInTheDocument();
  });

  it("the composer is disabled and shows a clear no-model state until a model is selected", async () => {
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderChat();
    const textarea = await screen.findByPlaceholderText("Select a model to start chatting");
    expect(textarea).toBeDisabled();
  });

  it("selecting a model and profile enables sending, and a message triggers generation with history in the prompt", async () => {
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));
    await waitFor(() => expect(showConversation).toHaveBeenCalledWith("conv-1"));

    const textarea = await screen.findByPlaceholderText("Message BRUTE…");
    fireEvent.change(textarea, { target: { value: "what is next" } });
    fireEvent.click(screen.getByText("Send"));

    await waitFor(() => expect(localRunGenerate).toHaveBeenCalled());
    const args = vi.mocked(localRunGenerate).mock.calls[0][0];
    expect(args.prompt).toContain("User: hello");
    expect(args.prompt).toContain("Assistant: hi there");
    expect(args.prompt).toContain("User: what is next");
    expect(args.library_id).toBe("lib-1");
    expect(args.profile_id).toBe("profile-1");
  });

  it("streams assistant output as local-run-chunk events arrive", async () => {
    vi.mocked(localRunGenerate).mockImplementation(async () => {
      emit("local-run-chunk", "Hello");
      emit("local-run-chunk", " world");
      return {
        succeeded: true,
        timed_out: false,
        cancelled: false,
        generation_tokens_per_second: 40,
        prompt_tokens_per_second: 300,
        elapsed_secs: 1,
        error: null,
      };
    });
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));
    const textarea = await screen.findByPlaceholderText("Message BRUTE…");
    fireEvent.change(textarea, { target: { value: "go" } });
    fireEvent.click(screen.getByText("Send"));

    await waitFor(() => expect(saveConversation).toHaveBeenCalled());
    const calls = vi.mocked(saveConversation).mock.calls;
    const saved = calls[calls.length - 1][0];
    expect(saved.messages[saved.messages.length - 1]?.content).toBe("Hello world");
  });

  it("strips llama-cli banner, chat-template echo, and stats footer from the saved reply", async () => {
    // Mirrors the real, observed llama-cli output shape for a model with an
    // embedded chat template: startup banner, its own "> User: ...\n\nAssistant:"
    // echo, then the actual reply, then a stats footer and "Exiting...".
    vi.mocked(localRunGenerate).mockImplementation(async () => {
      emit(
        "local-run-chunk",
        "build: 1234 (abcdef)\nmodel: qwen2.5-0.5b-instruct-q4_k_m.gguf\n" +
          "available commands: /exit /regen /clear /read /glob\n\n> User: go\n\nAssistant:",
      );
      emit("local-run-chunk", " Hello world");
      emit("local-run-chunk", "\n[ Prompt: 354.2 t/s | Generation: 57.7 t/s ]\nExiting...");
      return {
        succeeded: true,
        timed_out: false,
        cancelled: false,
        generation_tokens_per_second: 57.7,
        prompt_tokens_per_second: 354.2,
        elapsed_secs: 1,
        error: null,
      };
    });
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));
    const textarea = await screen.findByPlaceholderText("Message BRUTE…");
    fireEvent.change(textarea, { target: { value: "go" } });
    fireEvent.click(screen.getByText("Send"));

    await waitFor(() => expect(saveConversation).toHaveBeenCalled());
    const calls = vi.mocked(saveConversation).mock.calls;
    const saved = calls[calls.length - 1][0];
    const lastMessage = saved.messages[saved.messages.length - 1];
    expect(lastMessage?.content).toBe("Hello world");
    expect(lastMessage?.content).not.toContain("build:");
    expect(lastMessage?.content).not.toContain("available commands:");
    expect(lastMessage?.content).not.toContain("[ Prompt:");
    expect(lastMessage?.content).not.toContain("Exiting...");
  });

  it("Stop calls local_run_cancel while generating", async () => {
    let resolveGeneration: (() => void) | undefined;
    vi.mocked(localRunGenerate).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveGeneration = () =>
            resolve({
              succeeded: false,
              timed_out: false,
              cancelled: true,
              generation_tokens_per_second: null,
              prompt_tokens_per_second: null,
              elapsed_secs: 0.5,
              error: null,
            });
        }),
    );
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));
    const textarea = await screen.findByPlaceholderText("Message BRUTE…");
    fireEvent.change(textarea, { target: { value: "go" } });
    fireEvent.click(screen.getByText("Send"));

    const stopBtn = await screen.findByText("Stop");
    fireEvent.click(stopBtn);
    expect(localRunCancel).toHaveBeenCalled();
    resolveGeneration?.();
  });

  it("prevents duplicate sends while a generation is already running", async () => {
    vi.mocked(localRunGenerate).mockImplementation(() => new Promise(() => {}));
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));
    const textarea = await screen.findByPlaceholderText("Message BRUTE…");
    fireEvent.change(textarea, { target: { value: "go" } });
    fireEvent.click(screen.getByText("Send"));
    await screen.findByText("Stop");
    // The composer swaps Send for Stop while running - there is no second
    // Send button to double-click.
    expect(screen.queryByText("Send")).not.toBeInTheDocument();
    expect(localRunGenerate).toHaveBeenCalledTimes(1);
  });

  it("Rename saves the conversation under its new title", async () => {
    renderChat();
    await screen.findByText("Existing chat");
    const item = screen.getByText("Existing chat").closest(".chat-history-item") as HTMLElement;
    fireEvent.click(within(item).getByLabelText("Rename"));
    await waitFor(() => expect(saveConversation).toHaveBeenCalled());
    const calls = vi.mocked(saveConversation).mock.calls;
    const saved = calls[calls.length - 1][0];
    expect(saved.title).toBe("Renamed chat");
  });

  it("Delete removes the conversation after confirmation", async () => {
    renderChat();
    await screen.findByText("Existing chat");
    const item = screen.getByText("Existing chat").closest(".chat-history-item") as HTMLElement;
    fireEvent.click(within(item).getByLabelText("Delete"));
    expect(window.confirm).toHaveBeenCalled();
    await waitFor(() => expect(deleteConversation).toHaveBeenCalledWith("conv-1"));
  });

  it("Clear history deletes everything after confirmation", async () => {
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Clear history"));
    expect(window.confirm).toHaveBeenCalled();
    await waitFor(() => expect(clearAllConversations).toHaveBeenCalled());
  });

  it("a temporary chat is never saved after a completed exchange", async () => {
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "lib-1" } });
    await waitFor(() => expect(getAssociations).toHaveBeenCalled());
    fireEvent.click(await screen.findByLabelText("Temporary chat"));
    await waitFor(() => expect(createConversation).toHaveBeenCalled());

    const textarea = await screen.findByPlaceholderText("Message BRUTE…");
    fireEvent.change(textarea, { target: { value: "temp message" } });
    fireEvent.click(screen.getByText("Send"));

    await waitFor(() => expect(localRunGenerate).toHaveBeenCalled());
    expect(saveConversation).not.toHaveBeenCalled();
  });

  it("renders existing messages with per-message direction: Arabic RTL, English LTR", async () => {
    vi.mocked(showConversation).mockResolvedValue(
      conversation({
        messages: [
          { message_id: "m1", role: "user", content: "مرحبا بك", created_at_rfc3339: "2026-01-01T00:00:00Z" },
          { message_id: "m2", role: "assistant", content: "Hello there", created_at_rfc3339: "2026-01-01T00:00:05Z" },
        ],
      }),
    );
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));

    const arabicMsg = await screen.findByText("مرحبا بك");
    expect(arabicMsg.closest(".chat-message")).toHaveAttribute("dir", "rtl");
    const englishMsg = screen.getByText("Hello there");
    expect(englishMsg.closest(".chat-message")).toHaveAttribute("dir", "ltr");
  });

  it("renders Markdown and fenced code blocks inside assistant messages", async () => {
    vi.mocked(showConversation).mockResolvedValue(
      conversation({
        messages: [
          { message_id: "m1", role: "user", content: "show code", created_at_rfc3339: "2026-01-01T00:00:00Z" },
          {
            message_id: "m2",
            role: "assistant",
            content: "```python\nprint(1)\n```",
            created_at_rfc3339: "2026-01-01T00:00:05Z",
          },
        ],
      }),
    );
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));
    await waitFor(() => {
      expect(screen.getByText("show code")).toBeInTheDocument();
      expect(screen.getByText("python")).toBeInTheDocument();
    });
  });

  it("Regenerate re-sends the last user message and drops the previous answer", async () => {
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));
    await screen.findByText("hi there");

    fireEvent.click(screen.getByText("Regenerate"));
    await waitFor(() => expect(localRunGenerate).toHaveBeenCalled());
    const args = vi.mocked(localRunGenerate).mock.calls[0][0];
    // Only "hello" should remain as history before the resend - "hi
    // there" (the previous answer) must have been dropped first.
    expect(args.prompt.trim().startsWith("User: hello")).toBe(true);
  });

  it("Edit and resend replaces a user message and truncates everything after it", async () => {
    renderChat();
    await screen.findByText("Existing chat");
    fireEvent.click(screen.getByText("Existing chat"));
    await screen.findByText("hello");

    const userMsg = screen.getByText("hello").closest(".chat-message") as HTMLElement;
    fireEvent.click(within(userMsg).getByText("Edit"));
    const editBox = within(userMsg).getByRole("textbox");
    fireEvent.change(editBox, { target: { value: "edited question" } });
    fireEvent.click(within(userMsg).getByText("Resend"));

    await waitFor(() => expect(localRunGenerate).toHaveBeenCalled());
    const args = vi.mocked(localRunGenerate).mock.calls[0][0];
    expect(args.prompt).toContain("edited question");
    expect(args.prompt).not.toContain("hi there");
  });
});
