use crate::domain::player::Player;
use crate::game::Game;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

/// A single award entry (player + stat value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwardEntry {
    pub player_id: String,
    pub player_name: String,
    pub team_id: String,
    pub team_name: String,
    pub value: f64,
}

/// Season award standings — top 5 in each category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonAwards {
    pub golden_boot: Vec<AwardEntry>,    // Top scorers
    pub assist_king: Vec<AwardEntry>,    // Top assists
    pub player_of_year: Vec<AwardEntry>, // Best avg rating (min 5 apps)

    pub most_appearances: Vec<AwardEntry>,
    pub young_player: Vec<AwardEntry>, // Best avg rating, age <= 21
}

struct PlayerAwardContext<'a> {
    player: &'a Player,
    team_id: String,
    team_name: String,
    age: i32,
}

fn player_age_on(today: &NaiveDate, date_of_birth: &str) -> i32 {
    if let Ok(dob) = NaiveDate::parse_from_str(date_of_birth, "%Y-%m-%d") {
        let mut age = today.year() - dob.year();
        if today.ordinal() < dob.ordinal() {
            age -= 1;
        }
        age
    } else {
        30
    }
}

fn resolve_team_info(game: &Game, player: &Player) -> (String, String) {
    let team_name = player
        .team_id
        .as_ref()
        .and_then(|team_id| game.teams.iter().find(|team| &team.id == team_id))
        .map(|team| team.name.clone())
        .unwrap_or_else(|| "Free Agent".to_string());
    let team_id = player.team_id.clone().unwrap_or_default();

    (team_id, team_name)
}

fn build_player_award_contexts(game: &Game) -> Vec<PlayerAwardContext<'_>> {
    let today = game.clock.current_date.date_naive();

    game.players
        .iter()
        .filter(|player| player.stats.appearances > 0)
        .map(|player| {
            let (team_id, team_name) = resolve_team_info(game, player);

            PlayerAwardContext {
                player,
                team_id,
                team_name,
                age: player_age_on(&today, &player.date_of_birth),
            }
        })
        .collect()
}

fn award_entry<'a>(context: &PlayerAwardContext<'a>, value: f64) -> AwardEntry {
    AwardEntry {
        player_id: context.player.id.clone(),
        player_name: context.player.match_name.clone(),
        team_id: context.team_id.clone(),
        team_name: context.team_name.clone(),
        value,
    }
}

