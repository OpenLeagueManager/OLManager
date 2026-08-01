use crate::domain::message::{InboxMessage, MessageCategory, MessageContext, MessagePriority};
use crate::domain::player::Player;
use crate::game::Game;
use std::collections::HashMap;

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub const POTENTIAL_RESEARCH_DURATION_DAYS: u8 = 7;

pub fn active_potential_research_player_id(game: &Game) -> Option<String> {
    game.players
        .iter()
        .find(|player| {
            player
                .potential_research_eta_days
                .is_some_and(|eta| eta > 0)
        })
        .map(|player| player.id.clone())
}

pub fn start_potential_research(game: &mut Game, player_id: &str) -> Result<(), String> {
    if let Some(active_player_id) = active_potential_research_player_id(game)
        && active_player_id != player_id
    {
        return Err("Another player is already being researched".to_string());
    }

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let mut managed_team_ids = std::collections::HashSet::new();
    managed_team_ids.insert(manager_team_id.clone());
    let parent_academy_id = game
        .teams
        .iter()
        .find(|candidate| candidate.id == manager_team_id)
        .and_then(|parent| parent.academy_team_id.as_deref());
    for team in &game.teams {
        if team.team_kind == crate::domain::team::TeamKind::Academy
            && (team.parent_team_id.as_deref() == Some(manager_team_id.as_str())
                || parent_academy_id == Some(team.id.as_str()))
        {
            managed_team_ids.insert(team.id.clone());
        }
    }

    let player = game
        .players
        .iter_mut()
        .find(|candidate| candidate.id == player_id)
        .ok_or_else(|| format!("Player not found: {}", player_id))?;

    if !player
        .team_id
        .as_ref()
        .map(|team_id| managed_team_ids.contains(team_id))
        .unwrap_or(false)
    {
        return Err("Player does not belong to manager team".to_string());
    }

    if player.potential_revealed.is_some() {
        return Err("Player potential is already revealed".to_string());
    }

    if player
        .potential_research_eta_days
        .is_some_and(|days| days > 0)
    {
        return Err("Potential research already active for this player".to_string());
    }

    player.potential_research_started_on =
        Some(game.clock.current_date.format("%Y-%m-%d").to_string());
    player.potential_research_eta_days = Some(POTENTIAL_RESEARCH_DURATION_DAYS);
    Ok(())
}

pub fn process_potential_research(game: &mut Game) {
    let mut completed_player_ids: Vec<String> = Vec::new();
    let team_morale = team_average_morale_by_team(game);

    for player in game.players.iter_mut() {
        let Some(days_remaining) = player.potential_research_eta_days else {
            continue;
        };

        if days_remaining == 0 {
            continue;
        }

        let next_days = days_remaining.saturating_sub(1);
        if next_days > 0 {
            player.potential_research_eta_days = Some(next_days);
            continue;
        }

        let avg_team_morale = player
            .team_id
            .as_ref()
            .and_then(|team_id| team_morale.get(team_id))
            .copied()
            .unwrap_or(0);
        let reveal_value = compute_revealed_potential(player, avg_team_morale);
        player.potential_revealed = Some(reveal_value);
        player.potential_research_eta_days = None;
        player.potential_research_started_on = None;
        completed_player_ids.push(player.id.clone());
    }

    for player_id in completed_player_ids {
        let Some(player) = game
            .players
            .iter()
            .find(|candidate| candidate.id == player_id)
        else {
            continue;
        };

        let date = game.clock.current_date.format("%Y-%m-%d").to_string();
        let revealed = player.potential_revealed.unwrap_or(player.potential_base);
        let message = InboxMessage::new(
            format!("potential-research-{}-{}", player.id, date),
            "Potential report completed".to_string(),
            format!(
                "Scouting finished evaluating {}. New potential estimate: {}.",
                player.match_name, revealed
            ),
            "Performance Staff".to_string(),
            date,
        )
        .with_sender_role("Staff")
        .with_category(MessageCategory::Training)
        .with_priority(MessagePriority::Normal)
        .with_i18n(
            "be.msg.potentialReport.subject",
            "be.msg.potentialReport.body",
            params(&[
                ("player", &player.match_name),
                ("potential", &revealed.to_string()),
            ]),
        )
        .with_sender_i18n("be.sender.performanceStaff", "be.role.performanceStaff")
        .with_context(MessageContext {
            player_id: Some(player.id.clone()),
            team_id: player.team_id.clone(),
            ..MessageContext::default()
        });

        game.messages.push(message);
    }
}

pub fn effective_potential_cap(player: &Player) -> u8 {
    player
        .potential_revealed
        .unwrap_or(player.potential_base)
        .min(99)
}

pub fn calculate_lol_ovr(player: &Player) -> u8 {
    let attrs = &player.attributes;
    let avg = (attrs.mechanics as f64
        + attrs.laning as f64
        + attrs.teamfighting as f64
        + attrs.macro_play as f64
        + attrs.consistency as f64
        + attrs.shotcalling as f64
        + attrs.champion_pool as f64
        + attrs.discipline as f64
        + attrs.mental_resilience as f64)
        / 9.0;
    avg.round().clamp(1.0, 99.0) as u8
}

