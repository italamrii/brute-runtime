//! Local Chat conversation history - list/show/create/save/delete/clear.
//! Every conversation is a single JSON file under
//! `%LOCALAPPDATA%\BruteRuntime\conversations\` (see
//! `brute::conversations`) - never uploaded, never synced, and a
//! **temporary chat is never persisted at all**: the frontend simply
//! never calls `conversations_save` for one.
//!
//! Every command runs on a blocking worker thread
//! (`tauri::async_runtime::spawn_blocking`), matching every other
//! filesystem-facing command in this codebase - see
//! `docs/architecture.md`'s "Stage 4 responsiveness" note.

use brute::conversations::{self, Conversation, ConversationSummary};

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

async fn off_thread<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn conversations_list() -> Result<Vec<ConversationSummary>, String> {
    off_thread(conversations_list_impl).await
}

fn conversations_list_impl() -> Result<Vec<ConversationSummary>, String> {
    conversations::list_conversations_in(&conversations::default_conversations_dir())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn conversations_show(conversation_id: String) -> Result<Conversation, String> {
    off_thread(move || conversations_show_impl(conversation_id)).await
}

fn conversations_show_impl(conversation_id: String) -> Result<Conversation, String> {
    conversations::load_conversation_from(
        &conversations::default_conversations_dir(),
        &conversation_id,
    )
    .map_err(|e| e.to_string())
}

/// Creates a fresh, empty, unsaved conversation object (generated ID and
/// timestamps) - the caller still owns when (or whether) to actually
/// persist it via `conversations_save`. A temporary chat calls this and
/// then simply never calls `conversations_save`.
#[tauri::command(rename_all = "snake_case")]
pub async fn conversations_create(
    title: String,
    library_id: Option<String>,
    profile_id: Option<String>,
    language: Option<String>,
) -> Result<Conversation, String> {
    off_thread(move || conversations_create_impl(title, library_id, profile_id, language)).await
}

fn conversations_create_impl(
    title: String,
    library_id: Option<String>,
    profile_id: Option<String>,
    language: Option<String>,
) -> Result<Conversation, String> {
    let now = now_rfc3339();
    Ok(Conversation {
        conversation_id: conversations::generate_conversation_id(),
        schema_version: conversations::CONVERSATION_SCHEMA_VERSION.to_string(),
        title,
        created_at_rfc3339: now.clone(),
        updated_at_rfc3339: now,
        library_id,
        profile_id,
        language,
        messages: Vec::new(),
    })
}

/// Persists a conversation (create-or-overwrite by `conversation_id`).
/// `updated_at_rfc3339` is always stamped here with the real current
/// time, regardless of what the caller sent - the frontend should not
/// need to keep its own clock in sync with this one.
#[tauri::command(rename_all = "snake_case")]
pub async fn conversations_save(mut conversation: Conversation) -> Result<(), String> {
    off_thread(move || {
        conversation.updated_at_rfc3339 = now_rfc3339();
        conversations_save_impl(conversation)
    })
    .await
}

fn conversations_save_impl(conversation: Conversation) -> Result<(), String> {
    conversations::save_conversation_to(&conversations::default_conversations_dir(), &conversation)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Deletes only the local conversation record - see
/// `brute::conversations::delete_conversation_from`'s doc comment.
#[tauri::command(rename_all = "snake_case")]
pub async fn conversations_delete(conversation_id: String) -> Result<(), String> {
    off_thread(move || conversations_delete_impl(conversation_id)).await
}

fn conversations_delete_impl(conversation_id: String) -> Result<(), String> {
    conversations::delete_conversation_from(
        &conversations::default_conversations_dir(),
        &conversation_id,
    )
    .map_err(|e| e.to_string())
}

/// "Clear history" - deletes every saved conversation. Returns how many
/// were removed.
#[tauri::command(rename_all = "snake_case")]
pub async fn conversations_clear_all() -> Result<usize, String> {
    off_thread(conversations_clear_all_impl).await
}

fn conversations_clear_all_impl() -> Result<usize, String> {
    conversations::clear_all_conversations_in(&conversations::default_conversations_dir())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONEXISTENT_CONVERSATION_ID: &str = "conv-definitely-not-real-brute-desktop-test";

    #[test]
    fn conversations_show_rejects_an_unknown_id_without_panicking() {
        let result = conversations_show_impl(NONEXISTENT_CONVERSATION_ID.to_string());
        assert!(result.is_err());
    }

    #[test]
    fn conversations_delete_rejects_an_unknown_id_without_panicking() {
        let result = conversations_delete_impl(NONEXISTENT_CONVERSATION_ID.to_string());
        assert!(result.is_err());
    }

    #[test]
    fn conversations_create_generates_a_fresh_id_and_matching_timestamps() {
        let conv = conversations_create_impl(
            "New chat".to_string(),
            Some("lib-1".to_string()),
            None,
            Some("en".to_string()),
        )
        .unwrap();
        assert!(conv.conversation_id.starts_with("conv-"));
        assert_eq!(conv.created_at_rfc3339, conv.updated_at_rfc3339);
        assert!(conv.messages.is_empty());
        assert_eq!(conv.title, "New chat");
    }

    // Save/load/delete roundtrip behavior (including atomic-write safety
    // and corruption recovery) is already thoroughly covered against a
    // real temp directory in brute::conversations's own test suite - the
    // command layer here is a thin off-thread wrapper around it and is
    // deliberately not re-tested against this machine's *real*
    // %LOCALAPPDATA% conversations directory, matching this codebase's
    // existing convention (see commands::profiles's tests) of never
    // writing throwaway data into the user's actual real local state.
}
