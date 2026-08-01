use crate::domain::league::Fixture;
use crate::domain::social::{SocialAuthorType, SocialPost, SocialPostCategory, SocialSentiment};
use crate::domain::team::Team;
use crate::engine::report::{MatchReport, PlayerMatchStats};

use crate::game::Game;
use crate::social_data::{MatchTexts, load_match_texts};
use crate::social_registry::{default_social_accounts, social_author};
use crate::social_templates::{
    MatchTemplateContext, MatchTemplateSlot, SelectedMatchTemplate, default_social_templates,
    select_match_template_for_language,
};
use std::path::Path;
use std::sync::OnceLock;

static MATCH_TEXTS: OnceLock<MatchTexts> = OnceLock::new();

fn cached_match_texts(data_base: Option<&Path>) -> Option<&'static MatchTexts> {
    if let Some(texts) = MATCH_TEXTS.get() {
        return Some(texts);
    }
    if let Some(base) = data_base {
        if let Some(texts) = load_match_texts(base) {
            let _ = MATCH_TEXTS.set(texts);
            return MATCH_TEXTS.get();
        }
    }
    None
}

fn social_handle(name: &str) -> String {
    let handle: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    format!(
        "@{}",
        if handle.is_empty() {
            "olmsocial"
        } else {
            &handle
        }
    )
}

fn variant_index(seed: &str, len: usize) -> usize {
    if len == 0 {
        return 0;
    }

    seed.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    }) % len
}

fn engagement(base: u32, team_reputation: u32, spicy: bool, seed: &str) -> (u32, u32, u32) {
    let reputation_boost = team_reputation.saturating_mul(3);
    let spice_boost = if spicy { base / 4 + 35 } else { 0 };
    let noise = variant_index(seed, 35) as u32;
    let likes = base
        .saturating_add(reputation_boost)
        .saturating_add(spice_boost)
        .saturating_add(noise);
    (likes, likes / 12, likes / 24)
}

fn team_by_id<'a>(game: &'a Game, team_id: &str) -> Option<&'a Team> {
    game.teams.iter().find(|team| team.id == team_id)
}

fn top_player_for_team<'a>(
    game: &'a Game,
    report: &'a MatchReport,
    team_id: &str,
) -> Option<(&'a crate::domain::player::Player, &'a PlayerMatchStats)> {
    game.players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(team_id))
        .filter_map(|player| {
            report
                .player_stats
                .get(&player.id)
                .map(|stats| (player, stats))
        })
        .max_by_key(|(_, stats)| {
            stats.kills as i32 * 3 + stats.assists as i32 * 2 - stats.deaths as i32
        })
}

fn scale_engagement(values: (u32, u32, u32), factor: f64) -> (u32, u32, u32) {
    let scale = |value: u32| -> u32 { ((value as f64) * factor).round().max(1.0) as u32 };
    (scale(values.0), scale(values.1), scale(values.2))
}

fn sentiment_from_tags(tags: &[String], is_winner: bool) -> SocialSentiment {
    let tag_set: std::collections::HashSet<String> =
        tags.iter().map(|t| t.to_lowercase()).collect();
    if tag_set.contains("stomp") {
        return if is_winner {
            SocialSentiment::Hype
        } else {
            SocialSentiment::Meltdown
        };
    }
    if tag_set.contains("close") {
        return SocialSentiment::Worried;
    }
    if tag_set.contains("hate") {
        return SocialSentiment::Angry;
    }
    if tag_set.contains("rivalry") {
        return if is_winner {
            SocialSentiment::Hype
        } else {
            SocialSentiment::Worried
        };
    }
    if is_winner {
        SocialSentiment::Calm
    } else {
        SocialSentiment::Worried
    }
}

fn pick_team_fan_account<'a>(
    game: &'a Game,
    team_id: &str,
    language: &str,
    seed: &str,
) -> Option<&'a crate::domain::social::SocialAccount> {
    let accounts: Vec<&crate::domain::social::SocialAccount> = game
        .social_accounts
        .iter()
        .filter(|account| account.active)
        .filter(|account| {
            matches!(
                account.author_type,
                SocialAuthorType::Fan | SocialAuthorType::MemeAccount
            )
        })
        .filter(|account| {
            account.language.eq_ignore_ascii_case("all")
                || account.language.eq_ignore_ascii_case(language)
        })
        .filter(|account| {
            account
                .favorite_team_ids
                .iter()
                .any(|favorite| favorite == team_id)
        })
        .collect();

    if accounts.is_empty() {
        return None;
    }

    let index = variant_index(seed, accounts.len());
    accounts.get(index).copied()
}

