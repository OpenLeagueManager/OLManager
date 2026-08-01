use crate::domain::league::{Fixture, FixtureStatus, League, MatchType};
use crate::generator::definitions::CompetitionManifest;
use chrono::{DateTime, Duration, TimeZone, Utc};
use uuid::Uuid;

fn build_fixture(
    matchday: u32,
    date: String,
    home_team_id: String,
    away_team_id: String,
    match_type: MatchType,
    best_of: u8,
) -> Fixture {
    Fixture {
        id: Uuid::new_v4().to_string(),
        matchday,
        date,
        home_team_id,
        away_team_id,
        match_type,
        best_of,
        status: FixtureStatus::Scheduled,
        result: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LecSplit {
    Winter,
    Spring,
    Summer,
}

pub fn regular_best_of(split: LecSplit) -> u8 {
    match split {
        LecSplit::Winter => 1,
        LecSplit::Spring | LecSplit::Summer => 3,
    }
}

pub fn playoff_best_of(split: LecSplit, is_grand_final: bool) -> u8 {
    match split {
        LecSplit::Winter => {
            if is_grand_final {
                5
            } else {
                3
            }
        }
        LecSplit::Spring | LecSplit::Summer => 5,
    }
}

pub fn parse_lec_split(name: &str) -> Option<LecSplit> {
    let key = name.to_lowercase();
    if key.contains("winter") {
        Some(LecSplit::Winter)
    } else if key.contains("spring") {
        Some(LecSplit::Spring)
    } else if key.contains("summer") {
        Some(LecSplit::Summer)
    } else {
        None
    }
}

/// Generate a full double round-robin schedule (home & away) for the given teams.
/// Matchdays are spaced 7 days apart starting from `start_date`.
/// Uses the "circle method" for balanced scheduling.
pub fn generate_league(
    name: &str,
    season: u32,
    team_ids: &[String],
    start_date: DateTime<Utc>,
) -> League {
    let n = team_ids.len();
    assert!(n >= 2, "Need at least 2 teams for a league");

    let league_id = Uuid::new_v4().to_string();
    let mut league = League::new(league_id, name.to_string(), season, team_ids, None);

    // For round-robin with n teams (n must be even; if odd, add a "bye" — we assume even here)
    // Number of rounds in a single round-robin = n - 1
    // Each round has n / 2 matches
    let rounds = n - 1;
    let half = n / 2;

    // Build a mutable list of team indices (circle method: fix index 0, rotate the rest)
    let mut indices: Vec<usize> = (0..n).collect();

    let mut matchday: u32 = 1;

    // First leg (home)
    for _round in 0..rounds {
        let round_date = start_date + Duration::days((matchday as i64 - 1) * 7);
        let date_str = round_date.format("%Y-%m-%d").to_string();

        for i in 0..half {
            let home_idx = indices[i];
            let away_idx = indices[n - 1 - i];

            let fixture = Fixture {
                id: Uuid::new_v4().to_string(),
                matchday,
                date: date_str.clone(),
                home_team_id: team_ids[home_idx].clone(),
                away_team_id: team_ids[away_idx].clone(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            };
            league.fixtures.push(fixture);
        }

        matchday += 1;

        // Rotate: keep index 0 fixed, rotate the rest
        let last = indices.pop().unwrap();
        indices.insert(1, last);
    }

    // Second leg (reverse home/away)
    let mut indices2: Vec<usize> = (0..n).collect();

    for _round in 0..rounds {
        let round_date = start_date + Duration::days((matchday as i64 - 1) * 7);
        let date_str = round_date.format("%Y-%m-%d").to_string();

        for i in 0..half {
            let home_idx = indices2[n - 1 - i]; // Reversed
            let away_idx = indices2[i];

            let fixture = Fixture {
                id: Uuid::new_v4().to_string(),
                matchday,
                date: date_str.clone(),
                home_team_id: team_ids[home_idx].clone(),
                away_team_id: team_ids[away_idx].clone(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            };
            league.fixtures.push(fixture);
        }

        matchday += 1;

        let last = indices2.pop().unwrap();
        indices2.insert(1, last);
    }

    league
}

/// Generate a single round-robin league (each pair plays once).
/// Used for LEC Winter regular season (9 matchdays with 10 teams).
pub fn generate_single_round_league(
    name: &str,
    season: u32,
    team_ids: &[String],
    start_date: DateTime<Utc>,
) -> League {
    generate_single_round_league_with_offsets(name, season, team_ids, start_date, None)
}

/// Generate a single round-robin league (each pair plays once) with optional
/// per-round day offsets from `start_date`.
///
/// When `round_day_offsets` is provided, it must contain exactly n-1 values
/// (one date offset for each round). This is useful for tournament formats like
/// LEC Winter regular season superweeks (Sat/Sun/Mon).
pub fn generate_single_round_league_with_offsets(
    name: &str,
    season: u32,
    team_ids: &[String],
    start_date: DateTime<Utc>,
    round_day_offsets: Option<&[i64]>,
) -> League {
    generate_single_round_league_with_offsets_and_bo(
        name,
        season,
        team_ids,
        start_date,
        round_day_offsets,
        1,
    )
}

pub fn generate_single_round_league_with_offsets_and_bo(
    name: &str,
    season: u32,
    team_ids: &[String],
    start_date: DateTime<Utc>,
    round_day_offsets: Option<&[i64]>,
    best_of: u8,
) -> League {
    let league_id = Uuid::new_v4().to_string();
    generate_single_round_league_with_offsets_and_bo_with_id(
        &league_id,
        name,
        season,
        team_ids,
        start_date,
        round_day_offsets,
        best_of,
    )
}

/// Same as `generate_single_round_league_with_offsets_and_bo` but accepts
/// an explicit `league_id` instead of generating a UUID. Used when the
/// `competition_id` should be used as the league id for consistent frontend
/// mapping (e.g. team.competition_id === league.id).
pub fn generate_single_round_league_with_offsets_and_bo_with_id(
    league_id: &str,
    name: &str,
    season: u32,
    team_ids: &[String],
    start_date: DateTime<Utc>,
    round_day_offsets: Option<&[i64]>,
    best_of: u8,
) -> League {
    let real_n = team_ids.len();
    assert!(real_n >= 2, "Need at least 2 teams for a league");

    let mut league = League::new(
        league_id.to_string(),
        name.to_string(),
        season,
        team_ids,
        None,
    );

    // Circle method requires an even number of slots. For an odd team count we
    // add a sentinel "bye" slot: whichever real team is paired with it sits out
    // that round (no fixture is emitted).
    let has_bye = real_n % 2 != 0;
    let n = if has_bye { real_n + 1 } else { real_n };
    let bye_idx = n - 1; // sentinel index, only present when has_bye

    let rounds = n - 1;
    let half = n / 2;

    if let Some(offsets) = round_day_offsets {
        assert!(
            offsets.len() == rounds,
            "round_day_offsets length must match round count"
        );
    }

    let mut indices: Vec<usize> = (0..n).collect();
    let mut matchday: u32 = 1;

    for round_idx in 0..rounds {
        let day_offset = round_day_offsets
            .and_then(|offsets| offsets.get(round_idx).copied())
            .unwrap_or((matchday as i64 - 1) * 7);
        let round_date = start_date + Duration::days(day_offset);
        let date_str = round_date.format("%Y-%m-%d").to_string();

        for i in 0..half {
            let home_idx = indices[i];
            let away_idx = indices[n - 1 - i];
            // Skip the pairing that involves the bye slot — that team rests.
            if has_bye && (home_idx == bye_idx || away_idx == bye_idx) {
                continue;
            }
            let (home_team_id, away_team_id) = if (matchday + i as u32) % 2 == 0 {
                (team_ids[home_idx].clone(), team_ids[away_idx].clone())
            } else {
                (team_ids[away_idx].clone(), team_ids[home_idx].clone())
            };

            league.fixtures.push(build_fixture(
                matchday,
                date_str.clone(),
                home_team_id,
                away_team_id,
                MatchType::League,
                best_of,
            ));
        }

        matchday += 1;
        let last = indices.pop().unwrap();
        indices.insert(1, last);
    }

    league
}

/// Generate a league schedule from a competition manifest.
/// Supports "single_round_robin" and "double_round_robin" formats.
/// League name is set to the manifest's name (no split suffix).
pub fn generate_schedule_from_config(
    manifest: &CompetitionManifest,
    year: u32,
    team_ids: &[String],
    split_index: usize,
) -> League {
    let config = &manifest.schedule;
    // Defensive: a manifest with no schedule splits cannot produce a calendar.
    // Return an empty league (id + teams, no fixtures) instead of panicking on
    // an out-of-bounds index. Callers that need a real schedule should skip
    // such competitions beforehand.
    if config.splits.get(split_index).is_none() {
        let mut league = League::new(
            manifest.id.clone(),
            manifest.name.clone(),
            year,
            team_ids,
            None,
        );
        league.competition_id = Some(manifest.id.clone());
        league.split_index = split_index;
        return league;
    }
    let split = &config.splits[split_index];
    let season_start = Utc
        .with_ymd_and_hms(
            year as i32,
            split.season_start.month,
            split.season_start.day,
            0,
            0,
            0,
        )
        .unwrap();

    let mut league = match config.format.as_str() {
        "single_round_robin" => generate_single_round_league_with_offsets_and_bo_with_id(
            &manifest.id,
            &manifest.name,
            year,
            team_ids,
            season_start,
            if split.superweek_offsets.is_empty() {
                None
            } else {
                Some(&split.superweek_offsets)
            },
            split.best_of as u8,
        ),
        "double_round_robin" => {
            let mut l = generate_single_round_league_with_offsets_and_bo_with_id(
                &manifest.id,
                &manifest.name,
                year,
                team_ids,
                season_start,
                if split.superweek_offsets.is_empty() {
                    None
                } else {
                    Some(&split.superweek_offsets)
                },
                split.best_of as u8,
            );
            let last_offset = split
                .superweek_offsets
                .last()
                .copied()
                .unwrap_or(7 * (team_ids.len() as i64 - 1));
            let return_start = season_start + Duration::days(last_offset + 7);
            let return_league = generate_single_round_league_with_offsets_and_bo(
                &format!("{} (Return)", manifest.name),
                year,
                team_ids,
                return_start,
                if split.superweek_offsets.is_empty() {
                    None
                } else {
                    Some(&split.superweek_offsets)
                },
                split.best_of as u8,
            );
            for fixture in return_league.fixtures {
                let reversed = Fixture {
                    home_team_id: fixture.away_team_id,
                    away_team_id: fixture.home_team_id,
                    ..fixture
                };
                l.fixtures.push(reversed);
            }
            l.fixtures
                .sort_by(|a, b| a.date.cmp(&b.date).then(a.matchday.cmp(&b.matchday)));
            l
        }
        _ => {
            log::warn!(
                "[schedule] unknown format '{}', falling back to single_round_robin",
                config.format
            );
            generate_single_round_league_with_offsets_and_bo(
                &manifest.name,
                year,
                team_ids,
                season_start,
                if split.superweek_offsets.is_empty() {
                    None
                } else {
                    Some(&split.superweek_offsets)
                },
                split.best_of as u8,
            )
        }
    };
    league.competition_id = Some(manifest.id.clone());
    league.split_index = split_index;
    league
}

pub fn generate_preseason_friendlies(
    user_team_id: &str,
    opponent_ids: &[String],
    season_start: DateTime<Utc>,
    max_friendlies: usize,
) -> Vec<Fixture> {
    opponent_ids
        .iter()
        .filter(|opponent_id| opponent_id.as_str() != user_team_id)
        .take(max_friendlies)
        .enumerate()
        .map(|(index, opponent_id)| {
            let weeks_before_start = (max_friendlies.saturating_sub(index)) as i64;
            let date = (season_start - Duration::days(weeks_before_start * 7))
                .format("%Y-%m-%d")
                .to_string();
            let (home_team_id, away_team_id) = if index % 2 == 0 {
                (user_team_id.to_string(), opponent_id.clone())
            } else {
                (opponent_id.clone(), user_team_id.to_string())
            };

            Fixture {
                id: Uuid::new_v4().to_string(),
                matchday: 0,
                date,
                home_team_id,
                away_team_id,
                match_type: MatchType::Friendly,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            }
        })
        .collect()
}

pub fn append_fixtures(league: &mut League, mut additional_fixtures: Vec<Fixture>) {
    league.fixtures.append(&mut additional_fixtures);
    league.fixtures.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then(left.matchday.cmp(&right.matchday))
            .then(left.id.cmp(&right.id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_generate_league_8_teams() {
        let teams: Vec<String> = (0..8).map(|i| format!("team_{}", i)).collect();
        let start = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let league = generate_league("Test League", 2026, &teams, start);

        // 8 teams: 7 rounds * 4 matches * 2 legs = 56 fixtures
        assert_eq!(league.fixtures.len(), 56);

        // 14 matchdays (7 per leg)
        let max_md = league.fixtures.iter().map(|f| f.matchday).max().unwrap();
        assert_eq!(max_md, 14);

        // Each team plays 14 matches total
        for team in &teams {
            let count = league
                .fixtures
                .iter()
                .filter(|f| f.home_team_id == *team || f.away_team_id == *team)
                .count();
            assert_eq!(count, 14, "Team {} plays {} matches", team, count);
        }

        // 8 standings entries
        assert_eq!(league.standings.len(), 8);
    }

    #[test]
    fn test_generate_league_16_teams() {
        let teams: Vec<String> = (0..16).map(|i| format!("team_{}", i)).collect();
        let start = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let league = generate_league("Premier Division", 2026, &teams, start);

        // 16 teams: 15 rounds * 8 matches * 2 legs = 240 fixtures
        assert_eq!(league.fixtures.len(), 240);

        // 30 matchdays (15 per leg)
        let max_md = league.fixtures.iter().map(|f| f.matchday).max().unwrap();
        assert_eq!(max_md, 30);

        // Each team plays 30 matches total (15 home + 15 away)
        for team in &teams {
            let count = league
                .fixtures
                .iter()
                .filter(|f| f.home_team_id == *team || f.away_team_id == *team)
                .count();
            assert_eq!(count, 30, "Team {} plays {} matches", team, count);
        }

        // 16 standings entries
        assert_eq!(league.standings.len(), 16);

        // No team plays itself
        for f in &league.fixtures {
            assert_ne!(f.home_team_id, f.away_team_id);
        }
    }

    #[test]
    fn generate_preseason_friendlies_marks_fixtures_as_friendlies() {
        let start = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let friendlies = generate_preseason_friendlies(
            "team_1",
            &[
                "team_2".to_string(),
                "team_3".to_string(),
                "team_4".to_string(),
            ],
            start,
            3,
        );

        assert_eq!(friendlies.len(), 3);
        assert!(
            friendlies
                .iter()
                .all(|fixture| fixture.match_type == MatchType::Friendly)
        );
        assert!(friendlies.iter().all(|fixture| fixture.matchday == 0));
        assert!(friendlies.iter().all(|fixture| fixture.best_of == 1));
        assert_eq!(friendlies[0].date, "2026-07-11");
        assert_eq!(friendlies[2].date, "2026-07-25");
    }
}
