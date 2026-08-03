//! Render manifest — the reproducibility receipt for every render.
//!
//! Architecture §17.2: "Every render should produce a manifest." The manifest
//! ties the rendered artifact (its sha256, path, format) to the exact inputs
//! it was produced from (source snapshot, command, engine versions, seed,
//! asset hashes). §17.1 requires every artifact to be reproducible from those
//! inputs — the manifest is what makes that checkable.
//!
//! [`RenderManifest::manifest_hash`] is a deterministic SHA-256 over the
//! *input-side* fields only (not the artifact id or checksum, which are
//! outputs). Two renders produced from identical inputs produce identical
//! hashes; [`RenderManifest::verify_artifact`] checks that an actual file's
//! bytes match the manifest's declared checksum — the golden-test tie-in.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cache::content_hash;

/// Render manifest — reproducibility receipt for one rendered artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderManifest {
    /// Unique artifact ID.
    pub artifact_id: String,
    /// Artifact type (`master_wav`, `stem`, `preview`).
    pub artifact_type: String,
    /// Output path (e.g. `renders/master.wav`).
    pub path: String,
    /// SHA-256 of the rendered artifact bytes.
    pub sha256: String,
    /// Sample rate of the render.
    pub sample_rate: u32,
    /// Number of channels (1 mono, 2 stereo).
    pub channels: u16,
    /// Render duration in seconds.
    pub duration_seconds: f64,
    /// Snapshot the render was produced from.
    pub source_snapshot_id: String,
    /// Command that triggered the render.
    pub command_id: String,
    /// Compiler crate version.
    pub compiler_version: String,
    /// Audio engine crate version.
    pub audio_engine_version: String,
    /// Seed used for deterministic generation.
    pub seed: Option<u64>,
    /// Hashes of source assets, keyed by asset path/name.
    pub asset_hashes: BTreeMap<String, String>,
}

impl RenderManifest {
    /// Create a new empty manifest for an artifact.
    #[must_use]
    pub fn new(artifact_id: &str) -> Self {
        Self {
            artifact_id: artifact_id.to_string(),
            artifact_type: String::new(),
            path: String::new(),
            sha256: String::new(),
            sample_rate: 0,
            channels: 0,
            duration_seconds: 0.0,
            source_snapshot_id: String::new(),
            command_id: String::new(),
            compiler_version: String::new(),
            audio_engine_version: String::new(),
            seed: None,
            asset_hashes: BTreeMap::new(),
        }
    }

    /// Build a manifest from a completed audio render.
    ///
    /// The checksum and format come from the render result; the
    /// reproducibility inputs (snapshot, command, versions, seed, assets) are
    /// supplied by the caller so the manifest can be verified later (§17.2).
    #[must_use]
    pub fn from_render(
        artifact_id: &str,
        artifact_type: &str,
        path: &str,
        checksum: &str,
        sample_rate: u32,
        channels: u16,
        duration_seconds: f64,
        source_snapshot_id: &str,
        command_id: &str,
        compiler_version: &str,
        audio_engine_version: &str,
        seed: Option<u64>,
        asset_hashes: BTreeMap<String, String>,
    ) -> Self {
        Self {
            artifact_id: artifact_id.to_string(),
            artifact_type: artifact_type.to_string(),
            path: path.to_string(),
            sha256: checksum.to_string(),
            sample_rate,
            channels,
            duration_seconds,
            source_snapshot_id: source_snapshot_id.to_string(),
            command_id: command_id.to_string(),
            compiler_version: compiler_version.to_string(),
            audio_engine_version: audio_engine_version.to_string(),
            seed,
            asset_hashes,
        }
    }

    /// Canonical serialization of the reproducibility *inputs*.
    ///
    /// `artifact_id`, `sha256`, `path`, and `duration_seconds` are outputs —
    /// the bytes produced — and must NOT participate in the hash. The
    /// hash covers the input-side fields only, so two renders from identical
    /// inputs hash identically regardless of output artifacts.
    fn push_field(out: &mut Vec<u8>, field: &[u8]) {
        out.extend_from_slice(&(field.len() as u64).to_le_bytes());
        out.extend_from_slice(field);
    }

