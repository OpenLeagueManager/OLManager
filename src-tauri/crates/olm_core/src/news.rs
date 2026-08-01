mod match_report;
pub mod template_store;
pub use match_report::match_report_article;

use crate::domain::news::*;
use rand::{Rng, RngExt};
use std::collections::HashMap;

/// Helper to build a HashMap<String, String> from key-value pairs.
fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn result_lines(results: &[(String, u8, String, u8)]) -> Vec<String> {
    results
        .iter()
        .map(|(home, hg, away, ag)| format!("  {} {} - {} {}", home, hg, ag, away))
        .collect()
}

fn biggest_winner_name(results: &[(String, u8, String, u8)]) -> String {
    results
        .iter()
        .filter(|(_, hg, _, ag)| hg != ag)
        .max_by_key(|(_, hg, _, ag)| (*hg as i8 - *ag as i8).unsigned_abs())
        .map(
            |(home, hg, away, ag)| {
                if hg > ag { home.clone() } else { away.clone() }
            },
        )
        .unwrap_or_default()
}

fn goal_difference_text(goal_difference: i16) -> String {
    if goal_difference >= 0 {
        format!("+{}", goal_difference)
    } else {
        goal_difference.to_string()
    }
}

fn standings_lines(top_teams: &[(String, u32, i16)]) -> Vec<String> {
    top_teams
        .iter()
        .enumerate()
        .map(|(idx, (name, points, goal_difference))| {
            format!(
                "  {}. {} — {} pts (GD: {})",
                idx + 1,
                name,
                points,
                goal_difference_text(*goal_difference)
            )
        })
        .collect()
}

/// Generate a league roundup article summarising all matchday results.
pub fn league_roundup_article(
    matchday: u32,
    results: &[(String, u8, String, u8)], // (home_name, home_goals, away_name, away_goals)
    date: &str,
) -> NewsArticle {
    let mut rng = rand::rng();
    let results_text = result_lines(results);
    let biggest_winner = biggest_winner_name(results);

    let mut body = format!(
        "Matchday {} is complete. Here are the full series results:\n",
        matchday
    );
    for line in &results_text {
        body.push_str(&format!("\n{}", line));
    }

    let total_maps: u8 = results.iter().map(|(_, hg, _, ag)| hg + ag).sum();
    body.push_str(&format!(
        "\n\n{} maps played across {} series. ",
        total_maps,
        results.len()
    ));

    if !biggest_winner.is_empty() {
        body.push_str(&format!(
            "{} recorded the cleanest closeout of the day.",
            biggest_winner
        ));
    }

    let headlines = [
        format!(
            "Matchday {} Round-Up: {} Maps Across the Rift",
            matchday, total_maps
        ),
        format!("League Matchday {}: All the Series Results", matchday),
        format!("Drafts, Objectives and Maps Define Matchday {}", matchday),
    ];

    let source_keys = [
        "be.source.riftWire",
        "be.source.riftHerald",
        "be.source.leaguePulse",
    ];
    let sources = ["Inven Global", "Riot Games Newsroom", "The Shotcaller"];
    let src_idx = rng.random_range(0..sources.len());
    let headline_idx = rng.random_range(0..headlines.len());

    NewsArticle::new(
        format!("roundup_md{}", matchday),
        headlines[headline_idx].clone(),
        body,
        sources[src_idx].to_string(),
        date.to_string(),
        NewsCategory::LeagueRoundup,
    )
    .with_i18n(
        &format!("be.news.roundup.headline{}", headline_idx),
        "be.news.roundup.body",
        source_keys[src_idx],
        params(&[
            ("matchday", &matchday.to_string()),
            ("totalMaps", &total_maps.to_string()),
            ("matchCount", &results.len().to_string()),
            ("results", &results_text.join("\n")),
            ("biggestWinner", &biggest_winner),
        ]),
    )
}

