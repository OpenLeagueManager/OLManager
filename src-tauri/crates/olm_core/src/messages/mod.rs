mod match_messages;
pub mod template_store;
pub use match_messages::{match_result_message, pre_match_message};

use crate::domain::message::*;
use rand::RngExt;
use std::collections::HashMap;

/// Helper to format fee values for display.
fn format_fee(fee: u64) -> String {
    if fee >= 1_000_000 {
        format!("€{:.1}M", fee as f64 / 1_000_000.0)
    } else if fee >= 1_000 {
        format!("€{}K", fee / 1_000)
    } else {
        format!("€{}", fee)
    }
}

/// Apply sender info (name, role, icon) from the senders store to a message.
/// Falls back silently if the sender ID is not found.
pub fn with_sender(
    mut msg: InboxMessage,
    sender_id: &str,
    i18n_params: Vec<(&str, &str)>,
) -> InboxMessage {
    if let Some(sender) = crate::messages::template_store::get_sender(sender_id) {
        let params: std::collections::HashMap<String, String> = i18n_params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        msg.sender = interpolate_text(&sender.name, &params);
        msg.sender_role = interpolate_text(&sender.role, &params);
        if let Some(icon) = &sender.icon {
            msg.sender_icon = Some(icon.clone());
        }
        if let Some(key) = &sender.name_key {
            msg.sender_key = Some(key.clone());
        }
        if let Some(key) = &sender.role_key {
            msg.sender_role_key = Some(key.clone());
        }
    }
    msg
}