fn team_fan_reaction_text(
    language: &str,
    won: bool,
    team_short_name: &str,
    opponent_short_name: &str,
    score: &str,
    seed: &str,
    data_base: Option<&Path>,
) -> String {
    if let Some(texts) = cached_match_texts(data_base) {
        let map = if won {
            &texts.fan_reaction_won
        } else {
            &texts.fan_reaction_lost
        };
        if let Some(options) = map.get(language).or_else(|| map.get("en")) {
            if !options.is_empty() {
                let idx = variant_index(seed, options.len());
                return options[idx]
                    .replace("{team}", team_short_name)
                    .replace("{opponent}", opponent_short_name)
                    .replace("{score}", score);
            }
        }
    }

    let options: &[&str] = match (language, won) {
        ("es", true) => &[
            "{team} gano y se noto en el mapa. Muy buena serie contra {opponent}. {score}",
            "Partido muy serio de {team}. Buenas decisiones y mejor cierre.",
            "Victoria de {team} y sensaciones muy buenas para lo que viene.",
        ],
        ("es", false) => &[
            "Hoy toco perder, pero seguimos confiando en {team}.",
            "Resultado duro para {team}. Reset y a por la siguiente serie. {score}",
            "No salio contra {opponent}, pero esto recien empieza para {team}.",
        ],
        (_, true) => &[
            "{team} got the win and looked clean on map play vs {opponent}. {score}",
            "Very solid game from {team}. Better setup, better closes.",
            "Big win for {team}. This version can compete with anyone.",
        ],
        (_, false) => &[
            "Tough loss today, but we still believe in {team}.",
            "Rough result for {team}. Reset and go next. {score}",
            "Did not work out vs {opponent}, but this split is long for {team}.",
        ],
    };

    options[variant_index(seed, options.len())]
        .replace("{team}", team_short_name)
        .replace("{opponent}", opponent_short_name)
        .replace("{score}", score)
}

fn bouzys_vs_fnatic_text(
    language: &str,
    winner_short_name: &str,
    seed: &str,
    data_base: Option<&Path>,
) -> String {
    if let Some(texts) = cached_match_texts(data_base) {
        if let Some(options) = texts
            .bouzys_vs_fnatic
            .get(language)
            .or_else(|| texts.bouzys_vs_fnatic.get("en"))
        {
            if !options.is_empty() {
                let idx = variant_index(seed, options.len());
                return options[idx].replace("{winner}", winner_short_name);
            }
        }
    }

    let options: &[&str] = match language {
        "es" => &[
            "Hoy soy {winner} Bouzys. Gracias por bajar a Fnatic, cine total.",
            "Confirmado: {winner} Bouzys por 24h. Lo de hoy contra Fnatic fue una locura.",
            "Sale cambio de camiseta: {winner} Bouzys hasta nuevo aviso. Qué victoria sobre Fnatic.",
        ],
        "pt-BR" => &[
            "Hoje eu sou {winner} Bouzys. Valeu por derrubar a Fnatic, cinema puro.",
            "Confirmado: {winner} Bouzys por 24h. O jogo de hoje contra a Fnatic foi loucura.",
            "Troquei de camisa: {winner} Bouzys até novo aviso. Vitória gigante sobre a Fnatic.",
        ],
        "de" => &[
            "Heute bin ich {winner} Bouzys. Danke fürs Runterholen von Fnatic, pures Kino.",
            "Bestätigt: {winner} Bouzys für 24 Stunden. Das heute gegen Fnatic war verrückt.",
            "Trikotwechsel ist durch: {winner} Bouzys bis auf Weiteres. Was für ein Sieg gegen Fnatic.",
        ],
        "fr" => &[
            "Aujourd'hui je suis {winner} Bouzys. Merci d'avoir fait tomber Fnatic, c'était du cinéma.",
            "Confirmé: {winner} Bouzys pendant 24h. Le match d'aujourd'hui contre Fnatic était dingue.",
            "Changement de maillot: {winner} Bouzys jusqu'à nouvel ordre. Quelle victoire contre Fnatic.",
        ],
        "tr" => &[
            "Bugün ben {winner} Bouzys oldum. Fnatic'i düşürdüğünüz için teşekkürler, tam sinema.",
            "Resmileşti: 24 saatliğine {winner} Bouzys. Bugünkü Fnatic maçı tam delilikti.",
            "Forma değişti: yeni ben {winner} Bouzys. Fnatic'e karşı müthiş galibiyet.",
        ],
        _ => &[
            "Today I'm {winner} Bouzys. Thanks for taking down Fnatic, absolute cinema.",
            "Confirmed: {winner} Bouzys for 24 hours. Today's game vs Fnatic was wild.",
            "Shirt swap complete: {winner} Bouzys until further notice. Huge win over Fnatic.",
        ],
    };

    options[variant_index(seed, options.len())].replace("{winner}", winner_short_name)
}