/// Generate a standings update article after a matchday.
pub fn standings_update_article(
    matchday: u32,
    top_teams: &[(String, u32, i16)], // (team_name, points, goal_diff)
    date: &str,
) -> NewsArticle {
    let mut rng = rand::rng();

    let leader = top_teams
        .first()
        .map(|(n, _, _)| n.as_str())
        .unwrap_or("Unknown");
    let mut body = format!(
        "After Matchday {}, {} sit at the top of the league table.\n\nStandings by map differential:",
        matchday, leader
    );

    let standings_text = standings_lines(top_teams);

    for line in &standings_text {
        body.push_str(&format!("\n{}", line));
    }

    let headlines = [
        format!("{} Lead the Standings After Matchday {}", leader, matchday),
        format!("League Table: {} Control the Top Spot", leader),
        format!("Power Rankings Update — Matchday {}", matchday),
    ];

    let source_keys = [
        "be.source.riftWire",
        "be.source.riftHerald",
        "be.source.leaguePulse",
    ];
    let sources = ["Inven Global", "Riot Games Newsroom", "The Shotcaller"];
    let src_idx = rng.random_range(0..sources.len());
    let headline_idx = rng.random_range(0..headlines.len());

    NewsArticle::new(
        format!("standings_md{}", matchday),
        headlines[headline_idx].clone(),
        body,
        sources[src_idx].to_string(),
        date.to_string(),
        NewsCategory::StandingsUpdate,
    )
    .with_i18n(
        &format!("be.news.standings.headline{}", headline_idx),
        "be.news.standings.body",
        source_keys[src_idx],
        params(&[
            ("matchday", &matchday.to_string()),
            ("leader", leader),
            ("standings", &standings_text.join("\n")),
        ]),
    )
}

/// Generate a season preview article at the start of the season.
///
/// Uses the template-based news system (see `data/news/season_preview/template.json`).
/// Falls back to the legacy inline generation if no template is registered.
pub fn season_preview_article(team_names: &[String], date: &str) -> NewsArticle {
    let mut rng = rand::rng();

    if let Some(tpl) = template_store::NewsTemplateStore::global().get(&NewsCategory::SeasonPreview)
    {
        let (favourite, dark_horse) = preview_contenders(team_names, &mut rng);
        return tpl.build_article(
            "season_preview".to_string(),
            date.to_string(),
            &[
                ("teamCount", &team_names.len().to_string()),
                ("favourite", favourite),
                ("darkHorse", dark_horse),
                ("teamList", &team_names.join(", ")),
            ],
            "en",
            None,
        );
    }

    // ── Legacy fallback ────────────────────────────────────────
    let (favourite, dark_horse) = preview_contenders(team_names, &mut rng);

    let body = format!(
        "The league is set to kick off with {} teams entering the split.\n\n\
        Analyst predictions have {} as the early favourites, but {} could be the dark horse \
        to watch this campaign.\n\n\
        With new coaching staffs refining draft prep, this split promises to be one of the \
        most competitive in recent memory. Every map will matter as the playoff race \
        heats up.\n\n\
        Teams: {}",
        team_names.len(),
        favourite,
        dark_horse,
        team_names.join(", ")
    );

    let headlines = [
        format!(
            "Split Preview: {} Teams Battle for the Top Spot",
            team_names.len()
        ),
        "League Split Set to Begin".to_string(),
        format!("Can {} Control the Meta? Split Preview", favourite),
    ];

    let headline_idx = rng.random_range(0..headlines.len());

    NewsArticle::new(
        "season_preview".to_string(),
        headlines[headline_idx].clone(),
        body,
        "Riot Games Newsroom".to_string(),
        date.to_string(),
        NewsCategory::SeasonPreview,
    )
    .with_i18n(
        &format!("be.news.seasonPreview.headline{}", headline_idx),
        "be.news.seasonPreview.body",
        "be.source.riftHerald",
        params(&[
            ("teamCount", &team_names.len().to_string()),
            ("favourite", favourite),
            ("darkHorse", dark_horse),
            ("teamList", &team_names.join(", ")),
        ]),
    )
}

fn preview_contenders<'a>(team_names: &'a [String], rng: &mut impl Rng) -> (&'a str, &'a str) {
    let favourite = &team_names[rng.random_range(0..team_names.len())];

    if team_names.len() == 1 {
        return (favourite.as_str(), favourite.as_str());
    }

    let dark_horse = loop {
        let pick = &team_names[rng.random_range(0..team_names.len())];
        if pick != favourite {
            break pick;
        }
    };

    (favourite.as_str(), dark_horse.as_str())
}

