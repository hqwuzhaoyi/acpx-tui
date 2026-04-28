use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const HISTORY_VERSION: u8 = 1;
const MAX_SESSION_HISTORY: usize = 100;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct HistoryFile {
    version: u8,
    sessions: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct PromptHistory {
    path: PathBuf,
    data: HistoryFile,
    memory_only: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HistoryNavigation {
    active_session: Option<String>,
    draft: Option<String>,
    index: Option<usize>,
}

impl PromptHistory {
    pub fn load_default() -> (Self, Option<String>) {
        Self::load_from(default_history_path())
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self {
            path: PathBuf::new(),
            data: HistoryFile {
                version: HISTORY_VERSION,
                sessions: BTreeMap::new(),
            },
            memory_only: true,
        }
    }

    pub fn load_from(path: PathBuf) -> (Self, Option<String>) {
        match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<HistoryFile>(&raw) {
                Ok(mut data) => {
                    data.version = HISTORY_VERSION;
                    (
                        Self {
                            path,
                            data,
                            memory_only: false,
                        },
                        None,
                    )
                }
                Err(error) => {
                    let warning = match backup_corrupt_file(&path) {
                        Ok(backup) => format!(
                            "Prompt history was reset; corrupt file moved to {} ({})",
                            backup.display(),
                            error
                        ),
                        Err(backup_error) => format!(
                            "Prompt history was reset; corrupt file could not be moved ({}, {})",
                            error, backup_error
                        ),
                    };
                    (Self::empty(path), Some(warning))
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => (Self::empty(path), None),
            Err(error) => {
                let mut history = Self::empty(path);
                history.memory_only = true;
                (
                    history,
                    Some(format!("Prompt history unavailable: {}", error)),
                )
            }
        }
    }

    fn empty(path: PathBuf) -> Self {
        Self {
            path,
            data: HistoryFile {
                version: HISTORY_VERSION,
                sessions: BTreeMap::new(),
            },
            memory_only: false,
        }
    }

    pub fn entries(&self, session_key: &str) -> &[String] {
        self.data
            .sessions
            .get(session_key)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn record(&mut self, session_key: &str, prompt: &str) -> Result<(), String> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Ok(());
        }

        let entries = self
            .data
            .sessions
            .entry(session_key.to_string())
            .or_default();
        if entries.last().map(|entry| entry == prompt).unwrap_or(false) {
            return Ok(());
        }

        entries.push(prompt.to_string());
        let overflow = entries.len().saturating_sub(MAX_SESSION_HISTORY);
        if overflow > 0 {
            entries.drain(0..overflow);
        }

        self.save()
    }

    fn save(&self) -> Result<(), String> {
        if self.memory_only {
            return Ok(());
        }
        let raw = serde_json::to_string_pretty(&self.data).map_err(|e| e.to_string())?;
        write_private_atomic(&self.path, raw.as_bytes()).map_err(|e| e.to_string())
    }
}

impl HistoryNavigation {
    pub fn reset(&mut self) {
        self.active_session = None;
        self.draft = None;
        self.index = None;
    }

    pub fn previous(
        &mut self,
        session_key: &str,
        current_text: &str,
        entries: &[String],
    ) -> Option<String> {
        if entries.is_empty() {
            return None;
        }
        self.ensure_session(session_key, current_text);

        let next_index = match self.index {
            Some(index) => index.saturating_sub(1),
            None => entries.len() - 1,
        };
        self.index = Some(next_index);
        entries.get(next_index).cloned()
    }

    pub fn next(
        &mut self,
        session_key: &str,
        current_text: &str,
        entries: &[String],
    ) -> Option<String> {
        self.ensure_session(session_key, current_text);
        let index = self.index?;

        if index + 1 < entries.len() {
            let next_index = index + 1;
            self.index = Some(next_index);
            entries.get(next_index).cloned()
        } else {
            let draft = self.draft.take().unwrap_or_default();
            self.reset();
            Some(draft)
        }
    }

    fn ensure_session(&mut self, session_key: &str, current_text: &str) {
        if self.active_session.as_deref() != Some(session_key) {
            self.active_session = Some(session_key.to_string());
            self.draft = Some(current_text.to_string());
            self.index = None;
        }
    }
}

fn default_history_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".acpx")
        .join("tui")
        .join("prompt-history.json")
}

fn backup_corrupt_file(path: &Path) -> io::Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let backup = path.with_file_name(format!(
        "{}.corrupt-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("prompt-history.json"),
        timestamp
    ));
    fs::rename(path, &backup)?;
    Ok(backup)
}

fn write_private_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    set_private_dir_permissions(parent)?;

    let temp_path = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("prompt-history.json"),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    write_private_file(&temp_path, bytes)?;
    fs::rename(&temp_path, path)?;
    set_private_file_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_history_per_session_with_dedupe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let (mut history, warning) = PromptHistory::load_from(path);
        assert!(warning.is_none());

        history.record("s1", "hello").unwrap();
        history.record("s1", "hello").unwrap();
        history.record("s2", "other").unwrap();

        assert_eq!(history.entries("s1"), &["hello".to_string()]);
        assert_eq!(history.entries("s2"), &["other".to_string()]);
    }

    #[test]
    fn truncates_old_entries() {
        let mut history = PromptHistory::in_memory();
        for i in 0..105 {
            history.record("s1", &format!("prompt {}", i)).unwrap();
        }

        let entries = history.entries("s1");
        assert_eq!(entries.len(), 100);
        assert_eq!(entries.first().unwrap(), "prompt 5");
        assert_eq!(entries.last().unwrap(), "prompt 104");
    }

    #[test]
    fn round_trips_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");

        let (mut history, _) = PromptHistory::load_from(path.clone());
        history.record("s1", "hello").unwrap();

        let (loaded, warning) = PromptHistory::load_from(path);
        assert!(warning.is_none());
        assert_eq!(loaded.entries("s1"), &["hello".to_string()]);
    }

    #[test]
    #[cfg(unix)]
    fn writes_history_with_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("history.json");
        let (mut history, _) = PromptHistory::load_from(path.clone());

        history.record("s1", "secret-ish prompt").unwrap();

        let file_mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        let dir_mode = fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
        assert_eq!(dir_mode, 0o700);
    }

    #[test]
    fn backs_up_corrupt_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prompt-history.json");
        fs::write(&path, "{not json").unwrap();

        let (history, warning) = PromptHistory::load_from(path.clone());

        assert!(history.entries("s1").is_empty());
        assert!(warning.unwrap().contains("corrupt file moved"));
        assert!(!path.exists());
        let backups: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn navigation_preserves_draft() {
        let entries = vec!["older".to_string(), "newer".to_string()];
        let mut nav = HistoryNavigation::default();

        assert_eq!(
            nav.previous("s1", "draft", &entries),
            Some("newer".to_string())
        );
        assert_eq!(
            nav.previous("s1", "newer", &entries),
            Some("older".to_string())
        );
        assert_eq!(nav.next("s1", "older", &entries), Some("newer".to_string()));
        assert_eq!(nav.next("s1", "newer", &entries), Some("draft".to_string()));
    }

    #[test]
    fn navigation_is_session_scoped() {
        let entries = vec!["one".to_string()];
        let mut nav = HistoryNavigation::default();

        assert_eq!(
            nav.previous("s1", "draft one", &entries),
            Some("one".to_string())
        );
        assert_eq!(
            nav.previous("s2", "draft two", &entries),
            Some("one".to_string())
        );
        assert_eq!(
            nav.next("s2", "one", &entries),
            Some("draft two".to_string())
        );
    }
}
