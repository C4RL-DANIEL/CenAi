//! Per-account durable workspace with atomic metadata writes and path-safe artifacts.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub file_name: String,
    pub media_type: String,
    pub byte_length: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub archived: bool,
    pub artifacts: Vec<Artifact>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct WorkspaceIndex {
    schema_version: u16,
    projects: Vec<Project>,
}

pub struct WorkspaceStore {
    user_root: PathBuf,
    index_path: PathBuf,
}

impl WorkspaceStore {
    pub fn new(root: impl AsRef<Path>, user_identity: &str) -> Result<Self, String> {
        if user_identity.trim().is_empty() { return Err("A signed-in user scope is required".into()); }
        let digest = Sha256::digest(user_identity.as_bytes());
        let scope = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let user_root = root.as_ref().join(scope);
        fs::create_dir_all(user_root.join("artifacts")).map_err(|error| error.to_string())?;
        let index_path = user_root.join("workspace.json");
        let store = Self { user_root, index_path };
        if !store.index_path.exists() {
            store.write_index(&WorkspaceIndex { schema_version: 1, projects: Vec::new() })?;
        }
        Ok(store)
    }

    fn read_index(&self) -> Result<WorkspaceIndex, String> {
        let bytes = fs::read(&self.index_path).map_err(|error| error.to_string())?;
        let index: WorkspaceIndex = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if index.schema_version != 1 { return Err("Unsupported workspace schema version".into()); }
        Ok(index)
    }

    fn write_index(&self, index: &WorkspaceIndex) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(index).map_err(|error| error.to_string())?;
        self.write_atomic(&self.index_path, &bytes)
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let parent = path.parent().ok_or("Invalid workspace path")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)
            .or_else(|_| OpenOptions::new().write(true).truncate(true).create(true).open(&temporary))
            .map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    }


    pub fn save_conversation(&self, conversation_id: &str, messages: &[crate::ui::model::Message]) -> Result<(), String> {
        if !safe_segment(conversation_id) { return Err("Conversation identifier is invalid".into()); }
        if messages.len() > 5000 { return Err("Conversation exceeds the local history limit".into()); }
        let bytes = serde_json::to_vec(messages).map_err(|error| error.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 { return Err("Conversation history exceeds the local storage limit".into()); }
        self.write_atomic(&self.user_root.join("conversations").join(format!("{conversation_id}.json")), &bytes)
    }

    pub fn load_conversation(&self, conversation_id: &str) -> Result<Vec<crate::ui::model::Message>, String> {
        if !safe_segment(conversation_id) { return Err("Conversation identifier is invalid".into()); }
        let path = self.user_root.join("conversations").join(format!("{conversation_id}.json"));
        if !path.exists() { return Ok(Vec::new()); }
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())
    }

    pub fn projects(&self, include_archived: bool) -> Result<Vec<Project>, String> {
        let mut projects = self.read_index()?.projects;
        if !include_archived { projects.retain(|project| !project.archived); }
        projects.sort_by(|left, right| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms));
        Ok(projects)
    }

    pub fn save_project(&self, project: Project) -> Result<(), String> {
        if project.id.trim().is_empty() || project.name.trim().is_empty() {
            return Err("Project identifier and name are required".into());
        }
        if project.id.len() > 80 || project.name.len() > 160 || project.description.len() > 20_000 {
            return Err("Project metadata exceeds the allowed size".into());
        }
        let mut index = self.read_index()?;
        match index.projects.iter_mut().find(|item| item.id == project.id) {
            Some(existing) => *existing = project,
            None => index.projects.push(project),
        }
        self.write_index(&index)
    }

    pub fn archive_project(&self, project_id: &str, archived: bool) -> Result<bool, String> {
        let mut index = self.read_index()?;
        let Some(project) = index.projects.iter_mut().find(|item| item.id == project_id) else {
            return Ok(false);
        };
        project.archived = archived;
        self.write_index(&index)?;
        Ok(true)
    }

    pub fn save_artifact(
        &self,
        project_id: &str,
        artifact_id: &str,
        file_name: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<Artifact, String> {
        if !safe_segment(artifact_id) || file_name.trim().is_empty() || file_name.len() > 240 {
            return Err("Artifact identity or filename is invalid".into());
        }
        let mut index = self.read_index()?;
        let project = index.projects.iter_mut().find(|item| item.id == project_id && !item.archived)
            .ok_or("Project not found or archived")?;
        let relative = PathBuf::from("artifacts").join(format!("{artifact_id}.bin"));
        if relative.components().any(|part| !matches!(part, Component::Normal(_))) {
            return Err("Artifact path is invalid".into());
        }
        let path = self.user_root.join(&relative);
        self.write_atomic(&path, bytes)?;
        let digest = Sha256::digest(bytes);
        let artifact = Artifact {
            id: artifact_id.to_owned(),
            file_name: file_name.to_owned(),
            media_type: media_type.chars().take(120).collect(),
            byte_length: bytes.len() as u64,
            sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        };
        project.artifacts.retain(|item| item.id != artifact_id);
        project.artifacts.push(artifact.clone());
        self.write_index(&index)?;
        Ok(artifact)
    }

    pub fn read_artifact(&self, artifact_id: &str) -> Result<Vec<u8>, String> {
        if !safe_segment(artifact_id) { return Err("Artifact identifier is invalid".into()); }
        fs::read(self.user_root.join("artifacts").join(format!("{artifact_id}.bin")))
            .map_err(|error| error.to_string())
    }
}

fn safe_segment(value: &str) -> bool {
    !value.is_empty() && value.len() <= 80
        && value.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(1);
            std::env::temp_dir().join(format!("cenai-workspace-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
        }
    }

    #[test]
    fn workspaces_are_isolated_by_user_identity() {
        let root = temp_root();
        let alice = WorkspaceStore::new(&root, "alice-account").unwrap();
        let bob = WorkspaceStore::new(&root, "bob-account").unwrap();
        let project = Project {
            id: "project-1".into(), name: "Project".into(), description: String::new(),
            created_at_unix_ms: 1, updated_at_unix_ms: 1, archived: false, artifacts: Vec::new(),
        };
        alice.save_project(project).unwrap();
        assert_eq!(alice.projects(false).unwrap().len(), 1);
        assert!(bob.projects(false).unwrap().is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn conversation_history_round_trips_per_user() {
        let root = temp_root();
        let store = WorkspaceStore::new(&root, "history-user").unwrap();
        let messages = vec![
            crate::ui::model::Message { id: 1, role: crate::ui::model::MessageRole::User, body: "hello".into(), streaming: false },
            crate::ui::model::Message { id: 2, role: crate::ui::model::MessageRole::Assistant, body: "world".into(), streaming: false },
        ];
        store.save_conversation("primary", &messages).unwrap();
        assert_eq!(store.load_conversation("primary").unwrap(), messages);
        assert!(store.load_conversation("../outside").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_paths_are_safe_and_content_is_integrity_tagged() {
        let root = temp_root();
        let store = WorkspaceStore::new(&root, "test-user").unwrap();
        store.save_project(Project {
            id: "p1".into(), name: "Artifacts".into(), description: String::new(),
            created_at_unix_ms: 1, updated_at_unix_ms: 1, archived: false, artifacts: Vec::new(),
        }).unwrap();
        let artifact = store.save_artifact("p1", "artifact_1", "report.txt", "text/plain", b"hello").unwrap();
        assert_eq!(artifact.byte_length, 5);
        assert_eq!(store.read_artifact("artifact_1").unwrap(), b"hello");
        assert!(store.save_artifact("p1", "../escape", "x", "text/plain", b"x").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
