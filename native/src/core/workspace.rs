//! Per-account durable workspace with atomic metadata writes and path-safe artifacts.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use crate::core::agents::TaskPlan;
use crate::core::automations::Automation;
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub plan: TaskPlan,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct WorkspaceIndex {
    schema_version: u16,
    projects: Vec<Project>,
    #[serde(default)]
    tasks: Vec<TaskRecord>,
    #[serde(default)]
    automations: Vec<Automation>,
}

pub struct WorkspaceStore {
    user_root: PathBuf,
    user_identity: String,
    index_path: PathBuf,
}

impl WorkspaceStore {
    pub fn new(root: impl AsRef<Path>, user_identity: &str) -> Result<Self, String> {
        if !valid_account_uuid(user_identity) {
            return Err("A canonical authenticated Supabase user UUID is required for workspace access".into());
        }
        let digest = Sha256::digest(user_identity.as_bytes());
        let scope = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let user_root = root.as_ref().join(scope);
        fs::create_dir_all(user_root.join("artifacts")).map_err(|error| error.to_string())?;
        let index_path = user_root.join("workspace.json");
        let store = Self { user_root, user_identity: user_identity.to_owned(), index_path };
        if !store.index_path.exists() {
            store.write_index(&WorkspaceIndex { schema_version: 1, projects: Vec::new(), tasks: Vec::new(), automations: Vec::new() })?;
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
        // Each writer gets a distinct temporary file. Never truncate another writer's
        // in-flight file: workspace writes may be triggered by concurrent UI/background tasks.
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
        let (temporary, mut file) = loop {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let candidate = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&candidate) {
                Ok(file) => break (candidate, file),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        };
        let write_result = (|| -> Result<(), String> {
            file.write_all(bytes).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            fs::rename(&temporary, path).map_err(|error| error.to_string())?;
            if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
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

    pub fn automations(&self) -> Result<Vec<Automation>, String> {
        let mut items = self.read_index()?.automations;
        items.retain(|automation| automation.owner_scope == self.user_identity);
        items.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(items)
    }

    pub fn save_automation(&self, automation: Automation) -> Result<(), String> {
        automation.validate().map_err(|error| format!("Invalid automation: {error}"))?;
        if automation.owner_scope != self.user_identity {
            return Err("Automation owner does not match the authenticated workspace account".into());
        }
        let mut index = self.read_index()?;
        match index.automations.iter_mut().find(|item| item.id == automation.id && item.owner_scope == self.user_identity) {
            Some(existing) => *existing = automation,
            None => {
                if index.automations.len() >= 1000 { return Err("Workspace automation limit reached".into()); }
                index.automations.push(automation);
            }
        }
        self.write_index(&index)
    }

    pub fn delete_automation(&self, automation_id: &str) -> Result<bool, String> {
        if !safe_segment(automation_id) { return Err("Automation identifier is invalid".into()); }
        let mut index = self.read_index()?;
        let previous = index.automations.len();
        index.automations.retain(|item| !(item.id == automation_id && item.owner_scope == self.user_identity));
        let removed = index.automations.len() != previous;
        if removed { self.write_index(&index)?; }
        Ok(removed)
    }

    pub fn task_plans(&self) -> Result<Vec<TaskRecord>, String> {
        let mut tasks = self.read_index()?.tasks;
        tasks.sort_by_key(|task| std::cmp::Reverse(task.updated_at_unix_ms));
        Ok(tasks)
    }

    pub fn save_task_plan(&self, plan: TaskPlan, now_unix_ms: u64) -> Result<TaskRecord, String> {
        plan.validate_persisted().map_err(|error| format!("Invalid task plan: {error}"))?;
        let mut index = self.read_index()?;
        let record = match index.tasks.iter_mut().find(|task| task.id == plan.id) {
            Some(existing) => {
                existing.plan = plan;
                existing.updated_at_unix_ms = now_unix_ms;
                existing.clone()
            }
            None => {
                let record = TaskRecord {
                    id: plan.id.clone(),
                    plan,
                    created_at_unix_ms: now_unix_ms,
                    updated_at_unix_ms: now_unix_ms,
                };
                index.tasks.push(record.clone());
                record
            }
        };
        if index.tasks.len() > 200 {
            index.tasks.sort_by_key(|task| std::cmp::Reverse(task.updated_at_unix_ms));
            index.tasks.truncate(200);
        }
        self.write_index(&index)?;
        Ok(record)
    }

    pub fn projects(&self, include_archived: bool) -> Result<Vec<Project>, String> {
        let mut projects = self.read_index()?.projects;
        if !include_archived { projects.retain(|project| !project.archived); }
        projects.sort_by_key(|project| std::cmp::Reverse(project.updated_at_unix_ms));
        Ok(projects)
    }

    pub fn save_project(&self, project: Project) -> Result<(), String> {
        if !safe_segment(&project.id) || project.name.trim().is_empty() {
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
        if !safe_segment(artifact_id)
            || file_name.trim().is_empty()
            || file_name.len() > 240
            || file_name.contains('/') || file_name.contains('\\')
            || file_name.chars().any(char::is_control)
        {
            return Err("Artifact identity or filename is invalid".into());
        }
        if media_type.trim().is_empty() || media_type.len() > 120
            || !media_type.bytes().all(|byte| byte.is_ascii_alphanumeric()
                || matches!(byte, b'/' | b'.' | b'+' | b'-' | b';' | b'=' | b' '))
        {
            return Err("Artifact media type is invalid".into());
        }
        if bytes.is_empty() || bytes.len() > 25 * 1024 * 1024 {
            return Err("Artifact must contain data and fit the 25 MB local workspace limit".into());
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

    pub fn delete_artifact(&self, artifact_id: &str) -> Result<bool, String> {
        if !safe_segment(artifact_id) { return Err("Artifact identifier is invalid".into()); }
        let mut index = self.read_index()?;
        let mut found = false;
        for project in &mut index.projects {
            let before = project.artifacts.len();
            project.artifacts.retain(|artifact| artifact.id != artifact_id);
            found |= project.artifacts.len() != before;
        }
        if !found { return Ok(false); }
        // Remove the index reference first. A failed cleanup can leave only an
        // unreachable orphan, never a visible artifact that points to missing data.
        self.write_index(&index)?;
        let path = self.user_root.join("artifacts").join(format!("{artifact_id}.bin"));
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(format!("Artifact was removed from the index but its file cleanup failed: {error}")),
        }
    }

    pub fn read_artifact(&self, artifact_id: &str) -> Result<Vec<u8>, String> {
        if !safe_segment(artifact_id) { return Err("Artifact identifier is invalid".into()); }
        let index = self.read_index()?;
        let metadata = index.projects.iter().flat_map(|project| project.artifacts.iter())
            .find(|artifact| artifact.id == artifact_id)
            .ok_or("Artifact is not registered in this account workspace")?;
        let bytes = fs::read(self.user_root.join("artifacts").join(format!("{artifact_id}.bin")))
            .map_err(|error| error.to_string())?;
        let digest = Sha256::digest(&bytes);
        let actual_hash = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        if bytes.len() as u64 != metadata.byte_length || actual_hash != metadata.sha256 {
            return Err("Artifact integrity check failed; the stored file may be incomplete or modified".into());
        }
        Ok(bytes)
    }
}

fn valid_account_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
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
    fn workspace_rejects_non_uuid_identity_scopes() {
        let root = temp_root();
        assert!(WorkspaceStore::new(&root, "person@example.com").is_err());
        assert!(WorkspaceStore::new(&root, "not-a-user-id").is_err());
        assert!(WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440007").is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_atomic_writes_never_expose_partial_file_contents() {
        let root = temp_root();
        let store = std::sync::Arc::new(
            WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440008").unwrap()
        );
        let target = store.user_root.join("concurrent.bin");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let writers = (0..8).map(|index| {
            let store = store.clone();
            let barrier = barrier.clone();
            let target = target.clone();
            std::thread::spawn(move || {
                let payload = vec![b'A' + index; 128 * 1024];
                barrier.wait();
                store.write_atomic(&target, &payload).unwrap();
                payload
            })
        }).collect::<Vec<_>>();
        let expected = writers.into_iter().map(|writer| writer.join().unwrap()).collect::<Vec<_>>();
        let actual = fs::read(&target).unwrap();
        assert!(expected.iter().any(|payload| payload == &actual));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn task_plans_persist_per_account_and_are_bounded() {
        let root = temp_root();
        let store = WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440001").unwrap();
        let plan = TaskPlan {
            id: "task-1".into(),
            goal: "Build a tested feature".into(),
            stopped: false,
            steps: vec![
                crate::core::agents::TaskStep { id: 1, role: crate::core::agents::AgentRole::Developer, title: "Implement".into(), status: crate::core::agents::StepStatus::Pending, depends_on: vec![] },
                crate::core::agents::TaskStep { id: 2, role: crate::core::agents::AgentRole::Tester, title: "Test".into(), status: crate::core::agents::StepStatus::Pending, depends_on: vec![1] },
            ],
        };
        store.save_task_plan(plan.clone(), 100).unwrap();
        let other = WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440002").unwrap();
        assert_eq!(store.task_plans().unwrap()[0].plan, plan);
        assert!(other.task_plans().unwrap().is_empty());
        assert_eq!(store.task_plans().unwrap()[0].created_at_unix_ms, 100);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn workspaces_are_isolated_by_user_identity() {
        let root = temp_root();
        let alice = WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440003").unwrap();
        let bob = WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440004").unwrap();
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
        let store = WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440005").unwrap();
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
    #[test]
    fn deleting_artifact_removes_metadata_and_prevents_future_reads() {
        let root = temp_root();
        let store = WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440021").unwrap();
        store.save_project(Project {
            id: "project-delete".into(), name: "Delete test".into(), description: String::new(),
            created_at_unix_ms: 1, updated_at_unix_ms: 1, archived: false, artifacts: Vec::new(),
        }).unwrap();
        store.save_artifact("project-delete", "artifact-delete", "note.txt", "text/plain", b"private note").unwrap();
        assert_eq!(store.read_artifact("artifact-delete").unwrap(), b"private note");
        assert!(store.delete_artifact("artifact-delete").unwrap());
        assert!(store.read_artifact("artifact-delete").is_err());
        assert!(!store.delete_artifact("artifact-delete").unwrap());
        let _ = fs::remove_dir_all(root);
    }

    fn artifact_paths_are_safe_and_content_is_integrity_tagged() {
        let root = temp_root();
        let store = WorkspaceStore::new(&root, "550e8400-e29b-41d4-a716-446655440006").unwrap();
        store.save_project(Project {
            id: "p1".into(), name: "Artifacts".into(), description: String::new(),
            created_at_unix_ms: 1, updated_at_unix_ms: 1, archived: false, artifacts: Vec::new(),
        }).unwrap();
        let artifact = store.save_artifact("p1", "artifact_1", "report.txt", "text/plain", b"hello").unwrap();
        assert_eq!(artifact.byte_length, 5);
        assert_eq!(store.read_artifact("artifact_1").unwrap(), b"hello");
        fs::write(store.user_root.join("artifacts").join("artifact_1.bin"), b"tampered").unwrap();
        assert!(store.read_artifact("artifact_1").unwrap_err().contains("integrity"));
        assert!(store.save_artifact("p1", "../escape", "x", "text/plain", b"x").is_err());
        assert!(store.save_artifact("p1", "bad_name", "../report.txt", "text/plain", b"x").is_err());
        assert!(store.save_artifact("p1", "bad_mime", "report.txt", "text/plain\\nInjected: yes", b"x").is_err());
        assert!(store.save_artifact("p1", "empty", "empty.txt", "text/plain", b"").is_err());
        assert!(store.save_artifact("p1", "too_large", "large.bin", "application/octet-stream", &vec![0; 25 * 1024 * 1024 + 1]).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