    /// Canonical serialization of reproducibility-relevant inputs.
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut push = |field: &str| Self::push_field(&mut out, field.as_bytes());
        push(&self.artifact_type);
        push(&format!("{}", self.sample_rate));
        push(&format!("{}", self.channels));
        push(&self.source_snapshot_id);
        push(&self.command_id);
        push(&self.compiler_version);
        push(&self.audio_engine_version);
        push(&format!("{:?}", self.seed));
        push("assets");
        for (k, v) in &self.asset_hashes {
            push(k);
            push(v);
        }
        out
    }

    /// Deterministic manifest hash (§17.2 reproducibility fingerprint).
    ///
    /// Two manifests produced from identical reproducibility inputs always
    /// produce the same hash; any drift in snapshot, command, versions, seed,
    /// or assets changes it.
    #[must_use]
    pub fn manifest_hash(&self) -> String {
        content_hash(&self.canonical_bytes())
    }

    /// Whether this manifest was produced from the same inputs as another.
    #[must_use]
    pub fn reproduces(&self, other: &Self) -> bool {
        self.manifest_hash() == other.manifest_hash()
    }

    /// Verify actual artifact bytes against the declared checksum.
    ///
    /// Returns `true` when the bytes' SHA-256 matches the manifest's declared
    /// `sha256` — the deterministic check that makes golden tests and
    /// replay/verification trustworthy (§16.5, §17.3).
    #[must_use]
    pub fn verify_artifact(&self, artifact_bytes: &[u8]) -> bool {
        content_hash(artifact_bytes) == self.sha256
    }

    /// Whether all required manifest fields are populated.
    ///
    /// `seed` is optional by design (unseeded renders). Everything else is
    /// required for a manifest to be a usable reproducibility receipt.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.artifact_id.is_empty()
            && !self.artifact_type.is_empty()
            && !self.path.is_empty()
            && !self.sha256.is_empty()
            && self.sample_rate > 0
            && self.channels > 0
            && self.duration_seconds > 0.0
            && !self.source_snapshot_id.is_empty()
            && !self.command_id.is_empty()
            && !self.compiler_version.is_empty()
            && !self.audio_engine_version.is_empty()
    }

    /// Content-addressed cache key for the artifact (§9.3).
    ///
    /// §9.3 requires cache keys to reflect the exact inputs that produced the
    /// artifact. The manifest hash covers exactly those inputs, so it is the
    /// correct key for caching (and reusing) rendered artifacts.
    #[must_use]
    pub fn cache_key(&self) -> String {
        format!("render/{}", self.manifest_hash())
    }
}

/// Store of render manifests, keyed by artifact ID.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderManifestStore {
    /// All known render manifests.
    pub manifests: BTreeMap<String, RenderManifest>,
}

impl RenderManifestStore {
    /// Create a new empty manifest store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a render manifest.
    pub fn record(&mut self, manifest: RenderManifest) {
        self.manifests
            .insert(manifest.artifact_id.clone(), manifest);
    }

    /// Get a manifest by artifact ID.
    #[must_use]
    pub fn get(&self, artifact_id: &str) -> Option<&RenderManifest> {
        self.manifests.get(artifact_id)
    }

    /// All manifests for a project (matched via id token).
    ///
    /// Manifests do not carry a project id (§17.2's schema has none), so a
    /// project's manifests are found by token matching: any `_`/`:`-separated
    /// token in the artifact id, snapshot id, or command id equal to the
    /// project id counts as belonging to that project (`art_p34_master`,
    /// `snap_p34_0042` both match `p34`). This is a documented heuristic —
    /// project ids should be sufficiently distinctive (e.g. `p34` over `p`)
    /// to avoid token collisions with unrelated ids.
    #[must_use]
    pub fn manifests_for_project(&self, project_id: &str) -> Vec<&RenderManifest> {
        self.manifests
            .values()
            .filter(|m| {
                let tokens = |id: &str| id.split(['_', ':']).any(|tok| tok == project_id);
                tokens(&m.artifact_id) || tokens(&m.source_snapshot_id) || tokens(&m.command_id)
            })
            .collect()
    }

