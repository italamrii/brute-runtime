//! Local-only Chat conversation storage. One JSON file per conversation
//! under `%LOCALAPPDATA%\BruteRuntime\conversations\<id>.json` - never
//! git-tracked, never uploaded, never synced anywhere. See
//! `docs/privacy-model.md`.
//!
//! **Storage shape deliberately borrows from two existing precedents**:
//! one-file-per-ID layout (`tuning::runtime_profile`'s
//! `profiles/<id>.json`), because a chat sidebar should be able to
//! delete/rename one conversation without touching any other, but
//! **atomic write-to-temp-then-rename** (`library::LibraryStore::save_to`),
//! not `runtime_profile`'s plain `fs::write` - a conversation file is
//! written far more often (every turn) and holds harder-to-regenerate
//! content (a real exchange, not a re-derivable tuning result), so a
//! crash mid-write corrupting it is a real, worse-than-average loss.
//!
//! A **temporary chat is never persisted at all** - the frontend simply
//! never calls the save command for one. There is no "temporary" flag in
//! this schema because a temporary conversation, by definition, never
//! reaches this module.

use crate::errors::ConversationError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONVERSATION_SCHEMA_VERSION: &str = "chat-conversation-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub message_id: String,
    pub role: MessageRole,
    pub content: String,
    pub created_at_rfc3339: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub conversation_id: String,
    pub schema_version: String,
    pub title: String,
    pub created_at_rfc3339: String,
    pub updated_at_rfc3339: String,
    /// The library entry and saved profile this conversation was using,
    /// if any - purely informational (e.g. "resume with the same
    /// model"), never re-validated automatically; a model/profile that
    /// no longer exists just means the Chat UI asks the user to pick
    /// again.
    pub library_id: Option<String>,
    pub profile_id: Option<String>,
    pub language: Option<String>,
    pub messages: Vec<ConversationMessage>,
}

/// Lightweight listing shape for the conversation sidebar - avoids
/// forcing every caller to hold every message body of every
/// conversation in memory just to render a title/timestamp list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub conversation_id: String,
    pub title: String,
    pub created_at_rfc3339: String,
    pub updated_at_rfc3339: String,
    pub message_count: usize,
    pub library_id: Option<String>,
}

impl From<&Conversation> for ConversationSummary {
    fn from(c: &Conversation) -> Self {
        Self {
            conversation_id: c.conversation_id.clone(),
            title: c.title.clone(),
            created_at_rfc3339: c.created_at_rfc3339.clone(),
            updated_at_rfc3339: c.updated_at_rfc3339.clone(),
            message_count: c.messages.len(),
            library_id: c.library_id.clone(),
        }
    }
}

/// `%LOCALAPPDATA%\BruteRuntime\conversations\` - see
/// `identity::default_local_state_dir` for the parent.
pub fn default_conversations_dir() -> PathBuf {
    crate::identity::default_local_state_dir().join("conversations")
}

/// A fresh random conversation ID - a non-identifying local label (same
/// OS-entropy generator as instance/profile IDs), never derived from
/// content or hardware.
pub fn generate_conversation_id() -> String {
    format!("conv-{}", crate::identity::generate_random_id())
}

pub fn generate_message_id() -> String {
    format!("msg-{}", crate::identity::generate_random_id())
}

fn conversation_path(dir: &Path, conversation_id: &str) -> PathBuf {
    dir.join(format!("{conversation_id}.json"))
}

