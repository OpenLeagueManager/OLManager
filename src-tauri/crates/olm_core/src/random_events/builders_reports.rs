use super::{action, params};
use crate::domain::message::*;
use crate::narrative::{NarrativeSelector, load_default_content_pack};
use rand::RngExt;

// ---------------------------------------------------------------------------
// Periodic / condition-triggered message builders
// ---------------------------------------------------------------------------

pub(super) fn mood_report_message(
    msg_id: &str,
    avg_morale: f64,
    low_count: usize,
    high_count: usize,
    total: usize,
    date: &str,
) -> InboxMessage {
    let mood = if avg_morale >= 75.0 {
        "common.moods.excellent"
    } else if avg_morale >= 60.0 {
        "common.moods.good"
    } else if avg_morale >= 45.0 {
        "common.moods.mixed"
    } else {
        "common.moods.poor"
    };

    let body = format!(
        "Here's your weekly dressing room report:\n\n\
        • Overall mood: {} (avg morale: {:.0})\n\
        • Players in high spirits (80+): {}\n\
        • Players with low morale (<40): {}\n\
        • Total squad: {}\n\n\
        {}",
        mood,
        avg_morale,
        high_count,
        low_count,
        total,
        if low_count >= 3 {
            "Several players are unhappy. You should address individual concerns before it spreads."
        } else if avg_morale >= 75.0 {
            "The dressing room is buzzing. Keep up the good work!"
        } else if avg_morale < 45.0 {
            "Morale is worryingly low. Consider positive team talks and results to turn things around."
        } else {
            "Morale is stable. A few good results would really lift the mood."
        }
    );

    InboxMessage::new(
        msg_id.to_string(),
        format!("Dressing Room Report — Mood: {}", mood),
        body,
        "Assistant Manager".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::PlayerMorale)
    .with_priority(if low_count >= 3 || avg_morale < 40.0 {
        MessagePriority::High
    } else {
        MessagePriority::Low
    })
    .with_sender_role("Assistant Manager")
    .with_action(action(
        "ack",
        "Thanks",
        "be.msg.event.ack",
        ActionType::Acknowledge,
    ))
    .with_i18n("be.msg.moodReport.subject", "be.msg.moodReport.body", {
        let mut p = params(&[("mood", mood)]);
        p.insert("avgMorale".to_string(), format!("{:.0}", avg_morale));
        p.insert("highCount".to_string(), high_count.to_string());
        p.insert("lowCount".to_string(), low_count.to_string());
        p.insert("total".to_string(), total.to_string());
        p
    })
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}

pub(super) fn board_confidence_message(msg_id: &str, date: &str) -> InboxMessage {
    let mut rng = rand::rng();
    let variations = [
        "The board has called an urgent meeting. Three consecutive defeats have raised serious concerns about the team's direction.\n\n\
        \"We need to see improvement quickly. The fans are restless and results must change.\"\n\n\
        How do you respond?",
        "After a string of poor results, the chairman has summoned you for a difficult conversation.\n\n\
        \"We backed you with resources and time. The results simply aren't good enough. What's your plan?\"\n\n\
        Choose your response carefully.",
    ];
    let idx = rng.random_range(0..variations.len());

    InboxMessage::new(
        msg_id.to_string(),
        "Board Meeting — Results Under Scrutiny".to_string(),
        variations[idx].to_string(),
        "Board of Directors".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::BoardDirective)
    .with_priority(MessagePriority::Urgent)
    .with_sender_role("Chairman")
    .with_action(action(
        "respond",
        "Respond",
        "be.msg.event.respond",
        ActionType::ChooseOption {
            options: vec![
                ActionOption {
                    id: "reassure_board".to_string(),
                    label: "Reassure them with a plan".to_string(),
                    description:
                        "Present a clear strategy for turning things around. Buys you time."
                            .to_string(),
                    label_key: Some(
                        "be.msg.boardConfidence.options.reassureBoard.label".to_string(),
                    ),
                    description_key: Some(
                        "be.msg.boardConfidence.options.reassureBoard.description".to_string(),
                    ),
                },
                ActionOption {
                    id: "accept_pressure".to_string(),
                    label: "Accept responsibility".to_string(),
                    description: "Own the poor results. The board respects honesty.".to_string(),
                    label_key: Some(
                        "be.msg.boardConfidence.options.acceptPressure.label".to_string(),
                    ),
                    description_key: Some(
                        "be.msg.boardConfidence.options.acceptPressure.description".to_string(),
                    ),
                },
                ActionOption {
                    id: "blame_circumstances".to_string(),
                    label: "Blame external factors and poor conditions".to_string(),
                    description: "Deflect blame to external factors. May or may not convince them."
                        .to_string(),
                    label_key: Some(
                        "be.msg.boardConfidence.options.blameCircumstances.label".to_string(),
                    ),
                    description_key: Some(
                        "be.msg.boardConfidence.options.blameCircumstances.description".to_string(),
                    ),
                },
            ],
        },
    ))
    .with_i18n(
        "be.msg.boardConfidence.subject",
        &format!("be.msg.boardConfidence.body{}", idx),
        params(&[]),
    )
    .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman")
}

