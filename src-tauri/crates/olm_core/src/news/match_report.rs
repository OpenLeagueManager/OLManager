use super::params;
use crate::domain::news::*;
use rand::RngExt;

fn result_text(home_name: &str, away_name: &str, home_goals: u8, away_goals: u8) -> String {
    if home_goals > away_goals {
        format!(
            "{} took the series {}-{} over {}",
            home_name, home_goals, away_goals, away_name
        )
    } else if away_goals > home_goals {
        format!(
            "{} took the series {}-{} over {}",
            away_name, away_goals, home_goals, home_name
        )
    } else {
        format!(
            "{} and {} closed a {}-{} series draw",
            home_name, away_name, home_goals, away_goals
        )
    }
}

fn scorer_parts(
    home_name: &str,
    away_name: &str,
    home_scorers: &[(String, u32)],
    away_scorers: &[(String, u32)],
) -> Vec<String> {
    let mut parts = Vec::new();
    for (name, minute) in home_scorers {
        parts.push(format!("{} ({}', {})", name, minute, home_name));
    }
    for (name, minute) in away_scorers {
        parts.push(format!("{} ({}', {})", name, minute, away_name));
    }
    parts
}

fn scorer_player_ids(
    home_scorers: &[(String, u32)],
    away_scorers: &[(String, u32)],
) -> Vec<String> {
    home_scorers
        .iter()
        .chain(away_scorers.iter())
        .map(|(name, _)| name.clone())
        .collect()
}

fn pick_player_of_match(
    home_scorers: &[(String, u32)],
    away_scorers: &[(String, u32)],
    home_goals: u8,
    away_goals: u8,
) -> String {
    if home_goals > away_goals {
        if let Some((name, _)) = home_scorers.first() {
            return name.clone();
        }
    } else if away_goals > home_goals {
        if let Some((name, _)) = away_scorers.first() {
            return name.clone();
        }
    }

    if let Some((name, _)) = home_scorers.first() {
        return name.clone();
    }

    if let Some((name, _)) = away_scorers.first() {
        return name.clone();
    }

    "N/A".to_string()
}

fn outcome_key(home_goals: u8, away_goals: u8) -> &'static str {
    if home_goals > away_goals {
        "homeWin"
    } else if away_goals > home_goals {
        "awayWin"
    } else {
        "draw"
    }
}