/// Writes via write-to-temp-then-rename, so a crash mid-write can never
/// leave a half-written conversation file in place.
pub fn save_conversation_to(
    dir: &Path,
    conversation: &Conversation,
) -> Result<PathBuf, ConversationError> {
    std::fs::create_dir_all(dir).map_err(|source| ConversationError::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = conversation_path(dir, &conversation.conversation_id);
    let json = serde_json::to_string_pretty(conversation).expect("Conversation always serializes");
    let tmp_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".tmp");
        PathBuf::from(p)
    };
    std::fs::write(&tmp_path, &json).map_err(|source| ConversationError::Write {
        path: tmp_path.clone(),
        source,
    })?;
    std::fs::rename(&tmp_path, &path).map_err(|source| ConversationError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

pub fn load_conversation_from(
    dir: &Path,
    conversation_id: &str,
) -> Result<Conversation, ConversationError> {
    let path = conversation_path(dir, conversation_id);
    let contents = std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            ConversationError::NotFound(conversation_id.to_string())
        } else {
            ConversationError::Read {
                path: path.clone(),
                source,
            }
        }
    })?;
    serde_json::from_str(&contents).map_err(|source| ConversationError::Parse { path, source })
}

/// Deletes only the local conversation record. There is nothing else to
/// delete alongside it - a conversation has no associated external file.
pub fn delete_conversation_from(
    dir: &Path,
    conversation_id: &str,
) -> Result<(), ConversationError> {
    let path = conversation_path(dir, conversation_id);
    std::fs::remove_file(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            ConversationError::NotFound(conversation_id.to_string())
        } else {
            ConversationError::Write { path, source }
        }
    })
}

/// Lists conversation summaries, most recently updated first. A single
/// corrupt/unparseable conversation file is skipped (not a hard error
/// for the whole list) - real corruption recovery, not just a promise:
/// one bad file must never make every other conversation inaccessible.
/// An empty (or not-yet-created) directory is not an error - it just
/// means no conversations have been saved yet.
pub fn list_conversations_in(dir: &Path) -> Result<Vec<ConversationSummary>, ConversationError> {
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let entries = std::fs::read_dir(dir).map_err(|source| ConversationError::Read {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut summaries = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| ConversationError::Read {
                path: dir.to_path_buf(),
                source,
            })?
            .path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(conversation) = serde_json::from_str::<Conversation>(&contents) else {
            continue;
        };
        summaries.push(ConversationSummary::from(&conversation));
    }
    summaries.sort_by(|a, b| b.updated_at_rfc3339.cmp(&a.updated_at_rfc3339));
    Ok(summaries)
}