fn team_loser_post_text(
    language: &str,
    team_short_name: &str,
    opponent_short_name: &str,
    score: &str,
    seed: &str,
    stomp: bool,
    data_base: Option<&Path>,
) -> String {
    if let Some(texts) = cached_match_texts(data_base) {
        let map = if stomp {
            &texts.team_loser_stomp
        } else {
            &texts.team_loser_close
        };
        if let Some(options) = map.get(language).or_else(|| map.get("en")) {
            if !options.is_empty() {
                let idx = variant_index(seed, options.len());
                return options[idx]
                    .replace("{team}", team_short_name)
                    .replace("{opponent}", opponent_short_name)
                    .replace("{score}", score);
            }
        }
        if let Some(options) = texts
            .team_loser
            .get(language)
            .or_else(|| texts.team_loser.get("en"))
        {
            if !options.is_empty() {
                let idx = variant_index(seed, options.len());
                return options[idx]
                    .replace("{team}", team_short_name)
                    .replace("{opponent}", opponent_short_name)
                    .replace("{score}", score);
            }
        }
    }

    let options: &[&str] = if stomp {
        match language {
            "es" => &[
                "No fue nuestro mejor dia. {score} y duele.",
                "Resultado duro para {team}. Necesitamos revisar TODO.",
                "GG {opponent}. Hoy no salio nada. {score}",
                "{score}. No hay palabras para esto. A levantarse.",
                "Dia para olvidar. {score} y a seguir.",
            ],
            _ => &[
                "Not our best day. {score} and it hurts.",
                "Tough result for {team}. We need to review EVERYTHING.",
                "GG {opponent}. Nothing worked today. {score}",
                "{score}. No words for this. Back to the grind.",
                "A day to forget. {score} and we move on.",
            ],
        }
    } else {
        match language {
            "es" => &[
                "Tensión hasta el final. {score} y nos duele.",
                "Casi. {score} pero no fue suficiente.",
                "Partido reñido. {score} y seguimos luchando.",
                "GG {opponent}. {score} y nos vamos con la cabeza alta.",
                "Estuvimos cerca. {score} y la próxima es nuestra.",
            ],
            _ => &[
                "Tension until the end. {score} and it hurts.",
                "So close. {score} but not enough.",
                "Close game. {score} and we keep fighting.",
                "GG {opponent}. {score} and we leave with our heads high.",
                "We were close. {score} and next one is ours.",
            ],
        }
    };

    options[variant_index(seed, options.len())]
        .replace("{team}", team_short_name)
        .replace("{opponent}", opponent_short_name)
        .replace("{score}", score)
}

