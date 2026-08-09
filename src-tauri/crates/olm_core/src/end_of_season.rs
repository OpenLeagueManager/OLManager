use crate::domain::league::{FixtureStatus, League, MatchType};
use crate::domain::message::*;
use crate::domain::player::PlayerSeasonStats;
use crate::domain::team::{FinancialTransactionKind, TeamSeasonRecord};
use crate::finances::{
    BudgetImpact, FinanceTransactionInput, push_board_financial_health_mail,
    push_prize_payout_mail, record_transaction,
};
use crate::game::Game;
use crate::generator::definitions::CompetitionManifest;
use crate::roster_stability::{RosterStabilityReason, repair_league};
use crate::schedule::{
    append_fixtures, generate_preseason_friendlies, generate_schedule_from_config,
};
use crate::season_awards::compute_season_awards;
use chrono::{Datelike, TimeZone, Utc};
use std::collections::HashMap;

pub fn expected_fixture_count(team_count: usize) -> Option<usize> {
    if team_count >= 2 && team_count % 2 == 0 {
        Some((team_count * (team_count - 1)) / 2)
    } else {
        None
    }
}

pub fn has_full_schedule(league: &League) -> bool {
    match expected_fixture_count(league.standings.len()) {
        Some(expected_single_round_count) => {
            let expected_double_round_count = expected_single_round_count * 2;
            let actual = league
                .fixtures
                .iter()
                .filter(|fixture| fixture.counts_for_league_standings())
                .count();

            actual == expected_single_round_count || actual == expected_double_round_count
        }
        None => false,
    }
}

/// Returns true if at least one competitive fixture has been completed or any
/// standing entry records a played match. Used as a guard to prevent premature
/// end-of-season processing for a season that has not yet kicked off.
pub fn season_has_started(league: &League) -> bool {
    league
        .fixtures
        .iter()
        .any(|f| f.counts_for_league_standings() && f.status == FixtureStatus::Completed)
        || league.standings.iter().any(|e| e.played > 0)
}

pub fn is_league_complete(league: &League) -> bool {
    let regular_complete = season_has_started(league)
        && has_full_schedule(league)
        && league
            .fixtures
            .iter()
            .filter(|fixture| fixture.counts_for_league_standings())
            .all(|fixture| fixture.status == FixtureStatus::Completed);

    let playoffs_exist = league
        .fixtures
        .iter()
        .any(|fixture| fixture.match_type == MatchType::Playoffs);
    let playoffs_complete = !playoffs_exist
        || league
            .fixtures
            .iter()
            .filter(|fixture| fixture.match_type == MatchType::Playoffs)
            .all(|fixture| fixture.status == FixtureStatus::Completed);

    regular_complete && playoffs_complete
}

/// Check if the season is complete (all fixtures played).
pub fn is_season_complete(game: &Game) -> bool {
    game.active_league().is_some_and(is_league_complete)
}

const PRIZE_MONEY_BY_POSITION: [i64; 10] = [
    800_000, 500_000, 350_000, 250_000, 180_000, 140_000, 110_000, 90_000, 70_000, 50_000,
];

