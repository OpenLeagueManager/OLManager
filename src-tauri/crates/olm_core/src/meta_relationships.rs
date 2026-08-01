//! Canonical draft relationships sourced from `assets/simulation/champions.json`.
//! The asset was extracted on 2026-04-17 (schema 1.0). Its semantic integrity is
//! validated at load time so new champions can be added without changing code.

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
struct AssetRoot {
    schema_version: String,
    kind: String,
    data: AssetData,
}

#[derive(Debug, Deserialize)]
struct AssetData {
    roles: HashMap<String, Vec<String>>,
    counterpicks: Vec<Relationship>,
    synergies: Vec<Relationship>,
}

#[derive(Debug, Deserialize)]
struct Relationship {
    a: String,
    b: String,
    value: i8,
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
    let asset: AssetRoot = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if asset.schema_version != "1.0" {
        return Err("unsupported champion relationship schema".to_string());
    }
    if asset.kind != "champions" {
        return Err("unexpected champion relationship asset kind".to_string());
    }

    let mut roles = HashMap::new();
    for (champion, role_names) in asset.data.roles {
        let champion_id = canonical_champion_id(&champion);
        if champion_id.is_empty() {
            return Err("champion role definition has an empty canonical ID".to_string());
        }
        if roles.contains_key(&champion_id) {
            return Err(format!("canonical champion ID collision: {champion_id}"));
        }
        let mut canonical_roles = role_names
            .iter()
            .map(|role| canonical_role(role).ok_or_else(|| format!("unknown role for {champion}")))
            .collect::<Result<Vec<_>, _>>()?;
        canonical_roles.sort_by_key(|role| format!("{role:?}"));
        canonical_roles.dedup();
        if canonical_roles.is_empty() {
            return Err(format!("champion has no canonical roles: {champion}"));
        }
        roles.insert(champion_id, canonical_roles);
    }

    let index_relationships = |relationships: Vec<Relationship>, kind: &str| {
        let mut index = HashMap::new();
        for relationship in relationships {
            let source = canonical_champion_id(&relationship.a);
            let target = canonical_champion_id(&relationship.b);
            if !roles.contains_key(&source) || !roles.contains_key(&target) {
                return Err(format!("{kind} references an unknown champion"));
            }
            if !(0..=5).contains(&relationship.value) {
                return Err(format!("{kind} value is outside the supported range"));
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
        counters: index_relationships(asset.data.counterpicks, "counter")?,
        synergies: index_relationships(asset.data.synergies, "synergy")?,
        roles,
    })
}

fn catalog() -> &'static MetaRelationships {
    CATALOG.get_or_init(|| {
        parse_catalog(include_str!("../../../../assets/simulation/champions.json"))
            .expect("champion relationship asset must be valid")
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
    catalog()
        .synergies
        .get(&(
            canonical_champion_id(first_champion_id),
            canonical_champion_id(second_champion_id),
        ))
        .copied()
        .unwrap_or(0)
}

fn pair_value(
    index: &HashMap<(String, String), i8>,
    first_champion_id: &str,
    second_champion_id: &str,
) -> i8 {
    let first = canonical_champion_id(first_champion_id);
    let second = canonical_champion_id(second_champion_id);
    index
        .get(&(first.clone(), second.clone()))
        .copied()
        .unwrap_or(0)
        .max(index.get(&(second, first)).copied().unwrap_or(0))
}

/// Synergy is composition-level information: pair order must not alter its value.
/// When an asset defines both directions, the stronger directional value wins.
pub fn synergy_pair_value(first_champion_id: &str, second_champion_id: &str) -> i8 {
    pair_value(&catalog().synergies, first_champion_id, second_champion_id)
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
        let catalog = parse_catalog(r#"{"schema_version":"1.0","kind":"champions","data":{"roles":{"A":["Top"],"B":["Jungle"]},"counterpicks":[],"synergies":[{"a":"A","b":"B","value":1},{"a":"B","b":"A","value":3}]}}"#).unwrap();
        assert_eq!(pair_value(&catalog.synergies, "A", "B"), 3);
        assert_eq!(pair_value(&catalog.synergies, "B", "A"), 3);
    }

    #[test]
    fn catalog_validation_allows_new_champions_and_rejects_unknown_references() {
        let extended = r#"{"schema_version":"1.0","kind":"champions","data":{"roles":{"Heimerdinger":["Mid","Support"],"Quinn":["Top"]},"counterpicks":[{"a":"Heimerdinger","b":"Quinn","value":1}],"synergies":[]}}"#;
        let catalog = parse_catalog(extended).unwrap();
        assert!(catalog.roles.contains_key("heimerdinger"));
        assert!(catalog.roles.contains_key("quinn"));

        let unknown_reference = r#"{"schema_version":"1.0","kind":"champions","data":{"roles":{"A":["Top"]},"counterpicks":[{"a":"A","b":"Missing","value":1}],"synergies":[]}}"#;
        assert_eq!(
            parse_catalog(unknown_reference).unwrap_err(),
            "counter references an unknown champion"
        );
    }

    #[test]
    fn catalog_rejects_canonical_collisions_in_roles_and_relationships() {
        let role_collision = r#"{"schema_version":"1.0","kind":"champions","data":{"roles":{"Wukong":["Jungle"],"MonkeyKing":["Jungle"]},"counterpicks":[],"synergies":[]}}"#;
        assert_eq!(
            parse_catalog(role_collision).unwrap_err(),
            "canonical champion ID collision: monkeyking"
        );

        let relationship_collision = r#"{"schema_version":"1.0","kind":"champions","data":{"roles":{"A":["Top"],"Wukong":["Jungle"]},"counterpicks":[{"a":"A","b":"Wukong","value":1},{"a":"A","b":"MonkeyKing","value":2}],"synergies":[]}}"#;
        assert_eq!(
            parse_catalog(relationship_collision).unwrap_err(),
            "canonical counter relationship collision: a->monkeyking"
        );
    }

    #[test]
    fn timing_is_explicitly_unknown_and_neutral() {
        assert_eq!(CanonicalGameTiming::Unknown.impact(), 0);
    }
}
