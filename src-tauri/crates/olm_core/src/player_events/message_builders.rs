use crate::domain::message::*;
use crate::narrative::{NarrativeSelector, load_default_content_pack};
use rand::RngExt;
use std::collections::HashMap;

/// Helper to build a HashMap<String, String> from key-value pairs.
fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn action(id: &str, label: &str, label_key: &str, action_type: ActionType) -> MessageAction {
    MessageAction {
        id: id.to_string(),
        label: label.to_string(),
        action_type,
        resolved: false,
        label_key: Some(label_key.to_string()),
    }
}

fn option(
    id: &str,
    label: &str,
    label_key: &str,
    description: &str,
    description_key: &str,
) -> ActionOption {
    ActionOption {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        label_key: Some(label_key.to_string()),
        description_key: Some(description_key.to_string()),
    }
}

pub(crate) fn low_morale_message(
    msg_id: &str,
    player_id: &str,
    player_name: &str,
    morale: u8,
    date: &str,
) -> InboxMessage {
    if let Some(message) = build_player_conversation_from_narrative(
        msg_id,
        "low_morale",
        player_id,
        player_name,
        Some(i64::from(morale)),
        date,
    ) {
        return message;
    }

    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "low_morale",
        msg_id,
        date,
        "en",
        vec![("player", player_name), ("morale", &morale.to_string())],
    ) {
        return msg;
    }

    let mut rng = rand::rng();
    let variations = [
        format!(
            "Boss, {} has asked for a private meeting. They seem really down lately and want to talk about their situation at the club.\n\n\
            Their morale is at {} — you should address this before it affects the dressing room.",
            player_name, morale
        ),
        format!(
            "{} has been looking dejected in training. They've requested a chat with you about their current state of mind.\n\n\
            Morale: {}. How you handle this could make or break their confidence.",
            player_name, morale
        ),
    ];
    let idx = rng.random_range(0..variations.len());

    InboxMessage::new(
        msg_id.to_string(),
        format!("{} — Morale Crisis", player_name),
        variations[idx].clone(),
        player_name.to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::PlayerMorale)
    .with_priority(MessagePriority::High)
    .with_sender_role("Player")
    .with_action(action(
        "respond",
        "Respond",
        "be.msg.playerEvent.respond",
        ActionType::ChooseOption {
            options: vec![
                option(
                    "encourage",
                    "Encourage them",
                    "be.msg.playerEvent.options.moraleCrisis.encourage.label",
                    "Show empathy and encourage the player to keep working hard.",
                    "be.msg.playerEvent.options.moraleCrisis.encourage.description",
                ),
                option(
                    "promise_time",
                    "Promise more playing time",
                    "be.msg.playerEvent.options.moraleCrisis.promiseTime.label",
                    "Tell them they'll get their chance — bigger morale boost but sets expectations.",
                    "be.msg.playerEvent.options.moraleCrisis.promiseTime.description",
                ),
                option(
                    "work_harder",
                    "Tell them to work harder",
                    "be.msg.playerEvent.options.moraleCrisis.workHarder.label",
                    "Tough love approach — could backfire or motivate them.",
                    "be.msg.playerEvent.options.moraleCrisis.workHarder.description",
                ),
            ],
        },
    ))
    .with_context(MessageContext {
        player_id: Some(player_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.moraleCrisis.subject",
        &format!("be.msg.moraleCrisis.body{}", idx),
        params(&[("player", player_name), ("morale", &morale.to_string())]),
    )
    .with_sender_i18n("be.sender.player", "be.role.player")
}

pub(crate) fn bench_complaint_message(
    msg_id: &str,
    player_id: &str,
    player_name: &str,
    date: &str,
) -> InboxMessage {
    if let Some(message) = build_player_conversation_from_narrative(
        msg_id,
        "bench_complaint",
        player_id,
        player_name,
        None,
        date,
    ) {
        return message;
    }

    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "bench_complaint",
        msg_id,
        date,
        "en",
        vec![("player", player_name)],
    ) {
        return msg;
    }

    let mut rng = rand::rng();
    let variations = [
        format!(
            "Boss, {} has come to see you. They're frustrated about their lack of game time in recent matches and want to know what they need to do to get back in the team.\n\n\
            \"I feel like I've been training well, but I'm not getting a chance to show it on the pitch.\"",
            player_name
        ),
        format!(
            "{} knocked on your office door looking unhappy. They haven't featured in the last few matches and want answers.\n\n\
            \"I came to this club to play competitively. If I'm not in your plans, I'd rather you tell me straight.\"",
            player_name
        ),
    ];
    let idx = rng.random_range(0..variations.len());

    InboxMessage::new(
        msg_id.to_string(),
        format!("{} — Wants More Game Time", player_name),
        variations[idx].clone(),
        player_name.to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::PlayerMorale)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Player")
    .with_action(action(
        "respond",
        "Respond",
        "be.msg.playerEvent.respond",
        ActionType::ChooseOption {
            options: vec![
                option(
                    "explain",
                    "Explain the situation",
                    "be.msg.playerEvent.options.benchComplaint.explain.label",
                    "Calmly explain squad competition and rotation. Steady morale boost.",
                    "be.msg.playerEvent.options.benchComplaint.explain.description",
                ),
                option(
                    "promise_chance",
                    "Promise them a chance soon",
                    "be.msg.playerEvent.options.benchComplaint.promiseChance.label",
                    "They'll be happier but will expect to start in upcoming matches.",
                    "be.msg.playerEvent.options.benchComplaint.promiseChance.description",
                ),
                option(
                    "prove_yourself",
                    "Tell them to prove themselves",
                    "be.msg.playerEvent.options.benchComplaint.proveYourself.label",
                    "Challenge them to earn their place. Risky — could motivate or frustrate.",
                    "be.msg.playerEvent.options.benchComplaint.proveYourself.description",
                ),
            ],
        },
    ))
    .with_context(MessageContext {
        player_id: Some(player_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.benchComplaint.subject",
        &format!("be.msg.benchComplaint.body{}", idx),
        params(&[("player", player_name)]),
    )
    .with_sender_i18n("be.sender.player", "be.role.player")
}

pub(crate) fn happy_player_message(
    msg_id: &str,
    player_id: &str,
    player_name: &str,
    date: &str,
) -> InboxMessage {
    if let Some(message) = build_player_conversation_from_narrative(
        msg_id,
        "happy_player",
        player_id,
        player_name,
        None,
        date,
    ) {
        return message;
    }

    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "happy_player",
        msg_id,
        date,
        "en",
        vec![("player", player_name), ("morale", "100")],
    ) {
        return msg;
    }

    let mut rng = rand::rng();
    let variations = [
        format!(
            "{} stopped by your office with a big smile. They're feeling great about their form and the team's direction.\n\n\
            \"Just wanted to say I'm really enjoying my game right now, boss. The mood in the team room is fantastic.\"",
            player_name
        ),
        format!(
            "Your assistant mentions that {} has been in excellent spirits lately. They approached you after training.\n\n\
            \"Boss, I'm loving every minute here. Keep things going like this and I'll run through walls for you.\"",
            player_name
        ),
    ];
    let idx = rng.random_range(0..variations.len());

    InboxMessage::new(
        msg_id.to_string(),
        format!("{} — Feeling Great", player_name),
        variations[idx].clone(),
        player_name.to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::PlayerMorale)
    .with_priority(MessagePriority::Low)
    .with_sender_role("Player")
    .with_action(action(
        "respond",
        "Respond",
        "be.msg.playerEvent.respond",
        ActionType::ChooseOption {
            options: vec![
                option(
                    "praise_back",
                    "Return the praise",
                    "be.msg.playerEvent.options.happyPlayer.praiseBack.label",
                    "Tell them how much you value their contribution.",
                    "be.msg.playerEvent.options.happyPlayer.praiseBack.description",
                ),
                option(
                    "stay_professional",
                    "Stay professional",
                    "be.msg.playerEvent.options.happyPlayer.stayProfessional.label",
                    "Acknowledge their form but keep things measured.",
                    "be.msg.playerEvent.options.happyPlayer.stayProfessional.description",
                ),
                option(
                    "higher_expectations",
                    "Set higher expectations",
                    "be.msg.playerEvent.options.happyPlayer.higherExpectations.label",
                    "Challenge them to reach an even higher level. Could push or pressure.",
                    "be.msg.playerEvent.options.happyPlayer.higherExpectations.description",
                ),
            ],
        },
    ))
    .with_context(MessageContext {
        player_id: Some(player_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.happyPlayer.subject",
        &format!("be.msg.happyPlayer.body{}", idx),
        params(&[("player", player_name)]),
    )
    .with_sender_i18n("be.sender.player", "be.role.player")
}

pub(crate) fn contract_concern_message(
    msg_id: &str,
    player_id: &str,
    player_name: &str,
    days_remaining: i64,
    date: &str,
) -> InboxMessage {
    let months = (days_remaining as f64 / 30.0).ceil() as u32;
    if let Some(message) = build_player_conversation_from_narrative(
        msg_id,
        "contract_concern",
        player_id,
        player_name,
        Some(days_remaining),
        date,
    ) {
        return message;
    }

    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "contract_concern",
        msg_id,
        date,
        "en",
        vec![
            ("player", player_name),
            ("days", &days_remaining.to_string()),
        ],
    ) {
        return msg;
    }

    let mut rng = rand::rng();
    let variations = [
        format!(
            "{} has approached you regarding their contract situation. With only {} days remaining on their deal, they want to know where they stand.\n\n\
            \"Boss, my contract is running down. I need to know if I'm part of your plans going forward or if I should start looking elsewhere.\"",
            player_name, days_remaining
        ),
        format!(
            "Your assistant flags that {}'s contract expires in roughly {} month(s). The player has been asking around the dressing room about their future.\n\n\
            It might be wise to have a conversation before they become unsettled — or before other clubs start circling.",
            player_name, months
        ),
    ];
    let idx = rng.random_range(0..variations.len());

    InboxMessage::new(
        msg_id.to_string(),
        format!("{} — Contract Running Down", player_name),
        variations[idx].clone(),
        "Assistant Manager".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Contract)
    .with_priority(MessagePriority::High)
    .with_sender_role("Assistant Manager")
    .with_action(action(
        "respond",
        "Respond",
        "be.msg.playerEvent.respond",
        ActionType::ChooseOption {
            options: vec![
                option(
                    "reassure",
                    "Reassure them about renewal",
                    "be.msg.playerEvent.options.contractConcern.reassure.label",
                    "Tell them you want them to stay. Big morale boost.",
                    "be.msg.playerEvent.options.contractConcern.reassure.description",
                ),
                option(
                    "noncommittal",
                    "Be noncommittal",
                    "be.msg.playerEvent.options.contractConcern.noncommittal.label",
                    "Keep your options open. Player may become unsettled.",
                    "be.msg.playerEvent.options.contractConcern.noncommittal.description",
                ),
                option(
                    "no_renewal",
                    "Tell them you won't renew",
                    "be.msg.playerEvent.options.contractConcern.noRenewal.label",
                    "Honest but brutal. Morale will tank.",
                    "be.msg.playerEvent.options.contractConcern.noRenewal.description",
                ),
            ],
        },
    ))
    .with_context(MessageContext {
        player_id: Some(player_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.contractConcern.subject",
        &format!("be.msg.contractConcern.body{}", idx),
        params(&[
            ("player", player_name),
            ("days", &days_remaining.to_string()),
            ("months", &months.to_string()),
        ]),
    )
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}

pub fn build_player_conversation_from_narrative(
    msg_id: &str,
    conversation_kind: &str,
    player_id: &str,
    player_name: &str,
    context_value: Option<i64>,
    date: &str,
) -> Option<InboxMessage> {
    let pack = load_default_content_pack().ok()?;
    let selector = NarrativeSelector::new(&pack);
    let tags = conversation_tags(conversation_kind)?;
    let template = selector.select_conversation(Some("default"), &tags)?;
    let (subject, body, sender, priority, options) =
        conversation_copy_and_options(conversation_kind, player_name, context_value)?;

    Some(
        InboxMessage::new(
            msg_id.to_string(),
            subject,
            body,
            sender.to_string(),
            date.to_string(),
        )
        .with_category(if conversation_kind == "contract_concern" {
            MessageCategory::Contract
        } else {
            MessageCategory::PlayerMorale
        })
        .with_priority(priority)
        .with_sender_role(sender)
        .with_action(action(
            "respond",
            "Respond",
            "be.msg.playerEvent.respond",
            ActionType::ChooseOption { options },
        ))
        .with_context(MessageContext {
            player_id: Some(player_id.to_string()),
            ..Default::default()
        })
        .with_i18n(
            conversation_subject_key(conversation_kind)?,
            &template.template_key,
            conversation_params(player_name, context_value, &template.effect_id),
        )
        .with_sender_i18n(
            if conversation_kind == "contract_concern" {
                "be.sender.assistantManager"
            } else {
                "be.sender.player"
            },
            if conversation_kind == "contract_concern" {
                "be.role.assistantManager"
            } else {
                "be.role.player"
            },
        ),
    )
}

fn conversation_tags(conversation_kind: &str) -> Option<Vec<&'static str>> {
    match conversation_kind {
        "low_morale" => Some(vec!["underperformance", "pressure"]),
        "bench_complaint" => Some(vec!["stage_time", "pressure"]),
        "happy_player" => Some(vec!["form", "positive"]),
        "contract_concern" => Some(vec!["contract", "pressure"]),
        _ => None,
    }
}

fn conversation_subject_key(conversation_kind: &str) -> Option<&'static str> {
    match conversation_kind {
        "low_morale" => Some("be.msg.moraleCrisis.subject"),
        "bench_complaint" => Some("be.msg.benchComplaint.subject"),
        "happy_player" => Some("be.msg.happyPlayer.subject"),
        "contract_concern" => Some("be.msg.contractConcern.subject"),
        _ => None,
    }
}

fn conversation_params(
    player_name: &str,
    context_value: Option<i64>,
    effect_id: &str,
) -> HashMap<String, String> {
    let mut values = HashMap::from([
        ("player".to_string(), player_name.to_string()),
        ("effectId".to_string(), effect_id.to_string()),
    ]);
    if let Some(value) = context_value {
        values.insert("morale".to_string(), value.to_string());
        values.insert("days".to_string(), value.to_string());
        values.insert(
            "months".to_string(),
            ((value as f64 / 30.0).ceil() as i64).to_string(),
        );
    }
    values
}

fn conversation_copy_and_options(
    conversation_kind: &str,
    player_name: &str,
    context_value: Option<i64>,
) -> Option<(
    String,
    String,
    &'static str,
    MessagePriority,
    Vec<ActionOption>,
)> {
    match conversation_kind {
        "low_morale" => Some((
            format!("{} — Mental Reset Needed", player_name),
            format!(
                "{} has asked for a private reset after a rough scrim block. Their morale is at {}, and the player is worried their stage confidence is starting to affect team comms.\n\n\
                \"I need to reset before this pressure follows me into the next series, boss.\"",
                player_name,
                context_value.unwrap_or_default()
            ),
            "Player",
            MessagePriority::High,
            vec![
                option(
                    "encourage",
                    "Encourage them",
                    "be.msg.playerEvent.options.moraleCrisis.encourage.label",
                    "Show empathy and help them reset around scrims and stage pressure.",
                    "be.msg.playerEvent.options.moraleCrisis.encourage.description",
                ),
                option(
                    "promise_time",
                    "Promise more stage time",
                    "be.msg.playerEvent.options.moraleCrisis.promiseTime.label",
                    "Tell them they'll get their chance in an upcoming series — bigger morale boost but sets expectations.",
                    "be.msg.playerEvent.options.moraleCrisis.promiseTime.description",
                ),
                option(
                    "work_harder",
                    "Challenge their preparation",
                    "be.msg.playerEvent.options.moraleCrisis.workHarder.label",
                    "Tough love around practice discipline — could backfire or motivate them.",
                    "be.msg.playerEvent.options.moraleCrisis.workHarder.description",
                ),
            ],
        )),
        "bench_complaint" => Some((
            format!("{} — Wants More Stage Time", player_name),
            format!(
                "{} came in frustrated about limited stage reps. They feel their scrims have been strong and want clarity on what earns them a spot in the next series.\n\n\
                \"If I'm not in the plan for stage time, I need to know what has to change.\"",
                player_name
            ),
            "Player",
            MessagePriority::Normal,
            vec![
                option(
                    "explain",
                    "Explain the rotation",
                    "be.msg.playerEvent.options.benchComplaint.explain.label",
                    "Calmly explain roster competition and map-specific plans. Steady morale boost.",
                    "be.msg.playerEvent.options.benchComplaint.explain.description",
                ),
                option(
                    "promise_chance",
                    "Promise a chance soon",
                    "be.msg.playerEvent.options.benchComplaint.promiseChance.label",
                    "They'll be happier but will expect to start in an upcoming series.",
                    "be.msg.playerEvent.options.benchComplaint.promiseChance.description",
                ),
                option(
                    "prove_yourself",
                    "Tell them to prove it in scrims",
                    "be.msg.playerEvent.options.benchComplaint.proveYourself.label",
                    "Challenge them to earn their place. Risky — could motivate or frustrate.",
                    "be.msg.playerEvent.options.benchComplaint.proveYourself.description",
                ),
            ],
        )),
        "happy_player" => Some((
            format!("{} — Feeling Locked In", player_name),
            format!(
                "{} checked in after a strong run of solo queue and team practice. They're enjoying the draft room clarity and believe the roster is close to finding another level.\n\n\
                \"Keep the structure like this and I'll bring that confidence onto stage.\"",
                player_name
            ),
            "Player",
            MessagePriority::Low,
            vec![
                option(
                    "praise_back",
                    "Return the praise",
                    "be.msg.playerEvent.options.happyPlayer.praiseBack.label",
                    "Tell them how much you value their contribution.",
                    "be.msg.playerEvent.options.happyPlayer.praiseBack.description",
                ),
                option(
                    "stay_professional",
                    "Stay professional",
                    "be.msg.playerEvent.options.happyPlayer.stayProfessional.label",
                    "Acknowledge their form but keep things measured.",
                    "be.msg.playerEvent.options.happyPlayer.stayProfessional.description",
                ),
                option(
                    "higher_expectations",
                    "Set higher expectations",
                    "be.msg.playerEvent.options.happyPlayer.higherExpectations.label",
                    "Challenge them to reach an even higher level. Could push or pressure.",
                    "be.msg.playerEvent.options.happyPlayer.higherExpectations.description",
                ),
            ],
        )),
        "contract_concern" => Some((
            format!("{} — Contract Running Down", player_name),
            format!(
                "{} wants clarity before the next league window. With {} days left, they're asking whether they're part of the roster plan or should prepare for other offers.\n\n\
                \"I need to know if this project still has a place for me after the split.\"",
                player_name,
                context_value.unwrap_or_default()
            ),
            "Assistant Manager",
            MessagePriority::High,
            vec![
                option(
                    "reassure",
                    "Reassure them about renewal",
                    "be.msg.playerEvent.options.contractConcern.reassure.label",
                    "Tell them you want them to stay. Big morale boost.",
                    "be.msg.playerEvent.options.contractConcern.reassure.description",
                ),
                option(
                    "noncommittal",
                    "Be noncommittal",
                    "be.msg.playerEvent.options.contractConcern.noncommittal.label",
                    "Keep your options open. Player may become unsettled.",
                    "be.msg.playerEvent.options.contractConcern.noncommittal.description",
                ),
                option(
                    "no_renewal",
                    "Tell them you won't renew",
                    "be.msg.playerEvent.options.contractConcern.noRenewal.label",
                    "Honest but brutal. Morale will tank.",
                    "be.msg.playerEvent.options.contractConcern.noRenewal.description",
                ),
            ],
        )),
        _ => None,
    }
}