/// Generate a match report news article for a completed fixture.
pub fn match_report_article(
    fixture_id: &str,
    home_name: &str,
    away_name: &str,
    home_goals: u8,
    away_goals: u8,
    home_team_id: &str,
    away_team_id: &str,
    matchday: u32,
    home_scorers: &[(String, u32)], // (player_name, minute)
    away_scorers: &[(String, u32)],
    date: &str,
) -> NewsArticle {
    let mut rng = rand::rng();

    let result_text = result_text(home_name, away_name, home_goals, away_goals);
    let _scorer_parts = scorer_parts(home_name, away_name, home_scorers, away_scorers);
    let player_of_match = pick_player_of_match(home_scorers, away_scorers, home_goals, away_goals);

    let commentary = [
        format!(
            "Matchday {} wrapped with {}. This result can influence standings momentum as the split progresses.\n\nPlayer of the match: {}",
            matchday, result_text, player_of_match
        ),
        format!(
            "{} in Matchday {} on {} side. Both teams traded draft adaptations and objective setups across the series.\n\nPlayer of the match: {}",
            result_text,
            matchday,
            if home_goals >= away_goals {
                format!("{}'s", home_name)
            } else {
                format!("{}'s", away_name)
            },
            player_of_match
        ),
        format!(
            "Matchday {} delivered another competitive series: {}. Fans got draft pivots and objective fights all the way.\n\nPlayer of the match: {}",
            matchday, result_text, player_of_match
        ),
    ];

    let idx = rng.random_range(0..commentary.len());

    let headline = if home_goals > away_goals {
        let headlines = [
            format!(
                "{} {} - {} {}: Rift Control Secured",
                home_name, home_goals, away_goals, away_name
            ),
            format!(
                "{} Outdraft {} in Matchday {}",
                home_name, away_name, matchday
            ),
            format!("Clean Series from {} over {}", home_name, away_name),
        ];
        headlines[rng.random_range(0..headlines.len())].clone()
    } else if away_goals > home_goals {
        let headlines = [
            format!(
                "{} {} - {} {}: Away Side Executes",
                home_name, home_goals, away_goals, away_name
            ),
            format!("{} Punish {} in Draft and Tempo", away_name, home_name),
            format!("Road Series Win for {}", away_name),
        ];
        headlines[rng.random_range(0..headlines.len())].clone()
    } else {
        let headlines = [
            format!(
                "{} {} - {} {}: Series Ends Level",
                home_name, home_goals, away_goals, away_name
            ),
            format!("{} and {} Split the Maps", home_name, away_name),
            format!("No Edge Found Between {} and {}", home_name, away_name),
        ];
        headlines[rng.random_range(0..headlines.len())].clone()
    };

    let source_keys = [
        "be.source.sportsGazette",
        "be.source.lolEsports",
        "be.source.matchDayPress",
        "be.source.leagueChronicle",
    ];
    let sources = [
        "Dot Esports",
        "LoL Esports",
        "Dexerto Esports",
        "Sheep Esports",
    ];
    let src_idx = rng.random_range(0..sources.len());
    let source = sources[src_idx];
    let source_key = source_keys[src_idx];

    let player_ids = scorer_player_ids(home_scorers, away_scorers);

    // Determine outcome for i18n key
    let outcome = outcome_key(home_goals, away_goals);
    let headline_variant = rng.random_range(0..3u8);

    NewsArticle::new(
        format!("report_{}", fixture_id),
        headline,
        commentary[idx].clone(),
        source.to_string(),
        date.to_string(),
        NewsCategory::MatchReport,
    )
    .with_teams(vec![home_team_id.to_string(), away_team_id.to_string()])
    .with_players(player_ids)
    .with_score(NewsMatchScore {
        home_team_id: home_team_id.to_string(),
        away_team_id: away_team_id.to_string(),
        home_goals,
        away_goals,
    })
    .with_i18n(
        &format!(
            "be.news.matchReport.headline.{}.{}",
            outcome, headline_variant
        ),
        &format!("be.news.matchReport.body{}", idx),
        source_key,
        {
            let mut p = params(&[
                ("home", home_name),
                ("away", away_name),
                ("homeGoals", &home_goals.to_string()),
                ("awayGoals", &away_goals.to_string()),
                ("matchday", &matchday.to_string()),
                ("playerOfMatch", &player_of_match),
            ]);
            // For winner-specific headlines
            if home_goals > away_goals {
                p.insert("winner".to_string(), home_name.to_string());
                p.insert("loser".to_string(), away_name.to_string());
            } else if away_goals > home_goals {
                p.insert("winner".to_string(), away_name.to_string());
                p.insert("loser".to_string(), home_name.to_string());
            }
            p
        },
    )
}

#[cfg(test)]
mod tests {
    use super::match_report_article;
    use crate::domain::news::NewsCategory;

    fn assert_valid_source_pair(source: &str, source_key: &str) {
        let valid = [
            ("Dot Esports", "be.source.sportsGazette"),
            ("LoL Esports", "be.source.lolEsports"),
            ("Dexerto Esports", "be.source.matchDayPress"),
            ("Sheep Esports", "be.source.leagueChronicle"),
        ];

        assert!(
            valid
                .iter()
                .any(|pair| pair.0 == source && pair.1 == source_key)
        );
    }