pub(super) fn fan_petition_message(msg_id: &str, team_name: &str, date: &str) -> InboxMessage {
    if let Some(message) = build_fan_petition_from_narrative(msg_id, team_name, date) {
        return message;
    }

    let subject = "Fan Petition — Draft Identity".to_string();
    let body = format!(
        "A group of {} supporters has organized a petition asking for a clearer draft identity and sharper objective setups.\n\n\
        \"We want proactive plans on the Rift, not passive scaling every series. Show us the team knows how it wants to win.\"\n\n\
        Over 500 signatures so far. How do you respond?",
        team_name
    );

    InboxMessage::new(
        msg_id.to_string(),
        subject,
        body,
        "Community Manager".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Media)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Community Manager")
    .with_action(action(
        "respond", "Respond", "be.msg.event.respond",
        ActionType::ChooseOption {
            options: vec![
                ActionOption {
                    id: "listen_fans".to_string(),
                    label: "Engage with the fans".to_string(),
                    description: "Meet with fan representatives and listen to their concerns. Good for morale.".to_string(),
                    label_key: Some("be.msg.fanPetition.options.listenFans.label".to_string()),
                    description_key: Some("be.msg.fanPetition.options.listenFans.description".to_string()),
                },
                ActionOption {
                    id: "ignore_fans".to_string(),
                    label: "Focus on prep".to_string(),
                    description: "Politely decline — competitive decisions stay with the staff.".to_string(),
                    label_key: Some("be.msg.fanPetition.options.ignoreFans.label".to_string()),
                    description_key: Some("be.msg.fanPetition.options.ignoreFans.description".to_string()),
                },
                ActionOption {
                    id: "address_publicly".to_string(),
                    label: "Make a public statement".to_string(),
                    description: "Address the petition in a press conference. Transparent and proactive.".to_string(),
                    label_key: Some("be.msg.fanPetition.options.addressPublicly.label".to_string()),
                    description_key: Some("be.msg.fanPetition.options.addressPublicly.description".to_string()),
                },
            ],
        },
    ))
        .with_i18n(
        "be.msg.fanPetition.subject",
        "be.msg.fanPetition.body",
        params(&[("team", team_name)]),
    )
    .with_sender_i18n("be.sender.communityManager", "be.role.communityManager")
}

