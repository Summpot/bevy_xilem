use std::collections::HashMap;

use bevy_ecs::prelude::*;
use fluent::{FluentResource, concurrent::FluentBundle};
use tracing::{debug, trace};
use unic_langid::{LanguageIdentifier, langid};

use crate::{LocalizeText, styling::ResolvedStyle};

fn default_language_identifier() -> LanguageIdentifier {
    langid!("en-US")
}

/// Synchronous app-level localization registry.
#[derive(Resource)]
pub struct AppI18n {
    pub active_locale: LanguageIdentifier,
    pub bundles: HashMap<LanguageIdentifier, FluentBundle<FluentResource>>,
}

impl Default for AppI18n {
    fn default() -> Self {
        Self {
            active_locale: default_language_identifier(),
            bundles: HashMap::new(),
        }
    }
}

impl AppI18n {
    #[must_use]
    pub fn new(active_locale: LanguageIdentifier) -> Self {
        Self {
            active_locale,
            bundles: HashMap::new(),
        }
    }

    pub fn set_active_locale(&mut self, locale: LanguageIdentifier) {
        self.active_locale = locale;
    }

    pub fn insert_bundle(
        &mut self,
        locale: LanguageIdentifier,
        bundle: FluentBundle<FluentResource>,
    ) {
        self.bundles.insert(locale, bundle);
    }

    #[must_use]
    pub fn translate(&self, key: &str) -> String {
        if let Some(bundle) = self.bundles.get(&self.active_locale)
            && let Some(message) = bundle.get_message(key)
            && let Some(pattern) = message.value()
        {
            let mut errors = vec![];
            return bundle
                .format_pattern(pattern, None, &mut errors)
                .into_owned();
        }

        key.to_string()
    }
}

/// Locale-aware font stack registry used for text rendering fallback.
#[derive(Resource, Debug, Clone, Default)]
pub struct LocaleFontRegistry {
    pub default_font_stack: Vec<String>,
    pub locale_mappings: HashMap<String, Vec<String>>,
}

impl LocaleFontRegistry {
    #[must_use]
    pub fn add_mapping(mut self, locale: &str, stack: Vec<&str>) -> Self {
        self.locale_mappings.insert(
            locale.to_string(),
            stack.into_iter().map(String::from).collect(),
        );
        self
    }

    #[must_use]
    pub fn set_default(mut self, stack: Vec<&str>) -> Self {
        self.default_font_stack = stack.into_iter().map(String::from).collect();
        self
    }

    #[must_use]
    pub fn font_stack_for_locale(&self, locale: &LanguageIdentifier) -> Option<Vec<String>> {
        let locale_key = locale.to_string();
        if let Some(stack) = self
            .locale_mappings
            .get(locale_key.as_str())
            .filter(|stack| !stack.is_empty())
        {
            return Some(stack.clone());
        }

        if self.default_font_stack.is_empty() {
            None
        } else {
            Some(self.default_font_stack.clone())
        }
    }
}

/// Resolve text for an entity carrying [`LocalizeText`], otherwise return fallback text.
#[must_use]
pub fn resolve_localized_text(world: &World, entity: Entity, fallback: &str) -> String {
    let Some(localize_text) = world.get::<LocalizeText>(entity) else {
        return fallback.to_string();
    };

    if let Some(i18n) = world.get_resource::<AppI18n>() {
        let translated = i18n.translate(localize_text.key.as_str());
        trace!(
            entity = ?entity,
            key = %localize_text.key,
            translated = %translated,
            "resolved localized text"
        );
        return translated;
    }

    debug!(
        entity = ?entity,
        key = %localize_text.key,
        fallback = %fallback,
        "AppI18n resource missing, using fallback UiLabel text"
    );

    if fallback.is_empty() {
        localize_text.key.clone()
    } else {
        fallback.to_string()
    }
}

/// Apply locale-aware fallback stack when no explicit style font stack is present.
pub fn apply_locale_font_family_fallback(world: &World, style: &mut ResolvedStyle) {
    if style.font_family.is_some() {
        return;
    }

    let locale = world
        .get_resource::<AppI18n>()
        .map_or_else(default_language_identifier, |i18n| {
            i18n.active_locale.clone()
        });

    let font_stack = world
        .get_resource::<LocaleFontRegistry>()
        .and_then(|registry| registry.font_stack_for_locale(&locale));

    if let Some(font_stack) = font_stack {
        style.font_family = Some(font_stack);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_i18n_translate_falls_back_to_key() {
        let i18n = AppI18n::default();
        assert_eq!(i18n.translate("missing-key"), "missing-key");
    }

    #[test]
    fn locale_font_registry_prefers_locale_mapping() {
        let registry = LocaleFontRegistry::default()
            .set_default(vec!["Default Sans", "sans-serif"])
            .add_mapping("fr-FR", vec!["French Sans", "sans-serif"]);

        let fr: LanguageIdentifier = "fr-FR"
            .parse()
            .expect("fr-FR locale identifier should parse");

        assert_eq!(
            registry.font_stack_for_locale(&fr),
            Some(vec!["French Sans".to_string(), "sans-serif".to_string()])
        );
    }
}
