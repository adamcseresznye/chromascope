//! Append-only local project snapshots. Raw sources are referenced and verified, never copied.
//! Each commit is a new immutable revision; prior revisions provide reversible history.
use crate::{domain::*, engine::Response, quant::Method};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: DatasetId,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedMethod {
    pub id: MethodId,
    pub method: Method,
    #[serde(default)]
    pub legacy_sha256: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub result_id: ResultId,
    pub dataset_id: DatasetId,
    pub sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: u32,
    pub id: ProjectId,
    pub revision: u64,
    pub sources: Vec<Source>,
    pub methods: Vec<SavedMethod>,
    pub results: Vec<Artifact>,
    #[serde(default)]
    pub failures: Vec<FailedOperation>,
    #[serde(default)]
    pub restored_from_revision: Option<u64>,
    /// Attributed MCP approvals and applied changes; never scientific certification.
    #[serde(default)]
    pub agent_commits: Vec<AgentCommit>,
    /// Optional GUI snapshot; content-addressed and retained with each revision.
    #[serde(default)]
    pub workspace_sha256: Option<String>,
    #[serde(default)]
    pub review_proposals: Vec<crate::proposals::Proposal>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCommit {
    pub result_id: ResultId,
    pub revision: u64,
    pub actor: String,
    pub reason: String,
    pub identity_verified: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailedOperation {
    pub request: Request,
    pub error: EngineError,
    pub unix_ms: u128,
}
fn io(e: impl ToString) -> EngineError {
    EngineError::new("corrupt_project", e)
}
pub fn source_identity(path: &Path) -> Result<(String, u64)> {
    source_identity_controlled(path, None)
}
pub(crate) fn source_identity_controlled(
    path: &Path,
    control: Option<&crate::jobs::JobControl>,
) -> Result<(String, u64)> {
    let mut f = File::open(path).map_err(|e| EngineError::new("missing_source", e))?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0u8; 65536];
    loop {
        if let Some(control) = control {
            control.check()?;
        }
        let n = f.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        if control.is_some_and(|control| bytes + n as u64 > control.max_source_bytes) {
            return Err(EngineError::new(
                "resource_limit",
                "Source exceeds byte budget during hashing",
            ));
        }
        hash.update(&buffer[..n]);
        bytes += n as u64;
    }
    Ok((format!("{:x}", hash.finalize()), bytes))
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) struct WriterLock {
    path: PathBuf,
}
impl WriterLock {
    pub(crate) fn acquire(root: &Path) -> Result<Self> {
        let path = root.join("writer.lock");
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| {
                EngineError::new(
                    "stale_revision",
                    format!("Project writer lock unavailable: {e}"),
                )
            })?;
        writeln!(f, "{}", std::process::id()).map_err(io)?;
        f.sync_all().map_err(io)?;
        Ok(Self { path })
    }
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| io("No parent directory"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(io)?;
    temp.write_all(bytes).map_err(io)?;
    temp.as_file().sync_all().map_err(io)?;
    temp.persist_noclobber(path).map_err(io)?;
    Ok(())
}
impl Project {
    pub fn create(root: &Path) -> Result<Self> {
        fs::create_dir(root).map_err(io)?;
        fs::create_dir(root.join("results")).map_err(io)?;
        let p = Self {
            schema_version: 1,
            id: ProjectId::default(),
            revision: 0,
            sources: vec![],
            methods: vec![],
            results: vec![],
            failures: vec![],
            restored_from_revision: None,
            agent_commits: vec![],
            workspace_sha256: None,
            review_proposals: vec![],
        };
        write_new(
            &root.join("revision-00000000000000000000.json"),
            &serde_json::to_vec_pretty(&p).map_err(io)?,
        )?;
        Ok(p)
    }
    pub fn open(root: &Path) -> Result<Self> {
        let mut revisions = Vec::new();
        for entry in fs::read_dir(root).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("revision-") && name.ends_with(".json") {
                revisions.push(entry.path());
            }
        }
        revisions.sort();
        let path = revisions
            .last()
            .ok_or_else(|| io("No committed project revision"))?;
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).map_err(io)?).map_err(io)?;
        if value.get("schema_version").and_then(|v| v.as_u64()) != Some(1) {
            return Err(EngineError::new(
                "unsupported_capability",
                "Unsupported project schema; original project preserved",
            ));
        }
        let mut p: Self = serde_json::from_value(value).map_err(io)?;
        crate::delivery::relocate(root, &mut p)?;
        p.validate()?;
        if path.file_name().unwrap_or_default()
            != format!("revision-{:020}.json", p.revision).as_str()
        {
            return Err(io("Revision filename mismatch"));
        }
        for a in &p.results {
            p.load_result(root, a.result_id)?;
        }
        p.load_workspace(root)?;
        Ok(p)
    }
    /// Restore historical content as a new revision, preserving every existing snapshot.
    pub fn restore_revision(root: &Path, target: u64, expected_revision: u64) -> Result<Self> {
        let current = Self::open(root)?;
        if current.revision != expected_revision {
            return Err(EngineError::new(
                "stale_revision",
                "Project changed before restore",
            ));
        }
        if target > current.revision {
            return Err(EngineError::new(
                "invalid_parameters",
                "Cannot restore a future revision",
            ));
        }
        let bytes = fs::read(root.join(format!("revision-{target:020}.json"))).map_err(io)?;
        let mut restored: Self = serde_json::from_slice(&bytes).map_err(io)?;
        crate::delivery::relocate(root, &mut restored)?;
        restored.validate()?;
        if restored.id != current.id || restored.revision != target {
            return Err(io("Historical project identity mismatch"));
        }
        for artifact in &restored.results {
            restored.load_result(root, artifact.result_id)?;
        }
        restored.revision = current.revision;
        restored.agent_commits = current.agent_commits;
        restored.review_proposals = current.review_proposals;
        restored.restored_from_revision = Some(target);
        restored.commit(root, expected_revision)?;
        Ok(restored)
    }
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(EngineError::new(
                "unsupported_capability",
                "Unsupported project schema; original project preserved",
            ));
        }
        if self
            .workspace_sha256
            .as_ref()
            .is_some_and(|hash| !valid_hash(hash))
        {
            return Err(io("Invalid workspace checksum"));
        }
        let mut proposal_ids = std::collections::HashSet::new();
        for proposal in &self.review_proposals {
            if proposal.version != 1
                || !proposal_ids.insert(proposal.id)
                || !valid_hash(&proposal.before_sha256)
            {
                return Err(io("Invalid or duplicate proposal identity"));
            }
        }
        let mut ids = std::collections::HashSet::new();
        for s in &self.sources {
            if !ids.insert(s.id) || !valid_hash(&s.sha256) || !s.path.is_absolute() {
                return Err(io("Invalid or duplicate dataset identity"));
            }
        }
        let mut result_ids = std::collections::HashSet::new();
        for a in &self.results {
            if !result_ids.insert(a.result_id)
                || !ids.contains(&a.dataset_id)
                || !valid_hash(&a.sha256)
            {
                return Err(io("Invalid result references"));
            }
        }
        let mut method_ids = std::collections::HashSet::new();
        for m in &self.methods {
            if !method_ids.insert(m.id) {
                return Err(io("Duplicate method identity"));
            }
            crate::quant::validate(&m.method).map_err(io)?;
        }
        Ok(())
    }
    /// Import the existing version-1 TOML method without modifying its source.
    /// Repeating the same import reuses its stable method ID.
    pub fn import_legacy_method(&mut self, path: &Path) -> Result<MethodId> {
        let bytes = fs::read(path).map_err(io)?;
        let digest = hash(&bytes);
        if let Some(m) = self
            .methods
            .iter()
            .find(|m| m.legacy_sha256.as_ref() == Some(&digest))
        {
            return Ok(m.id);
        }
        let text = std::str::from_utf8(&bytes).map_err(io)?;
        let method: Method = toml::from_str(text).map_err(io)?;
        crate::quant::validate(&method).map_err(|e| EngineError::new("invalid_parameters", e))?;
        let id = MethodId::default();
        self.methods.push(SavedMethod {
            id,
            method,
            legacy_sha256: Some(digest),
        });
        Ok(id)
    }
    pub fn record_failure(&mut self, request: Request, error: EngineError) {
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        self.failures.push(FailedOperation {
            request,
            error,
            unix_ms,
        });
    }
    pub fn register(&mut self, path: &Path) -> Result<DatasetId> {
        let path = fs::canonicalize(path).map_err(|e| EngineError::new("missing_source", e))?;
        let (sha256, bytes) = source_identity(&path)?;
        if let Some(s) = self
            .sources
            .iter()
            .find(|s| s.path == path && s.sha256 == sha256)
        {
            return Ok(s.id);
        }
        if self.sources.iter().any(|s| s.path == path) {
            return Err(io(
                "Registered source content changed; original dataset preserved",
            ));
        }
        let id = DatasetId::default();
        self.sources.push(Source {
            id,
            path,
            sha256,
            bytes,
        });
        Ok(id)
    }
    pub fn verified_source(&self, id: DatasetId) -> Result<PathBuf> {
        let s = self
            .sources
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| io("Unknown dataset"))?;
        let (hash, bytes) = source_identity(&s.path)?;
        if hash != s.sha256 || bytes != s.bytes {
            return Err(io(
                "Source identity changed; relink using verified original content",
            ));
        }
        Ok(s.path.clone())
    }
    pub fn relink(&mut self, id: DatasetId, path: &Path) -> Result<()> {
        let (hash, bytes) = source_identity(path)?;
        let s = self
            .sources
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| io("Unknown dataset"))?;
        if hash != s.sha256 || bytes != s.bytes {
            return Err(io("Relink content differs"));
        }
        s.path = fs::canonicalize(path).map_err(io)?;
        Ok(())
    }
    pub fn add_result(
        &mut self,
        root: &Path,
        dataset_id: DatasetId,
        response: &Response,
    ) -> Result<()> {
        if matches!(
            response.output,
            crate::engine::Output::ChromatographicProcessing { preview: true, .. }
        ) {
            return Err(EngineError::new(
                "invalid_parameters",
                "Preview results cannot be committed",
            ));
        }
        self.verified_source(dataset_id)?;
        if matches!(
            response.output,
            crate::engine::Output::Statistics { .. }
                | crate::engine::Output::StatisticsTable { .. }
        ) {
            crate::statistics::verify_response(response)?;
            let table = match (&response.output, &response.request.operation) {
                (crate::engine::Output::Statistics { report }, _) => &report.table,
                (_, crate::domain::Operation::ExportStatistics { report }) => &report.table,
                _ => return Err(io("Statistics operation mismatch")),
            };
            if let Some(value) = &table.targeted {
                let batch: crate::targeted::BatchResult =
                    serde_json::from_value(value.clone()).map_err(io)?;
                for hash in batch.source_hashes.values() {
                    let source = self
                        .sources
                        .iter()
                        .find(|s| &s.sha256 == hash)
                        .ok_or_else(|| io("Statistics targeted raw source is not registered"))?;
                    self.verified_source(source.id)?;
                }
            }
            if let Some(matrix) = &table.matrix {
                for hash in matrix.source_hashes.values() {
                    let source = self
                        .sources
                        .iter()
                        .find(|s| &s.sha256 == hash)
                        .ok_or_else(|| io("Statistics raw source is not registered"))?;
                    self.verified_source(source.id)?;
                }
            }
        }
        let annotation_ledger = match (&response.output, &response.request.operation) {
            (crate::engine::Output::FeatureAnnotations { ledger }, _) => Some(ledger),
            (
                crate::engine::Output::FeatureAnnotationTable { .. },
                crate::domain::Operation::ExportFeatureAnnotations { ledger },
            ) => Some(ledger),
            (crate::engine::Output::FeatureAnnotationTable { .. }, _) => {
                return Err(io("Annotation table operation mismatch"))
            }
            _ => None,
        };
        if let Some(ledger) = annotation_ledger {
            crate::annotation::verify_response(response)?;
            for hash in ledger.matrix.source_hashes.values() {
                let source = self
                    .sources
                    .iter()
                    .find(|s| &s.sha256 == hash)
                    .ok_or_else(|| io("Annotation raw source is not registered"))?;
                self.verified_source(source.id)?;
            }
        }
        if let crate::engine::Output::FeatureMatrix { report } = &response.output {
            crate::untargeted::verify_response(response)?;
            for hash in report.source_hashes.values() {
                let source = self
                    .sources
                    .iter()
                    .find(|s| &s.sha256 == hash)
                    .ok_or_else(|| io("Untargeted raw source is not registered"))?;
                self.verified_source(source.id)?;
            }
        }
        match &response.output {
            crate::engine::Output::SpectralProcessing { .. }
            | crate::engine::Output::SpectralLibrary { .. }
            | crate::engine::Output::SpectralSearch { .. }
            | crate::engine::Output::SpectralComparison { .. }
            | crate::engine::Output::SpectralCandidateTable { .. }
            | crate::engine::Output::FormulaCandidates { .. }
            | crate::engine::Output::IsotopeAnalysis { .. } => {
                crate::spectral::verify_response(response)?
            }
            _ => (),
        }
        let spectral_query = match &response.output {
            crate::engine::Output::SpectralProcessing { processed } => Some(processed.as_ref()),
            crate::engine::Output::SpectralSearch { report } => Some(&report.query),
            _ => None,
        };
        if let Some(query) = spectral_query {
            for scan in query.inputs.iter().chain(&query.background) {
                if let Some(hash) = scan.metadata.get("source_sha256") {
                    let source = self
                        .sources
                        .iter()
                        .find(|source| &source.sha256 == hash)
                        .ok_or_else(|| io("Spectrum raw source is not registered"))?;
                    self.verified_source(source.id)?;
                }
            }
        }
        let targeted_batch = match &response.output {
            crate::engine::Output::TargetedQuantification { batch } => Some(batch.as_ref()),
            crate::engine::Output::QcReport { report } => {
                crate::qc::verify(report)?;
                report.study.targeted_evidence.as_deref()
            }
            _ => None,
        };
        if let Some(batch) = targeted_batch {
            crate::targeted::verify(batch)?;
            for sample in &batch.request.samples {
                let hash = batch
                    .source_hashes
                    .get(&sample.id)
                    .ok_or_else(|| io("Targeted result lacks raw-source identity"))?;
                let source = self
                    .sources
                    .iter()
                    .find(|s| &s.sha256 == hash)
                    .ok_or_else(|| io("Targeted raw source is not registered"))?;
                self.verified_source(source.id)?;
            }
        }
        let source = self
            .sources
            .iter()
            .find(|s| s.id == dataset_id)
            .ok_or_else(|| io("Unknown dataset"))?;
        if response
            .source_sha256
            .as_ref()
            .is_some_and(|h| h != &source.sha256)
        {
            return Err(io("Result source hash mismatch"));
        }
        if self
            .results
            .iter()
            .any(|a| a.result_id == response.result_id)
        {
            return Err(io("Result ID already exists"));
        }
        let bytes = serde_json::to_vec(response).map_err(io)?;
        let sha256 = hash(&bytes);
        let path = root.join("results").join(format!("{sha256}.json"));
        if path.exists() {
            if fs::read(&path).map_err(io)? != bytes {
                return Err(io("Artifact hash collision or corruption"));
            }
        } else {
            write_new(&path, &bytes)?;
        }
        if let crate::domain::Operation::Quantify { method } = &response.request.operation {
            self.methods.push(SavedMethod {
                id: MethodId::default(),
                method: method.clone(),
                legacy_sha256: None,
            });
        }
        self.results.push(Artifact {
            result_id: response.result_id,
            dataset_id,
            sha256,
        });
        Ok(())
    }
    pub fn load_result(&self, root: &Path, id: ResultId) -> Result<Response> {
        let a = self
            .results
            .iter()
            .find(|a| a.result_id == id)
            .ok_or_else(|| io("Unknown result"))?;
        if !valid_hash(&a.sha256) {
            return Err(io("Invalid artifact digest"));
        }
        let bytes =
            fs::read(root.join("results").join(format!("{}.json", a.sha256))).map_err(io)?;
        if hash(&bytes) != a.sha256 {
            return Err(io("Artifact checksum mismatch"));
        }
        let r: Response = serde_json::from_slice(&bytes).map_err(io)?;
        if r.result_id != id || r.version != 1 {
            return Err(io("Artifact identity/version mismatch"));
        }
        Ok(r)
    }
    pub fn save_workspace(&mut self, root: &Path, value: &serde_json::Value) -> Result<()> {
        let bytes = serde_json::to_vec(value).map_err(io)?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let path = root.join("results").join(format!("workspace-{hash}.json"));
        if !path.exists() {
            write_new(&path, &bytes)?;
        }
        let actual = fs::read(&path).map_err(io)?;
        if actual != bytes {
            return Err(io("Workspace artifact content mismatch"));
        }
        self.workspace_sha256 = Some(hash);
        Ok(())
    }
    pub fn load_workspace(&self, root: &Path) -> Result<Option<serde_json::Value>> {
        let Some(hash) = &self.workspace_sha256 else {
            return Ok(None);
        };
        if !valid_hash(hash) {
            return Err(io("Invalid workspace checksum"));
        }
        let bytes =
            fs::read(root.join("results").join(format!("workspace-{hash}.json"))).map_err(io)?;
        if format!("{:x}", Sha256::digest(&bytes)) != *hash {
            return Err(io("Workspace checksum mismatch"));
        }
        Ok(Some(serde_json::from_slice(&bytes).map_err(io)?))
    }
    pub fn commit(&mut self, root: &Path, expected_revision: u64) -> Result<()> {
        let _lock = WriterLock::acquire(root)?;
        let current = Self::open(root)?;
        if current.id != self.id
            || current.revision != expected_revision
            || self.revision != expected_revision
        {
            return Err(EngineError::new(
                "stale_revision",
                "Project changed since it was opened",
            ));
        }
        self.validate()?;
        let next = expected_revision
            .checked_add(1)
            .ok_or_else(|| io("Revision overflow"))?;
        self.revision = next;
        let result = (|| {
            let bytes = serde_json::to_vec_pretty(self).map_err(io)?;
            write_new(&root.join(format!("revision-{next:020}.json")), &bytes)
        })();
        if result.is_err() {
            self.revision = expected_revision;
        }
        result
    }
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    #[test]
    fn hashing_enforces_actual_bytes_and_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("source");
        fs::write(&path, vec![7u8; 131_073]).unwrap();
        let control = crate::jobs::JobControl::with_limits(10, 131_072).unwrap();
        assert_eq!(
            source_identity_controlled(&path, Some(&control))
                .unwrap_err()
                .code,
            "resource_limit"
        );
        let control = crate::jobs::JobControl::with_limits(10, 131_073).unwrap();
        assert_eq!(
            source_identity_controlled(&path, Some(&control)).unwrap(),
            source_identity(&path).unwrap()
        );
        control.cancel();
        assert_eq!(
            source_identity_controlled(&path, Some(&control))
                .unwrap_err()
                .code,
            "cancelled"
        );
        assert_eq!(fs::metadata(path).unwrap().len(), 131_073);
    }
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    #[test]
    fn workspace_roundtrip_detects_corruption_and_preserves_revision_identity() {
        let root =
            std::env::temp_dir().join(format!("chromascope-workspace-{}", uuid::Uuid::new_v4()));
        let mut project = Project::create(&root).unwrap();
        let id = project.id;
        let snapshot =
            serde_json::json!({"version":1,"draft":{"rt":1.234567890123,"excluded":["sample-b"]}});
        project.save_workspace(&root, &snapshot).unwrap();
        let revision = project.revision;
        project.commit(&root, revision).unwrap();
        let reopened = Project::open(&root).unwrap();
        assert_eq!(reopened.id, id);
        assert_eq!(reopened.revision, revision + 1);
        assert_eq!(reopened.load_workspace(&root).unwrap(), Some(snapshot));
        let artifact = root.join("results").join(format!(
            "workspace-{}.json",
            reopened.workspace_sha256.unwrap()
        ));
        fs::write(artifact, b"{}").unwrap();
        assert!(project
            .load_workspace(&root)
            .unwrap_err()
            .to_string()
            .contains("checksum"));
        fs::remove_dir_all(root).unwrap();
    }
}
