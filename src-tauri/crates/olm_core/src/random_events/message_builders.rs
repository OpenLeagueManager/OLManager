use super::{action, format_money, params};
use crate::domain::message::*;
use crate::narrative::{NarrativeSelector, load_default_content_pack};
use rand::RngExt;

// ---------------------------------------------------------------------------
// Message builders
// ---------------------------------------------------------------------------

pub(super) fn sponsor_offer_message(
    msg_id: &str,
    team_name: &str,
    sponsor: &str,
    amount: u64,
    date: &str,
) -> InboxMessage {
    InboxMessage::new(
        msg_id.to_string(),
        format!("Sponsorship Offer — {}", sponsor),
        format!(
            "Good news, boss! {} has expressed interest in becoming a sponsor of {}.\n\n\
            They're offering an annual sponsorship of €{} paid monthly over the next 3 months in exchange for advertising space at the training ground.\n\n\
            This seems like a reasonable deal, but it's your call.",
            sponsor, team_name, format_money(amount)
        ),
        "Commercial Director".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Finance)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Commercial Director")
    .with_action(action(
        "respond", "Respond", "be.msg.event.respond",
        ActionType::ChooseOption {
            options: vec![
                ActionOption {
                    id: "accept".to_string(),
                    label: "Accept the deal".to_string(),
                    description: format!("Receive €{} in sponsorship income.", format_money(amount)),
                    label_key: Some("be.msg.sponsor.options.accept.label".to_string()),
                    description_key: Some("be.msg.sponsor.options.accept.description".to_string()),
                },
                ActionOption {
                    id: "decline".to_string(),
                    label: "Decline politely".to_string(),
                    description: "Turn down the offer. No financial impact.".to_string(),
                    label_key: Some("be.msg.sponsor.options.decline.label".to_string()),
                    description_key: Some("be.msg.sponsor.options.decline.description".to_string()),
                },
            ],
        },
    ))
    .with_i18n(
        "be.msg.sponsor.subject",
        "be.msg.sponsor.body",
        params(&[("sponsor", sponsor), ("team", team_name), ("amount", &format_money(amount))]),
    )
    .with_sender_i18n("be.sender.commercialDirector", "be.role.commercialDirector")
}

const ESPORTMANIACOS_VARIANT_COUNT: usize = 10;