    #[test]
    fn home_win_article_includes_match_metadata_and_scorers() {
        let article = match_report_article(
            "fix1",
            "Alpha FC",
            "Beta FC",
            2,
            1,
            "team1",
            "team2",
            5,
            &[("Alice".to_string(), 10)],
            &[("Bob".to_string(), 75)],
            "2025-06-15",
        );

        assert_eq!(article.id, "report_fix1");
        assert_eq!(article.category, NewsCategory::MatchReport);
        assert_eq!(
            article.team_ids,
            vec!["team1".to_string(), "team2".to_string()]
        );
        assert_eq!(
            article.player_ids,
            vec!["Alice".to_string(), "Bob".to_string()]
        );
        let score = article.match_score.as_ref().unwrap();
        assert_eq!(score.home_team_id, "team1");
        assert_eq!(score.away_team_id, "team2");
        assert_eq!(score.home_goals, 2);
        assert_eq!(score.away_goals, 1);

        assert!(
            article
                .body
                .contains("Alpha FC took the series 2-1 over Beta FC")
        );
        assert!(article.body.contains("Player of the match: Alice"));
        assert!(
            article
                .headline_key
                .as_deref()
                .unwrap()
                .starts_with("be.news.matchReport.headline.homeWin.")
        );
        assert!(
            [
                "be.news.matchReport.body0",
                "be.news.matchReport.body1",
                "be.news.matchReport.body2"
            ]
            .contains(&article.body_key.as_deref().unwrap())
        );
        assert_valid_source_pair(&article.source, article.source_key.as_deref().unwrap());
        assert_eq!(
            article.i18n_params.get("home"),
            Some(&"Alpha FC".to_string())
        );
        assert_eq!(
            article.i18n_params.get("away"),
            Some(&"Beta FC".to_string())
        );
        assert_eq!(article.i18n_params.get("homeGoals"), Some(&"2".to_string()));
        assert_eq!(article.i18n_params.get("awayGoals"), Some(&"1".to_string()));
        assert_eq!(article.i18n_params.get("matchday"), Some(&"5".to_string()));
        assert_eq!(
            article.i18n_params.get("playerOfMatch"),
            Some(&"Alice".to_string())
        );
        assert_eq!(
            article.i18n_params.get("winner"),
            Some(&"Alpha FC".to_string())
        );
        assert_eq!(
            article.i18n_params.get("loser"),
            Some(&"Beta FC".to_string())
        );
    }

    #[test]
    fn away_win_article_sets_away_winner_params() {
        let article = match_report_article(
            "fix2",
            "Alpha FC",
            "Beta FC",
            1,
            3,
            "team1",
            "team2",
            6,
            &[("Alice".to_string(), 12)],
            &[("Bob".to_string(), 40), ("Ben".to_string(), 88)],
            "2025-06-22",
        );

        assert!(
            article
                .body
                .contains("Beta FC took the series 3-1 over Alpha FC")
        );
        assert_eq!(
            article.i18n_params.get("winner"),
            Some(&"Beta FC".to_string())
        );
        assert_eq!(
            article.i18n_params.get("loser"),
            Some(&"Alpha FC".to_string())
        );
        assert!(
            article
                .headline_key
                .as_deref()
                .unwrap()
                .starts_with("be.news.matchReport.headline.awayWin.")
        );
        assert_eq!(
            article.player_ids,
            vec!["Alice".to_string(), "Bob".to_string(), "Ben".to_string()]
        );
    }

    #[test]
    fn draw_article_omits_winner_params_and_goal_section_when_scoreless() {
        let article = match_report_article(
            "fix3",
            "Alpha FC",
            "Beta FC",
            0,
            0,
            "team1",
            "team2",
            7,
            &[],
            &[],
            "2025-06-29",
        );

        assert!(
            article
                .body
                .contains("Alpha FC and Beta FC closed a 0-0 series draw")
        );
        assert!(!article.body.contains("Goals:"));
        assert!(article.body.contains("Player of the match: N/A"));
        assert!(
            article
                .headline_key
                .as_deref()
                .unwrap()
                .starts_with("be.news.matchReport.headline.draw.")
        );
        assert_eq!(article.i18n_params.get("winner"), None);
        assert_eq!(article.i18n_params.get("loser"), None);
        assert_eq!(
            article.i18n_params.get("playerOfMatch"),
            Some(&"N/A".to_string())
        );
        assert!(article.player_ids.is_empty());
    }
}