fn interpolate_text(text: &str, params: &std::collections::HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (key, value) in params {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

/// Helper to build a HashMap<String, String> from key-value pairs.
fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Helper to create a MessageAction with an i18n label key.
fn action(id: &str, label: &str, label_key: &str, action_type: ActionType) -> MessageAction {
    MessageAction {
        id: id.to_string(),
        label: label.to_string(),
        action_type,
        resolved: false,
        label_key: Some(label_key.to_string()),
    }
}

/// Message template system — generates rich messages with variations.

pub fn welcome_message(team_name: &str, team_id: &str, date: &str, lang: &str) -> InboxMessage {
    // Use template store (with inline translations) if available
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "select_team",
        &format!("welcome_{}", team_id),
        date,
        lang,
        vec![("team", team_name), ("days", "0")],
    ) {
        return msg;
    }

    // Legacy fallback
    let mut rng = rand::rng();
    let variations = [
        (
            format!("Welcome to {}", team_name),
            format!(
                "The board of directors at {} is delighted to welcome you as the new manager.\n\n\
                We have high hopes for your tenure and believe you can lead this club to glory. \
                Your first task will be to review the squad and prepare a tactical plan for the upcoming season.\n\n\
                We wish you the best of luck.",
                team_name
            ),
        ),
        (
            format!("New Era at {}", team_name),
            format!(
                "On behalf of the entire {} family, we are thrilled to announce your appointment as manager.\n\n\
                The fans are eager to see your vision for the team. Please take time to assess the squad, \
                review our financial position, and set your tactical approach.\n\n\
                The board stands behind you.",
                team_name
            ),
        ),
        (
            format!("{} Awaits Your Leadership", team_name),
            format!(
                "Welcome to {}! The supporters and staff are excited about the future under your guidance.\n\n\
                We recommend you start by reviewing your squad's strengths and weaknesses, \
                then set up your preferred formation and training regime.\n\n\
                The upcoming season will be a true test — make us proud.",
                team_name
            ),
        ),
    ];

    let idx = rng.random_range(0..variations.len());
    let (subject, body) = &variations[idx];

    InboxMessage::new(
        format!("welcome_{}", team_id),
        subject.clone(),
        body.clone(),
        "Board of Directors".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Welcome)
    .with_priority(MessagePriority::High)
    .with_sender_role("Chairman")
    .with_action(action(
        "review_squad",
        "Review Squad",
        "be.msg.welcome.actionReview",
        ActionType::NavigateTo {
            route: "/dashboard?tab=Squad".to_string(),
        },
    ))
    .with_action(action(
        "ack_welcome",
        "Thank the Board",
        "be.msg.welcome.actionThank",
        ActionType::Acknowledge,
    ))
    .with_context(MessageContext {
        team_id: Some(team_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        &format!("be.msg.welcome.subject{}", idx),
        &format!("be.msg.welcome.body{}", idx),
        params(&[("team", team_name)]),
    )
    .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman")
}

pub fn season_schedule_message(league_name: &str, season_start: &str, date: &str) -> InboxMessage {
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "season_schedule",
        "season_schedule",
        date,
        "en",
        vec![("league", league_name), ("start", season_start)],
    ) {
        return msg;
    }
    let mut rng = rand::rng();
    let variations = [
        format!(
            "The {} schedule has been released. The season kicks off on {}.\n\n\
            Review the fixture list and ensure your squad is ready for the challenges ahead. \
            Pre-season preparation will be crucial.",
            league_name, season_start
        ),
        format!(
            "Fixture list confirmed! The {} season begins on {}.\n\n\
            Study the opening fixtures carefully — a strong start can set the tone for the whole campaign. \
            Make sure your key players are match-fit.",
            league_name, season_start
        ),
    ];

    let idx = rng.random_range(0..variations.len());

    InboxMessage::new(
        "season_1".to_string(),
        "Season Schedule Released".to_string(),
        variations[idx].clone(),
        "League Office".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::LeagueInfo)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Competition Secretary")
    .with_action(action(
        "view_schedule",
        "View Fixtures",
        "be.msg.schedule.actionView",
        ActionType::NavigateTo {
            route: "/dashboard?tab=Schedule".to_string(),
        },
    ))
    .with_i18n(
        "be.msg.schedule.subject",
        &format!("be.msg.schedule.body{}", idx),
        params(&[("league", league_name), ("start", season_start)]),
    )
    .with_sender_i18n("be.sender.leagueOffice", "be.role.competitionSecretary")
}

pub fn staff_advice_message(team_name: &str, team_id: &str, date: &str) -> InboxMessage {
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "staff_advice",
        &format!("staff_{team_id}"),
        date,
        "en",
        vec![("team", team_name)],
    ) {
        return msg;
    }
    InboxMessage::new(
        "staff_advice_1".to_string(),
        "Staff Report — Coaching Vacancies".to_string(),
        format!(
            "Boss, I've had a look at the staff situation at {} and wanted to flag a few things:\n\n\
            • A good **Coach** will significantly improve training effectiveness — your players will develop faster.\n\
            • A qualified **Physio** helps speed up recovery between matches and keep players match-fit.\n\
            • Our **Scouts** can help identify transfer targets and assess opponents.\n\n\
            I'd strongly recommend filling any vacancies before the season starts. \
            You can find available staff in the Staff section.\n\n\
            Check it out when you get a chance.",
            team_name
        ),
        "Assistant Manager".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Training)
    .with_priority(MessagePriority::High)
    .with_sender_role("Assistant Manager")
    .with_action(action("view_staff", "View Staff", "be.msg.staffAdvice.actionView", ActionType::NavigateTo { route: "/dashboard?tab=Staff".to_string() }))
    .with_context(MessageContext {
        team_id: Some(team_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.staffAdvice.subject",
        "be.msg.staffAdvice.body",
        params(&[("team", team_name)]),
    )
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}

pub fn board_expectations_message(team_name: &str, team_id: &str, date: &str) -> InboxMessage {
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "board_expectations",
        &format!("expectations_{team_id}"),
        date,
        "en",
        vec![("team", team_name)],
    ) {
        return msg;
    }
    InboxMessage::new(
        "board_expect_1".to_string(),
        format!("{} — Season Objectives", team_name),
        "The board has set the following expectations for this season:\n\n\
            • Reach the top half of the LEC table\n\
            • Maintain financial stability\n\
            • Develop academy talent for the roster pipeline\n\n\
            Meeting these objectives will strengthen your position. Failure to meet minimum \
            expectations may result in a review of your tenure.\n\n\
            We trust in your abilities."
            .to_string(),
        "Board of Directors".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::BoardDirective)
    .with_priority(MessagePriority::High)
    .with_sender_role("Chairman")
    .with_action(action(
        "ack_objectives",
        "Accept Objectives",
        "be.msg.boardExpect.actionAccept",
        ActionType::Acknowledge,
    ))
    .with_context(MessageContext {
        team_id: Some(team_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.boardExpect.subject",
        "be.msg.boardExpect.body",
        params(&[("team", team_name)]),
    )
    .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman")
}

pub fn transfer_complete_message(player_name: &str, fee: u64, date: &str) -> InboxMessage {
    let fee_display = format_fee(fee);
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "transfer_complete",
        &format!("transfer_{player_name}"),
        date,
        "en",
        vec![("player", player_name), ("fee", &fee_display)],
    ) {
        return msg;
    }
    let id = format!("transfer_{}", uuid::Uuid::new_v4());
    InboxMessage::new(
        id,
        format!("Transfer Complete: {}", player_name),
        format!(
            "The transfer of {} has been completed for a fee of {}.\n\n\
            The player has joined the squad and is available for selection.",
            player_name, fee_display
        ),
        "Transfer Committee".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Transfer)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Director of Football")
    .with_i18n(
        "be.msg.transferComplete.subject",
        "be.msg.transferComplete.body",
        params(&[("player", player_name), ("fee", &fee_display)]),
    )
    .with_sender_i18n("be.sender.directorOfFootball", "be.role.directorOfFootball")
}

pub fn incoming_transfer_offer_message(
    offer_id: &str,
    player_id: &str,
    player_name: &str,
    buying_team_name: &str,
    fee: u64,
    date: &str,
) -> InboxMessage {
    let fee_display = format_fee(fee);
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "transfer_offer",
        &format!("offer_{offer_id}"),
        date,
        "en",
        vec![
            ("player", player_name),
            ("buyer", buying_team_name),
            ("fee", &fee_display),
        ],
    ) {
        return msg;
    }
    InboxMessage::new(
        format!("transfer_offer_{}", offer_id),
        format!("Incoming Offer for {}", player_name),
        format!(
            "{} have submitted an offer of {} for {}. Review the bid in the Transfers tab to accept or reject it.",
            buying_team_name, fee_display, player_name
        ),
        "Director of Football".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Transfer)
    .with_priority(MessagePriority::High)
    .with_sender_role("Director of Football")
    .with_action(action(
        "view_transfers",
        "Review Offer",
        "be.msg.transferOffer.actionReview",
        ActionType::NavigateTo {
            route: "/dashboard?tab=Transfers".to_string(),
        },
    ))
    .with_context(MessageContext {
        player_id: Some(player_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.transferOffer.subject",
        "be.msg.transferOffer.body",
        params(&[
            ("club", buying_team_name),
            ("fee", &fee_display),
            ("player", player_name),
        ]),
    )
    .with_sender_i18n("be.sender.directorOfFootball", "be.role.directorOfFootball")
}
