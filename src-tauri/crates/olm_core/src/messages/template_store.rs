use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::domain::message::{
    ActionOption, ActionType, InboxMessage, MessageAction, MessageCategory, MessagePriority,
};

// ─── Sender data structures ───

#[derive(Debug, Deserialize, Clone)]
pub struct MessageTemplateSender {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub name_key: Option<String>,
    pub role: String,
    #[serde(default)]
    pub role_key: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

// ─── Senders store ───

static SENDERS_STORE: OnceLock<HashMap<String, MessageTemplateSender>> = OnceLock::new();

fn load_senders(senders_dir: &Path) -> HashMap<String, MessageTemplateSender> {
    let mut senders = HashMap::new();
    if !senders_dir.is_dir() {
        return senders;
    }
    let Ok(entries) = fs::read_dir(senders_dir) else {
        return senders;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(sender) = serde_json::from_str::<MessageTemplateSender>(&content) else {
            continue;
        };
        if let Some(id) = &sender.id {
            senders.insert(id.clone(), sender);
        }
    }
    senders
}

pub fn init_senders_store(senders_dir: &Path) {
    let senders = load_senders(senders_dir);
    eprintln!("[senders_store] loaded {} sender(s)", senders.len());
    let _ = SENDERS_STORE.set(senders);
}

pub fn get_sender(id: &str) -> Option<MessageTemplateSender> {
    SENDERS_STORE.get_or_init(HashMap::new).get(id).cloned()
}

// ─── Template data structures ───

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SenderRef {
    Id(String),
    Inline(MessageTemplateSender),
}

#[derive(Debug, Deserialize)]
pub struct MessageTemplateAction {
    #[serde(rename = "type")]
    pub action_type: String,
    pub label: String,
    #[serde(default)]
    pub label_key: Option<String>,
    #[serde(default)]
    pub route: Option<String>,
    #[serde(default)]
    pub options: Vec<MessageTemplateOption>,
}

#[derive(Debug, Deserialize)]
pub struct MessageTemplateOption {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub label_key: Option<String>,
    #[serde(default)]
    pub description_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageTemplate {
    pub id: String,
    pub trigger: String,
    #[serde(default)]
    pub weight: u32,
    pub sender: SenderRef,
    pub category: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub actions: Vec<MessageTemplateAction>,
    pub subject: String,
    pub body: String,
    /// Inline translations keyed by language code: { "es": { "subject": ..., "body": ... } }
    #[serde(default)]
    pub translations: HashMap<String, HashMap<String, String>>,
}

// ─── Template store ───

static TEMPLATE_STORE: OnceLock<TemplateStore> = OnceLock::new();

pub struct TemplateStore {
    /// Templates grouped by trigger
    by_trigger: HashMap<String, Vec<MessageTemplate>>,
}

impl TemplateStore {
    /// Scan `data/messages/*.json` and load all templates.
    pub fn load(messages_dir: &Path) -> Result<Self, String> {
        let mut by_trigger: HashMap<String, Vec<MessageTemplate>> = HashMap::new();

        if !messages_dir.is_dir() {
            return Ok(Self { by_trigger });
        }

        let entries = fs::read_dir(messages_dir)
            .map_err(|e| format!("Failed to read messages dir {messages_dir:?}: {e}"))?;

        // Scan flat JSONs
        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let content =
                fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;
            let template: MessageTemplate = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse {path:?}: {e}"))?;
            by_trigger
                .entry(template.trigger.clone())
                .or_default()
                .push(template);
        }

        // Log loaded templates
        eprintln!(
            "[template_store] loaded triggers: {:?}",
            by_trigger.keys().collect::<Vec<_>>()
        );

        // Also scan subdirectories for nested JSONs
        let dir_entries =
            fs::read_dir(messages_dir).map_err(|e| format!("Failed to read messages dir: {e}"))?;
        for entry in dir_entries {
            let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
            let subdir = entry.path();
            if !subdir.is_dir() {
                continue;
            }
            let sub_entries =
                fs::read_dir(&subdir).map_err(|e| format!("Failed to read {subdir:?}: {e}"))?;
            for sub_entry in sub_entries {
                let sub_entry = sub_entry.map_err(|e| format!("Entry error: {e}"))?;
                let path = sub_entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let content = fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {path:?}: {e}"))?;
                let template: MessageTemplate = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse {path:?}: {e}"))?;
                by_trigger
                    .entry(template.trigger.clone())
                    .or_default()
                    .push(template);
            }
        }

        // Log loaded templates
        for (trigger, templates) in &by_trigger {
            eprintln!(
                "[template_store] trigger={trigger}: {} template(s)",
                templates.len()
            );
        }

        Ok(Self { by_trigger })
    }

    /// Get a weighted random template for a given trigger.
    pub fn pick_random(&self, trigger: &str) -> Option<&MessageTemplate> {
        let templates = self.by_trigger.get(trigger)?;
        if templates.is_empty() {
            return None;
        }
        // Weighted random selection
        let total_weight: u32 = templates.iter().map(|t| t.weight.max(1)).sum();
        let mut roll = rand::random_range(1..=total_weight);
        for t in templates {
            let w = t.weight.max(1);
            if roll <= w {
                return Some(t);
            }
            roll -= w;
        }
        Some(&templates[0])
    }

