use serde::{Deserialize, Serialize};
#[cfg(feature = "typescript")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum LeagueKind {
    #[default]
    Main,
    Academy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct League {
    pub id: String,
    pub name: String,
    pub season: u32,
    pub fixtures: Vec<Fixture>,
    pub standings: Vec<StandingEntry>,
    #[serde(default)]
    pub competition_id: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub league_kind: LeagueKind,
    #[serde(default)]
    pub split_index: usize,
    #[serde(default)]
    pub tier: u8,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum MatchType {
    #[default]
    League,
    Friendly,
    PreseasonTournament,
    Playoffs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct Fixture {
    pub id: String,
    pub matchday: u32,
    pub date: String,
    pub home_team_id: String,
    pub away_team_id: String,
    #[serde(alias = "competition")]
    pub match_type: MatchType,
    #[serde(default = "default_best_of")]
    pub best_of: u8,
    pub status: FixtureStatus,
    pub result: Option<MatchResult>,
}

fn default_best_of() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum FixtureStatus {
    Scheduled,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum MatchEndReason {
    NexusDestroyed,
    TimeLimit,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct MatchResult {
    #[serde(alias = "home_goals")]
    pub home_wins: u8,
    #[serde(alias = "away_goals")]
    pub away_wins: u8,
    pub ended_by: MatchEndReason,
    pub game_duration_seconds: u32,
    pub report: Option<CompactMatchReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct CompactMatchReport {
    #[serde(default)]
    pub total_minutes: u32,
    pub game_duration_seconds: u32,
    pub home_stats: CompactTeamMatchStats,
    pub away_stats: CompactTeamMatchStats,
    pub events: Vec<CompactMatchEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct CompactTeamMatchStats {
    #[serde(default)]
    pub possession_pct: u8,
    pub kills: u16,
    pub deaths: u16,
    pub gold_earned: u32,
    pub damage_dealt: u32,
    pub objectives: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct CompactMatchEvent {
    pub minute: u8,
    pub event_type: String,
    pub side: String,
    pub player_id: Option<String>,
    pub secondary_player_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct StandingEntry {
    pub team_id: String,
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    #[serde(alias = "kills_for", alias = "goals_for")]
    pub maps_won: u32,
    #[serde(alias = "kills_against", alias = "goals_against")]
    pub maps_lost: u32,
    pub points: u32,
}

impl StandingEntry {
    pub fn new(team_id: String) -> Self {
        Self {
            team_id,
            played: 0,
            won: 0,
            lost: 0,
            maps_won: 0,
            maps_lost: 0,
            points: 0,
        }
    }

    pub fn kill_difference(&self) -> i32 {
        self.maps_won as i32 - self.maps_lost as i32
    }

    pub fn record_result(&mut self, kills_for: u8, kills_against: u8) {
        self.played += 1;
        self.maps_won += kills_for as u32;
        self.maps_lost += kills_against as u32;
        if kills_for > kills_against {
            self.won += 1;
            self.points += 3;
        } else if kills_for == kills_against {
            self.points += 1;
        } else {
            self.lost += 1;
        }
    }
}

impl Fixture {
    pub fn counts_for_league_standings(&self) -> bool {
        matches!(self.match_type, MatchType::League)
    }
}

impl League {
    pub fn new(
        id: String,
        name: String,
        season: u32,
        team_ids: &[String],
        competition_id: Option<String>,
    ) -> Self {
        let standings = team_ids
            .iter()
            .map(|tid| StandingEntry::new(tid.clone()))
            .collect();

        Self {
            id,
            name,
            season,
            fixtures: Vec::new(),
            standings,
            competition_id,
            logo: None,
            league_kind: LeagueKind::Main,
            split_index: 0,
            tier: 0,
            active: false,
        }
    }

    pub fn sorted_standings(&self) -> Vec<StandingEntry> {
        let mut sorted = self.standings.clone();
        sorted.sort_by(|a, b| {
            b.points
                .cmp(&a.points)
                .then(b.kill_difference().cmp(&a.kill_difference()))
                .then(b.maps_won.cmp(&a.maps_won))
        });
        sorted
    }
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            id: String::new(),
            matchday: 0,
            date: String::new(),
            home_team_id: String::new(),
            away_team_id: String::new(),
            match_type: MatchType::League,
            best_of: default_best_of(),
            status: FixtureStatus::Scheduled,
            result: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that two fixtures with the same date and matchday but different
    /// fixture_ids are treated as distinct — no collision via fixture_id routing.
    #[test]
    fn test_fixture_id_uniquely_identifies() {
        let fix_a = Fixture {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            matchday: 5,
            date: "2025-02-15".to_string(),
            home_team_id: "team-a".to_string(),
            away_team_id: "team-b".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Scheduled,
            result: None,
        };

        let fix_b = Fixture {
            id: "660e8400-e29b-41d4-a716-446655440000".to_string(),
            matchday: 5,
            date: "2025-02-15".to_string(),
            home_team_id: "team-a".to_string(),
            away_team_id: "team-b".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Scheduled,
            result: None,
        };

        // Same date, same matchday, same teams — but different fixture_id
        // They must NOT be equal
        assert_ne!(
            fix_a, fix_b,
            "fixtures with different IDs must not be equal"
        );
        assert_ne!(fix_a.id, fix_b.id, "fixture IDs must be distinct");
    }

    /// Verify that applying a result to one fixture does not affect another
    /// fixture in the same collection with the same date/index.
    #[test]
    fn test_result_isolation_by_fixture_id() {
        let team_ids = vec!["team-a".to_string(), "team-b".to_string()];
        let mut league = League::new(
            "test-league".into(),
            "Test League".into(),
            2026,
            &team_ids,
            None,
        );

        let fix_a = Fixture {
            id: "fix-001".to_string(),
            matchday: 5,
            date: "2025-02-15".to_string(),
            home_team_id: "team-a".to_string(),
            away_team_id: "team-b".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Scheduled,
            result: None,
        };

        let fix_b = Fixture {
            id: "fix-002".to_string(),
            matchday: 5,
            date: "2025-02-15".to_string(),
            home_team_id: "team-a".to_string(),
            away_team_id: "team-b".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Scheduled,
            result: None,
        };

        league.fixtures = vec![fix_a, fix_b];

        // Apply result to fixture fix-001
        if let Some(f) = league.fixtures.iter_mut().find(|f| f.id == "fix-001") {
            f.status = FixtureStatus::Completed;
            f.result = Some(MatchResult {
                home_wins: 2,
                away_wins: 1,
                ended_by: MatchEndReason::NexusDestroyed,
                game_duration_seconds: 2400,
                report: None,
            });
        }

        // Verify fix-002 is untouched
        let fix_b = league.fixtures.iter().find(|f| f.id == "fix-002").unwrap();
        assert_eq!(fix_b.status, FixtureStatus::Scheduled);
        assert!(
            fix_b.result.is_none(),
            "result must NOT leak to other fixture"
        );
    }

    /// Verify that looking up a fixture by wrong fixture_id returns None.
    #[test]
    fn test_fixture_lookup_by_id_returns_none_for_missing() {
        let team_ids = vec!["team-a".to_string()];
        let league = League::new(
            "test-league".into(),
            "Test League".into(),
            2026,
            &team_ids,
            None,
        );

        let result = league.fixtures.iter().find(|f| f.id == "nonexistent-id");
        assert!(
            result.is_none(),
            "lookup by wrong fixture_id must return None"
        );
    }

    /// Verify that two competitions with fixtures on the same date do not
    /// interfere with each other when accessed via their respective leagues.
    #[test]
    fn test_cross_competition_fixture_isolation() {
        let team_ids = vec!["team-a".to_string(), "team-b".to_string()];

        // Competition A has a fixture on 2025-02-15 matchday 5
        let mut league_a = League::new("comp-a".into(), "Comp A".into(), 2026, &team_ids, None);
        league_a.fixtures = vec![Fixture {
            id: "a-fixture-1".to_string(),
            matchday: 5,
            date: "2025-02-15".to_string(),
            home_team_id: "team-a".to_string(),
            away_team_id: "team-b".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Completed,
            result: Some(MatchResult {
                home_wins: 2,
                away_wins: 0,
                ended_by: MatchEndReason::NexusDestroyed,
                game_duration_seconds: 1800,
                report: None,
            }),
        }];

        // Competition B has a fixture on the same date but different result
        let mut league_b = League::new("comp-b".into(), "Comp B".into(), 2026, &team_ids, None);
        league_b.fixtures = vec![Fixture {
            id: "b-fixture-1".to_string(),
            matchday: 5,
            date: "2025-02-15".to_string(),
            home_team_id: "team-a".to_string(),
            away_team_id: "team-b".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Completed,
            result: Some(MatchResult {
                home_wins: 1,
                away_wins: 2,
                ended_by: MatchEndReason::NexusDestroyed,
                game_duration_seconds: 2100,
                report: None,
            }),
        }];

        // Verify results are independent
        let a_result = league_a.fixtures[0].result.as_ref().unwrap();
        let b_result = league_b.fixtures[0].result.as_ref().unwrap();
        assert_eq!(a_result.home_wins, 2);
        assert_eq!(b_result.home_wins, 1);
        assert_ne!(
            a_result, b_result,
            "cross-competition results must be independent"
        );
    }
}