pub fn generate_match_social_posts(
    game: &mut Game,
    fixture_index: usize,
    report: &MatchReport,
    locale: Option<&str>,
    data_base: Option<&Path>,
) {
    ensure_social_registry_defaults(game, data_base);

    let Some(league) = game.active_league() else {
        return;
    };
    let Some(fixture) = league.fixtures.get(fixture_index).cloned() else {
        return;
    };
    if game
        .social_posts
        .iter()
        .any(|post| post.fixture_id.as_deref() == Some(&fixture.id))
    {
        return;
    }

    let Some((winner_id, loser_id, winner_wins, loser_wins)) = winner_loser(&fixture, report)
    else {
        return;
    };
    let Some(winner) = team_by_id(game, winner_id).cloned() else {
        return;
    };
    let Some(loser) = team_by_id(game, loser_id).cloned() else {
        return;
    };

    let score = format!("{}-{}", winner_wins, loser_wins);
    let stomp = winner_wins.saturating_sub(loser_wins) >= 2
        || kill_difference_for_winner(&fixture, report) >= 10;
    let date = game.clock.current_date.format("%Y-%m-%d").to_string();
    let seed = format!("{}-{}-{}", fixture.id, winner.id, score);
    let winner_objectives = if report.home_wins > report.away_wins {
        report.home_stats.objectives
    } else {
        report.away_stats.objectives
    };
    let featured_player = top_player_for_team(game, report, &winner.id)
        .map(|(player, _stats)| (player.id.clone(), player.match_name.clone()));
    let context = MatchTemplateContext {
        winner: &winner,
        loser: &loser,
        manager_team_id: game.manager.team_id.as_deref(),
        featured_player_id: featured_player
            .as_ref()
            .map(|(player_id, _)| player_id.as_str()),
        score: &score,
        seed: &seed,
        stomp,
        winner_objectives,
        player_name: featured_player
            .as_ref()
            .map(|(_, player_name)| player_name.as_str()),
    };

    let language = locale
        .map(normalize_social_language)
        .unwrap_or_else(|| manager_language(&game.manager.nationality).to_string());
    let team_template: SelectedMatchTemplate = select_match_template_for_language(
        &game.social_templates,
        &language,
        MatchTemplateSlot::TeamBanter,
        &context,
    );
    let (likes, reposts, replies) = engagement(120, winner.reputation, true, &seed);
    let team_post = SocialPost::new(
        format!("social_{}_team", fixture.id),
        date.clone(),
        winner.name.clone(),
        social_handle(&winner.name),
        SocialAuthorType::Team,
        team_template.text,
        SocialPostCategory::Banter,
        sentiment_from_tags(&team_template.tags, true),
    )
    .with_engagement(likes, reposts, replies)
    .with_tags(if team_template.tags.is_empty() {
        vec!["match".to_string(), "banter".to_string()]
    } else {
        team_template.tags
    })
    .with_teams(vec![winner.id.clone(), loser.id.clone()])
    .with_fixture(fixture.id.clone());

    let (loss_likes, loss_reposts, loss_replies) =
        engagement(75, loser.reputation, false, &format!("{}-team-loss", seed));
    let loser_team_text = team_loser_post_text(
        &language,
        &loser.short_name,
        &winner.short_name,
        &score,
        &seed,
        stomp,
        data_base,
    );
    let loser_team_post = SocialPost::new(
        format!("social_{}_team_loser", fixture.id),
        date.clone(),
        loser.name.clone(),
        social_handle(&loser.name),
        SocialAuthorType::Team,
        loser_team_text,
        SocialPostCategory::MatchResult,
        sentiment_from_tags(&[], false),
    )
    .with_engagement(loss_likes, loss_reposts, loss_replies)
    .with_tags(vec![
        "match".to_string(),
        "team".to_string(),
        "loss".to_string(),
    ])
    .with_teams(vec![winner.id.clone(), loser.id.clone()])
    .with_fixture(fixture.id.clone());

    let fan_template: SelectedMatchTemplate = select_match_template_for_language(
        &game.social_templates,
        &language,
        MatchTemplateSlot::FanOpinion,
        &context,
    );
    let fan_profile = fan_template
        .author_id
        .as_deref()
        .and_then(social_author)
        .or_else(|| social_author("fan_random_lec"));
    let (likes, reposts, replies) = engagement(
        if stomp { 55 } else { 35 },
        winner.reputation / 2,
        stomp,
        &seed,
    );
    let fan_post = SocialPost::new(
        format!("social_{}_fan", fixture.id),
        date.clone(),
        fan_profile
            .as_ref()
            .map(|profile| profile.display_name.to_string())
            .unwrap_or_else(|| "LEC Enjoyer".to_string()),
        fan_profile
            .as_ref()
            .map(|profile| profile.handle.to_string())
            .unwrap_or_else(|| "@randomLECEnjoyer".to_string()),
        fan_profile
            .as_ref()
            .map(|profile| profile.author_type.clone())
            .unwrap_or(SocialAuthorType::Fan),
        fan_template.text,
        SocialPostCategory::FanOpinion,
        sentiment_from_tags(&fan_template.tags, true),
    )
    .with_engagement(likes, reposts, replies)
    .with_tags(if fan_template.tags.is_empty() {
        vec!["fan".to_string(), "match".to_string()]
    } else {
        fan_template.tags
    })
    .with_teams(vec![winner.id.clone(), loser.id.clone()])
    .with_fixture(fixture.id.clone());

    let analyst_template: SelectedMatchTemplate = select_match_template_for_language(
        &game.social_templates,
        &language,
        MatchTemplateSlot::AnalystTake,
        &context,
    );
    let analyst_profile = analyst_template
        .author_id
        .as_deref()
        .and_then(social_author)
        .or_else(|| social_author("analyst_manu"));
    let (likes, reposts, replies) = engagement(45, winner.reputation / 2, false, &seed);
    let analyst_post = SocialPost::new(
        format!("social_{}_analyst", fixture.id),
        date.clone(),
        analyst_profile
            .as_ref()
            .map(|profile| profile.display_name.to_string())
            .unwrap_or_else(|| "Manu 𓃵𓃶".to_string()),
        analyst_profile
            .as_ref()
            .map(|profile| profile.handle.to_string())
            .unwrap_or_else(|| "@Cabramaravilla".to_string()),
        analyst_profile
            .as_ref()
            .map(|profile| profile.author_type.clone())
            .unwrap_or(SocialAuthorType::Analyst),
        analyst_template.text,
        SocialPostCategory::MediaTake,
        sentiment_from_tags(&analyst_template.tags, true),
    )
    .with_engagement(likes, reposts, replies)
    .with_tags(if analyst_template.tags.is_empty() {
        vec!["analysis".to_string(), "match".to_string()]
    } else {
        analyst_template.tags
    })
    .with_teams(vec![winner.id.clone(), loser.id.clone()])
    .with_fixture(fixture.id.clone());

    game.social_posts
        .extend([team_post, loser_team_post, fan_post, analyst_post]);

    if let Some(winner_fan) =
        pick_team_fan_account(game, &winner.id, &language, &format!("{}-fan-win", seed))
    {
        let (likes, reposts, replies) = scale_engagement(
            engagement(
                48,
                winner.reputation / 2,
                true,
                &format!("{}-fan-win", seed),
            ),
            0.10,
        );
        let winner_fan_post = SocialPost::new(
            format!("social_{}_fan_winner_team", fixture.id),
            date.clone(),
            winner_fan.display_name.clone(),
            winner_fan.handle.clone(),
            winner_fan.author_type.clone(),
            team_fan_reaction_text(
                &language,
                true,
                &winner.short_name,
                &loser.short_name,
                &score,
                &format!("{}-fan-win", seed),
                data_base,
            ),
            SocialPostCategory::FanOpinion,
            sentiment_from_tags(&[], true),
        )
        .with_engagement(likes, reposts, replies)
        .with_tags(vec!["fan".to_string(), "team-win".to_string()])
        .with_teams(vec![winner.id.clone(), loser.id.clone()])
        .with_fixture(fixture.id.clone());
        game.social_posts.push(winner_fan_post);
    }

    if let Some(loser_fan) =
        pick_team_fan_account(game, &loser.id, &language, &format!("{}-fan-loss", seed))
    {
        let (likes, reposts, replies) = scale_engagement(
            engagement(
                42,
                loser.reputation / 2,
                false,
                &format!("{}-fan-loss", seed),
            ),
            0.10,
        );
        let loser_fan_post = SocialPost::new(
            format!("social_{}_fan_loser_team", fixture.id),
            date.clone(),
            loser_fan.display_name.clone(),
            loser_fan.handle.clone(),
            loser_fan.author_type.clone(),
            team_fan_reaction_text(
                &language,
                false,
                &loser.short_name,
                &winner.short_name,
                &score,
                &format!("{}-fan-loss", seed),
                data_base,
            ),
            SocialPostCategory::FanOpinion,
            sentiment_from_tags(&[], false),
        )
        .with_engagement(likes, reposts, replies)
        .with_tags(vec!["fan".to_string(), "team-loss".to_string()])
        .with_teams(vec![winner.id.clone(), loser.id.clone()])
        .with_fixture(fixture.id.clone());
        game.social_posts.push(loser_fan_post);
    }

    if let Some((player_id, player_name)) = featured_player {
        let (likes, reposts, replies) = engagement(105, winner.reputation, false, &seed);
        let player_context = MatchTemplateContext {
            winner: &winner,
            loser: &loser,
            manager_team_id: game.manager.team_id.as_deref(),
            featured_player_id: Some(&player_id),
            score: &score,
            seed: &seed,
            stomp,
            winner_objectives,
            player_name: Some(&player_name),
        };
        let player_template = select_match_template_for_language(
            &game.social_templates,
            &language,
            MatchTemplateSlot::PlayerReaction,
            &player_context,
        );
        let player_post = SocialPost::new(
            format!("social_{}_player_{}", fixture.id, player_id),
            date,
            player_name.clone(),
            social_handle(&player_name),
            SocialAuthorType::Player,
            player_template.text,
            SocialPostCategory::PlayerReaction,
            sentiment_from_tags(&player_template.tags, true),
        )
        .with_engagement(likes, reposts, replies)
        .with_tags(if player_template.tags.is_empty() {
            vec!["player".to_string(), "gg".to_string()]
        } else {
            player_template.tags
        })
        .with_teams(vec![winner.id.clone()])
        .with_players(vec![player_id])
        .with_fixture(fixture.id.clone());
        game.social_posts.push(player_post);
    }

    if loser.id == "lec-fnatic" && language.eq_ignore_ascii_case("es") {
        let (likes, reposts, replies) = scale_engagement(
            engagement(38, winner.reputation / 2, true, &format!("{}-bouzys", seed)),
            0.10,
        );
        let bouzys_post = SocialPost::new(
            format!("social_{}_fan_bouzys_fnatic", fixture.id),
            game.clock.current_date.format("%Y-%m-%d").to_string(),
            if language.eq_ignore_ascii_case("es") {
                format!("{} Bouzys", winner.short_name)
            } else {
                "X Bouzys".to_string()
            },
            "@Bouzyslol".to_string(),
            SocialAuthorType::Fan,
            bouzys_vs_fnatic_text(
                &language,
                &winner.short_name,
                &format!("{}-bouzys", seed),
                data_base,
            ),
            SocialPostCategory::FanOpinion,
            sentiment_from_tags(&[], true),
        )
        .with_engagement(likes, reposts, replies)
        .with_tags(vec![
            "fan".to_string(),
            "fnatic".to_string(),
            "banter".to_string(),
        ])
        .with_teams(vec![winner.id.clone(), loser.id.clone()])
        .with_fixture(fixture.id.clone());
        game.social_posts.push(bouzys_post);
    }
}

