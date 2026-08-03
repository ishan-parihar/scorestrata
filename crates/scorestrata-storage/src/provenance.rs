//! Provenance manifest — reproducibility metadata for every artifact.
//!
//! Architecture §15.2: "Every artifact must be reproducible from: brief,
//! config, policy, seed, assets, asset hashes, snapshot, compiler version,
//! validator version, audio engine version, render settings, DSP graph
//! version."
//!
//! The [`ProvenanceManifest`] is the canonical record of those inputs. Its
//! [`ProvenanceManifest::provenance_hash`] is a deterministic SHA-256 over the
//! canonical serialization of *all* reproducibility-relevant fields, so two
//! artifacts built from identical inputs produce identical hashes — and any
//! drift in seed, policy, versions, or assets is immediately visible.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cache::content_hash;

/// Provenance manifest — the full set of reproducibility inputs for an artifact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceManifest {
    /// Project this artifact belongs to.
    pub project_id: String,
    /// Hash of the brief the project was generated from.
    pub brief_hash: String,
    /// Hash of the project config.
    pub config_hash: String,
    /// Hash of the active policy profile.
    pub policy_hash: String,
    /// Seed used for deterministic generation (None = unseeded).
    pub seed: Option<u64>,
    /// Hashes of source assets, keyed by asset path/name.
    pub asset_hashes: BTreeMap<String, String>,
    /// Hash of the base snapshot (initial project state).
    pub snapshot_hash: String,
    /// Compiler crate version.
    pub compiler_version: String,
    /// Validator crate version.
    pub validator_version: String,
    /// Audio engine crate version.
    pub audio_engine_version: String,
    /// Render settings that affect output.
    pub render_settings: BTreeMap<String, String>,
    /// DSP graph version.
    pub dsp_graph_version: String,
}

impl ProvenanceManifest {
    /// Create a new empty manifest for a project.
    #[must_use]
    pub fn new(project_id: &str) -> Self {
        Self {
            project_id: project_id.to_string(),
            ..Self::default()
        }
    }

    /// Canonical serialization of reproducibility-relevant inputs.
    ///
    /// Uses a stable field order, length-prefixed fields, and `BTreeMap`
    /// (already sorted) so the hash never changes across processes or
    /// serializer versions — and cannot collide on embedded delimiter bytes.
    fn push_field(out: &mut Vec<u8>, field: &[u8]) {
        out.extend_from_slice(&(field.len() as u64).to_le_bytes());
        out.extend_from_slice(field);
    }

