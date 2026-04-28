use ignore::WalkBuilder;
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MAX_DISCOVERED_DIRS: usize = 2_000;
const MAX_SCAN_DEPTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherStep {
    Directory,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryCandidate {
    pub path: PathBuf,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerRow {
    pub label: String,
    pub detail: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    pub cwd: PathBuf,
    pub agent: String,
}

#[derive(Debug, Clone)]
pub struct LauncherState {
    pub step: LauncherStep,
    pub directory_query: String,
    pub directories: Vec<DirectoryCandidate>,
    pub directory_matches: Vec<usize>,
    pub directory_selected: usize,
    pub selected_directory: Option<PathBuf>,
    pub agent_query: String,
    pub agents: Vec<String>,
    pub agent_matches: Vec<usize>,
    pub agent_selected: usize,
}

impl LauncherState {
    pub fn new(agents: Vec<String>, roots: Vec<PathBuf>) -> Self {
        let directories = discover_directory_candidates(&roots);
        let directory_labels: Vec<String> = directories.iter().map(|d| d.display.clone()).collect();
        let agent_matches = ranked_indices("", &agents, agents.len());
        let directory_matches = ranked_indices("", &directory_labels, directories.len());

        Self {
            step: LauncherStep::Directory,
            directory_query: String::new(),
            directories,
            directory_matches,
            directory_selected: 0,
            selected_directory: None,
            agent_query: String::new(),
            agents,
            agent_matches,
            agent_selected: 0,
        }
    }

    pub fn push_char(&mut self, c: char) {
        match self.step {
            LauncherStep::Directory => {
                self.directory_query.push(c);
                self.refresh_directory_matches();
            }
            LauncherStep::Agent => {
                self.agent_query.push(c);
                self.refresh_agent_matches();
            }
        }
    }

    pub fn paste(&mut self, text: &str) {
        match self.step {
            LauncherStep::Directory => {
                self.directory_query.push_str(text);
                self.refresh_directory_matches();
            }
            LauncherStep::Agent => {
                self.agent_query.push_str(text);
                self.refresh_agent_matches();
            }
        }
    }

    pub fn backspace(&mut self) {
        match self.step {
            LauncherStep::Directory => {
                self.directory_query.pop();
                self.refresh_directory_matches();
            }
            LauncherStep::Agent => {
                self.agent_query.pop();
                self.refresh_agent_matches();
            }
        }
    }

    pub fn select_next(&mut self) {
        match self.step {
            LauncherStep::Directory => {
                let len = self.directory_option_count();
                if len > 0 {
                    self.directory_selected = (self.directory_selected + 1) % len;
                }
            }
            LauncherStep::Agent => {
                if !self.agent_matches.is_empty() {
                    self.agent_selected = (self.agent_selected + 1) % self.agent_matches.len();
                }
            }
        }
    }

    pub fn select_prev(&mut self) {
        match self.step {
            LauncherStep::Directory => {
                let len = self.directory_option_count();
                if len > 0 {
                    self.directory_selected = if self.directory_selected == 0 {
                        len - 1
                    } else {
                        self.directory_selected - 1
                    };
                }
            }
            LauncherStep::Agent => {
                let len = self.agent_matches.len();
                if len > 0 {
                    self.agent_selected = if self.agent_selected == 0 {
                        len - 1
                    } else {
                        self.agent_selected - 1
                    };
                }
            }
        }
    }

    pub fn confirm(&mut self) -> Result<Option<LaunchRequest>, String> {
        match self.step {
            LauncherStep::Directory => {
                let Some(path) = self.current_directory() else {
                    return Err(
                        "No matching directory; type an existing path or change the filter"
                            .to_string(),
                    );
                };
                self.selected_directory = Some(path);
                self.step = LauncherStep::Agent;
                self.agent_selected = 0;
                Ok(None)
            }
            LauncherStep::Agent => {
                let cwd = self
                    .selected_directory
                    .clone()
                    .ok_or_else(|| "No directory selected".to_string())?;
                let agent = self
                    .current_agent()
                    .ok_or_else(|| "No matching agent".to_string())?;
                Ok(Some(LaunchRequest { cwd, agent }))
            }
        }
    }

    pub fn current_directory(&self) -> Option<PathBuf> {
        let typed_path = typed_directory(&self.directory_query);
        if let Some(path) = typed_path.as_ref() {
            if self.directory_selected == 0 {
                return Some(path.clone());
            }
        }

        let match_index = self
            .directory_selected
            .saturating_sub(usize::from(typed_path.is_some()));
        self.directory_matches
            .get(match_index)
            .and_then(|idx| self.directories.get(*idx))
            .map(|candidate| candidate.path.clone())
    }

    pub fn current_agent(&self) -> Option<String> {
        self.agent_matches
            .get(self.agent_selected)
            .and_then(|idx| self.agents.get(*idx))
            .cloned()
    }

    pub fn visible_rows(&self, limit: usize) -> Vec<PickerRow> {
        match self.step {
            LauncherStep::Directory => self.visible_directory_rows(limit),
            LauncherStep::Agent => self.visible_agent_rows(limit),
        }
    }

    pub fn current_query(&self) -> &str {
        match self.step {
            LauncherStep::Directory => &self.directory_query,
            LauncherStep::Agent => &self.agent_query,
        }
    }

    fn visible_directory_rows(&self, limit: usize) -> Vec<PickerRow> {
        let mut rows = Vec::new();
        if let Some(path) = typed_directory(&self.directory_query) {
            rows.push(PickerRow {
                label: format!("Use typed path: {}", display_path(&path)),
                detail: Some(path.display().to_string()),
                selected: self.directory_selected == 0,
            });
        }

        let typed_offset = rows.len();
        for (visible_index, idx) in self.directory_matches.iter().enumerate() {
            if let Some(candidate) = self.directories.get(*idx) {
                rows.push(PickerRow {
                    label: candidate.display.clone(),
                    detail: Some(candidate.path.display().to_string()),
                    selected: self.directory_selected == visible_index + typed_offset,
                });
            }
        }
        window_rows(rows, self.directory_selected, limit)
    }

    fn visible_agent_rows(&self, limit: usize) -> Vec<PickerRow> {
        let rows: Vec<PickerRow> = self
            .agent_matches
            .iter()
            .enumerate()
            .filter_map(|(visible_index, idx)| {
                self.agents.get(*idx).map(|agent| PickerRow {
                    label: agent.clone(),
                    detail: Some("registered by acpx".to_string()),
                    selected: self.agent_selected == visible_index,
                })
            })
            .collect();
        window_rows(rows, self.agent_selected, limit)
    }

    fn refresh_directory_matches(&mut self) {
        let query = typed_directory(&self.directory_query)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| self.directory_query.clone());
        let labels: Vec<String> = self
            .directories
            .iter()
            .map(|d| format!("{} {}", d.display, d.path.display()))
            .collect();
        self.directory_matches = ranked_indices(&query, &labels, self.directories.len());
        self.directory_selected = 0;
    }

    fn refresh_agent_matches(&mut self) {
        self.agent_matches = ranked_indices(&self.agent_query, &self.agents, self.agents.len());
        self.agent_selected = 0;
    }

    fn directory_option_count(&self) -> usize {
        self.directory_matches.len() + usize::from(typed_directory(&self.directory_query).is_some())
    }
}

pub fn default_directory_roots(current_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    push_unique_root(&mut roots, current_dir.to_path_buf());
    if let Some(parent) = current_dir.parent() {
        push_unique_root(&mut roots, parent.to_path_buf());
    }
    if let Some(home) = dirs::home_dir() {
        let workspace = home.join("workspace");
        if workspace.is_dir() {
            push_unique_root(&mut roots, workspace);
        } else {
            push_unique_root(&mut roots, home);
        }
    }
    roots
}

fn push_unique_root(roots: &mut Vec<PathBuf>, root: PathBuf) {
    let normalized = normalize_path(&root);
    if normalized.is_dir()
        && !roots
            .iter()
            .any(|existing| normalize_path(existing) == normalized)
    {
        roots.push(normalized);
    }
}

pub fn discover_directory_candidates(roots: &[PathBuf]) -> Vec<DirectoryCandidate> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for root in roots {
        let root = normalize_path(root);
        if !root.is_dir() {
            continue;
        }
        add_candidate(&mut candidates, &mut seen, root.clone());
        let walker = WalkBuilder::new(&root)
            .max_depth(Some(MAX_SCAN_DEPTH))
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .parents(true)
            .build();

        for entry in walker.filter_map(Result::ok) {
            if candidates.len() >= MAX_DISCOVERED_DIRS {
                break;
            }
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                add_candidate(&mut candidates, &mut seen, entry.path().to_path_buf());
            }
        }
    }

    candidates.sort_by(|a, b| a.display.cmp(&b.display));
    candidates
}