fn winner_loser<'a>(
    fixture: &'a Fixture,
    report: &MatchReport,
) -> Option<(&'a str, &'a str, u8, u8)> {
    if report.home_wins > report.away_wins {
        Some((
            &fixture.home_team_id,
            &fixture.away_team_id,
            report.home_wins,
            report.away_wins,
        ))
    } else if report.away_wins > report.home_wins {
        Some((
            &fixture.away_team_id,
            &fixture.home_team_id,
            report.away_wins,
            report.home_wins,
        ))
    } else {
        None
    }
}

fn kill_difference_for_winner(fixture: &Fixture, report: &MatchReport) -> u16 {
    let home_won = report.home_wins > report.away_wins;
    let winner_kills = if home_won {
        report.home_stats.kills
    } else {
        report.away_stats.kills
    };
    let loser_kills = if fixture.home_team_id == fixture.away_team_id {
        0
    } else if home_won {
        report.away_stats.kills
    } else {
        report.home_stats.kills
    };
    winner_kills.saturating_sub(loser_kills)
}

pub fn publish_manager_post(
    game: &mut Game,
    raw_text: &str,
    data_base: Option<&Path>,
) -> Result<SocialPost, String> {
    ensure_social_registry_defaults(game, data_base);

    let text = raw_text.trim();
    if text.is_empty() {
        return Err("Post cannot be empty".to_string());
    }
    if text.chars().count() > 280 {
        return Err("Post exceeds 280 characters".to_string());
    }

    let date = game.clock.current_date.format("%Y-%m-%d").to_string();
    let manager_name = game.manager.display_name();
    let manager_handle = social_handle(&manager_name);
    let id = format!("social_manager_{}_{}", date, game.social_posts.len() + 1);
    let seed = format!("{}-{}", id, game.manager.id);
    let (likes, reposts, replies) = engagement(25, game.manager.reputation / 2, false, &seed);

    let post = SocialPost::new(
        id,
        date,
        manager_name,
        manager_handle,
        SocialAuthorType::Manager,
        text.to_string(),
        SocialPostCategory::ManagerPost,
        SocialSentiment::Calm,
    )
    .with_engagement(likes, reposts, replies)
    .with_tags(vec!["manager".to_string(), "post".to_string()]);

    game.social_posts.push(post.clone());
    Ok(post)
}

