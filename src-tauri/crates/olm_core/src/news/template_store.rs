use std::collections::HashMap;
use std::sync::OnceLock;

use rand::{Rng, RngExt};
use serde::Deserialize;

use crate::domain::news::*;

// ─── Raw deserialization structures ───

#[derive(Debug, Deserialize)]
struct RawNewsTemplate {
    id: String,
    category: String,
    headlines: Vec<RawHeadline>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    body_key: Option<String>,
    #[serde(default)]
    body_variants: Vec<RawBodyVariant>,
    sources: Vec<RawSource>,
    #[serde(default)]
    translations: HashMap<String, RawCategoryTranslations>,
}

#[derive(Debug, Deserialize)]
struct RawHeadline {
    key: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct RawBodyVariant {
    body_key: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    key: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct RawCategoryTranslations {
    #[serde(default)]
    headlines: Vec<RawHeadline>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    body_variants: Vec<RawBodyVariant>,
}

// ─── Compiled template ───

pub struct NewsTemplate {
    pub template_id: String,
    pub category: NewsCategory,
    headlines: Vec<HeadlineVariant>,
    body_default: Option<BodyVariant>,
    pub body_variants: Vec<BodyVariant>,
    sources: Vec<SourceVariant>,
    translations: HashMap<String, CategoryTranslations>,
}

struct HeadlineVariant {
    key: String,
    text: String,
}

pub struct BodyVariant {
    pub body_key: String,
    pub text: String,
}

impl BodyVariant {
    pub fn new(body_key: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            body_key: body_key.into(),
            text: text.into(),
        }
    }
}

struct SourceVariant {
    key: String,
    text: String,
}

struct CategoryTranslations {
    headlines: Vec<HeadlineVariant>,
    body_default: Option<BodyVariant>,
    body_variants: Vec<BodyVariant>,
}

// ─── Global store ───

static NEWS_TEMPLATE_STORE: OnceLock<NewsTemplateStore> = OnceLock::new();

pub struct NewsTemplateStore {
    by_category: HashMap<NewsCategory, NewsTemplate>,
    by_id: HashMap<String, NewsTemplate>,
}

impl NewsTemplateStore {
    fn load() -> Self {
        let mut by_id: HashMap<String, NewsTemplate> = HashMap::new();
        let mut by_category: HashMap<NewsCategory, NewsTemplate> = HashMap::new();

        macro_rules! load_template {
            ($path:literal, $label:literal) => {{
                let raw: RawNewsTemplate = serde_json::from_str(include_str!($path))
                    .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", $label));
                let template = compile_template(raw);
                let cat = template.category.clone();
                if cat != NewsCategory::Editorial {
                    by_category.insert(cat, template);
                } else {
                    by_id.insert(template.template_id.clone(), template);
                }
            }};
        }

        load_template!(
            "../../../../../data/news/season_preview/template.json",
            "season_preview"
        );
        load_template!(
            "../../../../../data/news/editorial/weekly_digest.json",
            "weekly_digest"
        );
        load_template!(
            "../../../../../data/news/editorial/title_race.json",
            "title_race"
        );
        load_template!(
            "../../../../../data/news/editorial/unbeaten_streak.json",
            "unbeaten_streak"
        );

        NewsTemplateStore { by_category, by_id }
    }

    pub fn global() -> &'static NewsTemplateStore {
        NEWS_TEMPLATE_STORE.get_or_init(Self::load)
    }

    /// Look up a template by its `NewsCategory` (only works for
    /// non-Editorial categories — editorials use `by_id`).
    pub fn get(&self, category: &NewsCategory) -> Option<&NewsTemplate> {
        self.by_category.get(category)
    }

    /// Look up a template by its string `id` (e.g. `"weekly_digest"`).
    /// Use this for Editorials where multiple templates share the same category.
    pub fn get_by_id(&self, id: &str) -> Option<&NewsTemplate> {
        self.by_id.get(id)
    }
}