fn top_awards<'a, F, G>(
    contexts: &[PlayerAwardContext<'a>],
    include: F,
    value: G,
) -> Vec<AwardEntry>
where
    F: Fn(&PlayerAwardContext<'a>) -> bool,
    G: Fn(&PlayerAwardContext<'a>) -> f64,
{
    let mut awards: Vec<_> = contexts
        .iter()
        .filter_map(|context| include(context).then(|| award_entry(context, value(context))))
        .collect();

    awards.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    awards.truncate(5);
    awards
}

/// Compute current season award standings from player stats.
pub fn compute_season_awards(game: &Game) -> SeasonAwards {
    let contexts = build_player_award_contexts(game);

    // Golden Boot — top scorers
    let golden_boot = top_awards(
        &contexts,
        |context| context.player.stats.kills > 0,
        |context| context.player.stats.kills as f64,
    );

    // Assist King
    let assist_king = top_awards(
        &contexts,
        |context| context.player.stats.assists > 0,
        |context| context.player.stats.assists as f64,
    );

    // Player of the Year — best avg rating, min 5 appearances
    let player_of_year = top_awards(
        &contexts,
        |context| context.player.stats.appearances >= 5 && context.player.stats.avg_rating > 0.0,
        |context| context.player.stats.avg_rating as f64,
    );

    // Most Appearances
    let most_appearances = top_awards(
        &contexts,
        |_| true,
        |context| context.player.stats.appearances as f64,
    );

    // Young Player of the Year — age <= 21, best avg rating, min 3 apps
    let young_player = top_awards(
        &contexts,
        |context| {
            context.age <= 21
                && context.player.stats.appearances >= 3
                && context.player.stats.avg_rating > 0.0
        },
        |context| context.player.stats.avg_rating as f64,
    );

    SeasonAwards {
        golden_boot,
        assist_king,
        player_of_year,
        most_appearances,
        young_player,
    }
}

#[cfg(test)]
mod tests {
    use super::compute_season_awards;
    use crate::domain::manager::Manager;
    use crate::domain::player::{LolRole, Player, PlayerAttributes, PlayerSeasonStats};
    use chrono::{TimeZone, Utc};

    use crate::domain::team::Team;

    use crate::clock::GameClock;
    use crate::game::Game;

    fn default_attrs() -> PlayerAttributes {
        PlayerAttributes {
            mechanics: 60,
            laning: 60,
            teamfighting: 60,
            macro_play: 60,
            consistency: 60,
            shotcalling: 60,
            champion_pool: 60,
            discipline: 60,
            mental_resilience: 60,
        }
    }

    fn make_team(id: &str, name: &str) -> Team {
        Team::new(
            id.to_string(),
            name.to_string(),
            name.to_string(),
            "England".to_string(),
            "Testville".to_string(),
            "Test Ground".to_string(),
            20_000,
        )
    }

    fn make_player(
        id: &str,
        name: &str,
        team_id: Option<&str>,
        role: LolRole,
        dob: &str,
        stats: PlayerSeasonStats,
    ) -> Player {
        let mut player = Player::new(
            id.to_string(),
            name.to_string(),
            name.to_string(),
            dob.to_string(),
            "England".to_string(),
            role,
            default_attrs(),
        );
        player.team_id = team_id.map(str::to_string);
        player.stats = stats;
        player
    }

    fn make_game(players: Vec<Player>, teams: Vec<Team>) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "mgr1".to_string(),
            "Alex".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );

        Game::new(clock, manager, teams, players, vec![], vec![])
    }

    #[test]
    fn golden_boot_is_sorted_descending_and_limited_to_top_five() {
        let team = make_team("team1", "Test FC");
        let mut players = vec![
            make_player(
                "p1",
                "Player 1",
                Some("team1"),
                LolRole::Adc,
                "2000-01-01",
                PlayerSeasonStats {
                    appearances: 8,
                    kills: 4,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "p2",
                "Player 2",
                Some("team1"),
                LolRole::Adc,
                "2000-01-01",
                PlayerSeasonStats {
                    appearances: 8,
                    kills: 6,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "p3",
                "Player 3",
                Some("team1"),
                LolRole::Adc,
                "2000-01-01",
                PlayerSeasonStats {
                    appearances: 8,
                    kills: 1,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "p4",
                "Player 4",
                Some("team1"),
                LolRole::Adc,
                "2000-01-01",
                PlayerSeasonStats {
                    appearances: 8,
                    kills: 5,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "p5",
                "Player 5",
                Some("team1"),
                LolRole::Adc,
                "2000-01-01",
                PlayerSeasonStats {
                    appearances: 8,
                    kills: 2,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "p6",
                "Player 6",
                Some("team1"),
                LolRole::Adc,
                "2000-01-01",
                PlayerSeasonStats {
                    appearances: 8,
                    kills: 3,
                    ..PlayerSeasonStats::default()
                },
            ),
        ];
        players.push(make_player(
            "p7",
            "Zero Apps",
            Some("team1"),
            LolRole::Adc,
            "2000-01-01",
            PlayerSeasonStats {
                appearances: 0,
                kills: 99,
                ..PlayerSeasonStats::default()
            },
        ));

        let awards = compute_season_awards(&make_game(players, vec![team]));

        let top_ids: Vec<_> = awards
            .golden_boot
            .iter()
            .map(|entry| entry.player_id.as_str())
            .collect();
        assert_eq!(top_ids, vec!["p2", "p4", "p1", "p6", "p5"]);
        assert_eq!(awards.golden_boot.len(), 5);
        assert!(
            awards
                .golden_boot
                .iter()
                .all(|entry| entry.player_name != "Zero Apps")
        );
    }

    #[test]
    fn player_of_year_and_young_player_apply_their_thresholds_and_age_rules() {
        let team = make_team("team1", "Test FC");
        let players = vec![
            make_player(
                "older-star",
                "Older Star",
                Some("team1"),
                LolRole::Mid,
                "2001-02-10",
                PlayerSeasonStats {
                    appearances: 6,
                    avg_rating: 8.4,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "young-eligible",
                "Young Eligible",
                Some("team1"),
                LolRole::Adc,
                "2004-06-15",
                PlayerSeasonStats {
                    appearances: 5,
                    avg_rating: 7.8,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "young-four-apps",
                "Young Four Apps",
                Some("team1"),
                LolRole::Adc,
                "2004-09-10",
                PlayerSeasonStats {
                    appearances: 4,
                    avg_rating: 9.0,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "young-low-apps",
                "Young Low Apps",
                Some("team1"),
                LolRole::Adc,
                "2005-03-10",
                PlayerSeasonStats {
                    appearances: 2,
                    avg_rating: 9.5,
                    ..PlayerSeasonStats::default()
                },
            ),
            make_player(
                "invalid-dob",
                "Invalid DOB",
                Some("team1"),
                LolRole::Mid,
                "unknown",
                PlayerSeasonStats {
                    appearances: 6,
                    avg_rating: 8.1,
                    ..PlayerSeasonStats::default()
                },
            ),
        ];

        let awards = compute_season_awards(&make_game(players, vec![team]));

        let player_of_year_ids: Vec<_> = awards
            .player_of_year
            .iter()
            .map(|entry| entry.player_id.as_str())
            .collect();
        assert_eq!(
            player_of_year_ids,
            vec!["older-star", "invalid-dob", "young-eligible"]
        );

        let young_player_ids: Vec<_> = awards
            .young_player
            .iter()
            .map(|entry| entry.player_id.as_str())
            .collect();
        assert_eq!(young_player_ids, vec!["young-four-apps", "young-eligible"]);
    }
}