pub fn ensure_social_registry_defaults(game: &mut Game, data_base: Option<&Path>) {
    let json_accounts = data_base.and_then(crate::social_data::load_social_accounts);
    let defaults = json_accounts.unwrap_or_else(default_social_accounts);
    if game.social_accounts.is_empty() {
        game.social_accounts = defaults;
    } else {
        for default_account in defaults {
            if let Some(existing) = game
                .social_accounts
                .iter_mut()
                .find(|account| account.handle.eq_ignore_ascii_case(&default_account.handle))
            {
                // Preserve user-edited fields but update Twitter URLs to local paths
                if let Some(ref default_url) = default_account.profile_image_url {
                    if existing
                        .profile_image_url
                        .as_ref()
                        .map(|url| url.contains("pbs.twimg.com"))
                        .unwrap_or(true)
                    {
                        existing.profile_image_url = Some(default_url.clone());
                    }
                }
                if existing.favorite_team_ids.is_empty()
                    && !default_account.favorite_team_ids.is_empty()
                {
                    existing.favorite_team_ids = default_account.favorite_team_ids.clone();
                }
            } else {
                game.social_accounts.push(default_account);
            }
        }
    }
    if game.social_templates.is_empty() {
        game.social_templates = default_social_templates(data_base);
    }
}