/// Deletes every saved conversation ("Clear history"). Never touches
/// anything outside `dir` - no external file is ever in scope.
pub fn clear_all_conversations_in(dir: &Path) -> Result<usize, ConversationError> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let entries = std::fs::read_dir(dir).map_err(|source| ConversationError::Read {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut removed = 0usize;
    for entry in entries {
        let path = entry
            .map_err(|source| ConversationError::Read {
                path: dir.to_path_buf(),
                source,
            })?
            .path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            std::fs::remove_file(&path)
                .map_err(|source| ConversationError::Write { path, source })?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-conversations-test-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_conversation(id: &str, title: &str, updated_at: &str) -> Conversation {
        Conversation {
            conversation_id: id.to_string(),
            schema_version: CONVERSATION_SCHEMA_VERSION.to_string(),
            title: title.to_string(),
            created_at_rfc3339: "2026-07-20T00:00:00Z".to_string(),
            updated_at_rfc3339: updated_at.to_string(),
            library_id: Some("lib-1".to_string()),
            profile_id: None,
            language: Some("ar".to_string()),
            messages: vec![ConversationMessage {
                message_id: "msg-1".to_string(),
                role: MessageRole::User,
                content: "مرحبا".to_string(),
                created_at_rfc3339: "2026-07-20T00:00:00Z".to_string(),
            }],
        }
    }

    #[test]
    fn save_and_load_roundtrip_preserves_messages() {
        let dir = tmp_dir("roundtrip");
        let conv = sample_conversation("conv-1", "Test chat", "2026-07-20T00:01:00Z");
        save_conversation_to(&dir, &conv).unwrap();

        let loaded = load_conversation_from(&dir, "conv-1").unwrap();
        assert_eq!(loaded.title, "Test chat");
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "مرحبا");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file_behind_on_success() {
        let dir = tmp_dir("atomic");
        let conv = sample_conversation("conv-1", "Test chat", "2026-07-20T00:01:00Z");
        save_conversation_to(&dir, &conv).unwrap();

        assert!(dir.join("conv-1.json").is_file());
        assert!(!dir.join("conv-1.json.tmp").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn listing_a_missing_directory_returns_an_empty_list_not_an_error() {
        let summaries = list_conversations_in(Path::new(r"C:\nonexistent\conversations")).unwrap();
        assert!(summaries.is_empty());
    }

    #[test]
    fn list_is_sorted_most_recently_updated_first() {
        let dir = tmp_dir("sort-order");
        save_conversation_to(
            &dir,
            &sample_conversation("conv-old", "Old", "2026-07-20T00:00:00Z"),
        )
        .unwrap();
        save_conversation_to(
            &dir,
            &sample_conversation("conv-new", "New", "2026-07-20T05:00:00Z"),
        )
        .unwrap();
        save_conversation_to(
            &dir,
            &sample_conversation("conv-mid", "Mid", "2026-07-20T02:00:00Z"),
        )
        .unwrap();

        let summaries = list_conversations_in(&dir).unwrap();
        let ids: Vec<&str> = summaries
            .iter()
            .map(|s| s.conversation_id.as_str())
            .collect();
        assert_eq!(ids, vec!["conv-new", "conv-mid", "conv-old"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_corrupt_conversation_file_is_skipped_not_a_hard_error_for_the_whole_list() {
        let dir = tmp_dir("corrupt-skip");
        save_conversation_to(
            &dir,
            &sample_conversation("conv-good", "Good", "2026-07-20T00:00:00Z"),
        )
        .unwrap();
        std::fs::write(dir.join("conv-bad.json"), "{ not valid json at all").unwrap();

        let summaries = list_conversations_in(&dir).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].conversation_id, "conv-good");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_removes_only_the_named_conversation() {
        let dir = tmp_dir("delete");
        save_conversation_to(
            &dir,
            &sample_conversation("conv-1", "Keep", "2026-07-20T00:00:00Z"),
        )
        .unwrap();
        save_conversation_to(
            &dir,
            &sample_conversation("conv-2", "Remove", "2026-07-20T00:00:00Z"),
        )
        .unwrap();

        delete_conversation_from(&dir, "conv-2").unwrap();

        assert!(load_conversation_from(&dir, "conv-1").is_ok());
        assert!(load_conversation_from(&dir, "conv-2").is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_an_unknown_conversation_is_a_clear_error_not_a_panic() {
        let dir = tmp_dir("delete-unknown");
        let result = delete_conversation_from(&dir, "conv-never-existed");
        assert!(result.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_all_removes_every_conversation_and_reports_the_count() {
        let dir = tmp_dir("clear-all");
        save_conversation_to(
            &dir,
            &sample_conversation("conv-1", "A", "2026-07-20T00:00:00Z"),
        )
        .unwrap();
        save_conversation_to(
            &dir,
            &sample_conversation("conv-2", "B", "2026-07-20T00:00:00Z"),
        )
        .unwrap();

        let removed = clear_all_conversations_in(&dir).unwrap();
        assert_eq!(removed, 2);
        assert!(list_conversations_in(&dir).unwrap().is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clearing_an_empty_or_missing_directory_removes_nothing() {
        let removed =
            clear_all_conversations_in(Path::new(r"C:\nonexistent\conversations")).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn summary_reports_the_real_message_count() {
        let mut conv = sample_conversation("conv-1", "Test", "2026-07-20T00:00:00Z");
        conv.messages.push(ConversationMessage {
            message_id: "msg-2".to_string(),
            role: MessageRole::Assistant,
            content: "أهلاً بك".to_string(),
            created_at_rfc3339: "2026-07-20T00:00:05Z".to_string(),
        });
        let summary = ConversationSummary::from(&conv);
        assert_eq!(summary.message_count, 2);
    }
}
