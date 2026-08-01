use crate::domain::player::LolRole as DomainLolRole;
use crate::engine::{DraftStrategy, LolRole, PlayerData, TeamData};
use crate::game::Game;
use crate::potential::calculate_lol_ovr;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Domain → Engine conversion (LoL: 5 titulares + banca)
// ---------------------------------------------------------------------------

/// Convert crate::domain::player::LolRole to crate::engine::LolRole
fn to_engine_role(role: DomainLolRole) -> LolRole {
    match role {
        DomainLolRole::Top => LolRole::Top,
        DomainLolRole::Jungle => LolRole::Jungle,
        DomainLolRole::Mid => LolRole::Mid,
        DomainLolRole::Adc => LolRole::Adc,
        DomainLolRole::Support => LolRole::Support,
        DomainLolRole::Unknown => LolRole::Top,
    }
}

pub(super) fn build_team_with_bench(game: &Game, team_id: &str) -> (TeamData, Vec<PlayerData>) {
    let team = game.teams.iter().find(|t| t.id == team_id);
    let (name, draft_strategy) = match team {
        Some(t) => (
            t.name.clone(),
            match t.draft_strategy {
                crate::domain::team::DraftStrategy::Aggressive => DraftStrategy::Aggressive,
                crate::domain::team::DraftStrategy::Passive => DraftStrategy::Passive,
                crate::domain::team::DraftStrategy::Scaling => DraftStrategy::Scaling,
                crate::domain::team::DraftStrategy::CounterPick => DraftStrategy::CounterPick,
                crate::domain::team::DraftStrategy::PriorityBans => DraftStrategy::PriorityBans,
                _ => DraftStrategy::Balanced,
            },
        ),
        None => ("Unknown".into(), DraftStrategy::Balanced),
    };

    // Collect all players for this team.
    // NOTE: Do not apply legacy injury filtering — rosters must not drop below 5
    // otherwise UI shows empty player slots.
    let available_players: Vec<&crate::domain::player::Player> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team_id))
        .collect();
    let starters = select_reconciled_lol_starters(team, available_players.as_slice());
    let starter_ids = starters
        .iter()
        .map(|player| player.id.as_str())
        .collect::<HashSet<_>>();
    let bench_domain = available_players
        .into_iter()
        .filter(|player| !starter_ids.contains(player.id.as_str()))
        .collect::<Vec<_>>();

    // Keep LoL lane order stable for draft/pre-match UIs.
    let mut starters = starters;
    starters.sort_by(|left, right| {
        lol_role_rank(&left.natural_position)
            .cmp(&lol_role_rank(&right.natural_position))
            .then_with(|| calculate_lol_ovr(right).cmp(&calculate_lol_ovr(left)))
            .then_with(|| right.condition.cmp(&left.condition))
    });

    let starting_xi = starters
        .into_iter()
        .map(to_engine_player)
        .collect::<Vec<_>>();
    let bench = bench_domain
        .into_iter()
        .map(to_engine_player)
        .collect::<Vec<_>>();

    let team_data = TeamData {
        id: team_id.to_string(),
        name,
        draft_strategy,
        players: starting_xi,
    };

    (team_data, bench)
}