pub fn relocalize_social_posts(game: &mut Game, locale: &str, data_base: Option<&Path>) {
    ensure_social_registry_defaults(game, data_base);
    let Some(league) = game.active_league() else {
        return;
    };
    let language = normalize_social_language(locale);
    let fixtures = league.fixtures.clone();
    let teams = game.teams.clone();
    let templates = game.social_templates.clone();
    let manager_team_id = game.manager.team_id.clone();

    for post in game.social_posts.iter_mut() {
        let Some(fixture_id) = post.fixture_id.as_deref() else {
            continue;
        };
        let Some(fixture) = fixtures.iter().find(|item| item.id == fixture_id) else {
            continue;
        };
        let Some(result) = fixture.result.as_ref() else {
            continue;
        };

        let (winner_id, loser_id, winner_wins, loser_wins) = if result.home_wins >= result.away_wins
        {
            (
                fixture.home_team_id.as_str(),
                fixture.away_team_id.as_str(),
                result.home_wins,
                result.away_wins,
            )
        } else {
            (
                fixture.away_team_id.as_str(),
                fixture.home_team_id.as_str(),
                result.away_wins,
                result.home_wins,
            )
        };

        let Some(winner) = teams.iter().find(|team| team.id == winner_id).cloned() else {
            continue;
        };
        let Some(loser) = teams.iter().find(|team| team.id == loser_id).cloned() else {
            continue;
        };

        let score = format!("{}-{}", winner_wins, loser_wins);
        let kill_diff = result
            .report
            .as_ref()
            .map(|report| {
                if winner_id == fixture.home_team_id {
                    report
                        .home_stats
                        .kills
                        .saturating_sub(report.away_stats.kills)
                } else {
                    report
                        .away_stats
                        .kills
                        .saturating_sub(report.home_stats.kills)
                }
            })
            .unwrap_or(0);
        let stomp = winner_wins.saturating_sub(loser_wins) >= 2 || kill_diff >= 10;
        let winner_objectives = result
            .report
            .as_ref()
            .map(|report| {
                if winner_id == fixture.home_team_id {
                    report.home_stats.objectives
                } else {
                    report.away_stats.objectives
                }
            })
            .unwrap_or(0);

        let player_name = if post.author_type == SocialAuthorType::Player {
            Some(post.author_name.as_str())
        } else {
            None
        };
        let featured_player_id = if post.author_type == SocialAuthorType::Player {
            post.player_ids.first().map(|id| id.as_str())
        } else {
            None
        };

        let seed = format!("{}-{}-{}", fixture.id, winner.id, score);
        let context = MatchTemplateContext {
            winner: &winner,
            loser: &loser,
            manager_team_id: manager_team_id.as_deref(),
            featured_player_id,
            score: &score,
            seed: &seed,
            stomp,
            winner_objectives,
            player_name,
        };

        post.body = if post.id.ends_with("_team") {
            select_match_template_for_language(
                &templates,
                &language,
                MatchTemplateSlot::TeamBanter,
                &context,
            )
            .text
        } else if post.id.ends_with("_team_loser") {
            team_loser_post_text(
                &language,
                &loser.short_name,
                &winner.short_name,
                &score,
                &seed,
                stomp,
                data_base,
            )
        } else if post.id.ends_with("_fan") {
            select_match_template_for_language(
                &templates,
                &language,
                MatchTemplateSlot::FanOpinion,
                &context,
            )
            .text
        } else if post.id.ends_with("_analyst") {
            select_match_template_for_language(
                &templates,
                &language,
                MatchTemplateSlot::AnalystTake,
                &context,
            )
            .text
        } else if post.id.ends_with("_fan_winner_team") {
            team_fan_reaction_text(
                &language,
                true,
                &winner.short_name,
                &loser.short_name,
                &score,
                &seed,
                data_base,
            )
        } else if post.id.ends_with("_fan_loser_team") {
            team_fan_reaction_text(
                &language,
                false,
                &loser.short_name,
                &winner.short_name,
                &score,
                &seed,
                data_base,
            )
        } else if post.id.ends_with("_fan_bouzys_fnatic") {
            if language.eq_ignore_ascii_case("es") {
                post.author_name = format!("{} Bouzys", winner.short_name);
            } else {
                post.author_name = "X Bouzys".to_string();
            }
            bouzys_vs_fnatic_text(
                &language,
                &winner.short_name,
                &format!("{}-bouzys", seed),
                data_base,
            )
        } else if post.id.contains("_player_") {
            select_match_template_for_language(
                &templates,
                &language,
                MatchTemplateSlot::PlayerReaction,
                &context,
            )
            .text
        } else {
            post.body.clone()
        };
    }
}