pub fn build_fan_petition_from_narrative(
    msg_id: &str,
    team_name: &str,
    date: &str,
) -> Option<InboxMessage> {
    let pack = load_default_content_pack().ok()?;
    let selector = NarrativeSelector::new(&pack);
    let template = selector.select_event(
        Some("default"),
        &["fan", "petition"],
        &["spicy", "community"],
    )?;

    Some(
        InboxMessage::new(
            msg_id.to_string(),
            "Fan Petition — Draft Identity".to_string(),
            format!(
                "A wave of {} supporters has organized a campaign asking for a clearer draft identity and sharper objective setups.\n\n\
                \"We want proactive plans on the Rift, not passive scaling every series. Show us the team knows how it wants to win.\"\n\n\
                The thread is gaining traction across esports social channels. How do you respond?",
                team_name
            ),
            "Community Manager".to_string(),
            date.to_string(),
        )
        .with_category(MessageCategory::Media)
        .with_priority(MessagePriority::Normal)
        .with_sender_role("Community Manager")
        .with_action(action(
            "respond", "Respond", "be.msg.event.respond",
            ActionType::ChooseOption {
                options: vec![
                    ActionOption {
                        id: "listen_fans".to_string(),
                        label: "Engage with the fans".to_string(),
                        description: "Meet with fan representatives and explain the team's competitive direction. Good for morale.".to_string(),
                        label_key: Some("be.msg.fanPetition.options.listenFans.label".to_string()),
                        description_key: Some("be.msg.fanPetition.options.listenFans.description".to_string()),
                    },
                    ActionOption {
                        id: "ignore_fans".to_string(),
                        label: "Focus on prep".to_string(),
                        description: "Politely decline — draft and roster decisions stay with the competitive staff.".to_string(),
                        label_key: Some("be.msg.fanPetition.options.ignoreFans.label".to_string()),
                        description_key: Some("be.msg.fanPetition.options.ignoreFans.description".to_string()),
                    },
                    ActionOption {
                        id: "address_publicly".to_string(),
                        label: "Make a public statement".to_string(),
                        description: "Address the petition in media. Transparent and proactive.".to_string(),
                        label_key: Some("be.msg.fanPetition.options.addressPublicly.label".to_string()),
                        description_key: Some("be.msg.fanPetition.options.addressPublicly.description".to_string()),
                    },
                ],
            },
        ))
        .with_i18n(
            "be.msg.fanPetitionLol.subject",
            &template.template_key,
            params(&[("team", team_name), ("effectId", &template.effect_id)]),
        )
        .with_sender_i18n("be.sender.communityManager", "be.role.communityManager"),
    )
}

