use std::{any::TypeId, collections::HashMap};

use bevy_asset::{AssetId, AssetServer, Assets, Handle, LoadedFolder};
use bevy_ecs::prelude::*;
use bevy_fluent::{
    exts::fluent::BundleExt,
    prelude::{BundleAsset, Locale, Localization},
};
use fluent_content::Content;
use tracing::{debug, trace};
use unic_langid::LanguageIdentifier;

use crate::{LocalizeText, styling::ResolvedStyle};

fn default_language_identifier() -> LanguageIdentifier {
    "en-US"
        .parse()
        .expect("default locale `en-US` should parse")
}

/// Active application locale used by `bevy_xilem` projection.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct ActiveLocale(pub LanguageIdentifier);

impl Default for ActiveLocale {
    fn default() -> Self {
        Self(default_language_identifier())
    }
}

impl ActiveLocale {
    #[must_use]
    pub fn new(locale: LanguageIdentifier) -> Self {
        Self(locale)
    }
}

/// Root folder path for fluent localization assets.
///
/// Defaults to `assets/locales` via Bevy asset path `"locales"`.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct LocalizationAssetRoot(pub String);

impl Default for LocalizationAssetRoot {
    fn default() -> Self {
        Self("locales".to_string())
    }
}

/// Handle to loaded locale folder.
#[derive(Resource, Debug, Default, Clone)]
pub struct LocalizationFolderHandle(pub Option<Handle<LoadedFolder>>);

/// Cached localization bundle chain for the current [`ActiveLocale`].
#[derive(Resource, Debug, Default)]
pub struct LocalizationCache {
    locale: Option<LanguageIdentifier>,
    folder_id: Option<AssetId<LoadedFolder>>,
    localization: Option<Localization>,
    bundle_asset_count: usize,
}

impl LocalizationCache {
    #[must_use]
    pub fn content(&self, key: &str) -> Option<String> {
        self.localization.as_ref().and_then(|loc| loc.content(key))
    }
}

/// Load locale folder handle once (`locales/**`).
pub fn load_localization_assets(
    asset_server: Option<Res<AssetServer>>,
    root: Res<LocalizationAssetRoot>,
    mut folder: ResMut<LocalizationFolderHandle>,
) {
    let Some(asset_server) = asset_server else {
        return;
    };

    if folder.0.is_none() {
        let handle = asset_server.load_folder(root.0.clone());
        trace!(
            asset_root = %root.0,
            folder_handle = ?handle.id(),
            "queued localization folder load"
        );
        folder.0 = Some(handle);
    }
}

/// Keep `bevy_fluent::Locale` in sync with [`ActiveLocale`].
pub fn sync_fluent_locale_from_active_locale(
    active_locale: Res<ActiveLocale>,
    fluent_locale: Option<ResMut<Locale>>,
) {
    let Some(mut fluent_locale) = fluent_locale else {
        return;
    };

    if fluent_locale.requested != active_locale.0 {
        debug!(
            previous_locale = %fluent_locale.requested,
            next_locale = %active_locale.0,
            "syncing fluent locale from ActiveLocale"
        );
        fluent_locale.requested = active_locale.0.clone();
    }
}

fn build_localization(
    locale: &Locale,
    loaded_folder: &LoadedFolder,
    bundle_assets: &Assets<BundleAsset>,
) -> Localization {
    struct Entry<'a> {
        handle: Handle<BundleAsset>,
        asset: &'a BundleAsset,
    }

    let locale_entries: HashMap<_, _> = loaded_folder
        .handles
        .iter()
        .filter_map(|untyped_handle| {
            if untyped_handle.type_id() != TypeId::of::<BundleAsset>() {
                return None;
            }

            let typed_handle = untyped_handle.clone().typed::<BundleAsset>();
            bundle_assets.get(&typed_handle).map(|asset| {
                (
                    asset.locale(),
                    Entry {
                        handle: typed_handle,
                        asset,
                    },
                )
            })
        })
        .collect();

    let mut localization = Localization::new();
    let fallback_chain = locale.fallback_chain(locale_entries.keys().copied());
    trace!(
        requested_locale = %locale.requested,
        available_locale_count = locale_entries.len(),
        fallback_chain = ?fallback_chain,
        "building localization cache chain"
    );
    for locale in fallback_chain {
        if let Some(entry) = locale_entries.get(locale) {
            localization.insert(&entry.handle, entry.asset);
        }
    }

    localization
}

fn localization_cache_needs_rebuild(
    cache: &LocalizationCache,
    locale_changed: bool,
    folder_changed: bool,
    bundle_assets_changed: bool,
) -> bool {
    cache.localization.is_none() || locale_changed || folder_changed || bundle_assets_changed
}