fn select_reconciled_lol_starters<'a>(
    team: Option<&crate::domain::team::Team>,
    available_players: &[&'a crate::domain::player::Player],
) -> Vec<&'a crate::domain::player::Player> {
    const ROLES: [DomainLolRole; 5] = [
        DomainLolRole::Top,
        DomainLolRole::Jungle,
        DomainLolRole::Mid,
        DomainLolRole::Adc,
        DomainLolRole::Support,
    ];

    let saved_ids = team
        .map(|team| team.active_lineup_ids.as_slice())
        .unwrap_or(&[]);
    let mut starters = Vec::with_capacity(ROLES.len());
    let mut used = HashSet::<String>::new();

    for (index, role) in ROLES.iter().enumerate() {
        if let Some(player) = saved_ids
            .get(index)
            .and_then(|id| current_available_player_by_id(available_players, id))
            .filter(|player| !used.contains(&player.id) && player.natural_position == *role)
        {
            used.insert(player.id.clone());
            starters.push(player);
            continue;
        }

        if let Some(player) = saved_ids
            .iter()
            .filter_map(|id| current_available_player_by_id(available_players, id))
            .find(|player| !used.contains(&player.id) && player.natural_position == *role)
        {
            used.insert(player.id.clone());
            starters.push(player);
            continue;
        }

        if let Some(player) = best_available_player_for_role(available_players, *role, &used) {
            used.insert(player.id.clone());
            starters.push(player);
        }
    }

    if starters.len() < ROLES.len() {
        let mut fallback_players = available_players
            .iter()
            .copied()
            .filter(|player| !used.contains(&player.id))
            .collect::<Vec<_>>();
        fallback_players.sort_by(|left, right| {
            calculate_lol_ovr(right)
                .cmp(&calculate_lol_ovr(left))
                .then_with(|| right.condition.cmp(&left.condition))
        });

        for player in fallback_players {
            used.insert(player.id.clone());
            starters.push(player);
            if starters.len() == ROLES.len() {
                break;
            }
        }
    }

    starters
}

fn current_available_player_by_id<'a>(
    available_players: &[&'a crate::domain::player::Player],
    player_id: &str,
) -> Option<&'a crate::domain::player::Player> {
    if player_id.is_empty() {
        return None;
    }

    available_players
        .iter()
        .copied()
        .find(|player| player.id == player_id)
}

fn best_available_player_for_role<'a>(
    available_players: &[&'a crate::domain::player::Player],
    role: DomainLolRole,
    used: &HashSet<String>,
) -> Option<&'a crate::domain::player::Player> {
    available_players
        .iter()
        .copied()
        .filter(|player| player.natural_position == role && !used.contains(&player.id))
        .max_by_key(|player| {
            (
                calculate_lol_ovr(player),
                player.condition,
                player.market_value,
            )
        })
}

fn to_engine_player(p: &crate::domain::player::Player) -> PlayerData {
    PlayerData {
        id: p.id.clone(),
        name: p.match_name.clone(),
        profile_image_url: p.profile_image_url.clone(),
        role: to_engine_role(p.natural_position),
        condition: p.condition,
        fitness: p.fitness,
        // Map domain attributes to LoL-native engine structure
        mechanics: p.attributes.mechanics,
        laning: p.attributes.laning,
        teamfighting: p.attributes.teamfighting,
        macro_play: p.attributes.macro_play,
        consistency: p.attributes.consistency,
        shotcalling: p.attributes.shotcalling,
        champion_pool: p.attributes.champion_pool,
        discipline: p.attributes.discipline,
        mental_resilience: p.attributes.mental_resilience,
        traits: p.traits.iter().map(|t| format!("{:?}", t)).collect(),
    }
}

fn lol_role_rank(role: &DomainLolRole) -> u8 {
    match role {
        DomainLolRole::Top => 0,
        DomainLolRole::Jungle => 1,
        DomainLolRole::Mid => 2,
        DomainLolRole::Adc => 3,
        DomainLolRole::Support => 4,
        DomainLolRole::Unknown => 5,
    }
}

/// Auto-select team roles from a set of player IDs.
/// Returns (captain_id, shotcaller_id).
pub fn auto_select_team_roles(
    game: &Game,
    player_ids: &[String],
) -> (Option<String>, Option<String>) {
    let players: Vec<&crate::domain::player::Player> = player_ids
        .iter()
        .filter_map(|id| game.players.iter().find(|p| &p.id == id))
        .collect();

    if players.is_empty() {
        return (None, None);
    }

    // Captain: highest leadership + teamwork
    let captain = players
        .iter()
        .max_by_key(|p| (p.attributes.shotcalling as u16) + (p.attributes.teamfighting as u16))
        .map(|p| p.id.clone());

    // Shotcaller: highest laning + macro_play + coordination (exclude Support)
    let shotcaller = players
        .iter()
        .filter(|p| p.position != DomainLolRole::Support)
        .max_by_key(|p| {
            (p.attributes.laning as u16)
                + (p.attributes.macro_play as u16)
                + (p.attributes.teamfighting as u16)
        })
        .map(|p| p.id.clone());

    (captain, shotcaller)
}

