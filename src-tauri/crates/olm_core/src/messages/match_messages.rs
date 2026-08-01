use super::{action, params};
use crate::domain::message::*;
use rand::RngExt;

pub fn pre_match_message(
    fixture_id: &str,
    opponent_name: &str,
    opponent_id: &str,
    is_home: bool,
    matchday: u32,
    match_date: &str,
    date: &str,
) -> InboxMessage {
    let venue = if is_home { "home" } else { "away" };
    let venue_label = if is_home { "Local" } else { "Visitante" };
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "match_preview",
        &format!("preview_{fixture_id}"),
        date,
        "en",
        vec![
            ("opponent", opponent_name),
            ("venue", venue_label),
            ("date", match_date),
        ],
    ) {
        return msg;
    }
    let mut rng = rand::rng();

    let variations = [
        format!(
            "Your {} match against {} is coming up on {}.\n\n\
            Matchday {} of the Premier Division. Make sure your starting XI is in good shape and \
            your tactics are set.\n\n\
            {} advantage could be key in this one.",
            venue,
            opponent_name,
            match_date,
            matchday,
            if is_home {
                "Home"
            } else {
                "Matching their intensity away from home"
            }
        ),
        format!(
            "Reminder: you face {} {} in 3 days ({}).\n\n\
            This is Matchday {} — review your squad fitness and consider any tactical adjustments. \
            {}",
            opponent_name,
            venue,
            match_date,
            matchday,
            if is_home {
                "The fans will be behind you at home."
            } else {
                "Away form will be tested — pack your strongest lineup."
            }
        ),
    ];

    let idx = rng.random_range(0..variations.len());

    InboxMessage::new(
        format!("prematch_{}", fixture_id),
        format!(
            "Upcoming: vs {} ({})",
            opponent_name,
            if is_home { "H" } else { "A" }
        ),
        variations[idx].clone(),
        "Assistant Manager".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::MatchPreview)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Assistant Manager")
    .with_action(action(
        "set_tactics",
        "Set Tactics",
        "be.msg.preMatch.actionTactics",
        ActionType::NavigateTo {
            route: "/dashboard?tab=Tactics".to_string(),
        },
    ))
    .with_action(action(
        "view_opponent",
        "Scout Opponent",
        "be.msg.preMatch.actionScout",
        ActionType::NavigateTo {
            route: format!("/team/{}", opponent_id),
        },
    ))
    .with_context(MessageContext {
        fixture_id: Some(fixture_id.to_string()),
        team_id: Some(opponent_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.preMatch.subject",
        &format!("be.msg.preMatch.body{}", idx),
        params(&[
            ("venue", venue),
            ("opponent", opponent_name),
            ("matchDate", match_date),
            ("matchday", &matchday.to_string()),
        ]),
    )
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}

pub fn match_result_message(
    fixture_id: &str,
    home_name: &str,
    away_name: &str,
    home_goals: u8,
    away_goals: u8,
    home_team_id: &str,
    away_team_id: &str,
    user_team_id: &str,
    matchday: u32,
    date: &str,
) -> InboxMessage {
    let is_home = home_team_id == user_team_id;
    let user_goals = if is_home { home_goals } else { away_goals };
    let opp_goals = if is_home { away_goals } else { home_goals };
    let score = format!(
        "{}:{user_goals}-{opp_goals}",
        if is_home { home_name } else { away_name }
    );
    let opponent = if is_home { away_name } else { home_name };

    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "match_result",
        &format!("result_{fixture_id}"),
        date,
        "en",
        vec![
            ("home", home_name),
            ("away", away_name),
            ("score", &score),
            ("opponent", opponent),
            ("matchday", &matchday.to_string()),
        ],
    ) {
        return msg;
    }

    let outcome = if user_goals > opp_goals {
        "Victory"
    } else if user_goals < opp_goals {
        "Defeat"
    } else {
        "Draw"
    };

    let mut rng = rand::rng();
    let body = match outcome {
        "Victory" => {
            let v = [
                format!(
                    "Full time: {} {} - {} {}.\n\n\
                    An excellent result! The team put in a strong performance. \
                    Matchday {} — keep this momentum going.",
                    home_name, home_goals, away_goals, away_name, matchday
                ),
                format!(
                    "Final whistle: {} {} - {} {}.\n\n\
                    Three points in the bag! The lads showed great character out there. \
                    Matchday {} complete.",
                    home_name, home_goals, away_goals, away_name, matchday
                ),
            ];
            v[rng.random_range(0..v.len())].clone()
        }
        "Defeat" => {
            let v = [
                format!(
                    "Full time: {} {} - {} {}.\n\n\
                    A disappointing result. We'll need to regroup and work on the areas that let us down. \
                    Matchday {} — there's still time to turn things around.",
                    home_name, home_goals, away_goals, away_name, matchday
                ),
                format!(
                    "Final score: {} {} - {} {}.\n\n\
                    Not the result we wanted. Matchday {} — the board will want to see improvement. \
                    Review what went wrong and prepare for the next challenge.",
                    home_name, home_goals, away_goals, away_name, matchday
                ),
            ];
            v[rng.random_range(0..v.len())].clone()
        }
        _ => {
            format!(
                "Full time: {} {} - {} {}.\n\n\
                A point earned in Matchday {}. Depending on results elsewhere, this could be valuable. \
                The team fought hard but couldn't find a winner.",
                home_name, home_goals, away_goals, away_name, matchday
            )
        }
    };

    InboxMessage::new(
        format!("result_{}", fixture_id),
        format!(
            "{}: {} {} - {} {}",
            outcome, home_name, home_goals, away_goals, away_name
        ),
        body,
        "Match Reporter".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::MatchResult)
    .with_priority(if outcome == "Victory" {
        MessagePriority::Normal
    } else {
        MessagePriority::High
    })
    .with_sender_role("Press Officer")
    .with_action(action(
        "view_standings",
        "View Standings",
        "be.msg.matchResult.actionStandings",
        ActionType::NavigateTo {
            route: "/dashboard?tab=Schedule".to_string(),
        },
    ))
    .with_context(MessageContext {
        fixture_id: Some(fixture_id.to_string()),
        match_result: Some(ContextMatchResult {
            home_team_id: home_team_id.to_string(),
            away_team_id: away_team_id.to_string(),
            home_goals,
            away_goals,
        }),
        ..Default::default()
    })
    .with_i18n(
        &format!("be.msg.matchResult.subject.{}", outcome.to_lowercase()),
        &format!(
            "be.msg.matchResult.body.{}{}",
            outcome.to_lowercase(),
            if outcome == "Draw" {
                String::new()
            } else {
                rng.random_range(0..2u8).to_string()
            }
        ),
        {
            let mut p = params(&[
                ("home", home_name),
                ("away", away_name),
                ("homeGoals", &home_goals.to_string()),
                ("awayGoals", &away_goals.to_string()),
                ("matchday", &matchday.to_string()),
            ]);
            p.insert("outcome".to_string(), outcome.to_string());
            p
        },
    )
    .with_sender_i18n("be.sender.matchReporter", "be.role.pressOfficer")
}
