use std::collections::HashMap;

use bevy_ecs::prelude::*;
use fluent::{FluentResource, concurrent::FluentBundle};
use tracing::{debug, trace};
use unic_langid::{LanguageIdentifier, langid};

use crate::LocalizeText;

fn default_language_identifier() -> LanguageIdentifier {
    langid!("en-US")
}

/// Synchronous app-level localization registry.
#[derive(Resource)]
pub struct AppI18n {
    pub active_locale: LanguageIdentifier,
    pub default_font_stack: Vec<String>,
    pub bundles: HashMap<LanguageIdentifier, FluentBundle<FluentResource>>,
    pub font_stacks: HashMap<LanguageIdentifier, Vec<String>>,
}

impl Default for AppI18n {
    fn default() -> Self {
        Self {
            active_locale: default_language_identifier(),
            default_font_stack: vec![],
            bundles: HashMap::new(),
            font_stacks: HashMap::new(),
        }
    }
}

impl AppI18n {
    #[must_use]
    pub fn new(active_locale: LanguageIdentifier) -> Self {
        Self {
            active_locale,
            default_font_stack: vec![],
            bundles: HashMap::new(),
            font_stacks: HashMap::new(),
        }
    }

    pub fn set_active_locale(&mut self, locale: LanguageIdentifier) {
        self.active_locale = locale;
    }

    pub fn insert_bundle(
        &mut self,
        locale: LanguageIdentifier,
        bundle: FluentBundle<FluentResource>,
        font_stack: Vec<String>,
    ) {
        if self.default_font_stack.is_empty() && !font_stack.is_empty() {
            self.default_font_stack = font_stack.clone();
        }

        self.font_stacks.insert(locale.clone(), font_stack);
        self.bundles.insert(locale, bundle);
    }

    #[must_use]
    pub fn get_font_stack(&self) -> Vec<String> {
        self.font_stacks
            .get(&self.active_locale)
            .cloned()
            .unwrap_or_else(|| self.default_font_stack.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_i18n_translate_falls_back_to_key() {
        let i18n = AppI18n::default();
        assert_eq!(i18n.translate("missing-key"), "missing-key");
    }

    #[test]
    fn app_i18n_get_font_stack_uses_active_locale_then_default() {
        let mut i18n = AppI18n::new(
            "fr-FR"
                .parse()
                .expect("fr-FR locale identifier should parse"),
        );
        i18n.default_font_stack = vec!["Default Sans".to_string(), "sans-serif".to_string()];
        i18n.font_stacks.insert(
            "fr-FR"
                .parse()
                .expect("fr-FR locale identifier should parse"),
            vec!["French Sans".to_string(), "sans-serif".to_string()],
        );

        assert_eq!(
            i18n.get_font_stack(),
            vec!["French Sans".to_string(), "sans-serif".to_string()]
        );

        i18n.set_active_locale(
            "en-US"
                .parse()
                .expect("en-US locale identifier should parse"),
        );

        assert_eq!(
            i18n.get_font_stack(),
            vec!["Default Sans".to_string(), "sans-serif".to_string()]
        );
    }
}