    /// Total number of recorded manifests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_manifest() -> RenderManifest {
        let mut assets = BTreeMap::new();
        assets.insert("samples/kick.wav".to_string(), "hash_kick".to_string());
        RenderManifest::from_render(
            "art_p34_master",
            "master_wav",
            "renders/master.wav",
            "abc123",
            48_000,
            2,
            158.4,
            "snap_p34_0042",
            "cmd_0042",
            "0.3.0",
            "0.3.0",
            Some(1337),
            assets,
        )
    }

    #[test]
    fn manifest_hash_is_deterministic_and_inputs_only() {
        let a = complete_manifest();
        let b = complete_manifest();
        assert_eq!(a.manifest_hash(), b.manifest_hash());
        assert_eq!(a.manifest_hash().len(), 64); // SHA-256 hex
        // Outputs (artifact id, checksum, path) must not affect the hash.
        let mut c = a.clone();
        c.artifact_id = "art_OTHER".to_string();
        c.sha256 = "different-checksum".to_string();
        c.path = "renders/other.wav".to_string();
        c.duration_seconds = 99.0;
        assert_eq!(a.manifest_hash(), c.manifest_hash());
    }

    #[test]
    fn seed_change_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.seed = Some(9999);
        assert!(!a.reproduces(&b));
        assert_ne!(a.manifest_hash(), b.manifest_hash());
    }

    #[test]
    fn snapshot_change_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.source_snapshot_id = "snap_p34_9999".to_string();
        assert!(!a.reproduces(&b));
    }

    #[test]
    fn asset_hash_change_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.asset_hashes
            .insert("samples/kick.wav".to_string(), "hash_kick_V2".to_string());
        assert!(!a.reproduces(&b));
    }

    #[test]
    fn engine_version_drift_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.audio_engine_version = "0.4.0".to_string();
        assert!(!a.reproduces(&b));
    }

    #[test]
    fn verify_artifact_accepts_matching_bytes_and_rejects_tampering() {
        let mut manifest = complete_manifest();
        // The declared checksum must match the actual bytes.
        let bytes = b"RIFF....WAV data".to_vec();
        let real_sha = content_hash(&bytes);
        manifest.sha256 = real_sha.clone();
        assert!(manifest.verify_artifact(&bytes));
        // Tampered bytes must fail verification.
        let mut tampered = bytes.clone();
        tampered[0] ^= 0xFF;
        assert!(!manifest.verify_artifact(&tampered));
    }

    #[test]
    fn manifest_completeness_detects_missing_inputs() {
        let m = complete_manifest();
        assert!(m.is_complete());
        let mut partial = RenderManifest::new("art_p34_master");
        partial.sample_rate = 48_000;
        assert!(!partial.is_complete());
    }

    #[test]
    fn cache_key_is_scoped_and_deterministic() {
        let a = complete_manifest();
        let b = complete_manifest();
        assert_eq!(a.cache_key(), b.cache_key());
        assert!(a.cache_key().starts_with("render/"));
        let mut c = complete_manifest();
        c.seed = Some(42);
        assert_ne!(a.cache_key(), c.cache_key());
    }

    #[test]
    fn store_records_and_queries_manifests() {
        let mut store = RenderManifestStore::new();
        store.record(complete_manifest());
        store.record(complete_manifest());
        assert_eq!(store.len(), 1, "same artifact id overwrites");
        assert!(store.get("art_p34_master").is_some());
        assert!(store.get("missing").is_none());
        let for_project = store.manifests_for_project("p34");
        assert_eq!(for_project.len(), 1);
        // Token matching is exact-token based: a prefix like "p3" or an
        // unrelated id must not match manifests belonging to project "p34" —
        // the heuristic's boundary is pinned so short ids can't bleed across
        // projects.
        assert!(store.manifests_for_project("p3").is_empty());
        assert!(store.manifests_for_project("x99").is_empty());
    }

    #[test]
    fn command_id_change_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.command_id = "cmd_9999".to_string();
        assert!(!a.reproduces(&b));
        assert_ne!(a.manifest_hash(), b.manifest_hash());
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let m = complete_manifest();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string(&m).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let back: RenderManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        assert_eq!(m.manifest_hash(), back.manifest_hash());
    }

    #[test]
    fn manifest_serializes_with_all_17_2_schema_fields() {
        // §17.2 schema contract: the manifest carries exactly the 13 declared
        // fields — no drift between the entity and its wire representation.
        let m = complete_manifest();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_value(&m).unwrap();
        let obj = json.as_object().expect("manifest serializes to an object");
        for field in [
            "artifact_id",
            "artifact_type",
            "path",
            "sha256",
            "sample_rate",
            "channels",
            "duration_seconds",
            "source_snapshot_id",
            "command_id",
            "compiler_version",
            "audio_engine_version",
            "seed",
            "asset_hashes",
        ] {
            assert!(obj.contains_key(field), "§17.2 field '{field}' present");
        }
        assert_eq!(
            obj.len(),
            13,
            "exactly the §17.2 schema fields — no accidental additions"
        );
    }
}