fn add_candidate(
    candidates: &mut Vec<DirectoryCandidate>,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
) {
    let path = normalize_path(&path);
    if seen.insert(path.clone()) {
        candidates.push(DirectoryCandidate {
            display: display_path(&path),
            path,
        });
    }
}

fn typed_directory(query: &str) -> Option<PathBuf> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let expanded = expand_tilde(trimmed);
    expanded.is_dir().then(|| normalize_path(&expanded))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

pub fn ranked_indices(query: &str, labels: &[String], limit: usize) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..labels.len().min(limit)).collect();
    }

    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut buf = Vec::new();
    let mut scored: Vec<(usize, u32)> = labels
        .iter()
        .enumerate()
        .filter_map(|(idx, label)| {
            pattern
                .score(Utf32Str::new(label, &mut buf), &mut matcher)
                .map(|score| (idx, score))
        })
        .collect();
    scored.sort_by(|(idx_a, score_a), (idx_b, score_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| labels[*idx_a].cmp(&labels[*idx_b]))
    });
    scored.into_iter().take(limit).map(|(idx, _)| idx).collect()
}

fn window_rows(rows: Vec<PickerRow>, selected: usize, limit: usize) -> Vec<PickerRow> {
    if rows.len() <= limit {
        return rows;
    }
    let start = selected
        .min(rows.len().saturating_sub(1))
        .saturating_sub(limit / 2);
    let start = start.min(rows.len().saturating_sub(limit));
    rows.into_iter().skip(start).take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranked_indices_filters_and_orders_matches() {
        let labels = vec![
            "~/workspace/acpx-tui".to_string(),
            "~/Downloads".to_string(),
            "~/workspace/api".to_string(),
        ];
        let ranked = ranked_indices("ac tui", &labels, labels.len());
        assert_eq!(ranked.first().copied(), Some(0));
        assert!(!ranked.contains(&1));
    }

    #[test]
    fn launcher_moves_from_directory_to_agent_then_request() {
        let temp = tempfile::tempdir().unwrap();
        let mut launcher = LauncherState::new(
            vec!["codex".into(), "claude".into()],
            vec![temp.path().into()],
        );
        assert_eq!(launcher.step, LauncherStep::Directory);
        assert!(launcher.confirm().unwrap().is_none());
        assert_eq!(launcher.step, LauncherStep::Agent);
        let request = launcher.confirm().unwrap().unwrap();
        assert_eq!(request.agent, "codex");
        assert_eq!(request.cwd, temp.path().canonicalize().unwrap());
    }

    #[test]
    fn typed_directory_can_be_selected_when_filter_has_no_match() {
        let temp = tempfile::tempdir().unwrap();
        let mut launcher = LauncherState::new(vec!["codex".into()], vec![]);
        launcher.paste(temp.path().to_str().unwrap());
        let rows = launcher.visible_rows(10);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].label.contains("Use typed path"));
        assert_eq!(
            launcher.current_directory().unwrap(),
            temp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn typed_directory_offset_does_not_shift_fuzzy_selection() {
        let temp = tempfile::tempdir().unwrap();
        let typed = temp.path().join("typed");
        let typed_child = typed.join("typed-child");
        let other = temp.path().join("other-typed");
        std::fs::create_dir_all(&typed_child).unwrap();
        std::fs::create_dir_all(&other).unwrap();

        let mut launcher = LauncherState::new(vec!["codex".into()], vec![temp.path().into()]);
        launcher.paste(typed.to_str().unwrap());

        let rows = launcher.visible_rows(10);
        let typed_child = typed_child.canonicalize().unwrap();
        let typed_child_display = typed_child.to_str();
        assert!(rows[0].label.contains("Use typed path"));
        assert!(rows
            .iter()
            .any(|row| row.detail.as_deref() == typed_child_display));

        launcher.directory_selected = rows
            .iter()
            .position(|row| row.detail.as_deref() == typed_child_display)
            .unwrap();

        assert_eq!(launcher.current_directory().unwrap(), typed_child);
    }
}