/// Rebuild localization fallback chain cache when locale/folder changes and assets are loaded.
pub fn refresh_localization_cache(
    asset_server: Option<Res<AssetServer>>,
    loaded_folders: Option<Res<Assets<LoadedFolder>>>,
    bundle_assets: Option<Res<Assets<BundleAsset>>>,
    fluent_locale: Option<Res<Locale>>,
    active_locale: Res<ActiveLocale>,
    folder: Res<LocalizationFolderHandle>,
    mut cache: ResMut<LocalizationCache>,
) {
    let Some(_asset_server) = asset_server else {
        trace!("skip localization cache refresh: AssetServer unavailable");
        return;
    };

    let Some(folder_handle) = folder.0.as_ref() else {
        trace!("skip localization cache refresh: locale folder handle missing");
        return;
    };

    let Some(loaded_folders) = loaded_folders else {
        trace!("skip localization cache refresh: LoadedFolder assets unavailable");
        return;
    };

    let Some(bundle_assets) = bundle_assets else {
        trace!("skip localization cache refresh: BundleAsset assets unavailable");
        return;
    };

    let Some(loaded_folder) = loaded_folders.get(folder_handle) else {
        trace!(
            folder_handle = ?folder_handle.id(),
            "skip localization cache refresh: folder asset not loaded yet"
        );
        return;
    };

    let locale = fluent_locale
        .as_deref()
        .cloned()
        .unwrap_or_else(|| Locale::new(active_locale.0.clone()));

    let folder_id = folder_handle.id();
    let bundle_asset_count = loaded_folder
        .handles
        .iter()
        .filter_map(|untyped_handle| {
            if untyped_handle.type_id() != TypeId::of::<BundleAsset>() {
                return None;
            }

            Some(untyped_handle.clone().typed::<BundleAsset>())
        })
        .filter(|typed_handle| bundle_assets.get(typed_handle).is_some())
        .count();
    let locale_changed = cache.locale.as_ref() != Some(&active_locale.0);
    let folder_changed = cache.folder_id != Some(folder_id);
    let bundle_assets_changed = cache.bundle_asset_count != bundle_asset_count;
    let needs_rebuild = localization_cache_needs_rebuild(
        &cache,
        locale_changed,
        folder_changed,
        bundle_assets_changed,
    );

    trace!(
        active_locale = %active_locale.0,
        cache_locale = ?cache.locale.as_ref().map(ToString::to_string),
        folder_id = ?folder_id,
        bundle_asset_count,
        previous_bundle_asset_count = cache.bundle_asset_count,
        locale_changed,
        folder_changed,
        bundle_assets_changed,
        needs_rebuild,
        "evaluated localization cache refresh"
    );

    if !needs_rebuild {
        return;
    }

    let localization = build_localization(&locale, loaded_folder, &bundle_assets);
    let hello_world = localization.content("hello_world");

    debug!(
        active_locale = %active_locale.0,
        hello_world = ?hello_world,
        bundle_asset_count,
        "rebuilt localization cache"
    );

    cache.localization = Some(localization);
    cache.locale = Some(active_locale.0.clone());
    cache.folder_id = Some(folder_id);
    cache.bundle_asset_count = bundle_asset_count;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_rebuilds_when_bundle_asset_count_changes() {
        let mut cache = LocalizationCache::default();
        cache.localization = Some(Localization::new());

        assert!(localization_cache_needs_rebuild(&cache, false, false, true));
    }

    #[test]
    fn cache_does_not_rebuild_when_state_is_stable() {
        let mut cache = LocalizationCache::default();
        cache.localization = Some(Localization::new());

        assert!(!localization_cache_needs_rebuild(
            &cache, false, false, false
        ));
    }
}

/// Resolve text for an entity carrying [`LocalizeText`], otherwise return fallback text.
#[must_use]
pub fn resolve_localized_text(world: &World, entity: Entity, fallback: &str) -> String {
    let Some(localize_text) = world.get::<LocalizeText>(entity) else {
        return fallback.to_string();
    };

    if let Some(translated) = world
        .get_resource::<LocalizationCache>()
        .and_then(|cache| cache.content(localize_text.key.as_str()))
    {
        trace!(
            entity = ?entity,
            key = %localize_text.key,
            translated = %translated,
            "resolved localized text"
        );
        return translated;
    }

    if let Some(cache) = world.get_resource::<LocalizationCache>() {
        debug!(
            entity = ?entity,
            key = %localize_text.key,
            fallback = %fallback,
            cache_locale = ?cache.locale.as_ref().map(ToString::to_string),
            bundle_asset_count = cache.bundle_asset_count,
            has_localization = cache.localization.is_some(),
            "localized key missing, using fallback UiLabel text"
        );
    } else {
        debug!(
            entity = ?entity,
            key = %localize_text.key,
            fallback = %fallback,
            "LocalizationCache resource missing, using fallback UiLabel text"
        );
    }

    if fallback.is_empty() {
        localize_text.key.clone()
    } else {
        fallback.to_string()
    }
}

/// Compute locale-aware CJK fallback stack used to resolve Han variants correctly.
#[must_use]
pub fn locale_font_family_stack(locale: &LanguageIdentifier) -> Vec<String> {
    if locale.language.as_str() == "ja" {
        return vec![
            "Inter".to_string(),
            "Noto Sans JP".to_string(),
            "Noto Sans CJK JP".to_string(),
            "Noto Sans SC".to_string(),
            "Noto Sans CJK SC".to_string(),
            "sans-serif".to_string(),
        ];
    }

    if locale.language.as_str() == "zh"
        && locale
            .region
            .is_some_and(|region| region.as_str().eq_ignore_ascii_case("CN"))
    {
        return vec![
            "Inter".to_string(),
            "Noto Sans SC".to_string(),
            "Noto Sans CJK SC".to_string(),
            "Noto Sans JP".to_string(),
            "Noto Sans CJK JP".to_string(),
            "sans-serif".to_string(),
        ];
    }

    vec![
        "Inter".to_string(),
        "Noto Sans SC".to_string(),
        "Noto Sans CJK SC".to_string(),
        "Noto Sans JP".to_string(),
        "Noto Sans CJK JP".to_string(),
        "sans-serif".to_string(),
    ]
}

/// Apply locale-aware fallback stack when no explicit style font stack is present.
pub fn apply_locale_font_family_fallback(world: &World, style: &mut ResolvedStyle) {
    if style.font_family.is_some() {
        return;
    }

    let locale = world
        .get_resource::<ActiveLocale>()
        .map_or_else(default_language_identifier, |active| active.0.clone());

    style.font_family = Some(locale_font_family_stack(&locale));
}
