import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import { Markdown } from "../components/Markdown";
import { formatBytes, formatParamCount, formatTokensPerSecond } from "../lib/format";
import {
  clearAllConversations,
  createConversation,
  deleteConversation,
  getAssociations,
  listConversations,
  listLibrary,
  localRunCancel,
  localRunGenerate,
  saveConversation,
  showConversation,
} from "../lib/api";
import type {
  Conversation,
  ConversationMessage,
  ConversationSummary,
  LibraryEntry,
  RunPhase,
  RuntimeProfile,
} from "../lib/types";

/** A simple heuristic, not a full bidi algorithm: if the first strong
 * (Arabic-script) character appears before the first Latin letter, the
 * message reads as RTL. Good enough for per-message direction on short
 * chat turns - a message that's genuinely mixed-script still renders
 * correctly either way since inline runs still shape correctly inside
 * a container with the "wrong" base direction, this only affects overall
 * alignment/reading order. */
function detectDir(text: string): "rtl" | "ltr" {
  const arabicMatch = /[؀-ۿ]/.exec(text);
  const latinMatch = /[A-Za-z]/.exec(text);
  if (arabicMatch && (!latinMatch || arabicMatch.index < latinMatch.index)) return "rtl";
  return "ltr";
}

/** Assembles prior turns into a plain-text transcript prepended to the
 * new prompt. This is a deliberately simple, honestly-documented
 * approach - not native chat-template tokens (e.g. Qwen's
 * `<|im_start|>`/`<|im_end|>`) - since `local_run_generate` takes a
 * single prompt string with no session/context concept on the backend
 * (see docs/known-limitations.md). Quality on longer conversations may
 * be lower than a chat-template-aware backend would give; this is a
 * known, documented limitation, not silently glossed over. */
function buildPrompt(history: ConversationMessage[], newUserText: string): string {
  const turns = history.map((m) => `${m.role === "user" ? "User" : "Assistant"}: ${m.content}`);
  turns.push(`User: ${newUserText}`, "Assistant:");
  return turns.join("\n\n");
}

/** Real, observed behavior (found during installed-build verification,
 * not a guess): llama-cli, given a model with an embedded chat template,
 * prints its own startup banner ("Loading model...", ASCII art, a
 * `build`/`model`/`ftype` block, an `available commands:` list) and
 * echoes its own chat-formatted framing ("> User: ...\n\nAssistant:")
 * around the actual reply, then a trailing `[ Prompt: ... | Generation:
 * ... ]` stats line and "Exiting...". None of that belongs in a chat
 * bubble. This is a best-effort, presentation-layer extraction tied to
 * this exact observed output shape - not a guarantee it holds for every
 * model/llama.cpp version. If the expected "Assistant:" marker isn't
 * found, the raw text is returned unchanged rather than mangled, so a
 * format this doesn't recognize degrades to "shows extra chrome," never
 * "silently drops real content." See docs/known-limitations.md. */
function extractAssistantReply(raw: string): string {
  const marker = "Assistant:";
  const lastMarkerIndex = raw.lastIndexOf(marker);
  if (lastMarkerIndex === -1) return raw.trim();
  let reply = raw.slice(lastMarkerIndex + marker.length);
  const statsIndex = reply.indexOf("[ Prompt:");
  if (statsIndex !== -1) reply = reply.slice(0, statsIndex);
  return reply.trim();
}

/** Real, already-known facts only (architecture/quantization/size/trust
 * state from the imported file, measured tok/s from actual tuning runs) -
 * no fabricated "Arabic capability," "task fit," or "device compatibility"
 * scoring. That kind of ranking needs the multi-family catalog/ranking
 * engine, which is a later, separate stage - not implemented here. */
function formatModelOption(m: LibraryEntry): string {
  const name = m.alias ?? m.current_path.split(/[\\/]/).pop() ?? m.library_id;
  const parts = [m.quantization ?? "?", formatParamCount(m.parameter_count), formatBytes(m.file_size_bytes)];
  let label = `${name} — ${parts.join(" · ")}`;
  if (m.file_status !== "unchanged") label += ` ⚠ ${m.file_status}`;
  return label;
}