fn esportmaniacos_story_copy(
    team_name: &str,
    player_name: &str,
    is_positive: bool,
    variant: usize,
) -> (String, String, String, String, MessagePriority) {
    let variant = variant % ESPORTMANIACOS_VARIANT_COUNT;
    let (subject, body, subject_key, priority) = if is_positive {
        match variant {
            0 => (
                format!(
                    "Esportmaníacos praises {}'s Rift control — the desk is unanimous",
                    player_name
                ),
                format!(
                    "The Esportmaníacos panel spent a good chunk of today's stream on {} at {}.\n\n\
                    \"Nothing to criticize this week. The guy is absolutely dominating the Rift: objective setups, draft execution, solo queue discipline, everything.\" \
                    The positive coverage should give confidence a boost in the team room.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject0",
                MessagePriority::Low,
            ),
            1 => (
                format!(
                    "Esportmaníacos: {} is carrying {} through every Draft",
                    player_name, team_name
                ),
                format!(
                    "The Esportmaníacos tertulianos rarely agree on anything, but {} got a full pass today: \
                    \"Consistent, solid, no dips in form. From draft reads to objective setups, one of the best players in the league right now. \
                    {} should do everything to keep him.\"\n\n\
                    Good for morale — and for the market value.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject1",
                MessagePriority::Low,
            ),
            2 => (
                format!(
                    "Esportmaníacos highlights {}'s objective calls",
                    player_name
                ),
                format!(
                    "The desk circled back to {} after reviewing {}'s latest map setups: \
                    \"This is the kind of player who makes the whole team look more ordered. Herald timer, dragon setup, reset timing; everything has intent.\"\n\n\
                    Quiet praise, but the sort that staff rooms notice.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject2",
                MessagePriority::Low,
            ),
            3 => (
                format!("Esportmaníacos buys into the {} project", team_name),
                format!(
                    "Today's show had an unusual moment of agreement around {}: \
                    \"You can see the work. The drafts make sense, the players know their jobs, and {} is setting the tone in-game. This is not random form.\"\n\n\
                    The clip is already moving through fan chats.",
                    team_name, player_name
                ),
                "be.msg.esportmaniacos.positive.subject3",
                MessagePriority::Low,
            ),
            4 => (
                format!("Esportmaníacos: {} looks stage-ready", player_name),
                format!(
                    "Esportmaníacos praised {} for turning pressure moments into clean decisions: \
                    \"Some players farm stats; this one wins the ugly minutes. If {} need someone calm when the map gets weird, that's him.\"\n\n\
                    That kind of narrative can settle a player before big matches.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject4",
                MessagePriority::Low,
            ),
            5 => (
                format!(
                    "Esportmaníacos gives {} credit for the turnaround",
                    player_name
                ),
                format!(
                    "The panel framed {} as one of the reasons {}'s week looked cleaner: \
                    \"Less panic, better first moves, better discipline after losing tempo. You can tell someone is calling with confidence.\"\n\n\
                    Staff will enjoy hearing this one.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject5",
                MessagePriority::Low,
            ),
            6 => (
                format!("Esportmaníacos praises {}'s champion pool", player_name),
                format!(
                    "A draft segment on Esportmaníacos turned into praise for {}: \
                    \"The value is not one pick; it's the threat of five. {} can enter draft with actual options because this player bends bans.\"\n\n\
                    Good timing if renewal talks are near.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject6",
                MessagePriority::Low,
            ),
            7 => (
                format!("Esportmaníacos says {} is raising the floor", player_name),
                format!(
                    "The tertulia was impressed by how stable {} has become for {}: \
                    \"He may not be the loudest name every week, but the floor is high. Coaches love that. Teams win splits with players who don't donate games.\"\n\n\
                    Sensible coverage, for once.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject7",
                MessagePriority::Low,
            ),
            8 => (
                format!("Esportmaníacos calls {} a playoff-level piece", player_name),
                format!(
                    "Esportmaníacos spent a full block on playoff ceilings and landed on {}: \
                    \"If {} reaches playoffs, this is one of the players I trust in a best-of. He understands when to slow the map and when to flip tempo.\"\n\n\
                    Useful fuel for the room.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject8",
                MessagePriority::Low,
            ),
            _ => (
                format!("Esportmaníacos: {} finally gets his flowers", player_name),
                format!(
                    "The panel admitted {} had been underrated in the wider conversation: \
                    \"We spend too much time on the flashy names. This guy has been doing the boring winning stuff for {} all split. Give him credit.\"\n\n\
                    A small media bump, but a deserved one.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject9",
                MessagePriority::Low,
            ),
        }
    } else {
        match variant {
            0 => (
                format!("Draft Pressure on {} — panel shows no mercy", player_name),
                format!(
                    "The Esportmaníacos panel did not hold back today on {}: \
                    \"The guy has completely disappeared under lane pressure. What happened to him this split? \
                    {} can't keep relying on a player performing like this.\"\n\n\
                    This kind of Rift coverage tends to hit morale hard. Worth having a word with the player after scrim review.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject0",
                MessagePriority::Normal,
            ),
            1 => (
                format!(
                    "Pressure Watch: Esportmaníacos questions {}'s consistency at {}",
                    player_name, team_name
                ),
                format!(
                    "Today's Esportmaníacos tertulianos session turned into a full breakdown of {}'s recent form. \
                    The verdict: \"Inconsistent in lane and shaky in scrim patterns. No regularity. Some days at top level, others completely invisible on the Rift. \
                    {} deserves better output from a player in that role.\"\n\n\
                    Keep an eye on the player's morale.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject1",
                MessagePriority::Normal,
            ),
            2 => (
                format!("Esportmaníacos worries about {}'s early game", player_name),
                format!(
                    "The desk clipped several early-game moments from {} and the conclusion was blunt: \
                    \"You cannot start every map ten seconds late. If {} want to play for objectives, this has to be cleaner.\"\n\n\
                    The criticism is specific enough to stick.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject2",
                MessagePriority::Normal,
            ),
            3 => (
                format!("Esportmaníacos puts {} under review", player_name),
                format!(
                    "A VOD segment on Esportmaníacos turned uncomfortable for {}: \
                    \"The mechanics are there, but the decision tree is messy. Some deaths look like communication problems, some look like impatience. Either way, {} need answers.\"\n\n\
                    The player may feel the heat this week.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject3",
                MessagePriority::Normal,
            ),
            4 => (
                format!("Esportmaníacos questions {}'s draft value", player_name),
                format!(
                    "Esportmaníacos focused on draft flexibility and {} did not escape criticism: \
                    \"If you need two bans and a comfort pick just to get an even lane, the draft becomes expensive. {} have to solve that.\"\n\n\
                    A tough angle before the next prep block.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject4",
                MessagePriority::Normal,
            ),
            5 => (
                format!("Esportmaníacos: {} is losing key minutes", player_name),
                format!(
                    "The panel argued that {}'s problem is not the scoreboard but the timing: \
                    \"Minute eight, minute fourteen, third dragon setup; that's where the game is slipping. {} need him present in those windows.\"\n\n\
                    Media pressure is now attached to very concrete moments.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject5",
                MessagePriority::Normal,
            ),
            6 => (
                format!("Esportmaníacos calls out {}'s map reads", player_name),
                format!(
                    "A map-control debate ended with {} taking the blame: \
                    \"This is not about one missed skillshot. The reads are late. When the play starts, {} are already reacting instead of setting the terms.\"\n\n\
                    Expect fans to repeat that line all week.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject6",
                MessagePriority::Normal,
            ),
            7 => (
                format!("Esportmaníacos asks if {} need a backup plan", team_name),
                format!(
                    "The tertulia floated an awkward question around {}: \
                    \"If this form continues, does {} need to prepare another look? Nobody is saying bench him tomorrow, but you cannot ignore the trend.\"\n\n\
                    That sort of speculation can unsettle a room fast.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject7",
                MessagePriority::Normal,
            ),
            8 => (
                format!("Esportmaníacos: {} is forcing bad map states", player_name),
                format!(
                    "Esportmaníacos reviewed {}'s recent deaths and found a pattern: \
                    \"These are not heroic plays gone wrong. These are low-percentage moves that make {} play the next two minutes with no map.\"\n\n\
                    Worth addressing before it becomes the week's storyline.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject8",
                MessagePriority::Normal,
            ),
            _ => (
                format!("Esportmaníacos cools the hype around {}", player_name),
                format!(
                    "The panel pushed back on the idea that {} is still untouchable: \
                    \"Reputation is not current form. Right now {} need more from him, especially when the draft gives resources to his side.\"\n\n\
                    A direct hit to the player's public stock.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject9",
                MessagePriority::Normal,
            ),
        }
    };
    let body_key = if is_positive {
        format!("be.msg.esportmaniacos.positive.body{}", variant)
    } else {
        format!("be.msg.esportmaniacos.negative.body{}", variant)
    };

    (subject, body, subject_key.to_string(), body_key, priority)
}

pub(super) fn media_story_message(
    msg_id: &str,
    team_name: &str,
    player_id: &str,
    player_name: &str,
    is_positive: bool,
    date: &str,
) -> InboxMessage {
    if let Some(message) = build_media_story_from_narrative(
        msg_id,
        team_name,
        player_id,
        player_name,
        is_positive,
        date,
    ) {
        return message;
    }

    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "media_story",
        msg_id,
        date,
        "en",
        vec![("team", team_name), ("player", player_name)],
    ) {
        return msg;
    }

    let mut rng = rand::rng();
    let variant = rng.random_range(0..ESPORTMANIACOS_VARIANT_COUNT);
    let (subject, body, subject_key, body_key, priority) =
        esportmaniacos_story_copy(team_name, player_name, is_positive, variant);

    InboxMessage::new(
        msg_id.to_string(),
        subject,
        body,
        "Esportmaníacos".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Media)
    .with_priority(priority)
    .with_sender_role("Panel")
    .with_action(action(
        "ack",
        "Noted",
        "be.msg.event.ack",
        ActionType::Acknowledge,
    ))
    .with_context(MessageContext {
        player_id: Some(player_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        &subject_key,
        &body_key,
        params(&[("player", player_name), ("team", team_name)]),
    )
    .with_sender_i18n("be.sender.esportmaniacos", "be.role.esportmaniacos")
}

pub fn build_media_story_from_narrative(
    msg_id: &str,
    team_name: &str,
    player_id: &str,
    player_name: &str,
    is_positive: bool,
    date: &str,
) -> Option<InboxMessage> {
    let pack = load_default_content_pack().ok()?;
    let selector = NarrativeSelector::new(&pack);
    let tags = if is_positive {
        vec!["media", "positive"]
    } else {
        vec!["media", "pressure"]
    };
    let tones = if is_positive {
        vec!["professional", "analytical", "community"]
    } else {
        vec!["spicy", "pressure"]
    };
    let template = selector.select_event(Some("default"), &tags, &tones)?;

    let mut rng = rand::rng();
    let variant = rng.random_range(0..2usize);

    let (subject, body, subject_key, priority) = if is_positive {
        match variant {
            0 => (
                format!(
                    "Esportmaníacos praises {}'s Rift control — the desk is unanimous",
                    player_name
                ),
                format!(
                    "The Esportmaníacos panel spent a good chunk of today's stream on {} at {}.\n\n\
                    \"Nothing to criticize this week. The guy is absolutely dominating the Rift: objective setups, draft execution, solo queue discipline, everything.\" \
                    The positive coverage should give confidence a boost in the team room.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject0",
                MessagePriority::Low,
            ),
            _ => (
                format!(
                    "Esportmaníacos: {} is carrying {} through every Draft",
                    player_name, team_name
                ),
                format!(
                    "The Esportmaníacos tertulianos rarely agree on anything, but {} got a full pass today: \
                    \"Consistent, solid, no dips in form. From draft reads to objective setups, one of the best players in the league right now. \
                    {} should do everything to keep him.\"\n\n\
                    Good for morale — and for the market value.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.positive.subject1",
                MessagePriority::Low,
            ),
        }
    } else {
        match variant {
            0 => (
                format!("Draft Pressure on {} — panel shows no mercy", player_name),
                format!(
                    "The Esportmaníacos panel did not hold back today on {}: \
                    \"The guy has completely disappeared under lane pressure. What happened to him this split? \
                    {} can't keep relying on a player performing like this.\"\n\n\
                    This kind of Rift coverage tends to hit morale hard. Worth having a word with the player after scrim review.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject0",
                MessagePriority::Normal,
            ),
            _ => (
                format!(
                    "Pressure Watch: Esportmaníacos questions {}'s consistency at {}",
                    player_name, team_name
                ),
                format!(
                    "Today's Esportmaníacos tertulianos session turned into a full breakdown of {}'s recent form. \
                    The verdict: \"Inconsistent in lane and shaky in scrim patterns. No regularity. Some days at top level, others completely invisible on the Rift. \
                    {} deserves better output from a player in that role.\"\n\n\
                    Keep an eye on the player's morale.",
                    player_name, team_name
                ),
                "be.msg.esportmaniacos.negative.subject1",
                MessagePriority::Normal,
            ),
        }
    };

    let body_key = if is_positive {
        format!("be.msg.esportmaniacos.positive.body{}", variant)
    } else {
        format!("be.msg.esportmaniacos.negative.body{}", variant)
    };

    Some(
        InboxMessage::new(
            msg_id.to_string(),
            subject,
            body,
            "Esportmaníacos".to_string(),
            date.to_string(),
        )
        .with_category(MessageCategory::Media)
        .with_priority(priority)
        .with_sender_role("Panel")
        .with_action(action(
            "ack",
            "Noted",
            "be.msg.event.ack",
            ActionType::Acknowledge,
        ))
        .with_context(MessageContext {
            player_id: Some(player_id.to_string()),
            ..Default::default()
        })
        .with_i18n(
            &subject_key,
            &body_key,
            params(&[
                ("player", player_name),
                ("team", team_name),
                ("effectId", &template.effect_id),
            ]),
        )
        .with_sender_i18n("be.sender.esportmaniacos", "be.role.esportmaniacos"),
    )
}

pub(super) fn allio_podcast_message(
    msg_id: &str,
    team_name: &str,
    player_id: Option<&str>,
    player_name: Option<&str>,
    date: &str,
) -> InboxMessage {
    let pname = player_name.unwrap_or("one of your players");
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "podcast",
        msg_id,
        date,
        "en",
        vec![("team", team_name), ("player", pname)],
    ) {
        return msg;
    }

    let mut rng = rand::rng();
    let variant = rng.random_range(0..10usize);
    let pname = player_name.unwrap_or("one of your players");

    let (subject, body, subject_key, body_key) = match variant {
        0 => (
            format!("Al Lío Podcast — Transfer rumour: {} being tracked", pname),
            format!(
                "Eros dropped a transfer hint on today's Al Lío episode: \
                \"I'm hearing things about {}. Multiple teams have been asking questions. \
                Nothing confirmed yet, but the mercato around {} is heating up.\"\n\n\
                Keep this in mind when planning your squad for next split.",
                pname, team_name
            ),
            "be.msg.allio.subject0",
            "be.msg.allio.body0",
        ),
        1 => (
            format!("Al Lío Podcast — Eros hints at incoming deal for {}", team_name),
            format!(
                "Eros has been dropping hints on the latest Al Lío episode: \
                \"I've been told there are conversations happening around {}. Something is being cooked. \
                I can't give names yet but stay tuned — this mercato is far from over.\"\n\n\
                Could be noise, could be real. Worth watching.",
                team_name
            ),
            "be.msg.allio.subject1",
            "be.msg.allio.body1",
        ),
        2 => (
            "Al Lío Podcast — League format leak, Eros breaks it down".to_string(),
            "Today's Al Lío episode went off-script when Eros revealed a potential leak: \
            \"I've been told the league is planning changes to the split format. Not confirmed, \
            but my sources are usually reliable. This could shake up how teams build their rosters.\"\n\n\
            If true, this could affect your long-term planning.".to_string(),
            "be.msg.allio.subject2",
            "be.msg.allio.body2",
        ),
        3 => (
            "Al Lío Podcast — Big move incoming, league shaken up".to_string(),
            "Eros went live on Al Lío with what he called a \"bombazo\": \
            \"There's a significant player movement about to happen in the league that nobody is talking about yet. \
            I'll just say this — some teams are going to have to rethink their rosters completely.\"\n\n\
            Stay alert — the market is moving.".to_string(),
            "be.msg.allio.subject3",
            "be.msg.allio.body3",
        ),
        4 => (
            format!("Al Lío Podcast — Eros puts {} on watch", team_name),
            format!(
                "Eros used the opening block of Al Lío to flag {} as a team to watch: \
                \"I am not saying something is signed, but I know there are people around the league asking what happens next with this roster. \
                The mercato never sleeps.\"\n\n\
                A little smoke, no confirmed fire yet.",
                team_name
            ),
            "be.msg.allio.subject4",
            "be.msg.allio.body4",
        ),
        5 => (
            format!("Al Lío Podcast — {} extension talks get mentioned", pname),
            format!(
                "Eros slipped a small note into the Al Lío rumour section: \
                \"Keep an eye on {}. When a player is performing and the contract calendar starts moving, calls happen. \
                It does not mean he leaves, but it means the room gets noisy.\"\n\n\
                Worth checking the player's situation before the story grows.",
                pname
            ),
            "be.msg.allio.subject5",
            "be.msg.allio.body5",
        ),
        6 => (
            "Al Lío Podcast — scrim whispers around the league".to_string(),
            format!(
                "Al Lío devoted a segment to scrim rumours, with Eros careful not to overclaim: \
                \"The only thing I will say is that {} are being talked about more than last week. \
                Sometimes that means progress, sometimes it means chaos. We will see on stage.\"\n\n\
                External noise is starting to build.",
                team_name
            ),
            "be.msg.allio.subject6",
            "be.msg.allio.body6",
        ),
        7 => (
            format!("Al Lío Podcast — buyout talk starts around {}", pname),
            format!(
                "Eros framed {} as one of the names that could shape the next market: \
                \"If the split keeps going like this, teams will ask. Maybe the answer is no, maybe the price is crazy, but the interest will exist.\"\n\n\
                The rumour mill has found a new angle.",
                pname
            ),
            "be.msg.allio.subject7",
            "be.msg.allio.body7",
        ),
        8 => (
            "Al Lío Podcast — Eros teases a staff-side move".to_string(),
            format!(
                "The latest Al Lío episode briefly moved away from players and into staff rumours: \
                \"There are teams looking at analysts, assistants, people behind the scenes. \
                Do not be surprised if {} has to protect more than just its starting five.\"\n\n\
                Not actionable yet, but relevant for long-term planning.",
                team_name
            ),
            "be.msg.allio.subject8",
            "be.msg.allio.body8",
        ),
        _ => (
            format!("Al Lío Podcast — Eros says {}'s offseason could be loud", team_name),
            format!(
                "Eros closed Al Lío with a broader mercato warning: \
                \"Some teams think they will have a quiet offseason and then the first call arrives. \
                With {}, I would prepare for movement, even if the official line is calm.\"\n\n\
                The smart play is to know your priorities early.",
                team_name
            ),
            "be.msg.allio.subject9",
            "be.msg.allio.body9",
        ),
    };

    let mut msg = InboxMessage::new(
        msg_id.to_string(),
        subject,
        body,
        "Al Lío Podcast".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Media)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Eros")
    .with_action(action(
        "ack",
        "Noted",
        "be.msg.event.ack",
        ActionType::Acknowledge,
    ))
    .with_i18n(
        subject_key,
        body_key,
        params(&[("player", pname), ("team", team_name)]),
    )
    .with_sender_i18n("be.sender.allioPodcast", "be.role.allioPodcast");

    if variant < 2 {
        if let Some(pid) = player_id {
            msg = msg.with_context(MessageContext {
                player_id: Some(pid.to_string()),
                ..Default::default()
            });
        }
    }

    msg
}

pub(super) fn yuste_stream_message(msg_id: &str, is_positive: bool, date: &str) -> InboxMessage {
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "stream",
        msg_id,
        date,
        "en",
        vec![],
    ) {
        return msg;
    }
    let mut rng = rand::rng();
    let variant = rng.random_range(0..10usize);

    let (subject, body, subject_key, body_key) = if is_positive {
        match variant {
            0 => (
                "el_yuste — even Yuste admits: viewership is up this week".to_string(),
                "Yuste surprised his stream with a rare positive take: \
                \"I'll be honest — the numbers are up this week and I'm not going to pretend otherwise. \
                When the matches are good, the audience comes. Simple.\"\n\n\
                A positive moment for the whole scene.".to_string(),
                "be.msg.yuste.up.subject0",
                "be.msg.yuste.up.body0",
            ),
            1 => (
                "el_yuste — this week's matches actually got Yuste excited".to_string(),
                "Not something you hear every day: Yuste was enthusiastic on his morning stream. \
                \"This week delivered. Real matches, proper stakes, viewers who stayed till the end. \
                This is what the league can be when it tries.\"\n\n\
                Good week to be in esports.".to_string(),
                "be.msg.yuste.up.subject1",
                "be.msg.yuste.up.body1",
            ),
            2 => (
                "el_yuste — por fin una jornada con intención".to_string(),
                "Yuste arrancó el directo sorprendentemente constructivo: \
                \"Hoy sí. Hoy he visto equipos jugando a algo, no cinco tíos esperando que pase el rival. \
                Si este es el nivel base, la liga tiene producto.\"\n\n\
                Una buena señal para la percepción pública.".to_string(),
                "be.msg.yuste.up.subject2",
                "be.msg.yuste.up.body2",
            ),
            3 => (
                "el_yuste — Yuste reconoce que la liga ha ganado ritmo".to_string(),
                "El comentario del día vino con menos ironía de lo habitual: \
                \"No me vendáis humo, pero el ritmo ha subido. Menos pausas muertas, mejores cierres de mapa, más razones para quedarse viendo.\"\n\n\
                Cuando incluso Yuste concede eso, la escena lo nota.".to_string(),
                "be.msg.yuste.up.subject3",
                "be.msg.yuste.up.body3",
            ),
            4 => (
                "el_yuste — buenas audiencias y menos excusas".to_string(),
                "Yuste leyó los números del fin de semana y fue directo: \
                \"Cuando haces las cosas medianamente bien, la gente aparece. Igual el problema nunca fue que no hubiera público, igual era que no le dabas motivos.\"\n\n\
                Lectura positiva, con su filo habitual.".to_string(),
                "be.msg.yuste.up.subject4",
                "be.msg.yuste.up.body4",
            ),
            5 => (
                "el_yuste — la semana competitiva salva el debate".to_string(),
                "En su repaso matinal, Yuste destacó que el nivel competitivo ayudó al show: \
                \"No necesito fuegos artificiales si las partidas tienen tensión real. Dame draft con sentido y equipos castigando errores. Con eso ya me quedo.\"\n\n\
                El clip está circulando bastante bien.".to_string(),
                "be.msg.yuste.up.subject5",
                "be.msg.yuste.up.body5",
            ),
            6 => (
                "el_yuste — hasta el chat compra la narrativa".to_string(),
                "Yuste se rió al ver el chat más optimista de lo normal: \
                \"Mirad, si hasta vosotros estáis diciendo que la jornada estuvo guapa, algo habrán hecho bien. Apuntadlo, que no pasa siempre.\"\n\n\
                Buen ambiente alrededor de la liga.".to_string(),
                "be.msg.yuste.up.subject6",
                "be.msg.yuste.up.body6",
            ),
            7 => (
                "el_yuste — una jornada que no pide perdón".to_string(),
                "Yuste resumió la jornada con una frase poco habitual en él: \
                \"Esto no ha necesitado excusas. Buenas partidas, buenos finales, historias que importan. Así sí puedes vender una liga.\"\n\n\
                Es el tipo de aprobación que pesa porque no la regala.".to_string(),
                "be.msg.yuste.up.subject7",
                "be.msg.yuste.up.body7",
            ),
            8 => (
                "el_yuste — el producto empieza a parecer producto".to_string(),
                "La valoración de Yuste fue seca, pero positiva: \
                \"Por una vez la retransmisión, el calendario y las partidas fueron en la misma dirección. No es tan difícil cuando todos reman.\"\n\n\
                Un mensaje útil para la liga y para los clubes.".to_string(),
                "be.msg.yuste.up.subject8",
                "be.msg.yuste.up.body8",
            ),
            _ => (
                "el_yuste — la liga vuelve a dar conversación".to_string(),
                "Yuste cerró su bloque con una admisión clara: \
                \"Esta semana se habló de la liga por las partidas, no por el drama de alrededor. Eso ya es una victoria. Pequeña, pero victoria.\"\n\n\
                La percepción general mejora.".to_string(),
                "be.msg.yuste.up.subject9",
                "be.msg.yuste.up.body9",
            ),
        }
    } else {
        match variant {
            0 => (
                "el_yuste — league viewers are tanking and nobody cares".to_string(),
                "Antonio Yuste opened his morning stream with a full rant: \
                \"The numbers don't lie. Viewership is going down week after week and the league keeps \
                doing the same things expecting different results. C'est fini if nothing changes.\"\n\n\
                Just background noise — but when Yuste talks, the community listens.".to_string(),
                "be.msg.yuste.down.subject0",
                "be.msg.yuste.down.body0",
            ),
            1 => (
                "el_yuste — the league product is broken, according to Yuste".to_string(),
                "Yuste spent a solid thirty minutes analysing the league's declining engagement on his morning stream: \
                \"I've been saying this for months. The product isn't good enough. \
                The format bores people, the scheduling kills momentum. The data is clear.\"\n\n\
                Harsh, but Yuste's audience takes it seriously.".to_string(),
                "be.msg.yuste.down.subject1",
                "be.msg.yuste.down.body1",
            ),
            2 => (
                "el_yuste — Yuste no compra el calendario de la liga".to_string(),
                "Yuste volvió al tema del calendario con pocas ganas de suavizarlo: \
                \"No puedes matar el momentum y luego preguntarte por qué la gente no vuelve. La audiencia no te debe nada.\"\n\n\
                Otra crítica que puede prender en redes.".to_string(),
                "be.msg.yuste.down.subject2",
                "be.msg.yuste.down.body2",
            ),
            3 => (
                "el_yuste — la liga pierde relato, según Yuste".to_string(),
                "El stream matinal fue duro con la narrativa competitiva: \
                \"Hay equipos interesantes, hay jugadores buenos, pero nadie está empaquetando esto para que importe. Si no hay relato, hay ruido de fondo.\"\n\n\
                Puede sonar injusto, pero la comunidad lo está comentando.".to_string(),
                "be.msg.yuste.down.subject3",
                "be.msg.yuste.down.body3",
            ),
            4 => (
                "el_yuste — demasiadas pausas, poca urgencia".to_string(),
                "Yuste apuntó a la experiencia de espectador: \
                \"La partida puede estar bien, pero si entre mapa y mapa me sacas del directo, me voy. No es hate, es comportamiento humano básico.\"\n\n\
                La crítica va directa al producto.".to_string(),
                "be.msg.yuste.down.subject4",
                "be.msg.yuste.down.body4",
            ),
            5 => (
                "el_yuste — el formato vuelve a estar bajo fuego".to_string(),
                "Yuste dedicó un bloque entero al formato: \
                \"No todo se arregla cambiando nombres de fases. La pregunta es si cada partido importa. Si la respuesta es no, la gente lo nota.\"\n\n\
                Otro golpe al debate público de la liga.".to_string(),
                "be.msg.yuste.down.subject5",
                "be.msg.yuste.down.body5",
            ),
            6 => (
                "el_yuste — Yuste pide menos maquillaje y más nivel".to_string(),
                "La frase que quedó del directo fue contundente: \
                \"Puedes ponerle el overlay que quieras. Si el nivel baja y las historias no enganchan, el viewer se va a otro sitio. Punto.\"\n\n\
                Ruido negativo para todos los clubes.".to_string(),
                "be.msg.yuste.down.subject6",
                "be.msg.yuste.down.body6",
            ),
            7 => (
                "el_yuste — la jornada se queda sin clips".to_string(),
                "Yuste fue especialmente ácido con la falta de momentos memorables: \
                \"Acaba la jornada y dime qué clip manda alguien por WhatsApp. Si tienes que pensarlo tanto, ese es el problema.\"\n\n\
                La conversación pública se enfría.".to_string(),
                "be.msg.yuste.down.subject7",
                "be.msg.yuste.down.body7",
            ),
            8 => (
                "el_yuste — la escena necesita autocrítica".to_string(),
                "Yuste pidió menos defensa automática de la liga: \
                \"Querer la escena no es aplaudir todo. A veces quererla es decir que esto no está funcionando antes de que sea tarde.\"\n\n\
                Un mensaje incómodo, pero influyente.".to_string(),
                "be.msg.yuste.down.subject8",
                "be.msg.yuste.down.body8",
            ),
            _ => (
                "el_yuste — los datos vuelven a enfriar el optimismo".to_string(),
                "Yuste abrió gráficos en pantalla y dejó una lectura dura: \
                \"No me habléis de sensaciones si los datos van para abajo. Puedes tener una buena partida suelta, pero la tendencia manda.\"\n\n\
                El debate de audiencias vuelve a activarse.".to_string(),
                "be.msg.yuste.down.subject9",
                "be.msg.yuste.down.body9",
            ),
        }
    };

    InboxMessage::new(
        msg_id.to_string(),
        subject,
        body,
        "el_yuste".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Media)
    .with_priority(MessagePriority::Low)
    .with_sender_role("el_yuste")
    .with_action(action(
        "ack",
        "Noted",
        "be.msg.event.ack",
        ActionType::Acknowledge,
    ))
    .with_i18n(subject_key, body_key, params(&[]))
    .with_sender_i18n("be.sender.elYuste", "be.role.elYuste")
}

pub(super) fn community_event_message(msg_id: &str, team_name: &str, date: &str) -> InboxMessage {
    let mut rng = rand::rng();
    let events = [
        (
            "Community Open Day",
            format!(
                "{} hosted a community open day at the training ground today.\n\n\
                Fans got to meet the players and watch a training session. \
                The atmosphere was fantastic and it's done wonders for team spirit.",
                team_name
            ),
        ),
        (
            "Youth Coaching Session",
            format!(
                "Several first-team players from {} volunteered for a youth coaching session at a local school.\n\n\
                Great PR for the club, and the players seem energized by the experience.",
                team_name
            ),
        ),
    ];
    let idx = rng.random_range(0..events.len());
    let (subject, body) = &events[idx];

    InboxMessage::new(
        msg_id.to_string(),
        subject.to_string(),
        body.clone(),
        "Community Manager".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::System)
    .with_priority(MessagePriority::Low)
    .with_sender_role("Community Manager")
    .with_action(action(
        "ack",
        "Great",
        "be.msg.event.ack",
        ActionType::Acknowledge,
    ))
    .with_i18n(
        &format!("be.msg.community.subject{}", idx),
        &format!("be.msg.community.body{}", idx),
        params(&[("team", team_name)]),
    )
    .with_sender_i18n("be.sender.communityManager", "be.role.communityManager")
}