fn normalize_social_language(locale: &str) -> String {
    let value = locale.trim();
    if value.eq_ignore_ascii_case("pt-br") {
        return "pt-BR".to_string();
    }
    value
        .split(['-', '_'])
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("en")
        .to_lowercase()
}

fn manager_language(nationality: &str) -> &str {
    let value = nationality.to_lowercase();
    if value.contains("argentina")
        || value.contains("uruguay")
        || value.contains("mexico")
        || value.contains("colombia")
        || value.contains("chile")
        || value.contains("peru")
        || value.contains("ecuador")
        || value.contains("venezuela")
        || value.contains("bolivia")
        || value.contains("paraguay")
        || value.contains("costa rica")
        || value.contains("guatemala")
        || value.contains("honduras")
        || value.contains("nicaragua")
        || value.contains("panama")
        || value.contains("dominican")
        || value.contains("puerto rico")
        || value.contains("latam")
        || value.contains("latin")
        || value == "ar"
        || value == "uy"
        || value == "mx"
        || value == "co"
        || value == "cl"
        || value == "pe"
        || value == "ec"
        || value == "ve"
        || value == "bo"
        || value == "py"
        || value == "cr"
        || value == "gt"
        || value == "hn"
        || value == "ni"
        || value == "pa"
        || value == "do"
        || value == "pr"
    {
        return "es";
    }
    if value.contains("ital") || value == "it" {
        return "it";
    }
    if value.contains("portugal") || value == "pt" {
        return "pt";
    }
    if value.contains("brazil") || value.contains("brasil") || value == "pt-br" || value == "br" {
        return "pt-BR";
    }
    if value.contains("turkey") || value.contains("turkiye") || value == "tr" {
        return "tr";
    }
    if value.contains("spain") || value.contains("espa") || value == "es" {
        return "es";
    }
    if value.contains("france") || value == "fr" {
        return "fr";
    }
    if value.contains("germany") || value == "de" {
        return "de";
    }
    "en"
}