fn compile_template(raw: RawNewsTemplate) -> NewsTemplate {
    let category = match raw.category.as_str() {
        "SeasonPreview" => NewsCategory::SeasonPreview,
        "LeagueRoundup" => NewsCategory::LeagueRoundup,
        "StandingsUpdate" => NewsCategory::StandingsUpdate,
        "TransferRumour" => NewsCategory::TransferRumour,
        "MatchReport" => NewsCategory::MatchReport,
        "Editorial" => NewsCategory::Editorial,
        "InjuryNews" => NewsCategory::InjuryNews,
        "ManagerialChange" => NewsCategory::ManagerialChange,
        other => panic!("Unknown news category in template: {other}"),
    };

    let body_default = match (raw.body, raw.body_key) {
        (Some(text), Some(body_key)) => Some(BodyVariant { body_key, text }),
        (Some(text), None) => Some(BodyVariant {
            body_key: String::new(),
            text,
        }),
        (None, _) => None,
    };

    let body_variants: Vec<BodyVariant> = raw
        .body_variants
        .into_iter()
        .map(|v| BodyVariant {
            body_key: v.body_key,
            text: v.text,
        })
        .collect();

    let translations = raw
        .translations
        .into_iter()
        .map(|(lang, t)| {
            let ct = CategoryTranslations {
                headlines: t
                    .headlines
                    .into_iter()
                    .map(|h| HeadlineVariant {
                        key: h.key,
                        text: h.text,
                    })
                    .collect(),
                body_default: match t.body {
                    Some(text) => Some(BodyVariant {
                        body_key: String::new(),
                        text,
                    }),
                    None => None,
                },
                body_variants: t
                    .body_variants
                    .into_iter()
                    .map(|v| BodyVariant {
                        body_key: v.body_key,
                        text: v.text,
                    })
                    .collect(),
            };
            (lang, ct)
        })
        .collect();

    NewsTemplate {
        template_id: raw.id,
        category,
        headlines: raw
            .headlines
            .into_iter()
            .map(|h| HeadlineVariant {
                key: h.key,
                text: h.text,
            })
            .collect(),
        body_default,
        body_variants,
        sources: raw
            .sources
            .into_iter()
            .map(|s| SourceVariant {
                key: s.key,
                text: s.text,
            })
            .collect(),
        translations,
    }
}

// ─── Article builder ───

impl NewsTemplate {
    /// Build a `NewsArticle` from the template.
    ///
    /// * `body_variant` — optional index into `body_variants`. When `Some`,
    ///   that variant's body_key and text are used instead of `body_default`.
    pub fn build_article(
        &self,
        id: String,
        date: String,
        params: &[(&str, &str)],
        lang: &str,
        body_variant: Option<usize>,
    ) -> NewsArticle {
        let mut rng = rand::rng();
        let lang_t = self.translations.get(lang);

        // ── Headline ────────────────────────────────────────────
        let (headline_key, headline_text) =
            if let Some(lt) = lang_t.and_then(|t| select_random(&t.headlines, &mut rng)) {
                (lt.key.clone(), interpolate(&lt.text, params))
            } else {
                let h = select_random(&self.headlines, &mut rng)
                    .expect("Template must have at least one headline");
                (h.key.clone(), interpolate(&h.text, params))
            };

        // ── Body ────────────────────────────────────────────────
        let (body_text, used_body_key) = if let Some(idx) = body_variant {
            // Caller-selected variant
            if let Some(lt) = lang_t.and_then(|t| t.body_variants.get(idx)) {
                (interpolate(&lt.text, params), lt.body_key.clone())
            } else if let Some(v) = self.body_variants.get(idx) {
                (interpolate(&v.text, params), v.body_key.clone())
            } else {
                fallback_body(&self.body_default, lang_t, params)
            }
        } else {
            fallback_body(&self.body_default, lang_t, params)
        };

        // ── Source ──────────────────────────────────────────────
        let src =
            select_random(&self.sources, &mut rng).expect("Template must have at least one source");

        let i18n_params: HashMap<String, String> = params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        NewsArticle::new(
            id,
            headline_text,
            body_text,
            src.text.clone(),
            date,
            self.category.clone(),
        )
        .with_i18n(&headline_key, &used_body_key, &src.key, i18n_params)
    }
}

fn fallback_body(
    default: &Option<BodyVariant>,
    lang_t: Option<&CategoryTranslations>,
    params: &[(&str, &str)],
) -> (String, String) {
    if let Some(lt) = lang_t.and_then(|t| t.body_default.as_ref()) {
        (interpolate(&lt.text, params), lt.body_key.clone())
    } else if let Some(def) = default {
        (interpolate(&def.text, params), def.body_key.clone())
    } else {
        (String::new(), String::new())
    }
}

// ─── Helpers ───

fn select_random<'a, T>(items: &'a [T], rng: &mut impl Rng) -> Option<&'a T> {
    if items.is_empty() {
        return None;
    }
    Some(&items[rng.random_range(0..items.len())])
}

fn interpolate(text: &str, params: &[(&str, &str)]) -> String {
    let mut result = text.to_string();
    for (key, value) in params {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}