fn compute_revealed_potential(player: &Player, team_average_morale: u8) -> u8 {
    let mut bonus = 0u8;
    let current_ovr = calculate_lol_ovr(player);

    // OVR stability proxy: player's current LoL OVR remains close to base potential.
    if current_ovr.saturating_add(2) >= player.potential_base {
        bonus += 1;
    }

    if player.morale >= 70 {
        bonus += 1;
    }

    if team_average_morale >= 70 {
        bonus += 1;
    }

    player.potential_base.saturating_add(bonus.min(3)).min(99)
}

fn team_average_morale_by_team(game: &Game) -> HashMap<String, u8> {
    let mut sums: HashMap<String, (u32, u32)> = HashMap::new();

    for player in &game.players {
        let Some(team_id) = &player.team_id else {
            continue;
        };
        let entry = sums.entry(team_id.clone()).or_insert((0, 0));
        entry.0 += player.morale as u32;
        entry.1 += 1;
    }

    sums.into_iter()
        .map(|(team_id, (sum, count))| (team_id, if count == 0 { 0 } else { (sum / count) as u8 }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        POTENTIAL_RESEARCH_DURATION_DAYS, active_potential_research_player_id, calculate_lol_ovr,
        effective_potential_cap, process_potential_research, start_potential_research,
    };
    use crate::clock::GameClock;
    use crate::domain::manager::Manager;
    use crate::domain::player::{LolRole, Player, PlayerAttributes};
    use crate::domain::team::Team;
    use crate::game::Game;
    use chrono::{TimeZone, Utc};

    fn attrs(stat: u8) -> PlayerAttributes {
        PlayerAttributes {
            mechanics: stat,
            laning: stat,
            teamfighting: stat,
            macro_play: stat,
            consistency: stat,
            shotcalling: stat,
            champion_pool: stat,
            discipline: stat,
            mental_resilience: stat,
        }
    }

    fn make_player(id: &str, name: &str, team_id: &str, stat: u8, potential: u8) -> Player {
        let mut player = Player::new(
            id.to_string(),
            name.to_string(),
            name.to_string(),
            "2001-01-01".to_string(),
            "GB".to_string(),
            LolRole::Mid,
            attrs(stat),
        );
        player.team_id = Some(team_id.to_string());
        player.morale = 80;
        player.potential_base = potential;
        player
    }

    fn make_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr-1".to_string(),
            "Ada".to_string(),
            "Coach".to_string(),
            "1980-01-01".to_string(),
            "GB".to_string(),
        );
        manager.hire("team-1".to_string());
        let team = Team::new(
            "team-1".to_string(),
            "Team One".to_string(),
            "ONE".to_string(),
            "England".to_string(),
            "London".to_string(),
            "Arena".to_string(),
            30000,
        );

        let p1 = make_player("p1", "Player One", "team-1", 82, 84);
        let p2 = make_player("p2", "Player Two", "team-1", 78, 82);
        Game::new(clock, manager, vec![team], vec![p1, p2], vec![], vec![])
    }

    #[test]
    fn only_one_active_research_allowed_globally() {
        let mut game = make_game();
        start_potential_research(&mut game, "p1").expect("first research should start");

        let error = start_potential_research(&mut game, "p2")
            .expect_err("second research must fail while first is active");
        assert!(error.contains("already being researched"));
        assert_eq!(
            active_potential_research_player_id(&game),
            Some("p1".to_string())
        );
    }

    #[test]
    fn completion_after_seven_days_reveals_and_clears_state() {
        let mut game = make_game();
        start_potential_research(&mut game, "p1").expect("research should start");
        let player = game.players.iter().find(|p| p.id == "p1").unwrap();
        assert_eq!(
            player.potential_research_eta_days,
            Some(POTENTIAL_RESEARCH_DURATION_DAYS)
        );

        for _ in 0..POTENTIAL_RESEARCH_DURATION_DAYS {
            process_potential_research(&mut game);
        }

        let player = game.players.iter().find(|p| p.id == "p1").unwrap();
        assert!(player.potential_revealed.is_some());
        assert_eq!(player.potential_research_eta_days, None);
        assert_eq!(player.potential_research_started_on, None);
        assert!(player.potential_revealed.unwrap() >= player.potential_base);
        assert!(player.potential_revealed.unwrap() <= player.potential_base + 3);
    }

    #[test]
    fn effective_cap_matches_lol_ovr_for_cap_enforcement() {
        let mut player = make_player("p-cap", "Cap", "team-1", 90, 90);
        assert_eq!(calculate_lol_ovr(&player), 90);
        assert_eq!(effective_potential_cap(&player), 90);

        player.potential_revealed = Some(92);
        assert_eq!(effective_potential_cap(&player), 92);
    }

    #[test]
    fn allows_potential_research_for_managed_academy_player() {
        let mut game = make_game();
        let mut academy_team = Team::new(
            "academy-1".to_string(),
            "Academy One".to_string(),
            "AC1".to_string(),
            "England".to_string(),
            "London".to_string(),
            "Academy Arena".to_string(),
            12000,
        );
        academy_team.team_kind = crate::domain::team::TeamKind::Academy;
        academy_team.parent_team_id = Some("team-1".to_string());

        let academy_player = make_player("p-academy", "Academy Player", "academy-1", 74, 86);

        game.teams.push(academy_team);
        game.players.push(academy_player);

        start_potential_research(&mut game, "p-academy")
            .expect("academy player potential research should start");

        let player = game
            .players
            .iter()
            .find(|candidate| candidate.id == "p-academy")
            .unwrap();
        assert_eq!(
            player.potential_research_eta_days,
            Some(POTENTIAL_RESEARCH_DURATION_DAYS)
        );
    }
}