#[cfg(test)]
mod tests {
    use super::build_team_with_bench;
    use crate::clock::GameClock;
    use crate::domain::manager::Manager;
    use crate::domain::player::{LolRole, Player, PlayerAttributes};
    use crate::domain::team::Team;
    use crate::engine::LolRole as EngineLolRole;
    use crate::game::Game;
    use chrono::{TimeZone, Utc};

    fn attrs(value: u8) -> PlayerAttributes {
        PlayerAttributes {
            mechanics: value,
            laning: value,
            teamfighting: value,
            macro_play: value,
            consistency: value,
            shotcalling: value,
            champion_pool: value,
            discipline: value,
            mental_resilience: value,
        }
    }

    fn player(id: &str, role: LolRole, rating: u8) -> Player {
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "2000-01-01".to_string(),
            "Spain".to_string(),
            role,
            attrs(rating),
        );
        player.team_id = Some("opponent".to_string());
        player.profile_image_url = Some(format!("/images/players/{id}.webp"));
        player
    }

    fn game_with_opponent(active_lineup_ids: Vec<&str>, players: Vec<Player>) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 5, 12, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "manager".to_string(),
            "Alex".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "Spain".to_string(),
        );
        let mut opponent = Team::new(
            "opponent".to_string(),
            "Opponent".to_string(),
            "OPP".to_string(),
            "Spain".to_string(),
            "Madrid".to_string(),
            "Arena".to_string(),
            10_000,
        );
        opponent.active_lineup_ids = active_lineup_ids.into_iter().map(str::to_string).collect();

        Game::new(clock, manager, vec![opponent], players, vec![], vec![])
    }

    #[test]
    fn pre_match_team_builder_prefers_current_reconciled_active_lineup_over_raw_ovr() {
        let game = game_with_opponent(
            vec!["top", "jungle", "oscar", "adc", "labrov"],
            vec![
                player("top", LolRole::Top, 70),
                player("jungle", LolRole::Jungle, 70),
                player("oscar", LolRole::Mid, 55),
                player("other-mid", LolRole::Mid, 95),
                player("adc", LolRole::Adc, 70),
                player("labrov", LolRole::Support, 70),
            ],
        );

        let (team, bench) = build_team_with_bench(&game, "opponent");

        assert_eq!(
            team.players
                .iter()
                .map(|player| player.id.as_str())
                .collect::<Vec<_>>(),
            vec!["top", "jungle", "oscar", "adc", "labrov"]
        );
        assert!(bench.iter().any(|player| player.id == "other-mid"));
    }

    #[test]
    fn pre_match_team_builder_replaces_duplicate_or_stale_lineup_slots_from_current_roster() {
        let game = game_with_opponent(
            vec!["top", "jungle", "labrov", "sold-adc", "labrov"],
            vec![
                player("top", LolRole::Top, 70),
                player("jungle", LolRole::Jungle, 70),
                player("oscar", LolRole::Mid, 85),
                player("adc", LolRole::Adc, 70),
                player("labrov", LolRole::Support, 90),
            ],
        );

        let (team, bench) = build_team_with_bench(&game, "opponent");

        assert_eq!(
            team.players
                .iter()
                .map(|player| (&player.id, player.role))
                .collect::<Vec<_>>(),
            vec![
                (&"top".to_string(), EngineLolRole::Top),
                (&"jungle".to_string(), EngineLolRole::Jungle),
                (&"oscar".to_string(), EngineLolRole::Mid),
                (&"adc".to_string(), EngineLolRole::Adc),
                (&"labrov".to_string(), EngineLolRole::Support),
            ]
        );
        assert_eq!(
            team.players
                .iter()
                .map(|player| (player.id.as_str(), player.profile_image_url.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                ("top", Some("/images/players/top.webp")),
                ("jungle", Some("/images/players/jungle.webp")),
                ("oscar", Some("/images/players/oscar.webp")),
                ("adc", Some("/images/players/adc.webp")),
                ("labrov", Some("/images/players/labrov.webp")),
            ]
        );
        assert_eq!(
            team.players
                .iter()
                .filter(|player| player.id == "labrov")
                .count(),
            1
        );
        assert!(!bench.iter().any(|player| player.id == "oscar"));
    }
}