pub fn major_transfer_article(
    id: &str,
    player_id: &str,
    player_name: &str,
    from_team_id: &str,
    from_team_name: &str,
    to_team_id: &str,
    to_team_name: &str,
    fee: u64,
    date: &str,
) -> NewsArticle {
    let fee_display = if fee >= 1_000_000 {
        format!("€{:.1}M", fee as f64 / 1_000_000.0)
    } else if fee >= 1_000 {
        format!("€{}K", fee / 1_000)
    } else {
        format!("€{}", fee)
    };

    NewsArticle::new(
        id.to_string(),
        format!("{} Complete Move to {}", player_name, to_team_name),
        format!(
            "{} have completed the signing of {} from {} for {}.",
            to_team_name, player_name, from_team_name, fee_display
        ),
        "League Chronicle".to_string(),
        date.to_string(),
        NewsCategory::TransferRumour,
    )
    .with_teams(vec![from_team_id.to_string(), to_team_id.to_string()])
    .with_players(vec![player_id.to_string()])
    .with_i18n(
        "be.news.transferRumour.headline",
        "be.news.transferRumour.body",
        "be.source.leagueChronicle",
        params(&[
            ("player", player_name),
            ("to", to_team_name),
            ("from", from_team_name),
            ("fee", &fee_display),
        ]),
    )
}

pub fn weekly_digest_article(
    id: &str,
    week_start: &str,
    leader: &str,
    top_scorer: &str,
    top_scorer_goals: u32,
    storyline_count: usize,
    date: &str,
) -> NewsArticle {
    let headline = format!("Weekly Digest — Week of {}", week_start);
    let (body, body_key) = if top_scorer.is_empty() {
        (
            format!(
                "The latest weekly power rankings are here. {} lead the table, and {} storyline(s) are shaping the league this week.",
                leader, storyline_count
            ),
            "be.news.weeklyDigest.bodyNoTopPerformer",
        )
    } else {
        (
            format!(
                "The latest weekly power rankings are here. {} lead the table, while {} heads the kill participation charts with {} standout play(s). {} storyline(s) are shaping the league this week.",
                leader, top_scorer, top_scorer_goals, storyline_count
            ),
            "be.news.weeklyDigest.bodyWithTopPerformer",
        )
    };

    NewsArticle::new(
        id.to_string(),
        headline,
        body,
        "The Shotcaller".to_string(),
        date.to_string(),
        NewsCategory::Editorial,
    )
    .with_i18n(
        "be.news.weeklyDigest.headline",
        body_key,
        "be.source.leaguePulse",
        params(&[
            ("weekStart", week_start),
            ("leader", leader),
            ("topPerformer", top_scorer),
            ("topPerformerPlays", &top_scorer_goals.to_string()),
            ("storylineCount", &storyline_count.to_string()),
        ]),
    )
}

pub fn title_race_storyline_article(
    id: &str,
    leader_team_id: &str,
    leader: &str,
    challenger_team_id: &str,
    challenger: &str,
    gap: u32,
    date: &str,
) -> NewsArticle {
    NewsArticle::new(
        id.to_string(),
        format!(
            "Top Spot Race Tightens — {} Lead {} by {} Point(s)",
            leader, challenger, gap
        ),
        format!(
            "{} remain in front, but {} are only {} point(s) behind as the playoff race takes shape.",
            leader, challenger, gap
        ),
        "The Shotcaller".to_string(),
        date.to_string(),
        NewsCategory::Editorial,
    )
    .with_teams(vec![
        leader_team_id.to_string(),
        challenger_team_id.to_string(),
    ])
    .with_i18n(
        "be.news.storyline.titleRace.headline",
        "be.news.storyline.titleRace.body",
        "be.source.leaguePulse",
        params(&[
            ("leader", leader),
            ("challenger", challenger),
            ("gap", &gap.to_string()),
        ]),
    )
}