    /// Build an InboxMessage from a template with given params and language.
    pub fn build_message(
        &self,
        trigger: &str,
        id: &str,
        date: &str,
        lang: &str,
        params: Vec<(&str, &str)>,
    ) -> Option<InboxMessage> {
        let tpl = self.pick_random(trigger)?;

        let i18n_params: HashMap<String, String> = params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        // Resolve translated text or fall back to default (English)
        let subject = resolve_text(
            &tpl.subject,
            &tpl.translations,
            lang,
            "subject",
            &i18n_params,
        );
        let body = resolve_text(&tpl.body, &tpl.translations, lang, "body", &i18n_params);

        let category = parse_category(&tpl.category);
        let priority = parse_priority(&tpl.priority);

        // Resolve sender: either inline or by reference ID
        let sender = match &tpl.sender {
            SenderRef::Id(id) => get_sender(id).unwrap_or_else(|| {
                // Fallback: create a minimal sender from the ID itself
                eprintln!("[template_store] sender '{id}' not found, using fallback");
                MessageTemplateSender {
                    id: Some(id.clone()),
                    name: id.clone(),
                    name_key: None,
                    role: String::new(),
                    role_key: None,
                    icon: None,
                }
            }),
            SenderRef::Inline(s) => s.clone(),
        };

        let mut msg = InboxMessage::new(
            id.to_string(),
            subject,
            body,
            interpolate(&sender.name, &i18n_params),
            date.to_string(),
        )
        .with_category(category)
        .with_priority(priority)
        .with_sender_role(&interpolate(&sender.role, &i18n_params));

        if let Some(icon) = &sender.icon {
            msg = msg.with_sender_icon(icon);
        }
        if let Some(key) = &sender.name_key {
            msg.sender_key = Some(key.clone());
        }
        if let Some(key) = &sender.role_key {
            msg.sender_role_key = Some(key.clone());
        }
        if !i18n_params.is_empty() {
            msg.i18n_params = i18n_params;
        }

        for (i, a) in tpl.actions.iter().enumerate() {
            let action_type = match a.action_type.to_lowercase().as_str() {
                "navigate" | "navigateto" => ActionType::NavigateTo {
                    route: a.route.clone().unwrap_or_default(),
                },
                "choose" | "chooseoption" => ActionType::ChooseOption {
                    options: a
                        .options
                        .iter()
                        .map(|o| ActionOption {
                            id: o.id.clone(),
                            label: o.label.clone(),
                            description: o.description.clone(),
                            label_key: o.label_key.clone(),
                            description_key: o.description_key.clone(),
                        })
                        .collect(),
                },
                "dismiss" => ActionType::Dismiss,
                _ => ActionType::Acknowledge,
            };
            msg.actions.push(MessageAction {
                id: format!("action_{}", i),
                label: a.label.clone(),
                action_type,
                resolved: false,
                label_key: a.label_key.clone(),
            });
        }

        Some(msg)
    }
}

/// Resolve translated text: try `translations.lang.field`, fall back to `default`.
fn resolve_text(
    default: &str,
    translations: &HashMap<String, HashMap<String, String>>,
    lang: &str,
    field: &str,
    params: &HashMap<String, String>,
) -> String {
    let text = translations
        .get(lang)
        .and_then(|t| t.get(field))
        .map(|s| s.as_str())
        .unwrap_or(default);
    interpolate(text, params)
}

/// Replace {key} placeholders in text with actual values.
fn interpolate(text: &str, params: &HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (key, value) in params {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

fn parse_category(s: &str) -> MessageCategory {
    match s.to_lowercase().as_str() {
        "welcome" => MessageCategory::Welcome,
        "leagueinfo" => MessageCategory::LeagueInfo,
        "matchpreview" => MessageCategory::MatchPreview,
        "matchresult" => MessageCategory::MatchResult,
        "transfer" => MessageCategory::Transfer,
        "boarddirective" => MessageCategory::BoardDirective,
        "playermorale" => MessageCategory::PlayerMorale,
        "injury" => MessageCategory::Injury,
        "training" => MessageCategory::Training,
        "finance" => MessageCategory::Finance,
        "contract" => MessageCategory::Contract,
        "scoutreport" => MessageCategory::ScoutReport,
        "media" => MessageCategory::Media,
        "system" => MessageCategory::System,
        "joboffer" => MessageCategory::JobOffer,
        _ => MessageCategory::System,
    }
}

fn parse_priority(s: &str) -> MessagePriority {
    match s.to_lowercase().as_str() {
        "low" => MessagePriority::Low,
        "normal" => MessagePriority::Normal,
        "high" => MessagePriority::High,
        "urgent" => MessagePriority::Urgent,
        _ => MessagePriority::Normal,
    }
}

/// Initialize the global template store. Call once at app startup.
pub fn init_template_store(messages_dir: &Path) -> Result<(), String> {
    let store = TemplateStore::load(messages_dir)?;
    TEMPLATE_STORE
        .set(store)
        .map_err(|_| "TemplateStore already initialized".to_string())
}

/// Get a reference to the global template store.
pub fn template_store() -> &'static TemplateStore {
    TEMPLATE_STORE.get_or_init(|| TemplateStore {
        by_trigger: HashMap::new(),
    })
}