function formatProfileOption(p: RuntimeProfile): string {
  const parts = [p.backend, `${p.threads} threads`, `${p.context_size} ctx`];
  if (p.mean_generation_tokens_per_second !== null) {
    parts.push(formatTokensPerSecond(p.mean_generation_tokens_per_second));
  }
  let label = parts.join(" · ");
  if (p.stability !== "stable") label += ` ⚠ ${p.stability}`;
  return label;
}

export function Chat() {
  const { t, lang } = useI18n();
  const status = useAppStatus();

  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");

  const [conversation, setConversation] = useState<Conversation | null>(null);
  const [isTemporary, setIsTemporary] = useState(false);

  const [models, setModels] = useState<LibraryEntry[]>([]);
  const [modelId, setModelId] = useState("");
  const [profiles, setProfiles] = useState<RuntimeProfile[]>([]);
  const [profileId, setProfileId] = useState("");

  const [prompt, setPrompt] = useState("");
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const [streaming, setStreaming] = useState("");
  const [running, setRunning] = useState(false);
  const [phase, setPhase] = useState<RunPhase | null>(null);
  const [error, setError] = useState<string | null>(null);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // `streaming` (state) drives the live UI re-render; this ref tracks the
  // exact same accumulating text so runGeneration can read the final
  // value synchronously right after the awaited generate call resolves -
  // a state setter's updater callback does not run synchronously, so it
  // cannot be used for this.
  const streamingRef = useRef("");

  function refreshConversations() {
    listConversations()
      .then(setConversations)
      .catch(() => setConversations([]));
  }

  useEffect(() => {
    refreshConversations();
  }, []);

  useEffect(() => {
    listLibrary()
      .then((list) => {
        setModels(list);
        if (status.libraryId && list.some((m) => m.library_id === status.libraryId)) {
          setModelId(status.libraryId);
        }
      })
      .catch(() => setModels([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!modelId) {
      setProfiles([]);
      setProfileId("");
      return;
    }
    getAssociations(modelId)
      .then((a) => {
        setProfiles(a.runtime_profiles);
        setProfileId(a.runtime_profiles[0]?.profile_id ?? "");
      })
      .catch(() => setProfiles([]));
  }, [modelId]);

  useEffect(() => {
    const unlisten = listen<string>("local-run-chunk", (event) => {
      streamingRef.current += event.payload;
      setStreaming((prev) => prev + event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<RunPhase>("local-run-phase", (event) => {
      setPhase(event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [conversation?.messages, streaming]);

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 200)}px`;
    }
  }, [prompt]);

  const filteredConversations = useMemo(() => {
    if (!searchQuery.trim()) return conversations;
    const q = searchQuery.toLowerCase();
    return conversations.filter((c) => c.title.toLowerCase().includes(q));
  }, [conversations, searchQuery]);

  async function startNewChat(temporary: boolean) {
    if (running) return;
    const fresh = await createConversation(t("chat_untitled"), modelId || null, profileId || null, lang);
    setConversation(fresh);
    setIsTemporary(temporary);
    setPrompt("");
    setStreaming("");
    setError(null);
    setPhase(null);
  }

  async function openConversation(id: string) {
    if (running) return;
    try {
      const full = await showConversation(id);
      setConversation(full);
      setIsTemporary(false);
      if (full.library_id) setModelId(full.library_id);
      if (full.profile_id) setProfileId(full.profile_id);
      setPrompt("");
      setStreaming("");
      setError(null);
      setPhase(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleRename(id: string, currentTitle: string) {
    const next = window.prompt(t("chat_rename_prompt"), currentTitle);
    if (!next || !next.trim() || next === currentTitle) return;
    const full = id === conversation?.conversation_id ? conversation : await showConversation(id).catch(() => null);
    if (!full) return;
    const updated = { ...full, title: next.trim() };
    await saveConversation(updated);
    if (id === conversation?.conversation_id) setConversation(updated);
    refreshConversations();
  }

  async function handleDelete(id: string) {
    if (!window.confirm(t("chat_delete_confirm"))) return;
    await deleteConversation(id).catch(() => undefined);
    if (id === conversation?.conversation_id) setConversation(null);
    refreshConversations();
  }

  async function handleClearHistory() {
    if (!window.confirm(t("chat_clear_history_confirm"))) return;
    await clearAllConversations().catch(() => undefined);
    setConversation(null);
    refreshConversations();
  }

  async function runGeneration(conv: Conversation, userText: string) {
    if (!modelId || !profileId || !status.llamaBinPath) return;
    setRunning(true);
    setError(null);
    streamingRef.current = "";
    setStreaming("");
    setPhase("preparing");
    status.setActiveModel(modelId, models.find((m) => m.library_id === modelId)?.alias ?? null);
    const backend = profiles.find((p) => p.profile_id === profileId)?.backend ?? null;
    status.setActiveProfile(profileId, backend);

    const historyForPrompt = conv.messages;
    const fullPrompt = buildPrompt(historyForPrompt, userText);

    let failed = false;
    try {
      const result = await localRunGenerate({
        library_id: modelId,
        profile_id: profileId,
        llama_bin: status.llamaBinPath,
        prompt: fullPrompt,
        allow_unverified_binary: false,
      });
      if (result.error) {
        setError(result.error);
        failed = !result.cancelled;
      }
    } catch (e) {
      setError(String(e));
      failed = true;
    } finally {
      setRunning(false);
      setPhase(null);
    }

    const finalText = extractAssistantReply(streamingRef.current);

    // Persist the exchange (user turn + whatever was generated, even a
    // partial/cancelled response - never silently discarded) unless this
    // is a temporary chat, which is never written to disk.
    const now = new Date().toISOString();
    const userMsg: ConversationMessage = {
      message_id: `local-${now}-u`,
      role: "user",
      content: userText,
      created_at_rfc3339: now,
    };
    const updatedMessages = [...conv.messages, userMsg];
    if (finalText.trim() || !failed) {
      updatedMessages.push({
        message_id: `local-${now}-a`,
        role: "assistant",
        content: finalText,
        created_at_rfc3339: new Date().toISOString(),
      });
    }
    const updatedConv: Conversation = {
      ...conv,
      library_id: modelId,
      profile_id: profileId,
      language: lang,
      messages: updatedMessages,
    };
    setConversation(updatedConv);
    setStreaming("");

    if (!isTemporary) {
      const toSave =
        updatedConv.title === t("chat_untitled") && userText.trim()
          ? { ...updatedConv, title: userText.trim().slice(0, 60) }
          : updatedConv;
      await saveConversation(toSave).catch(() => undefined);
      setConversation(toSave);
      refreshConversations();
    }
  }

  async function handleSend() {
    if (!prompt.trim() || running) return;
    let conv = conversation;
    if (!conv) {
      conv = await createConversation(t("chat_untitled"), modelId || null, profileId || null, lang);
      setConversation(conv);
    }
    const userText = prompt.trim();
    setPrompt("");
    await runGeneration(conv, userText);
  }

  async function handleStop() {
    await localRunCancel().catch(() => undefined);
  }

  async function handleRegenerate() {
    if (!conversation || running || conversation.messages.length === 0) return;
    const messages = conversation.messages;
    const lastUser = [...messages].reverse().find((m) => m.role === "user");
    if (!lastUser) return;
    // Drop everything from the last user message onward, then resend it.
    const lastUserIndex = messages.lastIndexOf(lastUser);
    const trimmed = { ...conversation, messages: messages.slice(0, lastUserIndex) };
    setConversation(trimmed);
    await runGeneration(trimmed, lastUser.content);
  }

  async function handleEditResend(messageId: string, newText: string) {
    if (!conversation || running || !newText.trim()) return;
    const index = conversation.messages.findIndex((m) => m.message_id === messageId);
    if (index === -1) return;
    const trimmed = { ...conversation, messages: conversation.messages.slice(0, index) };
    setConversation(trimmed);
    setEditingMessageId(null);
    await runGeneration(trimmed, newText.trim());
  }

  function copyToClipboard(text: string) {
    void navigator.clipboard.writeText(text);
  }

  const ready = Boolean(modelId && profileId && status.llamaBinPath);
  const selectedModel = useMemo(() => models.find((m) => m.library_id === modelId) ?? null, [models, modelId]);
  const selectedProfile = useMemo(() => profiles.find((p) => p.profile_id === profileId) ?? null, [profiles, profileId]);
  const suggestions = [
    t("chat_suggestion_1"),
    t("chat_suggestion_2"),
    t("chat_suggestion_3"),
    t("chat_suggestion_4"),
    t("chat_suggestion_5"),
  ];

  return (
    <div className="chat-shell">
      {!sidebarCollapsed && (
        <aside className="chat-sidebar">
          <div className="chat-sidebar-top">
            <button className="btn btn-primary chat-new-btn" type="button" onClick={() => startNewChat(false)}>
              {t("chat_new")}
            </button>
            <button
              className="chat-sidebar-toggle"
              type="button"
              onClick={() => startNewChat(true)}
              aria-label={t("chat_temporary")}
              title={t("chat_temporary_hint")}
            >
              ⏱
            </button>
            <button
              className="chat-sidebar-toggle"
              type="button"
              onClick={() => setSidebarCollapsed(true)}
              aria-label={t("chat_collapse_sidebar")}
              title={t("chat_collapse_sidebar")}
            >
              «
            </button>
          </div>

          <input
            className="chat-search"
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t("chat_search_placeholder")}
            aria-label={t("chat_search_placeholder")}
          />

          <div className="chat-history-list">
            {filteredConversations.length === 0 && <div className="chat-history-empty">{t("chat_no_conversations")}</div>}
            {filteredConversations.map((c) => (
              <div
                key={c.conversation_id}
                className={`chat-history-item ${conversation?.conversation_id === c.conversation_id ? "is-active" : ""}`}
              >
                <button className="chat-history-item-title" type="button" onClick={() => openConversation(c.conversation_id)}>
                  {c.title}
                </button>
                <div className="chat-history-item-actions">
                  <button type="button" title={t("chat_rename")} aria-label={t("chat_rename")} onClick={() => handleRename(c.conversation_id, c.title)}>
                    ✎
                  </button>
                  <button type="button" title={t("chat_delete")} aria-label={t("chat_delete")} onClick={() => handleDelete(c.conversation_id)}>
                    ×
                  </button>
                </div>
              </div>
            ))}
          </div>

          <div className="chat-sidebar-footer">
            <p className="text-tertiary chat-history-note">{t("chat_history_note")}</p>
            <button className="btn btn-ghost" type="button" onClick={handleClearHistory} disabled={conversations.length === 0}>
              {t("chat_clear_history")}
            </button>
          </div>
        </aside>
      )}

      <div className="chat-main">
        <header className="chat-topbar">
          {sidebarCollapsed && (
            <button
              className="chat-sidebar-toggle"
              type="button"
              onClick={() => setSidebarCollapsed(false)}
              aria-label={t("chat_expand_sidebar")}
              title={t("chat_expand_sidebar")}
            >
              »
            </button>
          )}

          <div className="chat-model-picker">
            <select
              aria-label={t("chat_model_label")}
              value={modelId}
              onChange={(e) => setModelId(e.target.value)}
              disabled={running}
            >
              <option value="">{t("chat_select_model_placeholder")}</option>
              {models.map((m) => (
                <option key={m.library_id} value={m.library_id}>
                  {formatModelOption(m)}
                </option>
              ))}
            </select>
            {profiles.length > 0 && (
              <select
                aria-label={t("chat_profile_label")}
                value={profileId}
                onChange={(e) => setProfileId(e.target.value)}
                disabled={running}
              >
                {profiles.map((p) => (
                  <option key={p.profile_id} value={p.profile_id}>
                    {formatProfileOption(p)}
                  </option>
                ))}
              </select>
            )}
            {selectedModel && (
              <span className="chat-model-meta" title={selectedModel.trust}>
                {selectedModel.architecture ?? "?"}
                {selectedProfile?.confidence ? ` · ${selectedProfile.confidence}` : ""}
              </span>
            )}
          </div>

          <div className="chat-topbar-right">
            {isTemporary && <span className="badge badge-warn">{t("chat_temporary")}</span>}
            <span className="badge badge-good" title={t("chat_history_note")}>
              {t("chat_privacy_local")}
            </span>
          </div>
        </header>

        <div className="chat-messages">
          {!conversation || conversation.messages.length === 0 ? (
            <div className="chat-empty-state">
              <h2>{t("chat_empty_title")}</h2>
              {!ready && <p className="chat-no-model-note">{t("chat_no_model")}</p>}
              <div className="chat-suggestions">
                {suggestions.map((s) => (
                  <button
                    key={s}
                    type="button"
                    className="chat-suggestion-chip"
                    disabled={!ready}
                    onClick={async () => {
                      let conv = conversation;
                      if (!conv) {
                        conv = await createConversation(t("chat_untitled"), modelId || null, profileId || null, lang);
                        setConversation(conv);
                      }
                      await runGeneration(conv, s);
                    }}
                  >
                    {s}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <div className="chat-message-list">
              {conversation.messages.map((m) => (
                <div key={m.message_id} className={`chat-message chat-message-${m.role}`} dir={detectDir(m.content)}>
                  <div className="chat-message-role">{m.role === "user" ? t("chat_you") : "BRUTE"}</div>
                  {editingMessageId === m.message_id ? (
                    <EditBox
                      initial={m.content}
                      onCancel={() => setEditingMessageId(null)}
                      onResend={(text) => handleEditResend(m.message_id, text)}
                      cancelLabel={t("chat_cancel_edit")}
                      resendLabel={t("chat_resend")}
                    />
                  ) : (
                    <>
                      <div className="chat-message-content">
                        <Markdown content={m.content} copyLabel={t("chat_copy")} />
                      </div>
                      <div className="chat-message-actions">
                        <button type="button" onClick={() => copyToClipboard(m.content)}>
                          {t("chat_copy")}
                        </button>
                        {m.role === "user" && !running && (
                          <button type="button" onClick={() => setEditingMessageId(m.message_id)}>
                            {t("chat_edit")}
                          </button>
                        )}
                      </div>
                    </>
                  )}
                </div>
              ))}

              {running && (
                <div className="chat-message chat-message-assistant" dir={detectDir(streaming || "a")}>
                  <div className="chat-message-role">BRUTE</div>
                  <div className="chat-message-content">
                    {streaming ? (
                      <Markdown content={extractAssistantReply(streaming)} copyLabel={t("chat_copy")} />
                    ) : (
                      <span className="chat-generating">{phase && phase !== "generating" ? t(`run_phase_${phase}`) : t("chat_generating")}</span>
                    )}
                  </div>
                </div>
              )}

              {error && !running && (
                <div className="chat-message chat-message-error">
                  <div className="chat-message-role">{t("chat_failed")}</div>
                  <p>{error}</p>
                  <button className="btn" type="button" onClick={handleRegenerate}>
                    {t("chat_retry")}
                  </button>
                </div>
              )}

              {!running && !error && conversation.messages.length > 0 && conversation.messages[conversation.messages.length - 1].role === "assistant" && (
                <div className="chat-actions-row">
                  <button className="btn" type="button" onClick={handleRegenerate}>
                    {t("chat_regenerate")}
                  </button>
                </div>
              )}

              <div ref={messagesEndRef} />
            </div>
          )}
        </div>

        <footer className="chat-composer">
          <div className="chat-composer-box">
            <textarea
              ref={textareaRef}
              className="chat-composer-input"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  handleSend();
                }
              }}
              placeholder={ready ? t("chat_composer_placeholder") : t("chat_no_model")}
              disabled={!ready || running}
              rows={1}
            />
            {running ? (
              <button className="btn btn-danger chat-send-btn" type="button" onClick={handleStop}>
                {t("chat_stop")}
              </button>
            ) : (
              <button
                className="btn btn-primary chat-send-btn"
                type="button"
                onClick={handleSend}
                disabled={!ready || !prompt.trim()}
              >
                {t("chat_send")}
              </button>
            )}
          </div>
        </footer>
      </div>
    </div>
  );
}

function EditBox({
  initial,
  onCancel,
  onResend,
  cancelLabel,
  resendLabel,
}: {
  initial: string;
  onCancel: () => void;
  onResend: (text: string) => void;
  cancelLabel: string;
  resendLabel: string;
}) {
  const [value, setValue] = useState(initial);
  return (
    <div className="chat-edit-box">
      <textarea value={value} onChange={(e) => setValue(e.target.value)} rows={3} />
      <div className="chat-edit-actions">
        <button className="btn" type="button" onClick={onCancel}>
          {cancelLabel}
        </button>
        <button className="btn btn-primary" type="button" onClick={() => onResend(value)} disabled={!value.trim()}>
          {resendLabel}
        </button>
      </div>
    </div>
  );
}