pub fn unbeaten_streak_storyline_article(
    id: &str,
    team_id: &str,
    team: &str,
    run_length: u32,
    date: &str,
) -> NewsArticle {
    NewsArticle::new(
        id.to_string(),
        format!("{} Extend Series Run to {}", team, run_length),
        format!(
            "{} have gone {} series without a loss and are building real momentum around drafts and objectives.",
            team, run_length
        ),
        "The Shotcaller".to_string(),
        date.to_string(),
        NewsCategory::Editorial,
    )
    .with_teams(vec![team_id.to_string()])
    .with_i18n(
        "be.news.storyline.unbeatenStreak.headline",
        "be.news.storyline.unbeatenStreak.body",
        "be.source.leaguePulse",
        params(&[("team", team), ("runLength", &run_length.to_string())]),
    )
}

#[cfg(test)]
mod tests {
    use super::{league_roundup_article, season_preview_article, standings_update_article};
    use crate::domain::news::NewsCategory;

    fn assert_valid_roundup_source_pair(source: &str, source_key: &str) {
        let valid = [
            ("Inven Global", "be.source.riftWire"),
            ("Riot Games Newsroom", "be.source.riftHerald"),
            ("The Shotcaller", "be.source.leaguePulse"),
        ];

        assert!(
            valid
                .iter()
                .any(|pair| pair.0 == source && pair.1 == source_key)
        );
    }

    fn assert_valid_standings_source_pair(source: &str, source_key: &str) {
        let valid = [
            ("Inven Global", "be.source.riftWire"),
            ("Riot Games Newsroom", "be.source.riftHerald"),
            ("The Shotcaller", "be.source.leaguePulse"),
        ];

        assert!(
            valid
                .iter()
                .any(|pair| pair.0 == source && pair.1 == source_key)
        );
    }

    #[test]
    fn league_roundup_article_includes_results_totals_and_biggest_winner() {
        let results = vec![
            ("Alpha FC".to_string(), 3, "Beta FC".to_string(), 0),
            ("Gamma FC".to_string(), 1, "Delta FC".to_string(), 1),
        ];

        let article = league_roundup_article(4, &results, "2025-08-12");

        assert_eq!(article.id, "roundup_md4");
        assert_eq!(article.category, NewsCategory::LeagueRoundup);
        assert!(article.body.contains("Matchday 4 is complete."));
        assert!(article.body.contains("Alpha FC 3 - 0 Beta FC"));
        assert!(article.body.contains("Gamma FC 1 - 1 Delta FC"));
        assert!(article.body.contains("5 maps played across 2 series."));
        assert!(
            article
                .body
                .contains("Alpha FC recorded the cleanest closeout of the day.")
        );
        assert!(
            [
                "be.news.roundup.headline0",
                "be.news.roundup.headline1",
                "be.news.roundup.headline2"
            ]
            .contains(&article.headline_key.as_deref().unwrap())
        );
        assert_eq!(article.body_key.as_deref(), Some("be.news.roundup.body"));
        assert_valid_roundup_source_pair(&article.source, article.source_key.as_deref().unwrap());
        assert_eq!(article.i18n_params.get("matchday"), Some(&"4".to_string()));
        assert_eq!(article.i18n_params.get("totalMaps"), Some(&"5".to_string()));
        assert_eq!(
            article.i18n_params.get("matchCount"),
            Some(&"2".to_string())
        );
        assert_eq!(
            article.i18n_params.get("results"),
            Some(&"  Alpha FC 3 - 0 Beta FC\n  Gamma FC 1 - 1 Delta FC".to_string())
        );
        assert_eq!(
            article.i18n_params.get("biggestWinner"),
            Some(&"Alpha FC".to_string())
        );
    }

    #[test]
    fn league_roundup_article_leaves_biggest_winner_empty_when_all_matches_are_draws() {
        let results = vec![
            ("Alpha FC".to_string(), 1, "Beta FC".to_string(), 1),
            ("Gamma FC".to_string(), 0, "Delta FC".to_string(), 0),
        ];

        let article = league_roundup_article(5, &results, "2025-08-19");

        assert!(
            !article
                .body
                .contains("recorded the cleanest closeout of the day")
        );
        assert_eq!(
            article.i18n_params.get("biggestWinner"),
            Some(&String::new())
        );
    }

