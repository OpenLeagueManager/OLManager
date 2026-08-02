//! Canonical draft relationships sourced from the versioned data submodule.

use crate::domain::player::LolRole;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalGameTiming {
    Unknown,
}

impl CanonicalGameTiming {
    pub const fn impact(self) -> i8 {
        0
    }
}

#[derive(Debug, Deserialize)]
struct DatasetRoot {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    dataset: String,
    captured_at: String,
    provenance: Provenance,
    semantics: Semantics,
    migration: Migration,
    roles: Vec<RoleDefinition>,
    counters: Vec<Relationship>,
    synergies: Vec<Relationship>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    status: String,
    source_catalog: String,
    source_schema_version: String,
    source_extracted_at: String,
    statement: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Semantics {
    counters: String,
    synergies: String,
    foundational: String,
    patch_sensitive: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Migration {
    legacy_counts: LegacyCounts,
    synergy_duplicate_reduction: SynergyDuplicateReduction,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCounts {
    roles: usize,
    counters: usize,
    synergies: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SynergyDuplicateReduction {
    strategy: String,
    rationale: String,
    duplicate_pairs_reduced: usize,
    duplicate_entries_removed: usize,
    audit: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleDefinition {
    champion_id: String,
    legacy_id: String,
    roles: Vec<String>,
    status: String,
    confidence: String,
    review_flags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Relationship {
    source: String,
    target: String,
    value: i8,
    directionality: String,
    stability: String,
    patch_validity: Option<PatchValidity>,
    status: String,
    confidence: String,
    review_flags: Vec<String>,
    review: Review,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchValidity {
    from_patch: Option<String>,
    through_patch: Option<String>,
    review_on_patch_change: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    required: bool,
    next_review_at: String,
    reason: String,
}

#[derive(Debug)]
pub struct MetaRelationships {
    roles: HashMap<String, Vec<LolRole>>,
    counters: HashMap<(String, String), i8>,
    synergies: HashMap<(String, String), i8>,
}

static CATALOG: OnceLock<MetaRelationships> = OnceLock::new();

pub fn canonical_champion_id(value: &str) -> String {
    let normalized = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "wukong" | "monkeyking" => "monkeyking".to_string(),
        "kaisa" => "kaisa".to_string(),
        "leesin" => "leesin".to_string(),
        _ => normalized,
    }
}

fn canonical_role(value: &str) -> Option<LolRole> {
    match value.to_ascii_lowercase().as_str() {
        "top" => Some(LolRole::Top),
        "jungle" | "jungler" => Some(LolRole::Jungle),
        "mid" | "middle" => Some(LolRole::Mid),
        "bot" | "bottom" | "adc" => Some(LolRole::Adc),
        "support" | "sup" => Some(LolRole::Support),
        _ => None,
    }
}

fn parse_catalog(raw: &str) -> Result<MetaRelationships, String> {
    let dataset: DatasetRoot = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if dataset.schema_version != 1 {
        return Err("unsupported champion relationship schema".to_string());
    }
    if dataset.dataset != "champion-relationships" {
        return Err("unexpected champion relationship dataset".to_string());
    }
    if dataset.captured_at.len() != 10
        || dataset.provenance.status != "legacy_baseline"
        || dataset.provenance.source_catalog != "assets/simulation/champions.json"
        || dataset.provenance.source_schema_version.is_empty()
        || dataset.provenance.source_extracted_at.is_empty()
        || !dataset
            .provenance
            .statement
            .contains("not independently verified official")
        || !dataset.semantics.counters.contains("Directed")
        || !dataset.semantics.synergies.contains("Undirected")
        || dataset.semantics.foundational.is_empty()
        || dataset.semantics.patch_sensitive.is_empty()
    {
        return Err("invalid champion relationship dataset metadata".to_string());
    }
    if dataset.migration.legacy_counts.roles != dataset.roles.len()
        || dataset.migration.legacy_counts.counters != dataset.counters.len()
        || dataset.migration.legacy_counts.synergies != 692
        || dataset.migration.synergy_duplicate_reduction.strategy != "maximum_value"
        || dataset
            .migration
            .synergy_duplicate_reduction
            .rationale
            .is_empty()
        || dataset
            .migration
            .synergy_duplicate_reduction
            .duplicate_pairs_reduced
            != 22
        || dataset
            .migration
            .synergy_duplicate_reduction
            .duplicate_entries_removed
            != 22
        || dataset.migration.synergy_duplicate_reduction.audit.len() != 22
    {
        return Err("invalid champion relationship migration audit".to_string());
    }

    let mut roles = HashMap::new();
    for definition in dataset.roles {
        let champion_id = canonical_champion_id(&definition.champion_id);
        if champion_id.is_empty() || champion_id != definition.champion_id {
            return Err("champion role definition has an empty canonical ID".to_string());
        }
        if definition.legacy_id.is_empty()
            || canonical_champion_id(&definition.legacy_id) != champion_id
            || definition.status != "legacy_baseline"
            || definition.confidence != "low"
            || !definition
                .review_flags
                .iter()
                .any(|flag| flag == "source_unverified")
        {
            return Err("champion role definition has invalid legacy provenance".to_string());
        }
        if roles.contains_key(&champion_id) {
            return Err(format!("canonical champion ID collision: {champion_id}"));
        }
        let mut canonical_roles = definition
            .roles
            .iter()
            .map(|role| {
                canonical_role(role).ok_or_else(|| format!("unknown role for {champion_id}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let declared_role_count = canonical_roles.len();
        canonical_roles.sort_by_key(|role| format!("{role:?}"));
        canonical_roles.dedup();
        if canonical_roles.len() != declared_role_count {
            return Err(format!(
                "champion has duplicate canonical roles: {champion_id}"
            ));
        }
        if canonical_roles.is_empty() {
            return Err(format!("champion has no canonical roles: {champion_id}"));
        }
        roles.insert(champion_id, canonical_roles);
    }

    let index_relationships =
        |relationships: Vec<Relationship>, kind: &str, directionality: &str| {
            let mut index = HashMap::new();
            for relationship in relationships {
                let source = canonical_champion_id(&relationship.source);
                let target = canonical_champion_id(&relationship.target);
                if source != relationship.source
                    || target != relationship.target
                    || !roles.contains_key(&source)
                    || !roles.contains_key(&target)
                {
                    return Err(format!("{kind} references an unknown champion"));
                }
                if source == target {
                    return Err(format!("{kind} cannot be self-referential"));
                }
                if !(0..=5).contains(&relationship.value) {
                    return Err(format!("{kind} value is outside the supported range"));
                }
                if relationship.directionality != directionality {
                    return Err(format!("{kind} has invalid directionality"));
                }
                if !matches!(
                    relationship.stability.as_str(),
                    "foundational" | "patch_sensitive"
                ) || relationship.status != "legacy_baseline"
                    || relationship.confidence != "low"
                    || !relationship
                        .review_flags
                        .iter()
                        .any(|flag| flag == "source_unverified")
                    || !relationship.review.required
                    || relationship.review.next_review_at.len() != 10
                    || relationship.review.reason.is_empty()
                {
                    return Err(format!("{kind} has invalid review metadata"));
                }
                match (&relationship.stability[..], &relationship.patch_validity) {
                    ("patch_sensitive", Some(validity)) if validity.review_on_patch_change => {
                        let _ = (&validity.from_patch, &validity.through_patch);
                    }
                    ("foundational", None) => {}
                    _ => return Err(format!("{kind} has invalid patch validity")),
                }
                if kind == "synergy" && source >= target {
                    return Err("synergy must be stored in canonical order".to_string());
                }
                if index
                    .insert((source.clone(), target.clone()), relationship.value)
                    .is_some()
                {
                    return Err(format!(
                        "canonical {kind} relationship collision: {source}->{target}"
                    ));
                }
            }
            Ok(index)
        };
    Ok(MetaRelationships {
        counters: index_relationships(dataset.counters, "counter", "directed")?,
        synergies: index_relationships(dataset.synergies, "synergy", "undirected")?,
        roles,
    })
}

fn catalog() -> &'static MetaRelationships {
    CATALOG.get_or_init(|| {
        parse_catalog(include_str!(
            "../../../../data/champions/champion-relationships.v1.json"
        ))
        .expect("champion relationship dataset must be valid")
    })
}

pub fn roles_for(champion_id: &str) -> &[LolRole] {
    catalog()
        .roles
        .get(&canonical_champion_id(champion_id))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub fn counter_value(ally_champion_id: &str, enemy_champion_id: &str) -> i8 {
    catalog()
        .counters
        .get(&(
            canonical_champion_id(ally_champion_id),
            canonical_champion_id(enemy_champion_id),
        ))
        .copied()
        .unwrap_or(0)
}

pub fn synergy_value(first_champion_id: &str, second_champion_id: &str) -> i8 {
    synergy_pair_value(first_champion_id, second_champion_id)
}

/// Synergy is composition-level information: pair order must not alter its value.
pub fn synergy_pair_value(first_champion_id: &str, second_champion_id: &str) -> i8 {
    let first = canonical_champion_id(first_champion_id);
    let second = canonical_champion_id(second_champion_id);
    let (source, target) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    catalog()
        .synergies
        .get(&(source, target))
        .copied()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_dataset_aliases_and_roles() {
        assert_eq!(canonical_champion_id("Wukong"), "monkeyking");
        assert_eq!(canonical_champion_id("Kai'Sa"), "kaisa");
        assert_eq!(canonical_champion_id("Lee Sin"), "leesin");
        assert_eq!(roles_for("Wukong"), &[LolRole::Jungle]);
        assert_eq!(roles_for("Kai'Sa"), &[LolRole::Adc]);
        assert_eq!(roles_for("unknown"), &[]);
    }

    #[test]
    fn uses_real_relationships_without_hash_fallbacks() {
        assert_eq!(counter_value("Aatrox", "Cho'Gath"), 1);
        assert_eq!(synergy_value("Aatrox", "Kindred"), 2);
        assert_eq!(counter_value("not-a-champion", "also-unknown"), 0);
        assert_eq!(synergy_value("not-a-champion", "also-unknown"), 0);
    }

    #[test]
    fn synergy_pairs_are_order_invariant_and_use_the_maximum_direction() {
        assert_eq!(synergy_pair_value("Aatrox", "Kindred"), 2);
        assert_eq!(synergy_pair_value("Kindred", "Aatrox"), 2);
        assert_eq!(synergy_value("Kindred", "Aatrox"), 2);
    }

    #[test]
    fn catalog_validation_allows_new_champions_and_rejects_unknown_references() {
        let extended = valid_dataset(
            r#"[{"source":"heimerdinger","target":"quinn","value":1,"directionality":"directed","stability":"patch_sensitive","patch_validity":{"from_patch":null,"through_patch":null,"review_on_patch_change":true},"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"],"review":{"required":true,"next_review_at":"2026-11-02","reason":"test"}}]"#,
        );
        let catalog = parse_catalog(&extended).unwrap();
        assert!(catalog.roles.contains_key("heimerdinger"));
        assert!(catalog.roles.contains_key("quinn"));

        let unknown_reference = valid_dataset(
            r#"[{"source":"heimerdinger","target":"missing","value":1,"directionality":"directed","stability":"patch_sensitive","patch_validity":{"from_patch":null,"through_patch":null,"review_on_patch_change":true},"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"],"review":{"required":true,"next_review_at":"2026-11-02","reason":"test"}}]"#,
        );
        assert_eq!(
            parse_catalog(&unknown_reference).unwrap_err(),
            "counter references an unknown champion"
        );
    }

    #[test]
    fn catalog_rejects_noncanonical_and_invalid_relationships() {
        let role_collision = valid_dataset_with_roles(
            r#"[{"champion_id":"wukong","legacy_id":"Wukong","roles":["Jungle"],"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"]},{"champion_id":"monkeyking","legacy_id":"MonkeyKing","roles":["Jungle"],"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"]}]"#,
            "[]",
        );
        assert_eq!(
            parse_catalog(&role_collision).unwrap_err(),
            "champion role definition has an empty canonical ID"
        );

        let relationship_collision = valid_dataset(
            r#"[{"source":"heimerdinger","target":"quinn","value":1,"directionality":"directed","stability":"patch_sensitive","patch_validity":{"from_patch":null,"through_patch":null,"review_on_patch_change":true},"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"],"review":{"required":true,"next_review_at":"2026-11-02","reason":"test"}},{"source":"heimerdinger","target":"quinn","value":2,"directionality":"directed","stability":"patch_sensitive","patch_validity":{"from_patch":null,"through_patch":null,"review_on_patch_change":true},"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"],"review":{"required":true,"next_review_at":"2026-11-02","reason":"test"}}]"#,
        );
        assert_eq!(
            parse_catalog(&relationship_collision).unwrap_err(),
            "canonical counter relationship collision: heimerdinger->quinn"
        );
    }

    #[test]
    fn catalog_rejects_self_relations_ranges_and_directionality() {
        let valid_counter = r#"[{"source":"heimerdinger","target":"quinn","value":1,"directionality":"directed","stability":"patch_sensitive","patch_validity":{"from_patch":null,"through_patch":null,"review_on_patch_change":true},"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"],"review":{"required":true,"next_review_at":"2026-11-02","reason":"test"}}]"#;
        let self_relation = valid_dataset(valid_counter).replace(
            "\"source\":\"heimerdinger\",\"target\":\"quinn\"",
            "\"source\":\"heimerdinger\",\"target\":\"heimerdinger\"",
        );
        assert_eq!(
            parse_catalog(&self_relation).unwrap_err(),
            "counter cannot be self-referential"
        );

        let out_of_range = valid_dataset(valid_counter).replace("\"value\":1", "\"value\":6");
        assert_eq!(
            parse_catalog(&out_of_range).unwrap_err(),
            "counter value is outside the supported range"
        );

        let wrong_directionality = valid_dataset(valid_counter).replace(
            "\"directionality\":\"directed\"",
            "\"directionality\":\"undirected\"",
        );
        assert_eq!(
            parse_catalog(&wrong_directionality).unwrap_err(),
            "counter has invalid directionality"
        );
    }

    fn valid_dataset(counters: &str) -> String {
        valid_dataset_with_roles(
            r#"[{"champion_id":"heimerdinger","legacy_id":"Heimerdinger","roles":["Mid","Support"],"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"]},{"champion_id":"quinn","legacy_id":"Quinn","roles":["Top"],"status":"legacy_baseline","confidence":"low","review_flags":["source_unverified"]}]"#,
            counters,
        )
    }

    fn valid_dataset_with_roles(roles: &str, counters: &str) -> String {
        format!(
            r#"{{"schemaVersion":1,"dataset":"champion-relationships","captured_at":"2026-08-02","provenance":{{"status":"legacy_baseline","source_catalog":"assets/simulation/champions.json","source_schema_version":"1.0","source_extracted_at":"2026-04-17","statement":"not independently verified official"}},"semantics":{{"counters":"Directed","synergies":"Undirected","foundational":"review","patch_sensitive":"review"}},"migration":{{"legacy_counts":{{"roles":2,"counters":{},"synergies":692}},"synergy_duplicate_reduction":{{"strategy":"maximum_value","rationale":"test","duplicate_pairs_reduced":22,"duplicate_entries_removed":22,"audit":[null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null]}}}},"roles":{},"counters":{},"synergies":[]}}"#,
            serde_json::from_str::<Vec<serde_json::Value>>(counters)
                .unwrap()
                .len(),
            roles,
            counters
        )
    }

    #[test]
    fn timing_is_explicitly_unknown_and_neutral() {
        assert_eq!(CanonicalGameTiming::Unknown.impact(), 0);
    }
}