fn position_suffix(position: u32) -> &'static str {
    match position {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

fn prize_money_for_position(position: u32) -> i64 {
    if position == 0 {
        return 0;
    }

    PRIZE_MONEY_BY_POSITION
        .get(position.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(150_000)
}

fn refresh_hiring_cycle_budgets(team: &mut crate::domain::team::Team) {
    // Minimal hook: after split settlements (prize/objectives), rebalance next-cycle
    // planning budgets from current treasury so offseason hiring decisions have
    // coherent funds available without a full finance redesign.
    team.wage_budget = ((team.finance.max(0) as f64) * 0.06).round() as i64;
    team.transfer_budget = ((team.finance.max(0) as f64) * 0.22).round() as i64;
}

/// Process end-of-season: record history, compute awards, reset stats, generate next season.
/// Returns a summary struct for the frontend to display.
/// Accepts an optional CompetitionManifest for manifest-driven schedule generation.
pub fn process_end_of_season(game: &mut Game) -> EndOfSeasonSummary {
    process_end_of_season_inner(game, None)
}

/// Like `process_end_of_season` but accepts a CompetitionManifest for manifest-driven
/// schedule generation in the next split.
pub fn process_end_of_season_with_config(
    game: &mut Game,
    manifest: Option<&CompetitionManifest>,
) -> EndOfSeasonSummary {
    process_end_of_season_inner(game, manifest)
}

/// Process end-of-split: generate next split's schedule using the league's
/// current `split_index`. Caller is responsible for:
///   - Calling `process_end_of_season` first when the split wraps the season
///   - Setting `split_index = 0` and `season += 1` before calling this
///     function for the new season's first split
/// Unlike end_of_season, this does NOT record history, award prizes,
/// reset player stats, or send board messages — only fixtures are created.
pub fn process_end_of_split(game: &mut Game, manifest: &CompetitionManifest) {
    let league = match game.active_league() {
        Some(l) => l,
        None => return,
    };
    let split_index = league.split_index;
    let season = league.season;
    let team_ids: Vec<String> = league.standings.iter().map(|s| s.team_id.clone()).collect();
    let user_team_id = game.manager.team_id.clone().unwrap_or_default();

    let mut new_league = generate_schedule_from_config(manifest, season, &team_ids, split_index);

    if !user_team_id.is_empty() {
        let opponents: Vec<String> = team_ids
            .iter()
            .filter(|tid| tid.as_str() != user_team_id)
            .cloned()
            .collect();
        let split = &manifest.schedule.splits[split_index];
        let next_start = Utc
            .with_ymd_and_hms(
                season as i32,
                split.season_start.month,
                split.season_start.day,
                0,
                0,
                0,
            )
            .unwrap();
        let friendlies = generate_preseason_friendlies(
            &user_team_id,
            &opponents,
            next_start,
            manifest.schedule.preseason_friendlies as usize,
        );
        append_fixtures(&mut new_league, friendlies);
    }

    if let Some(active) = game.active_league_mut() {
        *active = new_league;
    }
}

/// Process end-of-season for background leagues (game.leagues[1..]).
/// Records team history for completed bg leagues and generates next season via
/// the competition manifest (looked up by competition_id).
/// Skips bg leagues that are not complete, or have no competition_id.
pub fn process_background_seasons(
    game: &mut Game,
    manifests: &HashMap<String, CompetitionManifest>,
) {
    // First pass: identify complete bg leagues (avoiding borrow conflicts)
    let complete_indices: Vec<usize> = (1..game.leagues.len())
        .filter(|i| is_league_complete(&game.leagues[*i]))
        .collect();

    for i in complete_indices {
        let season = game.leagues[i].season;
        let final_standings = game.leagues[i].sorted_standings();
        let competition_id = game.leagues[i].competition_id.clone();
        let team_ids: Vec<String> = game.leagues[i]
            .standings
            .iter()
            .map(|entry| entry.team_id.clone())
            .collect();

        // Determine if this split wraps the season (last split → new year).
        // Leagues without a known manifest are treated as end-of-season so that
        // completed background leagues still record history even when their
        // config has been removed or their competition_id is not resolvable.
        let is_end_of_season = competition_id
            .as_ref()
            .map(|cid| match manifests.get(cid) {
                Some(manifest) => game.leagues[i].split_index + 1 >= manifest.schedule.splits.len(),
                None => true,
            })
            .unwrap_or(true);

        if is_end_of_season {
            // Record TeamSeasonRecord for each team (no prize money, no messages)
            for (idx, standing) in final_standings.iter().enumerate() {
                if let Some(team) = game.teams.iter_mut().find(|t| t.id == standing.team_id) {
                    let already_recorded =
                        team.history.iter().any(|record| record.season == season);
                    if !already_recorded {
                        team.history.push(TeamSeasonRecord {
                            season,
                            league_position: (idx + 1) as u32,
                            played: standing.played,
                            won: standing.won,
                            lost: standing.lost,
                            kills_for: standing.maps_won,
                            kills_against: standing.maps_lost,
                        });
                    }
                    team.form.clear();
                }
            }
        }

        // Generate next split/season schedule
        if let Some(ref cid) = competition_id {
            if let Some(manifest) = manifests.get(cid) {
                let next_idx = game.leagues[i].split_index + 1;
                let (ns, split_idx) = if next_idx >= manifest.schedule.splits.len() {
                    (season + 1, 0)
                } else {
                    (season, next_idx)
                };
                let new_league = generate_schedule_from_config(manifest, ns, &team_ids, split_idx);
                game.leagues[i] = new_league;
            }
        }
    }
}

fn process_end_of_season_inner(
    game: &mut Game,
    _manifest: Option<&CompetitionManifest>,
) -> EndOfSeasonSummary {
    crate::board_objectives::update_objective_progress(game);

    let league = match game.active_league() {
        Some(l) => l,
        None => return EndOfSeasonSummary::default(),
    };

    let _league_id = league
        .competition_id
        .as_deref()
        .unwrap_or(&league.id)
        .to_string();
    let season = league.season;
    let league_name = league.name.clone();
    let _current_split_index = league.split_index;
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    // Messages should be dated on the last match day, not on the clock date
    // (which may already be one day ahead due to process_day advancing the clock).
    let last_fixture_date = league
        .fixtures
        .iter()
        .filter(|f| f.counts_for_league_standings() && f.status == FixtureStatus::Completed)
        .map(|f| f.date.as_str())
        .max()
        .unwrap_or(today.as_str())
        .to_string();

    // 1. Compute final standings
    let final_standings = league.sorted_standings();

    // 2. Compute awards before resetting stats
    let awards = compute_season_awards(game);

    // 3. Build summary
    let user_team_id = game.manager.team_id.clone().unwrap_or_default();
    let user_position = final_standings
        .iter()
        .position(|s| s.team_id == user_team_id)
        .map(|i| i + 1)
        .unwrap_or(0) as u32;
    let user_standing = final_standings
        .iter()
        .find(|s| s.team_id == user_team_id)
        .cloned();

    // --- Adjust position for playoff performance ---
    // The regular-season `user_position` may be misleading when the team
    // over- or under-performed in playoffs.  A playoff champion should be
    // counted as 1st; a playoff finalist as at least 2nd, etc.
    let playoff_champion_id = league
        .fixtures
        .iter()
        .filter(|fixture| fixture.match_type == MatchType::Playoffs)
        .max_by_key(|fixture| fixture.matchday)
        .and_then(|fixture| {
            fixture.result.as_ref().and_then(|result| {
                if result.home_wins > result.away_wins {
                    Some(fixture.home_team_id.clone())
                } else if result.away_wins > result.home_wins {
                    Some(fixture.away_team_id.clone())
                } else {
                    None
                }
            })
        });

    let champion_id = playoff_champion_id.unwrap_or_else(|| {
        final_standings
            .first()
            .map(|s| s.team_id.clone())
            .unwrap_or_default()
    });

    // If the user's team participated in playoffs, adjust their reported
    // position to reflect the playoff outcome rather than just the regular-
    // season standing.  A playoff champion is counted as 1st.
    let playoff_fixtures: Vec<&crate::domain::league::Fixture> = league
        .fixtures
        .iter()
        .filter(|f| f.match_type == MatchType::Playoffs)
        .collect();

    let user_playoff_position = if !playoff_fixtures.is_empty() {
        if champion_id == user_team_id {
            // Playoff champion → position 1
            Some(1u32)
        } else if playoff_fixtures.iter().any(|f| {
            f.home_team_id == user_team_id || f.away_team_id == user_team_id
        }) {
            // Participated in playoffs but didn't win → boost to at least 2nd
            Some(user_position.min(2))
        } else {
            None
        }
    } else {
        None
    };

    let user_position = user_playoff_position.unwrap_or(user_position);
    let champion_name = game
        .teams
        .iter()
        .find(|t| t.id == champion_id)
        .map(|t| t.name.clone())
        .unwrap_or_default();

    let summary = EndOfSeasonSummary {
        season,
        league_name: league_name.clone(),
        champion_id: champion_id.clone(),
        champion_name,
        user_position,
        user_points: user_standing.as_ref().map(|s| s.points).unwrap_or(0),
        user_won: user_standing.as_ref().map(|s| s.won).unwrap_or(0),
        user_lost: user_standing.as_ref().map(|s| s.lost).unwrap_or(0),
        user_maps_won: user_standing.as_ref().map(|s| s.maps_won).unwrap_or(0),
        user_maps_lost: user_standing.as_ref().map(|s| s.maps_lost).unwrap_or(0),
        golden_boot_player: awards
            .golden_boot
            .first()
            .map(|e| e.player_name.clone())
            .unwrap_or_default(),
        golden_boot_goals: awards
            .golden_boot
            .first()
            .map(|e| e.value as u32)
            .unwrap_or(0),
        poty_player: awards
            .player_of_year
            .first()
            .map(|e| e.player_name.clone())
            .unwrap_or_default(),
        poty_rating: awards
            .player_of_year
            .first()
            .map(|e| e.value)
            .unwrap_or(0.0),
        total_teams: final_standings.len() as u32,
    };

    // 4. Record team season history
    for (idx, standing) in final_standings.iter().enumerate() {
        if let Some(team) = game.teams.iter_mut().find(|t| t.id == standing.team_id) {
            let position = (idx + 1) as u32;
            let prize_money = prize_money_for_position(position);

            team.history.push(TeamSeasonRecord {
                season,
                league_position: position,
                played: standing.played,
                won: standing.won,
                lost: standing.lost,
                kills_for: standing.maps_won,
                kills_against: standing.maps_lost,
            });
            // Reset form
            team.form.clear();

            if prize_money > 0 {
                record_transaction(
                    team,
                    FinanceTransactionInput {
                        date: last_fixture_date.clone(),
                        description: format!(
                            "Season {} prize money for {}{} place",
                            season,
                            position,
                            position_suffix(position)
                        ),
                        amount: prize_money,
                        kind: FinancialTransactionKind::PrizeMoney,
                        budget_impact: BudgetImpact::None,
                        affects_season_totals: true,
                        source: "prize".to_string(),
                        source_id: Some(format!("season-{season}-position-{position}")),
                        correlation_id: Some(format!("prize:{}:{}:{}", team.id, season, position)),
                    },
                )
                .ok();
            }

            refresh_hiring_cycle_budgets(team);
        }
    }

    // 5. Record player career entries and reset stats
    for player in game.players.iter_mut() {
        if player.stats.appearances > 0 {
            let team_name = player
                .team_id
                .as_ref()
                .and_then(|tid| game.teams.iter().find(|t| &t.id == tid))
                .map(|t| t.name.clone())
                .unwrap_or_else(|| "Free Agent".to_string());
            let team_id = player.team_id.clone();

            player.career.push(crate::domain::player::CareerEntry {
                season,
                team_id,
                team_name,
                appearances: player.stats.appearances,
                kills: player.stats.kills,
                deaths: 0, // TODO: track deaths in match stats
                assists: player.stats.assists,
                avg_rating: player.stats.avg_rating,
            });
        }
        // Reset stats for next season
        player.stats = PlayerSeasonStats::default();
    }

    // 6. Update manager career stats
    if let Some(standing) = &user_standing {
        let total_matches = standing.won + standing.lost;
        game.manager.career_stats.matches_managed += total_matches;
        game.manager.career_stats.wins += standing.won;
        game.manager.career_stats.losses += standing.lost;
        if user_position == 1 {
            game.manager.career_stats.trophies += 1;
        }
        let best = game.manager.career_stats.best_finish;
        if best.is_none() || best.unwrap() > user_position {
            game.manager.career_stats.best_finish = Some(user_position);
        }
        // Update or create career history entry for current team
        let team_name = game
            .teams
            .iter()
            .find(|t| t.id == user_team_id)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let today_str = game.clock.current_date.format("%Y-%m-%d").to_string();
        // Check if there's an existing open entry for this team
        let existing = game
            .manager
            .career_history
            .iter_mut()
            .find(|e| e.team_id == user_team_id && e.end_date.is_none());
        if let Some(entry) = existing {
            entry.matches += total_matches;
            entry.wins += standing.won;
            entry.losses += standing.lost;
            let prev_best = entry.best_league_position;
            if prev_best.is_none() || prev_best.unwrap() > user_position {
                entry.best_league_position = Some(user_position);
            }
        } else {
            game.manager
                .career_history
                .push(crate::domain::manager::ManagerCareerEntry {
                    team_id: user_team_id.clone(),
                    team_name,
                    start_date: today_str,
                    end_date: None,
                    matches: total_matches,
                    wins: standing.won,
                    losses: standing.lost,
                    best_league_position: Some(user_position),
                });
        }
    }

    // 6b. Evaluate board objectives and adjust satisfaction
    let objective_result = crate::board_objectives::evaluate_objective_result(game);
    let obj_delta = objective_result.satisfaction_delta;
    let new_sat = (game.manager.satisfaction as i16 + obj_delta as i16).clamp(0, 100) as u8;
    game.manager.satisfaction = new_sat;
    // Clear objectives for next season (will be regenerated on first process_day)
    game.board_objectives.clear();

    // 6c. Clear old news articles from the previous season
    game.news.clear();

    // 7. Advance to next season: increment year, reset split.
    // Schedule generation is handled by the caller (process_end_of_split).
    let next_season_num = season + 1;
    if let Some(active) = game.active_league_mut() {
        active.season = next_season_num;
        active.split_index = 0;
    }

    let team_names: Vec<String> = final_standings
        .iter()
        .map(|s| {
            game.teams
                .iter()
                .find(|t| t.id == s.team_id)
                .map(|t| t.name.clone())
                .unwrap_or_default()
        })
        .collect();
    game.news.push(crate::news::season_preview_article(
        &team_names,
        &game.clock.current_date.to_rfc3339(),
    ));

    // 8. Send end-of-season messages
    let pos_suffix = position_suffix(user_position);

    let user_team_name = game
        .teams
        .iter()
        .find(|t| t.id == user_team_id)
        .map(|t| t.name.clone())
        .unwrap_or_default();

    let board_msg = if user_position == 1 {
        format!(
            "Congratulations! {} are league champions! What an incredible achievement.\n\n\
            The board is absolutely delighted with your performance. You finished on {} points.\n\n\
            We look forward to defending the title next season.",
            user_team_name, summary.user_points
        )
    } else if user_position <= 4 {
        format!(
            "A solid season for {}. You finished in {}{} place with {} points.\n\n\
            The board is satisfied with the campaign. Let's push for the title next season.",
            user_team_name, user_position, pos_suffix, summary.user_points
        )
    } else if user_position <= summary.total_teams / 2 {
        format!(
            "{} finished the season in {}{} place with {} points.\n\n\
            A mid-table finish. The board expects improvement next season. \
            We need to be more competitive.",
            user_team_name, user_position, pos_suffix, summary.user_points
        )
    } else {
        format!(
            "A disappointing season for {}. Finishing {}{} with only {} points is below expectations.\n\n\
            The board is concerned. Significant improvement will be needed next season, \
            or your position may come under review.",
            user_team_name, user_position, pos_suffix, summary.user_points
        )
    };

    let existing_ids: std::collections::HashSet<String> =
        game.messages.iter().map(|m| m.id.clone()).collect();

    let objective_msg_id = format!("board_objective_review_{}", season);
    if objective_result.total > 0 && !existing_ids.contains(&objective_msg_id) {
        let delta_label = if obj_delta > 0 {
            format!("+{}", obj_delta)
        } else {
            obj_delta.to_string()
        };
        let objective_message = InboxMessage::new(
            objective_msg_id,
            format!("Season {} — Board Objective Review", season),
            format!(
                "The board has completed its end-of-split objective review. You delivered {}/{} objectives.\n\nManager satisfaction impact: {}.\n\nThis review reflects competitive performance across the split: final standing, series wins, map wins, draft preparation, and roster execution.",
                objective_result.met_count,
                objective_result.total,
                delta_label
            ),
            "Board of Directors".to_string(),
            last_fixture_date.clone(),
        )
        .with_category(MessageCategory::BoardDirective)
        .with_priority(MessagePriority::High)
        .with_sender_role("Chairman")
        .with_i18n("be.msg.boardObjectiveReview.subject", "be.msg.boardObjectiveReview.body", {
            let mut params = std::collections::HashMap::new();
            params.insert("season".to_string(), season.to_string());
            params.insert("metCount".to_string(), objective_result.met_count.to_string());
            params.insert("total".to_string(), objective_result.total.to_string());
            params.insert("satisfactionDelta".to_string(), delta_label);
            params
        })
        .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman");
        game.messages.push(objective_message);
    }

    let payout_msg_id = format!("season_payout_{}", season);
    let user_prize_money = prize_money_for_position(user_position);
    if user_prize_money > 0 && !existing_ids.contains(&payout_msg_id) {
        let payout_message = InboxMessage::new(
            payout_msg_id,
            format!("Season {} Prize Money Awarded", season),
            format!(
                "The board has confirmed a prize payout of €{} for your {}{}-place league finish. The amount has been added to the club balance.",
                user_prize_money,
                user_position,
                pos_suffix
            ),
            "Board of Directors".to_string(),
            last_fixture_date.clone(),
        )
        .with_category(MessageCategory::Finance)
        .with_priority(MessagePriority::High)
        .with_sender_role("Chairman")
        .with_i18n("be.msg.seasonPayout.subject", "be.msg.seasonPayout.body", {
            let mut params = std::collections::HashMap::new();
            params.insert("season".to_string(), season.to_string());
            params.insert("amount".to_string(), user_prize_money.to_string());
            params.insert("position".to_string(), user_position.to_string());
            params
        })
        .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman");
        game.messages.push(payout_message);
    }
    if user_prize_money > 0 && !user_team_id.is_empty() {
        push_prize_payout_mail(
            game,
            &user_team_id,
            season,
            user_position,
            user_prize_money,
            &last_fixture_date,
        );
    }

    if let Some(user_team) = game.teams.iter().find(|team| team.id == user_team_id) {
        let monthly_net = (user_team.season_income - user_team.season_expenses) / 12;
        let months_left = if monthly_net < 0 {
            Some((user_team.finance / monthly_net.abs()).max(0))
        } else {
            None
        };
        push_board_financial_health_mail(
            game,
            &user_team_id,
            user_team.finance,
            monthly_net,
            months_left,
            &last_fixture_date,
        );
    }

    let msg_id = format!("season_end_{}", season);
    if !existing_ids.contains(&msg_id) {
        let (body_key, mut i18n_params) = if user_position == 1 {
            let mut p = std::collections::HashMap::new();
            p.insert("team".to_string(), user_team_name.clone());
            p.insert("points".to_string(), summary.user_points.to_string());
            ("be.msg.seasonReview.body.champion", p)
        } else if user_position <= 4 {
            let mut p = std::collections::HashMap::new();
            p.insert("team".to_string(), user_team_name.clone());
            p.insert("position".to_string(), user_position.to_string());
            p.insert("suffix".to_string(), pos_suffix.to_string());
            p.insert("points".to_string(), summary.user_points.to_string());
            ("be.msg.seasonReview.body.topFour", p)
        } else if user_position <= summary.total_teams / 2 {
            let mut p = std::collections::HashMap::new();
            p.insert("team".to_string(), user_team_name.clone());
            p.insert("position".to_string(), user_position.to_string());
            p.insert("suffix".to_string(), pos_suffix.to_string());
            p.insert("points".to_string(), summary.user_points.to_string());
            ("be.msg.seasonReview.body.midTable", p)
        } else {
            let mut p = std::collections::HashMap::new();
            p.insert("team".to_string(), user_team_name.clone());
            p.insert("position".to_string(), user_position.to_string());
            p.insert("suffix".to_string(), pos_suffix.to_string());
            p.insert("points".to_string(), summary.user_points.to_string());
            ("be.msg.seasonReview.body.lowerHalf", p)
        };
        i18n_params.insert("season".to_string(), season.to_string());

        let msg = InboxMessage::new(
            msg_id,
            format!("Season {} Review", season),
            board_msg,
            "Board of Directors".to_string(),
            last_fixture_date.clone(),
        )
        .with_category(MessageCategory::BoardDirective)
        .with_priority(MessagePriority::High)
        .with_sender_role("Chairman")
        .with_i18n("be.msg.seasonReview.subject", body_key, i18n_params)
        .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman");
        game.messages.push(msg);
    }

    let sched_msg_id = format!("new_season_{}", next_season_num);
    if !existing_ids.contains(&sched_msg_id) {
        let mut sched_params = std::collections::HashMap::new();
        sched_params.insert("season".to_string(), next_season_num.to_string());
        let sched_msg = InboxMessage::new(
            sched_msg_id,
            format!("Season {} — New Schedule Released", next_season_num),
            format!(
                "The schedule for Season {} has been released! The new campaign kicks off in 4 weeks.\n\n\
                Use this break to assess your squad, make any necessary changes, and prepare for the challenges ahead.\n\n\
                Good luck!",
                next_season_num
            ),
            "League Office".to_string(),
            last_fixture_date,
        )
        .with_category(MessageCategory::LeagueInfo)
        .with_priority(MessagePriority::Normal)
        .with_sender_role("Competition Secretary")
        .with_i18n(
            "be.msg.newSeasonSchedule.subject",
            "be.msg.newSeasonSchedule.body",
            sched_params,
        )
        .with_sender_i18n("be.sender.leagueOffice", "be.role.competitionSecretary");
        game.messages.push(sched_msg);
    }

    crate::season_context::refresh_game_context(game);

    // 9. Renew contracts for players who finished the season with a team
    renew_contracts_for_retained_players(game);

    // 10. Repair depleted rosters via the roster stability seam (replaces old
    //     replenish_depleted_rosters). Handles all schedulable AI main teams.
    let _ = repair_league(game, RosterStabilityReason::SeasonTransition);

    summary
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EndOfSeasonSummary {
    pub season: u32,
    pub league_name: String,
    pub champion_id: String,
    pub champion_name: String,
    pub user_position: u32,
    pub user_points: u32,
    pub user_won: u32,
    pub user_lost: u32,
    pub user_maps_won: u32,
    pub user_maps_lost: u32,
    pub golden_boot_player: String,
    pub golden_boot_goals: u32,
    pub poty_player: String,
    pub poty_rating: f64,
    pub total_teams: u32,
}

#[cfg(test)]
mod summary_contract_tests {
    use super::EndOfSeasonSummary;

    #[test]
    fn serializes_the_tauri_summary_contract() {
        let value = serde_json::to_value(EndOfSeasonSummary::default()).unwrap();
        let object = value.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            [
                "champion_id",
                "champion_name",
                "golden_boot_goals",
                "golden_boot_player",
                "league_name",
                "poty_player",
                "poty_rating",
                "season",
                "total_teams",
                "user_lost",
                "user_maps_lost",
                "user_maps_won",
                "user_points",
                "user_position",
                "user_won",
            ]
        );
    }
}

fn contract_days_remaining(
    contract_end: Option<&str>,
    current_date: chrono::NaiveDate,
) -> Option<i64> {
    let end_str = contract_end?;
    let end_date = chrono::NaiveDate::parse_from_str(end_str, "%Y-%m-%d").ok()?;
    Some((end_date - current_date).num_days())
}

/// Renew contracts for all players who finished the season with a team.
/// Players without contracts or with expired contracts get renewed automatically
/// based on their importance and market value.
fn renew_contracts_for_retained_players(game: &mut Game) {
    let current_date = game.clock.current_date.date_naive();
    let next_season_year = current_date.year() + 1;

    for player in game.players.iter_mut() {
        let Some(team_id) = &player.team_id else {
            continue;
        };
        let Some(team) = game
            .teams
            .iter()
            .find(|t| t.id.as_str() == team_id.as_str())
        else {
            continue;
        };

        let needs_renewal = player.contract_end.is_none()
            || contract_days_remaining(player.contract_end.as_deref(), current_date)
                .is_some_and(|days| days <= 0);

        if !needs_renewal {
            continue;
        }

        let base_years = if player.market_value >= 250_000 || player.morale >= 75 {
            2
        } else {
            1
        };

        let wage = crate::contracts::expected_wage(player, team, current_date);
        let contract_end = current_date
            .checked_add_months(chrono::Months::new(base_years * 12))
            .unwrap_or_else(|| {
                chrono::NaiveDate::from_ymd_opt(next_season_year + base_years as i32, 11, 30)
                    .unwrap()
            });

        player.contract_end = Some(contract_end.format("%Y-%m-%d").to_string());
        player.wage = wage;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::generator::definitions::{CompetitionManifest, SeasonStart, SplitConfig};

    fn make_team(id: &str) -> crate::domain::team::Team {
        let team = crate::domain::team::Team::new(
            id.to_string(),
            format!("Team {id}"),
            id.to_string(),
            "ES".to_string(),
            "Test League".to_string(),
            "Test Arena".to_string(),
            1000,
        );
        team
    }

    fn make_completed_fixture(
        id: &str,
        matchday: u32,
        date: &str,
        home: &str,
        away: &str,
    ) -> crate::domain::league::Fixture {
        crate::domain::league::Fixture {
            id: id.to_string(),
            matchday,
            date: date.to_string(),
            home_team_id: home.to_string(),
            away_team_id: away.to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Completed,
            result: Some(crate::domain::league::MatchResult {
                home_wins: 2,
                away_wins: 0,
                ..Default::default()
            }),
        }
    }

    fn sample_schedule_config() -> CompetitionManifest {
        CompetitionManifest {
            id: "lck".to_string(),
            name: "LCK".to_string(),
            region: "KR".to_string(),
            schedule: crate::generator::definitions::ScheduleConfig {
                format: "single_round_robin".to_string(),
                team_count: 2,
                splits: vec![SplitConfig {
                    name: "Spring".to_string(),
                    season_start: SeasonStart { month: 1, day: 15 },
                    superweek_offsets: vec![0],
                    best_of: 1,
                    playoffs: None,
                }],
                preseason_friendlies: 0,
            },
            teams_file: "teams.json".to_string(),
            players_file: "players.json".to_string(),
            staff_file: None,
            championships_file: None,
            full_name: None,
            country: None,
            tier: None,
            logo: None,
            erls: vec![],
            reputation: None,
            nearby_country_codes: vec![],
            legacy: false,
            active: true,
        }
    }

    /// Create a game with an active league (index 0) and a bg league (index 1).
    /// The bg league can be set to complete or incomplete.
    fn bg_eos_test_game(bg_complete: bool) -> Game {
        let clock = GameClock::new(chrono::Utc.with_ymd_and_hms(2025, 6, 1, 12, 0, 0).unwrap());
        let manager = crate::domain::manager::Manager::new(
            "mgr".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "ES".to_string(),
        );
        let teams = vec![
            make_team("team_a"),
            make_team("team_b"),
            make_team("team_c"),
            make_team("team_d"),
        ];

        let mut game = crate::game::Game::new(clock, manager, teams, vec![], vec![], vec![]);

        // Active league (index 0) — single fixture, not completed
        let active_league = League::new(
            "active".to_string(),
            "Active League".to_string(),
            2025,
            &["team_a".to_string(), "team_b".to_string()],
            None,
        );
        game.leagues.push(active_league);

        // BG league (index 1) — 2 teams, 2 required fixtures for double round-robin
        let bg_league = League::new(
            "bg".to_string(),
            "BG League".to_string(),
            2025,
            &["team_c".to_string(), "team_d".to_string()],
            Some("lck".to_string()),
        );
        game.leagues.push(bg_league);

        if bg_complete {
            // Add 2 completed fixtures and update standings so season_has_started is true
            if let Some(league) = game.leagues.get_mut(1) {
                league.fixtures.push(make_completed_fixture(
                    "bg-fix-1",
                    1,
                    "2025-01-15",
                    "team_c",
                    "team_d",
                ));
                league.fixtures.push(make_completed_fixture(
                    "bg-fix-2",
                    2,
                    "2025-01-22",
                    "team_d",
                    "team_c",
                ));
                // Mark standings as having played (so season_has_started returns true)
                for entry in league.standings.iter_mut() {
                    entry.played = 10; // Any positive value works
                    entry.won = 5;
                    entry.lost = 5;
                    entry.maps_won = 15;
                    entry.maps_lost = 10;
                    entry.points = 15;
                }
            }
        }

        game
    }

    #[test]
    fn test_bg_eos_complete_league_cycles() {
        let mut game = bg_eos_test_game(true);
        let mut configs = HashMap::new();
        configs.insert("lck".to_string(), sample_schedule_config());

        let team_c_history_before = game
            .teams
            .iter()
            .find(|t| t.id == "team_c")
            .map(|t| t.history.len())
            .unwrap_or(0);
        let team_d_history_before = game
            .teams
            .iter()
            .find(|t| t.id == "team_d")
            .map(|t| t.history.len())
            .unwrap_or(0);

        process_background_seasons(&mut game, &configs);

        // TeamSeasonRecord should be pushed for both teams
        let team_c_history_after = game
            .teams
            .iter()
            .find(|t| t.id == "team_c")
            .map(|t| t.history.len())
            .unwrap_or(0);
        let team_d_history_after = game
            .teams
            .iter()
            .find(|t| t.id == "team_d")
            .map(|t| t.history.len())
            .unwrap_or(0);
        assert_eq!(team_c_history_after, team_c_history_before + 1);
        assert_eq!(team_d_history_after, team_d_history_before + 1);

        // BG league should have new fixtures (regenerated) — at minimum the league was replaced
        // The new league should have fixtures (even if empty for single round robin with 2 teams = 1 round)
        assert!(!game.leagues[1].fixtures.is_empty() || game.leagues[1].season > 2025);
        // Season should have been incremented
        assert_eq!(game.leagues[1].season, 2026);

        // Active league (index 0) should be untouched
        assert_eq!(game.leagues[0].season, 2025);
        assert!(game.leagues[0].fixtures.is_empty());
    }

    #[test]
    fn test_bg_eos_incomplete_league_skipped() {
        let mut game = bg_eos_test_game(false);
        let mut configs = HashMap::new();
        configs.insert("lck".to_string(), sample_schedule_config());

        let team_c_history_before = game
            .teams
            .iter()
            .find(|t| t.id == "team_c")
            .map(|t| t.history.len())
            .unwrap_or(0);

        process_background_seasons(&mut game, &configs);

        // No history should have been recorded
        let team_c_history_after = game
            .teams
            .iter()
            .find(|t| t.id == "team_c")
            .map(|t| t.history.len())
            .unwrap_or(0);
        assert_eq!(team_c_history_after, team_c_history_before);

        // BG league should be unchanged (no fixtures regenerated)
        assert!(game.leagues[1].fixtures.is_empty());
        assert_eq!(game.leagues[1].season, 2025);
    }

    #[test]
    fn test_bg_eos_none_competition_id_skips_regen() {
        let mut game = bg_eos_test_game(true);
        // Override competition_id to None
        game.leagues[1].competition_id = None;

        let mut configs = HashMap::new();
        configs.insert("lck".to_string(), sample_schedule_config());

        let team_c_history_before = game
            .teams
            .iter()
            .find(|t| t.id == "team_c")
            .map(|t| t.history.len())
            .unwrap_or(0);

        process_background_seasons(&mut game, &configs);

        // History IS still recorded
        let team_c_history_after = game
            .teams
            .iter()
            .find(|t| t.id == "team_c")
            .map(|t| t.history.len())
            .unwrap_or(0);
        assert_eq!(team_c_history_after, team_c_history_before + 1);

        // But no schedule regeneration — fixtures should remain unchanged (the original ones)
        // The bg league had fixtures pushed, they stay because competition_id is None
        // Actually, process_background_seasons only replaces league[1] if competition_id is Some
        // and the config exists. Since it's None, the league is not replaced.
        // The fixtures stay as they were (both completed).
        assert_eq!(game.leagues[1].fixtures.len(), 2);
        assert_eq!(game.leagues[1].season, 2025); // season does NOT increment
    }

    #[test]
    fn test_bg_eos_active_league_untouched() {
        let mut game = bg_eos_test_game(true);
        let configs = HashMap::new(); // empty configs — bg can't regen anyway

        process_background_seasons(&mut game, &configs);

        // Active league (index 0) should be completely unchanged
        assert_eq!(game.leagues[0].season, 2025);
        assert!(game.leagues[0].fixtures.is_empty());
        assert_eq!(game.leagues[0].id, "active");
    }
}