    #[test]
    fn standings_update_article_formats_leader_and_goal_differences() {
        let standings = vec![
            ("Alpha FC".to_string(), 12, 5),
            ("Beta FC".to_string(), 10, 0),
            ("Gamma FC".to_string(), 9, -3),
        ];

        let article = standings_update_article(4, &standings, "2025-08-12");

        assert_eq!(article.id, "standings_md4");
        assert_eq!(article.category, NewsCategory::StandingsUpdate);
        assert!(
            article
                .body
                .contains("After Matchday 4, Alpha FC sit at the top")
        );
        assert!(article.body.contains("1. Alpha FC — 12 pts (GD: +5)"));
        assert!(article.body.contains("2. Beta FC — 10 pts (GD: +0)"));
        assert!(article.body.contains("3. Gamma FC — 9 pts (GD: -3)"));
        assert!(
            [
                "be.news.standings.headline0",
                "be.news.standings.headline1",
                "be.news.standings.headline2"
            ]
            .contains(&article.headline_key.as_deref().unwrap())
        );
        assert_eq!(article.body_key.as_deref(), Some("be.news.standings.body"));
        assert_valid_standings_source_pair(&article.source, article.source_key.as_deref().unwrap());
        assert_eq!(article.i18n_params.get("matchday"), Some(&"4".to_string()));
        assert_eq!(
            article.i18n_params.get("leader"),
            Some(&"Alpha FC".to_string())
        );
        assert_eq!(
            article.i18n_params.get("standings"),
            Some(&"  1. Alpha FC — 12 pts (GD: +5)\n  2. Beta FC — 10 pts (GD: +0)\n  3. Gamma FC — 9 pts (GD: -3)".to_string())
        );
    }

    #[test]
    fn standings_update_article_handles_empty_table_with_unknown_leader() {
        let article = standings_update_article(1, &[], "2025-08-01");

        assert!(
            article
                .body
                .contains("After Matchday 1, Unknown sit at the top")
        );
        assert_eq!(
            article.i18n_params.get("leader"),
            Some(&"Unknown".to_string())
        );
        assert_eq!(article.i18n_params.get("standings"), Some(&String::new()));
    }

    #[test]
    fn season_preview_article_includes_team_list_and_distinct_contenders() {
        let teams = vec![
            "Alpha FC".to_string(),
            "Beta FC".to_string(),
            "Gamma FC".to_string(),
        ];

        let article = season_preview_article(&teams, "2025-08-01");

        assert_eq!(article.id, "season_preview");
        assert_eq!(article.category, NewsCategory::SeasonPreview);
        assert_eq!(article.source, "Riot Games Newsroom");
        assert_eq!(article.source_key.as_deref(), Some("be.source.riftHerald"));
        assert!(
            [
                "be.news.seasonPreview.headline0",
                "be.news.seasonPreview.headline1",
                "be.news.seasonPreview.headline2"
            ]
            .contains(&article.headline_key.as_deref().unwrap())
        );
        assert_eq!(
            article.body_key.as_deref(),
            Some("be.news.seasonPreview.body")
        );
        assert!(article.body.contains("3 teams entering the split"));
        assert!(article.body.contains("Teams: Alpha FC, Beta FC, Gamma FC"));
        assert_eq!(article.i18n_params.get("teamCount"), Some(&"3".to_string()));
        assert_eq!(
            article.i18n_params.get("teamList"),
            Some(&"Alpha FC, Beta FC, Gamma FC".to_string())
        );

        let favourite = article.i18n_params.get("favourite").unwrap();
        let dark_horse = article.i18n_params.get("darkHorse").unwrap();
        assert!(teams.contains(favourite));
        assert!(teams.contains(dark_horse));
        assert_ne!(favourite, dark_horse);
    }

    #[test]
    fn season_preview_article_handles_single_team_without_looping() {
        let teams = vec!["Solo FC".to_string()];

        let article = season_preview_article(&teams, "2025-08-01");

        assert!(article.body.contains("1 teams entering the split"));
        assert!(article.body.contains("Teams: Solo FC"));
        assert_eq!(article.i18n_params.get("teamCount"), Some(&"1".to_string()));
        assert_eq!(
            article.i18n_params.get("favourite"),
            Some(&"Solo FC".to_string())
        );
        assert_eq!(
            article.i18n_params.get("darkHorse"),
            Some(&"Solo FC".to_string())
        );
    }
}