pub(super) fn rival_interest_message(
    msg_id: &str,
    player_id: &str,
    player_name: &str,
    rival_name: &str,
    date: &str,
) -> InboxMessage {
    if let Some(msg) = crate::messages::template_store::template_store().build_message(
        "rival_interest",
        msg_id,
        date,
        "en",
        vec![("player", player_name), ("rival", rival_name)],
    ) {
        return msg;
    }
    let mut rng = rand::rng();
    let variations = [
        format!(
            "Eros ha abierto el último episodio de Al Lío con una bomba: \"Tengo información de que {} ha empezado a moverse por {}. \
            Sus analistas llevan semanas revisando sus últimas series y datos de solo queue. \
            No hay oferta formal todavía, pero la cosa se está calentando rápido.\"\n\n\
            ¿Cuál es vuestra postura si hacen contacto?",
            rival_name, player_name
        ),
        format!(
            "Al Lío Podcast ha publicado en exclusiva que {} está en la órbita concreta de {}.\n\n\
            Según Eros, el jugador llamó la atención por sus últimos rendimientos en stage. \
            Aún no hay oferta oficial, pero la presión viene creciendo rápido.\n\n\
            ¿Cuál es tu postura?",
            player_name, rival_name
        ),
        format!(
            "Eros ha dejado caer en Al Lío que {} está midiendo el mercado por {}: \"No es una oferta encima de la mesa, \
            pero cuando un club pregunta dos veces por el mismo jugador ya no es casualidad.\"\n\n\
            Conviene decidir una postura antes de que el rumor crezca.",
            rival_name, player_name
        ),
        format!(
            "Al Lío Podcast ha señalado a {} como nombre caliente para el próximo mercado.\n\n\
            Según Eros, {} encaja en varios escenarios de {} si deciden mover piezas tras el split. \
            No hay contacto formal confirmado, pero el interés existe.\n\n\
            ¿Bloqueas cualquier conversación o escuchas?",
            player_name, player_name, rival_name
        ),
        format!(
            "Eros ha sido prudente, pero claro: \"{} gusta mucho en {}. \
            No diría que está cerca, pero sí que hay gente mirando números, VODs y situación contractual.\"\n\n\
            El entorno del jugador puede empezar a escuchar ruido.",
            player_name, rival_name
        ),
        format!(
            "El tramo de mercato de Al Lío ha terminado con {} en titulares: \
            \"Si {} quiere dar un golpe de efecto, este es el perfil que tiene sentido. \
            Caro, complicado, pero tiene sentido.\"\n\n\
            La pregunta es cuánto quieres resistir.",
            player_name, rival_name
        ),
        format!(
            "Eros ha contado que varios agentes esperan movimiento alrededor de {}, y {} aparece entre los equipos atentos.\n\n\
            \"A veces estas cosas no empiezan con una oferta, empiezan con una llamada para saber si hay puerta.\"\n\n\
            Tu respuesta marcará el tono.",
            player_name, rival_name
        ),
        format!(
            "Al Lío Podcast ha conectado a {} con {} después de sus últimas actuaciones en stage.\n\n\
            Eros lo resumió así: \"Cuando un jugador gana valor tan rápido, el teléfono se mueve aunque el club diga que no vende.\"\n\n\
            El mercado está probando límites.",
            player_name, rival_name
        ),
        format!(
            "Eros ha incluido a {} en su lista de nombres que pueden agitar la liga: \
            \"No digo que salga. Digo que si {} llama, la conversación será incómoda para todos.\"\n\n\
            Puede ser el momento de reforzar públicamente tu postura.",
            player_name, rival_name
        ),
        format!(
            "La última exclusiva de Al Lío apunta a una vigilancia discreta de {} sobre {}.\n\n\
            Según Eros, el club rival no quiere precipitarse, pero valora al jugador como una pieza de alto impacto si se abre una ventana.\n\n\
            ¿Cuál es el plan?",
            rival_name, player_name
        ),
        format!(
            "Eros ha cerrado el episodio con un aviso: \"Si pensáis que {} va a pasar desapercibido para {}, \
            no estáis mirando el mismo split que yo. Otra cosa es que puedan sacarlo.\"\n\n\
            La narrativa ya está fuera.",
            player_name, rival_name
        ),
    ];
    let idx = rng.random_range(0..variations.len());

    InboxMessage::new(
        msg_id.to_string(),
        format!(
            "Al Lío Podcast — {} en la órbita de {}",
            player_name, rival_name
        ),
        variations[idx].clone(),
        "Al Lío Podcast".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Transfer)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Eros")
    .with_action(action(
        "respond",
        "Respond",
        "be.msg.event.respond",
        ActionType::ChooseOption {
            options: vec![
                ActionOption {
                    id: "not_for_sale".to_string(),
                    label: "Not for sale".to_string(),
                    description: "Make it clear the player is going nowhere. Boosts their morale."
                        .to_string(),
                    label_key: Some("be.msg.rivalInterest.options.notForSale.label".to_string()),
                    description_key: Some(
                        "be.msg.rivalInterest.options.notForSale.description".to_string(),
                    ),
                },
                ActionOption {
                    id: "open_to_offers".to_string(),
                    label: "Open to offers".to_string(),
                    description: "Signal willingness to negotiate. Player may become unsettled."
                        .to_string(),
                    label_key: Some("be.msg.rivalInterest.options.openToOffers.label".to_string()),
                    description_key: Some(
                        "be.msg.rivalInterest.options.openToOffers.description".to_string(),
                    ),
                },
                ActionOption {
                    id: "no_comment".to_string(),
                    label: "No comment".to_string(),
                    description: "Stay quiet and let things play out. Neutral stance.".to_string(),
                    label_key: Some("be.msg.rivalInterest.options.noComment.label".to_string()),
                    description_key: Some(
                        "be.msg.rivalInterest.options.noComment.description".to_string(),
                    ),
                },
            ],
        },
    ))
    .with_context(MessageContext {
        player_id: Some(player_id.to_string()),
        ..Default::default()
    })
    .with_i18n(
        "be.msg.allio.rivalInterest.subject",
        &format!("be.msg.allio.rivalInterest.body{}", idx),
        params(&[("player", player_name), ("rival", rival_name)]),
    )
    .with_sender_i18n("be.sender.allioPodcast", "be.role.allioPodcast")
}