    /// Canonical serialization of reproducibility-relevant inputs.
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut push = |field: &str| Self::push_field(&mut out, field.as_bytes());
        // `project_id` is included deliberately: it identifies *which* artifact
        // these inputs reproduce, distinguishing byte-identical inputs across
        // different projects (§15.2 list covers generation inputs; identity
        // scopes them).
        push(&self.project_id);
        push(&self.brief_hash);
        push(&self.config_hash);
        push(&self.policy_hash);
        push(&format!("{:?}", self.seed));
        push(&self.snapshot_hash);
        push(&self.compiler_version);
        push(&self.validator_version);
        push(&self.audio_engine_version);
        push(&self.dsp_graph_version);
        // Group markers keep the two maps distinct: without them
        // `asset_hashes={"a":"b"}` and `render_settings={"a":"b"}` would
        // serialize identically and collide.
        push("assets");
        for (k, v) in &self.asset_hashes {
            push(k);
            push(v);
        }
        push("render");
        for (k, v) in &self.render_settings {
            push(k);
            push(v);
        }
        out
    }

    /// Deterministic composite provenance hash (§15.2).
    ///
    /// Two artifacts built from identical inputs always produce the same hash;
    /// any change to seed, policy, versions, assets, or settings changes it.
    #[must_use]
    pub fn provenance_hash(&self) -> String {
        content_hash(&self.canonical_bytes())
    }

    /// Whether this manifest can reproduce another (same provenance inputs).
    #[must_use]
    pub fn reproduces(&self, other: &Self) -> bool {
        self.provenance_hash() == other.provenance_hash()
    }

    /// Whether all required provenance inputs are populated.
    ///
    /// A manifest is "complete" when every non-optional reproducibility input
    /// has been recorded. `seed` is optional by design (unseeded projects).
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.project_id.is_empty()
            && !self.brief_hash.is_empty()
            && !self.config_hash.is_empty()
            && !self.policy_hash.is_empty()
            && !self.snapshot_hash.is_empty()
            && !self.compiler_version.is_empty()
            && !self.validator_version.is_empty()
            && !self.audio_engine_version.is_empty()
            && !self.dsp_graph_version.is_empty()
    }

    /// Compose a content-addressed cache key for this manifest (§9.3).
    ///
    /// §9.3 requires cache keys to include: config hash, snapshot hash, asset
    /// hashes, validator version, compiler version, audio engine version,
    /// render settings, policy profile, seed. The provenance hash covers all
    /// of those — so it is the correct cache key for an artifact produced
    /// under these exact inputs.
    #[must_use]
    pub fn cache_key(&self, category: &str) -> String {
        format!("{category}/{}", self.provenance_hash())
    }

    /// Resolve the on-disk cache path for an artifact produced under this
    /// manifest, wired directly into the content-addressed cache (§9.3).
    ///
    /// This composes with [`crate::cache::cache_path`] so callers get the
    /// full `cache/objects/<category>/<hash>.json` path from a manifest.
    #[must_use]
    pub fn cache_path_for(
        &self,
        cache_dir: &std::path::Path,
        category: &str,
    ) -> std::path::PathBuf {
        crate::cache::cache_path(cache_dir, category, &self.provenance_hash())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_manifest() -> ProvenanceManifest {
        let mut m = ProvenanceManifest::new("p1");
        m.brief_hash = "brief_abc".to_string();
        m.config_hash = "config_abc".to_string();
        m.policy_hash = "policy_abc".to_string();
        m.seed = Some(1337);
        m.asset_hashes
            .insert("kick.wav".to_string(), "hash_kick".to_string());
        m.snapshot_hash = "snap_abc".to_string();
        m.compiler_version = "0.1.0".to_string();
        m.validator_version = "0.1.0".to_string();
        m.audio_engine_version = "0.1.0".to_string();
        m.render_settings
            .insert("sample_rate".to_string(), "44100".to_string());
        m.dsp_graph_version = "v3".to_string();
        m
    }

    #[test]
    fn provenance_hash_is_deterministic() {
        let a = complete_manifest();
        let b = complete_manifest();
        assert_eq!(a.provenance_hash(), b.provenance_hash());
        assert_eq!(a.provenance_hash().len(), 64); // SHA-256 hex
    }

    #[test]
    fn seed_change_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.seed = Some(9999);
        assert!(!a.reproduces(&b));
        assert_ne!(a.provenance_hash(), b.provenance_hash());
    }

    #[test]
    fn asset_hash_change_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.asset_hashes
            .insert("kick.wav".to_string(), "hash_kick_V2".to_string());
        assert!(!a.reproduces(&b));
    }

    #[test]
    fn version_drift_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.validator_version = "0.2.0".to_string();
        assert!(!a.reproduces(&b));
    }

    #[test]
    fn render_settings_change_breaks_reproducibility() {
        let a = complete_manifest();
        let mut b = complete_manifest();
        b.render_settings
            .insert("sample_rate".to_string(), "48000".to_string());
        assert!(!a.reproduces(&b));
    }

    #[test]
    fn manifest_completeness_detects_missing_inputs() {
        let m = complete_manifest();
        assert!(m.is_complete());
        let mut partial = ProvenanceManifest::new("p1");
        partial.compiler_version = "0.1.0".to_string();
        assert!(!partial.is_complete());
    }

    #[test]
    fn cache_key_is_scoped_and_deterministic() {
        let a = complete_manifest();
        let b = complete_manifest();
        assert_eq!(a.cache_key("render"), b.cache_key("render"));
        assert!(a.cache_key("render").starts_with("render/"));
        let mut c = complete_manifest();
        c.seed = Some(42);
        assert_ne!(a.cache_key("render"), c.cache_key("render"));
    }

    #[test]
    fn cache_path_composes_with_content_addressed_cache() {
        let m = complete_manifest();
        let path = m.cache_path_for(std::path::Path::new("/tmp/ss"), "render");
        assert!(path.ends_with(format!("render/{}.json", m.provenance_hash())));
    }

    #[test]
    fn asset_and_render_maps_do_not_collide() {
        // The group markers must keep the two maps distinct: a key in
        // `asset_hashes` must not hash like the same key in `render_settings`.
        let mut a = complete_manifest();
        a.asset_hashes = BTreeMap::new();
        a.render_settings.insert("x".to_string(), "y".to_string());
        let mut b = complete_manifest();
        b.render_settings = BTreeMap::new();
        b.asset_hashes.insert("x".to_string(), "y".to_string());
        assert_ne!(a.provenance_hash(), b.provenance_hash());
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let m = complete_manifest();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string(&m).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let back: ProvenanceManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        assert_eq!(m.provenance_hash(), back.provenance_hash());
    }
}
