use std::{
    any::TypeId,
    borrow::Cow,
    collections::{HashMap, HashSet},
    io,
    time::Duration,
};

use bevy_asset::{
    Asset, AssetEvent, AssetLoader, AssetServer, Assets, Handle, LoadContext, io::Reader,
};
use bevy_ecs::{
    change_detection::Mut,
    component::ComponentId,
    entity::Entity,
    hierarchy::{ChildOf, Children},
    message::{MessageCursor, Messages},
    prelude::*,
};
use bevy_reflect::TypePath;
use bevy_tweening::{EaseMethod, Lens, Tween, TweenAnim};
use masonry::core::HasProperty;
use masonry::theme;
use serde::Deserialize;
use xilem::{Color, style::Style as _};
use xilem_masonry::masonry::parley::{FontFamily, GenericFamily, style::FontStack};
use xilem_masonry::masonry::properties::{
    Background, BorderColor, BorderWidth, BoxShadow, CornerRadius, LineBreaking, Padding,
};
use xilem_masonry::{
    WidgetView,
    view::{Label, TextInput, sized_box},
};

use crate::UiEventQueue;

/// Marker component for CSS-like class names attached to an entity.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleClass(pub Vec<String>);

/// Marker component for entities whose style cache needs recomputation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StyleDirty;

/// Inline layout style that can be attached to entities.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct LayoutStyle {
    pub padding: Option<f64>,
    pub gap: Option<f64>,
    pub corner_radius: Option<f64>,
    pub border_width: Option<f64>,
}

/// Inline color style that can be attached to entities.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct ColorStyle {
    pub bg: Option<Color>,
    pub text: Option<Color>,
    pub border: Option<Color>,
    pub hover_bg: Option<Color>,
    pub hover_text: Option<Color>,
    pub hover_border: Option<Color>,
    pub pressed_bg: Option<Color>,
    pub pressed_text: Option<Color>,
    pub pressed_border: Option<Color>,
}

/// Inline text style that can be attached to entities.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct TextStyle {
    pub size: Option<f32>,
}

/// Marker for hover pseudo-class state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hovered;

/// Marker for pressed pseudo-class state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pressed;

/// Transition settings for style animation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct StyleTransition {
    /// Duration in seconds.
    pub duration: f32,
}

/// Cached resolved style used by projectors.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct ComputedStyle {
    pub layout: ResolvedLayoutStyle,
    pub colors: ResolvedColorStyle,
    pub text: ResolvedTextStyle,
    pub font_family: Option<Vec<String>>,
    pub box_shadow: Option<BoxShadow>,
    pub transition: Option<StyleTransition>,
}

/// Interpolated color state currently rendered by projectors.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct CurrentColorStyle {
    pub bg: Option<Color>,
    pub text: Option<Color>,
    pub border: Option<Color>,
}

/// Target color state derived from classes + inline style + pseudo state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct TargetColorStyle {
    pub bg: Option<Color>,
    pub text: Option<Color>,
    pub border: Option<Color>,
}

/// Marker identifying a [`TweenAnim`] created by the style transition pipeline.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct StyleManagedTween;

/// Pseudo classes supported by selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum PseudoClass {
    Hovered,
    Pressed,
}

/// CSS-like selector AST for style rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selector {
    Type(TypeId),
    TypeName(String),
    Class(String),
    PseudoClass(PseudoClass),
    And(Vec<Selector>),
    Descendant {
        ancestor: Box<Selector>,
        descendant: Box<Selector>,
    },
}

impl Selector {
    #[must_use]
    pub fn of_type<T: Component>() -> Self {
        Self::Type(TypeId::of::<T>())
    }

    #[must_use]
    pub fn class(name: impl Into<String>) -> Self {
        Self::Class(name.into())
    }

    #[must_use]
    pub fn type_name(name: impl Into<String>) -> Self {
        Self::TypeName(name.into())
    }

    #[must_use]
    pub const fn pseudo(pseudo: PseudoClass) -> Self {
        Self::PseudoClass(pseudo)
    }

    #[must_use]
    pub fn and(selectors: impl Into<Vec<Selector>>) -> Self {
        Self::And(selectors.into())
    }

    #[must_use]
    pub fn descendant(ancestor: Selector, descendant: Selector) -> Self {
        Self::Descendant {
            ancestor: Box::new(ancestor),
            descendant: Box::new(descendant),
        }
    }

    #[must_use]
    fn contains_type(&self) -> bool {
        match self {
            Selector::Type(_) | Selector::TypeName(_) => true,
            Selector::Class(_) | Selector::PseudoClass(_) => false,
            Selector::And(selectors) => selectors.iter().any(Self::contains_type),
            Selector::Descendant {
                ancestor,
                descendant,
            } => ancestor.contains_type() || descendant.contains_type(),
        }
    }

    #[must_use]
    fn contains_descendant(&self) -> bool {
        match self {
            Selector::Descendant { .. } => true,
            Selector::And(selectors) => selectors.iter().any(Self::contains_descendant),
            Selector::Type(_)
            | Selector::TypeName(_)
            | Selector::Class(_)
            | Selector::PseudoClass(_) => false,
        }
    }
}

/// Style payload set by a matching rule.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleSetter {
    pub layout: LayoutStyle,
    pub colors: ColorStyle,
    pub text: TextStyle,
    pub font_family: Option<Vec<String>>,
    pub box_shadow: Option<BoxShadow>,
    pub transition: Option<StyleTransition>,
}

/// Selector + style payload.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRule {
    pub selector: Selector,
    pub setter: StyleSetter,
}

impl StyleRule {
    #[must_use]
    pub fn new(selector: Selector, setter: StyleSetter) -> Self {
        Self { selector, setter }
    }

    #[must_use]
    pub fn class(class_name: impl Into<String>, setter: StyleSetter) -> Self {
        Self::new(Selector::class(class_name), setter)
    }
}

/// Global class-based style table.
#[derive(Resource, Asset, TypePath, Debug, Clone, Default)]
pub struct StyleSheet {
    pub rules: Vec<StyleRule>,
}

/// Handle/path of the active stylesheet asset used for runtime style hot-reload.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveStyleSheetAsset {
    pub handle: Option<Handle<StyleSheet>>,
    pub path: Option<String>,
}

/// Message cursor for [`AssetEvent<StyleSheet>`] in world-exclusive systems.
#[derive(Resource, Default)]
pub struct StyleAssetEventCursor(pub MessageCursor<AssetEvent<StyleSheet>>);

/// Selector set currently owned by the active stylesheet asset.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveStyleSheetSelectors(pub HashSet<Selector>);

/// Name-to-component-type map used by selector type tags loaded from RON assets.
#[derive(Resource, Debug, Clone, Default)]
pub struct StyleTypeRegistry {
    by_name: HashMap<String, TypeId>,
}

impl StyleTypeRegistry {
    pub fn register_type_name<T: Component>(&mut self, name: impl Into<String>) {
        self.by_name.insert(name.into(), TypeId::of::<T>());
    }

    pub fn register_type_aliases<T: Component>(&mut self) {
        let full = std::any::type_name::<T>();
        self.register_type_name::<T>(full);
        if let Some(short) = full.rsplit("::").next() {
            self.register_type_name::<T>(short);
        }
    }

    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<TypeId> {
        self.by_name.get(name).copied()
    }
}

impl StyleSheet {
    #[must_use]
    pub fn with_rule(mut self, rule: StyleRule) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn add_rule(&mut self, rule: StyleRule) {
        self.rules.push(rule);
    }

    #[must_use]
    pub fn with_class(mut self, class_name: impl Into<String>, setter: StyleSetter) -> Self {
        self.set_class(class_name, setter);
        self
    }

    pub fn set_class(&mut self, class_name: impl Into<String>, setter: StyleSetter) {
        let class_name = class_name.into();
        if let Some(existing) = self.rules.iter_mut().find(|rule| {
            matches!(&rule.selector, Selector::Class(existing_name) if existing_name == &class_name)
        }) {
            existing.setter = setter;
            return;
        }

        self.rules.push(StyleRule::class(class_name, setter));
    }

    #[must_use]
    pub fn get_class(&self, class_name: &str) -> Option<&StyleSetter> {
        self.rules.iter().find_map(|rule| {
            if matches!(&rule.selector, Selector::Class(name) if name == class_name) {
                Some(&rule.setter)
            } else {
                None
            }
        })
    }

    #[must_use]
    fn has_type_selectors(&self) -> bool {
        self.rules.iter().any(|rule| rule.selector.contains_type())
    }

    #[must_use]
    fn has_descendant_selectors(&self) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.selector.contains_descendant())
    }
}

/// Default stylesheet asset path loaded by [`crate::BevyXilemPlugin`].
pub const DEFAULT_STYLE_SHEET_ASSET_PATH: &str = "themes/default_theme.ron";

/// Register built-in ECS component type aliases usable from RON selectors.
pub fn register_builtin_style_type_aliases(world: &mut World) {
    world.init_resource::<StyleTypeRegistry>();
    let mut registry = world.resource_mut::<StyleTypeRegistry>();

    use crate::ecs::*;
    registry.register_type_aliases::<UiRoot>();
    registry.register_type_aliases::<UiOverlayRoot>();
    registry.register_type_aliases::<UiFlexColumn>();
    registry.register_type_aliases::<UiFlexRow>();
    registry.register_type_aliases::<UiLabel>();
    registry.register_type_aliases::<UiButton>();
    registry.register_type_aliases::<UiCheckbox>();
    registry.register_type_aliases::<UiSlider>();
    registry.register_type_aliases::<UiSwitch>();
    registry.register_type_aliases::<UiTextInput>();
    registry.register_type_aliases::<UiDialog>();
    registry.register_type_aliases::<UiComboBox>();
    registry.register_type_aliases::<UiDropdownMenu>();
    registry.register_type_aliases::<UiRadioGroup>();
    registry.register_type_aliases::<UiTabBar>();
    registry.register_type_aliases::<UiTreeNode>();
    registry.register_type_aliases::<UiTable>();
    registry.register_type_aliases::<UiMenuBar>();
    registry.register_type_aliases::<UiMenuBarItem>();
    registry.register_type_aliases::<UiMenuItemPanel>();
    registry.register_type_aliases::<UiTooltip>();
    registry.register_type_aliases::<UiSpinner>();
    registry.register_type_aliases::<UiColorPicker>();
    registry.register_type_aliases::<UiColorPickerPanel>();
    registry.register_type_aliases::<UiGroupBox>();
    registry.register_type_aliases::<UiSplitPane>();
    registry.register_type_aliases::<UiToast>();
    registry.register_type_aliases::<UiDatePicker>();
    registry.register_type_aliases::<UiDatePickerPanel>();
}

/// Set the active stylesheet asset path used for loading + hot-reload.
pub fn set_active_stylesheet_asset_path(world: &mut World, asset_path: impl Into<String>) {
    let asset_path = asset_path.into();
    world.init_resource::<ActiveStyleSheetAsset>();

    let needs_reload = world
        .get_resource::<ActiveStyleSheetAsset>()
        .is_none_or(|active| active.path.as_deref() != Some(asset_path.as_str()));

    if !needs_reload {
        return;
    }

    let mut active = world.resource_mut::<ActiveStyleSheetAsset>();
    active.path = Some(asset_path);
    active.handle = None;
}

/// Ensure the active stylesheet asset handle is loaded from the configured path.
pub fn ensure_active_stylesheet_asset_handle(world: &mut World) {
    let path = world
        .get_resource::<ActiveStyleSheetAsset>()
        .and_then(|active| active.path.clone());

    let Some(path) = path else {
        return;
    };

    let has_handle = world
        .get_resource::<ActiveStyleSheetAsset>()
        .is_some_and(|active| active.handle.is_some());
    if has_handle {
        return;
    }

    let Some(asset_server) = world.get_resource::<AssetServer>() else {
        return;
    };

    let handle = asset_server.load::<StyleSheet>(path);
    world.resource_mut::<ActiveStyleSheetAsset>().handle = Some(handle);
}

/// Apply loaded stylesheet asset updates to the live [`StyleSheet`] resource.
pub fn sync_stylesheet_asset_events(world: &mut World) {
    let active_handle_id = world
        .get_resource::<ActiveStyleSheetAsset>()
        .and_then(|active| active.handle.as_ref())
        .map(|handle| handle.id());

    let Some(active_handle_id) = active_handle_id else {
        return;
    };

    if !world.contains_resource::<Messages<AssetEvent<StyleSheet>>>() {
        return;
    }

    let mut should_refresh = false;
    world.resource_scope(|world, mut cursor: Mut<StyleAssetEventCursor>| {
        let messages = world.resource::<Messages<AssetEvent<StyleSheet>>>();
        for event in cursor.0.read(messages) {
            match event {
                AssetEvent::Added { id }
                | AssetEvent::Modified { id }
                | AssetEvent::LoadedWithDependencies { id } => {
                    if *id == active_handle_id {
                        should_refresh = true;
                    }
                }
                _ => {}
            }
        }
    });

    if !should_refresh {
        return;
    }

    let Some(active_handle) = world
        .get_resource::<ActiveStyleSheetAsset>()
        .and_then(|active| active.handle.clone())
    else {
        return;
    };

    let Some(loaded_stylesheet) = world
        .get_resource::<Assets<StyleSheet>>()
        .and_then(|assets| assets.get(&active_handle))
        .cloned()
    else {
        return;
    };

    world.init_resource::<ActiveStyleSheetSelectors>();

    let incoming_selectors = loaded_stylesheet
        .rules
        .iter()
        .map(|rule| rule.selector.clone())
        .collect::<HashSet<_>>();

    let previous_asset_selectors = world
        .get_resource::<ActiveStyleSheetSelectors>()
        .map(|selectors| selectors.0.clone())
        .unwrap_or_default();

    let mut runtime_sheet = world.resource_mut::<StyleSheet>();
    runtime_sheet
        .rules
        .retain(|rule| !previous_asset_selectors.contains(&rule.selector));
    runtime_sheet.rules.extend(loaded_stylesheet.rules);

    world.resource_mut::<ActiveStyleSheetSelectors>().0 = incoming_selectors;
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResolvedLayoutStyle {
    pub padding: f64,
    pub gap: f64,
    pub corner_radius: f64,
    pub border_width: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResolvedColorStyle {
    pub bg: Option<Color>,
    pub text: Option<Color>,
    pub border: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedTextStyle {
    pub size: f32,
}

impl Default for ResolvedTextStyle {
    fn default() -> Self {
        Self {
            size: theme::TEXT_SIZE_NORMAL,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedStyle {
    pub layout: ResolvedLayoutStyle,
    pub colors: ResolvedColorStyle,
    pub text: ResolvedTextStyle,
    pub font_family: Option<Vec<String>>,
    pub box_shadow: Option<BoxShadow>,
    pub transition: Option<StyleTransition>,
}

/// Structural interaction events emitted by ECS-backed widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiInteractionEvent {
    PointerEntered,
    PointerLeft,
    PointerPressed,
    PointerReleased,
}

fn merge_layout(dst: &mut LayoutStyle, src: &LayoutStyle) {
    if src.padding.is_some() {
        dst.padding = src.padding;
    }
    if src.gap.is_some() {
        dst.gap = src.gap;
    }
    if src.corner_radius.is_some() {
        dst.corner_radius = src.corner_radius;
    }
    if src.border_width.is_some() {
        dst.border_width = src.border_width;
    }
}

fn merge_colors(dst: &mut ColorStyle, src: &ColorStyle) {
    if src.bg.is_some() {
        dst.bg = src.bg;
    }
    if src.text.is_some() {
        dst.text = src.text;
    }
    if src.border.is_some() {
        dst.border = src.border;
    }
    if src.hover_bg.is_some() {
        dst.hover_bg = src.hover_bg;
    }
    if src.hover_text.is_some() {
        dst.hover_text = src.hover_text;
    }
    if src.hover_border.is_some() {
        dst.hover_border = src.hover_border;
    }
    if src.pressed_bg.is_some() {
        dst.pressed_bg = src.pressed_bg;
    }
    if src.pressed_text.is_some() {
        dst.pressed_text = src.pressed_text;
    }
    if src.pressed_border.is_some() {
        dst.pressed_border = src.pressed_border;
    }
}

fn merge_text(dst: &mut TextStyle, src: &TextStyle) {
    if src.size.is_some() {
        dst.size = src.size;
    }
}

fn merge_setter(dst: &mut StyleSetter, setter: &StyleSetter) {
    merge_layout(&mut dst.layout, &setter.layout);
    merge_colors(&mut dst.colors, &setter.colors);
    merge_text(&mut dst.text, &setter.text);
    if setter.font_family.is_some() {
        dst.font_family = setter.font_family.clone();
    }
    if setter.box_shadow.is_some() {
        dst.box_shadow = setter.box_shadow;
    }
    if setter.transition.is_some() {
        dst.transition = setter.transition;
    }
}

fn component_matches_type(world: &World, entity: Entity, component_id: ComponentId) -> bool {
    world
        .get_entity(entity)
        .is_ok_and(|entity_ref| entity_ref.contains_id(component_id))
}

fn entity_has_matching_ancestor(
    world: &World,
    entity: Entity,
    ancestor_selector: &Selector,
) -> bool {
    let mut current = entity;

    while let Some(child_of) = world.get::<ChildOf>(current) {
        let parent = child_of.parent();
        if selector_matches_entity(world, parent, ancestor_selector) {
            return true;
        }
        current = parent;
    }

    false
}

fn selector_matches_entity(world: &World, entity: Entity, selector: &Selector) -> bool {
    match selector {
        Selector::Type(type_id) => world
            .components()
            .get_id(*type_id)
            .is_some_and(|component_id| component_matches_type(world, entity, component_id)),
        Selector::TypeName(name) => world
            .get_resource::<StyleTypeRegistry>()
            .and_then(|registry| registry.resolve(name))
            .and_then(|type_id| world.components().get_id(type_id))
            .is_some_and(|component_id| component_matches_type(world, entity, component_id)),
        Selector::Class(name) => world
            .get::<StyleClass>(entity)
            .is_some_and(|style_class| style_class.0.iter().any(|class| class == name)),
        Selector::PseudoClass(PseudoClass::Hovered) => world.get::<Hovered>(entity).is_some(),
        Selector::PseudoClass(PseudoClass::Pressed) => world.get::<Pressed>(entity).is_some(),
        Selector::And(selectors) => selectors
            .iter()
            .all(|selector| selector_matches_entity(world, entity, selector)),
        Selector::Descendant {
            ancestor,
            descendant,
        } => {
            selector_matches_entity(world, entity, descendant)
                && entity_has_matching_ancestor(world, entity, ancestor)
        }
    }
}

fn selector_matches_class_context(
    world: &World,
    entity: Option<Entity>,
    selector: &Selector,
    has_class: &impl Fn(&str) -> bool,
) -> bool {
    match selector {
        Selector::Type(_) | Selector::TypeName(_) => false,
        Selector::Class(name) => has_class(name),
        Selector::PseudoClass(PseudoClass::Hovered) => {
            entity.is_some_and(|entity| world.get::<Hovered>(entity).is_some())
        }
        Selector::PseudoClass(PseudoClass::Pressed) => {
            entity.is_some_and(|entity| world.get::<Pressed>(entity).is_some())
        }
        Selector::And(selectors) => selectors
            .iter()
            .all(|selector| selector_matches_class_context(world, entity, selector, has_class)),
        Selector::Descendant {
            ancestor,
            descendant,
        } => {
            let Some(entity) = entity else {
                return false;
            };

            selector_matches_class_context(world, Some(entity), descendant, has_class)
                && entity_has_matching_ancestor(world, entity, ancestor)
        }
    }
}

fn merged_from_class_names<'a>(
    world: &World,
    entity: Option<Entity>,
    class_names: impl IntoIterator<Item = &'a str>,
) -> StyleSetter {
    let mut merged = StyleSetter::default();
    let Some(sheet) = world.get_resource::<StyleSheet>() else {
        return merged;
    };

    let class_set = class_names.into_iter().collect::<HashSet<_>>();
    let has_class = |class_name: &str| class_set.contains(class_name);

    for rule in &sheet.rules {
        if selector_matches_class_context(world, entity, &rule.selector, &has_class) {
            merge_setter(&mut merged, &rule.setter);
        }
    }

    merged
}

fn merged_for_entity(world: &World, entity: Entity) -> (StyleSetter, bool) {
    let mut merged = StyleSetter::default();
    let mut matched_rule = false;

    if let Some(sheet) = world.get_resource::<StyleSheet>() {
        for rule in &sheet.rules {
            if selector_matches_entity(world, entity, &rule.selector) {
                merge_setter(&mut merged, &rule.setter);
                matched_rule = true;
            }
        }
    }

    if let Some(layout) = world.get::<LayoutStyle>(entity) {
        merge_layout(&mut merged.layout, layout);
    }
    if let Some(colors) = world.get::<ColorStyle>(entity) {
        merge_colors(&mut merged.colors, colors);
    }
    if let Some(text) = world.get::<TextStyle>(entity) {
        merge_text(&mut merged.text, text);
    }
    if let Some(transition) = world.get::<StyleTransition>(entity) {
        merged.transition = Some(*transition);
    }

    (merged, matched_rule)
}

fn target_colors(world: &World, entity: Entity, colors: &ColorStyle) -> ResolvedColorStyle {
    let hovered = world.get::<Hovered>(entity).is_some();
    let pressed = world.get::<Pressed>(entity).is_some();

    let mut resolved = ResolvedColorStyle {
        bg: colors.bg,
        text: colors.text,
        border: colors.border,
    };

    if hovered {
        if colors.hover_bg.is_some() {
            resolved.bg = colors.hover_bg;
        }
        if colors.hover_text.is_some() {
            resolved.text = colors.hover_text;
        }
        if colors.hover_border.is_some() {
            resolved.border = colors.hover_border;
        }
    }

    if pressed {
        if colors.pressed_bg.is_some() {
            resolved.bg = colors.pressed_bg;
        }
        if colors.pressed_text.is_some() {
            resolved.text = colors.pressed_text;
        }
        if colors.pressed_border.is_some() {
            resolved.border = colors.pressed_border;
        }
    }

    resolved
}

fn to_resolved_layout(layout: &LayoutStyle) -> ResolvedLayoutStyle {
    ResolvedLayoutStyle {
        padding: layout.padding.unwrap_or(0.0),
        gap: layout.gap.unwrap_or(0.0),
        corner_radius: layout.corner_radius.unwrap_or(0.0),
        border_width: layout.border_width.unwrap_or(0.0),
    }
}

fn to_resolved_text(text: &TextStyle) -> ResolvedTextStyle {
    ResolvedTextStyle {
        size: text.size.unwrap_or(theme::TEXT_SIZE_NORMAL),
    }
}

fn has_any_style_source(world: &World, entity: Entity, matched_rule: bool) -> bool {
    matched_rule
        || world.get::<StyleClass>(entity).is_some()
        || world.get::<LayoutStyle>(entity).is_some()
        || world.get::<ColorStyle>(entity).is_some()
        || world.get::<TextStyle>(entity).is_some()
        || world.get::<StyleTransition>(entity).is_some()
}

fn resolved_from_merged(
    world: &World,
    entity: Entity,
    merged: &StyleSetter,
    include_current_override: bool,
) -> ResolvedStyle {
    let mut colors = target_colors(world, entity, &merged.colors);

    if include_current_override && let Some(current) = world.get::<CurrentColorStyle>(entity) {
        if current.bg.is_some() {
            colors.bg = current.bg;
        }
        if current.text.is_some() {
            colors.text = current.text;
        }
        if current.border.is_some() {
            colors.border = current.border;
        }
    }

    ResolvedStyle {
        layout: to_resolved_layout(&merged.layout),
        colors,
        text: to_resolved_text(&merged.text),
        font_family: merged.font_family.clone(),
        box_shadow: merged.box_shadow,
        transition: merged.transition,
    }
}

fn compute_resolved_style(world: &World, entity: Entity) -> Option<ResolvedStyle> {
    let (merged, matched_rule) = merged_for_entity(world, entity);
    if !has_any_style_source(world, entity, matched_rule) {
        return None;
    }

    Some(resolved_from_merged(world, entity, &merged, false))
}

/// Resolve final style for an entity.
///
/// Cascading order:
/// 1. class styles from [`StyleSheet`] and [`StyleClass`]
/// 2. inline components [`LayoutStyle`], [`ColorStyle`], [`TextStyle`]
/// 3. pseudo classes [`Hovered`] / [`Pressed`]
/// 4. animated override from [`CurrentColorStyle`] when present
#[must_use]
pub fn resolve_style(world: &World, entity: Entity) -> ResolvedStyle {
    if let Some(computed) = world.get::<ComputedStyle>(entity) {
        let mut style = ResolvedStyle {
            layout: computed.layout,
            colors: computed.colors,
            text: computed.text,
            font_family: computed.font_family.clone(),
            box_shadow: computed.box_shadow,
            transition: computed.transition,
        };

        if let Some(current) = world.get::<CurrentColorStyle>(entity) {
            if current.bg.is_some() {
                style.colors.bg = current.bg;
            }
            if current.text.is_some() {
                style.colors.text = current.text;
            }
            if current.border.is_some() {
                style.colors.border = current.border;
            }
        }

        return style;
    }

    compute_resolved_style(world, entity).unwrap_or_default()
}

/// Resolve style from class names only, without inline entity overrides.
#[must_use]
pub fn resolve_style_for_classes<'a>(
    world: &World,
    class_names: impl IntoIterator<Item = &'a str>,
) -> ResolvedStyle {
    let merged = merged_from_class_names(world, None, class_names);

    ResolvedStyle {
        layout: to_resolved_layout(&merged.layout),
        colors: ResolvedColorStyle {
            bg: merged.colors.bg,
            text: merged.colors.text,
            border: merged.colors.border,
        },
        text: to_resolved_text(&merged.text),
        font_family: merged.font_family,
        box_shadow: merged.box_shadow,
        transition: merged.transition,
    }
}

/// Resolve style from class names while applying pseudo-state from a specific entity.
///
/// This is useful when a control's visual style is class-driven, but hover/pressed
/// state is tracked on an ECS entity via [`Hovered`] / [`Pressed`].
#[must_use]
pub fn resolve_style_for_entity_classes<'a>(
    world: &World,
    entity: Entity,
    class_names: impl IntoIterator<Item = &'a str>,
) -> ResolvedStyle {
    let merged = merged_from_class_names(world, Some(entity), class_names);
    resolved_from_merged(world, entity, &merged, false)
}

/// Apply box/layout styling on any widget view.
pub fn apply_widget_style<V>(view: V, style: &ResolvedStyle) -> impl WidgetView<(), ()>
where
    V: WidgetView<(), ()>,
{
    sized_box(view)
        .padding(style.layout.padding)
        .corner_radius(style.layout.corner_radius)
        .border(
            style.colors.border.unwrap_or(Color::TRANSPARENT),
            style.layout.border_width,
        )
        .background_color(style.colors.bg.unwrap_or(Color::TRANSPARENT))
        .box_shadow(style.box_shadow.unwrap_or_default())
}

/// Apply style directly on the target widget.
///
/// This should be preferred for interactive controls to ensure visual bounds
/// and hit-testing bounds remain identical.
pub fn apply_direct_widget_style<V>(view: V, style: &ResolvedStyle) -> impl WidgetView<(), ()>
where
    V: WidgetView<(), ()>,
    V::Widget: Sized
        + HasProperty<Padding>
        + HasProperty<CornerRadius>
        + HasProperty<BorderColor>
        + HasProperty<BorderWidth>
        + HasProperty<Background>
        + HasProperty<BoxShadow>,
{
    view.padding(style.layout.padding)
        .corner_radius(style.layout.corner_radius)
        .border(
            style.colors.border.unwrap_or(Color::TRANSPARENT),
            style.layout.border_width,
        )
        .background_color(style.colors.bg.unwrap_or(Color::TRANSPARENT))
        .box_shadow(style.box_shadow.unwrap_or_default())
}

fn to_target_component(colors: ResolvedColorStyle) -> TargetColorStyle {
    TargetColorStyle {
        bg: colors.bg,
        text: colors.text,
        border: colors.border,
    }
}

fn to_current_component(colors: TargetColorStyle) -> CurrentColorStyle {
    CurrentColorStyle {
        bg: colors.bg,
        text: colors.text,
        border: colors.border,
    }
}

fn ensure_current(world: &mut World, entity: Entity, current: CurrentColorStyle) {
    if let Some(mut current_component) = world.get_mut::<CurrentColorStyle>(entity) {
        *current_component = current;
    } else {
        world.entity_mut(entity).insert(current);
    }
}

fn quadratic_in_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x < 0.5 {
        2.0 * x * x
    } else {
        1.0 - ((-2.0 * x + 2.0).powi(2) / 2.0)
    }
}

fn spawn_color_style_tween(
    world: &mut World,
    entity: Entity,
    start: CurrentColorStyle,
    end: CurrentColorStyle,
    duration_secs: f32,
) {
    let tween = Tween::new::<CurrentColorStyle, _>(
        EaseMethod::CustomFunction(quadratic_in_out),
        Duration::from_secs_f32(duration_secs.max(0.0)),
        ColorStyleLens { start, end },
    );

    world
        .entity_mut(entity)
        .insert((TweenAnim::new(tween), StyleManagedTween));
}

fn clear_style_managed_tween(world: &mut World, entity: Entity) {
    if world.get::<StyleManagedTween>(entity).is_some() {
        world.entity_mut(entity).remove::<TweenAnim>();
        world.entity_mut(entity).remove::<StyleManagedTween>();
    }
}

/// Consume interaction events and synchronize [`Hovered`] / [`Pressed`] marker components.
pub fn sync_ui_interaction_markers(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiInteractionEvent>();

    for event in events {
        if world.get_entity(event.entity).is_err() {
            continue;
        }

        match event.action {
            UiInteractionEvent::PointerEntered => {
                if world.get::<Hovered>(event.entity).is_none() {
                    world.entity_mut(event.entity).insert(Hovered);
                    world.entity_mut(event.entity).insert(StyleDirty);
                }
            }
            UiInteractionEvent::PointerLeft => {
                if world.get::<Hovered>(event.entity).is_some() {
                    world.entity_mut(event.entity).remove::<Hovered>();
                    world.entity_mut(event.entity).insert(StyleDirty);
                }
            }
            UiInteractionEvent::PointerPressed => {
                if world.get::<Pressed>(event.entity).is_none() {
                    world.entity_mut(event.entity).insert(Pressed);
                    world.entity_mut(event.entity).insert(StyleDirty);
                }
            }
            UiInteractionEvent::PointerReleased => {
                if world.get::<Pressed>(event.entity).is_some() {
                    world.entity_mut(event.entity).remove::<Pressed>();
                    world.entity_mut(event.entity).insert(StyleDirty);
                }
            }
        }
    }
}

/// Incremental invalidation: marks entities that need style recomputation.
pub fn mark_style_dirty(world: &mut World) {
    let stylesheet_changed =
        world.is_resource_added::<StyleSheet>() || world.is_resource_changed::<StyleSheet>();

    let mut dirty = {
        let mut query = world.query_filtered::<Entity, Or<(
            Changed<StyleClass>,
            Changed<LayoutStyle>,
            Changed<ColorStyle>,
            Changed<TextStyle>,
            Changed<StyleTransition>,
            Changed<Hovered>,
            Changed<Pressed>,
        )>>();
        query.iter(world).collect::<Vec<_>>()
    };

    let has_type_selectors = world
        .get_resource::<StyleSheet>()
        .is_some_and(StyleSheet::has_type_selectors);
    let has_descendant_selectors = world
        .get_resource::<StyleSheet>()
        .is_some_and(StyleSheet::has_descendant_selectors);

    if stylesheet_changed {
        if has_type_selectors || has_descendant_selectors {
            let mut all_entities = world.query::<Entity>();
            dirty.extend(all_entities.iter(world));
        } else {
            let mut candidates = world.query_filtered::<Entity, Or<(
                With<StyleClass>,
                With<LayoutStyle>,
                With<ColorStyle>,
                With<TextStyle>,
                With<StyleTransition>,
                With<ComputedStyle>,
            )>>();
            dirty.extend(candidates.iter(world));
        }
    }

    if has_descendant_selectors {
        let mut descendants = Vec::new();
        for entity in &dirty {
            let mut stack = vec![*entity];
            while let Some(current) = stack.pop() {
                if let Some(children) = world.get::<Children>(current) {
                    for child in children.iter() {
                        descendants.push(child);
                        stack.push(child);
                    }
                }
            }
        }
        dirty.extend(descendants);
    }

    if !has_type_selectors && !has_descendant_selectors {
        let stale = {
            let mut stale_query =
                world.query_filtered::<Entity, (With<ComputedStyle>, Without<StyleDirty>)>();
            stale_query
                .iter(world)
                .filter(|entity| {
                    world.get::<StyleClass>(*entity).is_none()
                        && world.get::<LayoutStyle>(*entity).is_none()
                        && world.get::<ColorStyle>(*entity).is_none()
                        && world.get::<TextStyle>(*entity).is_none()
                        && world.get::<StyleTransition>(*entity).is_none()
                })
                .collect::<Vec<_>>()
        };
        dirty.extend(stale);
    }

    let mut unique = HashSet::new();
    for entity in dirty {
        if unique.insert(entity) && world.get_entity(entity).is_ok() {
            world.entity_mut(entity).insert(StyleDirty);
        }
    }
}

/// Compute and store target/current style states used by transition animation.
pub fn sync_style_targets(world: &mut World) {
    let entities = {
        let mut query = world.query_filtered::<Entity, With<StyleDirty>>();
        query.iter(world).collect::<Vec<_>>()
    };

    if entities.is_empty() {
        return;
    }

    let snapshots = {
        let world_ref: &World = world;
        entities
            .into_iter()
            .map(|entity| (entity, compute_resolved_style(world_ref, entity)))
            .collect::<Vec<_>>()
    };

    for (entity, resolved) in snapshots {
        match resolved {
            Some(resolved) => {
                if let Some(mut computed) = world.get_mut::<ComputedStyle>(entity) {
                    computed.layout = resolved.layout;
                    computed.colors = resolved.colors;
                    computed.text = resolved.text;
                    computed.font_family = resolved.font_family.clone();
                    computed.box_shadow = resolved.box_shadow;
                    computed.transition = resolved.transition;
                } else {
                    world.entity_mut(entity).insert(ComputedStyle {
                        layout: resolved.layout,
                        colors: resolved.colors,
                        text: resolved.text,
                        font_family: resolved.font_family.clone(),
                        box_shadow: resolved.box_shadow,
                        transition: resolved.transition,
                    });
                }

                let target = to_target_component(resolved.colors);
                match resolved.transition {
                    Some(transition) => {
                        if let Some(mut target_component) =
                            world.get_mut::<TargetColorStyle>(entity)
                        {
                            *target_component = target;
                        } else {
                            world.entity_mut(entity).insert(target);
                        }

                        if world.get::<CurrentColorStyle>(entity).is_none() {
                            world
                                .entity_mut(entity)
                                .insert(to_current_component(target));
                        }

                        let end = to_current_component(target);

                        if transition.duration <= f32::EPSILON {
                            ensure_current(world, entity, end);
                            clear_style_managed_tween(world, entity);
                        } else {
                            let start = world
                                .get::<CurrentColorStyle>(entity)
                                .copied()
                                .unwrap_or(end);

                            if start != end {
                                spawn_color_style_tween(
                                    world,
                                    entity,
                                    start,
                                    end,
                                    transition.duration,
                                );
                            } else {
                                clear_style_managed_tween(world, entity);
                            }
                        }
                    }
                    None => {
                        world.entity_mut(entity).remove::<TargetColorStyle>();
                        world.entity_mut(entity).remove::<CurrentColorStyle>();
                        clear_style_managed_tween(world, entity);
                    }
                }
            }
            None => {
                world.entity_mut(entity).remove::<ComputedStyle>();
                world.entity_mut(entity).remove::<TargetColorStyle>();
                world.entity_mut(entity).remove::<CurrentColorStyle>();
                clear_style_managed_tween(world, entity);
            }
        }

        world.entity_mut(entity).remove::<StyleDirty>();
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let a = a as f32;
    let b = b as f32;
    (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
}

fn unpack_rgba(color: Color) -> (u8, u8, u8, u8) {
    let rgba = color.to_rgba8();
    (rgba.r, rgba.g, rgba.b, rgba.a)
}

fn lerp_color(current: Color, target: Color, t: f32) -> Color {
    let (cr, cg, cb, ca) = unpack_rgba(current);
    let (tr, tg, tb, ta) = unpack_rgba(target);
    Color::from_rgba8(
        lerp_u8(cr, tr, t),
        lerp_u8(cg, tg, t),
        lerp_u8(cb, tb, t),
        lerp_u8(ca, ta, t),
    )
}

fn transparent_like(color: Color) -> Color {
    let rgba = color.to_rgba8();
    Color::from_rgba8(rgba.r, rgba.g, rgba.b, 0)
}

fn lerp_optional_color(start: Option<Color>, end: Option<Color>, t: f32) -> Option<Color> {
    match (start, end) {
        (Some(start), Some(end)) => Some(lerp_color(start, end, t)),
        (None, Some(end)) => Some(lerp_color(transparent_like(end), end, t)),
        (Some(start), None) => {
            if t >= 1.0 {
                None
            } else {
                Some(lerp_color(start, transparent_like(start), t))
            }
        }
        (None, None) => None,
    }
}

fn lerp_f32(start: f32, end: f32, t: f32) -> f32 {
    start + ((end - start) * t)
}

fn lerp_f64(start: f64, end: f64, t: f32) -> f64 {
    start + ((end - start) * t as f64)
}

fn map_font_family_name(name: &str) -> FontFamily<'static> {
    let trimmed = name.trim();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(generic) = GenericFamily::parse(lower.as_str()) {
        FontFamily::Generic(generic)
    } else {
        FontFamily::Named(trimmed.to_string().into())
    }
}

fn font_stack_from_style(style: &ResolvedStyle) -> Option<FontStack<'static>> {
    let families = style.font_family.as_ref()?;
    if families.is_empty() {
        return None;
    }

    let mapped = families
        .iter()
        .map(|name| map_font_family_name(name))
        .collect::<Vec<_>>();

    if mapped.len() == 1 {
        Some(FontStack::Single(mapped.into_iter().next().unwrap()))
    } else {
        Some(FontStack::List(Cow::Owned(mapped)))
    }
}

/// Tween lens for animating computed style fields.
///
/// `font_family` is intentionally non-interpolated and only switches at the
/// end of the tween.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyleLens {
    pub start: ComputedStyle,
    pub end: ComputedStyle,
}

impl Lens<ComputedStyle> for ComputedStyleLens {
    fn lerp(&mut self, mut target: Mut<'_, ComputedStyle>, ratio: f32) {
        let t = ratio.clamp(0.0, 1.0);

        target.layout.padding = lerp_f64(self.start.layout.padding, self.end.layout.padding, t);
        target.layout.gap = lerp_f64(self.start.layout.gap, self.end.layout.gap, t);
        target.layout.corner_radius = lerp_f64(
            self.start.layout.corner_radius,
            self.end.layout.corner_radius,
            t,
        );
        target.layout.border_width = lerp_f64(
            self.start.layout.border_width,
            self.end.layout.border_width,
            t,
        );

        target.colors.bg = lerp_optional_color(self.start.colors.bg, self.end.colors.bg, t);
        target.colors.text = lerp_optional_color(self.start.colors.text, self.end.colors.text, t);
        target.colors.border =
            lerp_optional_color(self.start.colors.border, self.end.colors.border, t);

        target.text.size = lerp_f32(self.start.text.size, self.end.text.size, t);
        target.transition = if t < 1.0 {
            self.start.transition
        } else {
            self.end.transition
        };

        // font family changes are discrete (non-interpolable)
        target.font_family = if t < 1.0 {
            self.start.font_family.clone()
        } else {
            self.end.font_family.clone()
        };
    }
}

/// Tween lens for animating [`CurrentColorStyle`] with CSS-like smooth transitions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorStyleLens {
    pub start: CurrentColorStyle,
    pub end: CurrentColorStyle,
}

impl Lens<CurrentColorStyle> for ColorStyleLens {
    fn lerp(&mut self, mut target: Mut<'_, CurrentColorStyle>, ratio: f32) {
        target.bg = lerp_optional_color(self.start.bg, self.end.bg, ratio);
        target.text = lerp_optional_color(self.start.text, self.end.text, ratio);
        target.border = lerp_optional_color(self.start.border, self.end.border, ratio);
    }
}

/// Style transition stepping is handled by `bevy_tweening::TweeningPlugin`.
///
/// This hook is intentionally kept as a no-op for schedule readability and
/// compatibility with existing system chains.
pub fn animate_style_transitions(world: &mut World) {
    let _ = world;
}

/// Apply text + box styling to a label view.
pub fn apply_label_style(view: Label, style: &ResolvedStyle) -> impl WidgetView<(), ()> {
    let mut styled = view.text_size(style.text.size);
    if let Some(font_stack) = font_stack_from_style(style) {
        styled = styled.font(font_stack);
    }

    styled
        .color(style.colors.text.unwrap_or(Color::WHITE))
        .line_break_mode(LineBreaking::WordWrap)
}

/// Apply text + box styling to a text input view.
pub fn apply_text_input_style(
    view: TextInput<(), ()>,
    style: &ResolvedStyle,
) -> impl WidgetView<(), ()> {
    let mut styled = view.text_size(style.text.size);
    if let Some(font_stack) = font_stack_from_style(style) {
        styled = styled.font(font_stack);
    }
    if let Some(text_color) = style.colors.text {
        styled = styled.text_color(text_color);
    }
    styled
}

#[derive(Debug, Deserialize)]
struct StyleSheetDef {
    #[serde(default)]
    rules: Vec<StyleRuleDef>,
}

#[derive(Debug, Deserialize)]
struct StyleRuleDef {
    selector: SelectorDef,
    #[serde(default)]
    setter: StyleSetterDef,
}

#[derive(Debug, Deserialize)]
enum SelectorDef {
    Type(String),
    Class(String),
    PseudoClass(PseudoClass),
    And(Vec<SelectorDef>),
    Descendant {
        ancestor: Box<SelectorDef>,
        descendant: Box<SelectorDef>,
    },
}

impl From<SelectorDef> for Selector {
    fn from(value: SelectorDef) -> Self {
        match value {
            SelectorDef::Type(name) => Selector::type_name(name),
            SelectorDef::Class(name) => Selector::class(name),
            SelectorDef::PseudoClass(pseudo) => Selector::pseudo(pseudo),
            SelectorDef::And(selectors) => {
                Selector::and(selectors.into_iter().map(Into::into).collect::<Vec<_>>())
            }
            SelectorDef::Descendant {
                ancestor,
                descendant,
            } => Selector::descendant((*ancestor).into(), (*descendant).into()),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct StyleSetterDef {
    #[serde(default)]
    layout: LayoutStyleDef,
    #[serde(default)]
    colors: ColorStyleDef,
    #[serde(default)]
    text: TextStyleDef,
    #[serde(default)]
    font_family: OptionalValue<Vec<String>>,
    #[serde(default)]
    box_shadow: OptionalValue<BoxShadowDef>,
    #[serde(default)]
    transition: OptionalValue<StyleTransition>,
}

#[derive(Debug, Default, Deserialize)]
struct LayoutStyleDef {
    #[serde(default)]
    padding: OptionalValue<f64>,
    #[serde(default)]
    gap: OptionalValue<f64>,
    #[serde(default)]
    corner_radius: OptionalValue<f64>,
    #[serde(default)]
    border_width: OptionalValue<f64>,
}

impl LayoutStyleDef {
    fn into_layout(self) -> LayoutStyle {
        LayoutStyle {
            padding: self.padding.into_option(),
            gap: self.gap.into_option(),
            corner_radius: self.corner_radius.into_option(),
            border_width: self.border_width.into_option(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct TextStyleDef {
    #[serde(default)]
    size: OptionalValue<f32>,
}

impl TextStyleDef {
    fn into_text(self) -> TextStyle {
        TextStyle {
            size: self.size.into_option(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ColorStyleDef {
    #[serde(default)]
    bg: OptionalColorDef,
    #[serde(default, rename = "text")]
    text_color: OptionalColorDef,
    #[serde(default)]
    border: OptionalColorDef,
    #[serde(default)]
    hover_bg: OptionalColorDef,
    #[serde(default)]
    hover_text: OptionalColorDef,
    #[serde(default)]
    hover_border: OptionalColorDef,
    #[serde(default)]
    pressed_bg: OptionalColorDef,
    #[serde(default)]
    pressed_text: OptionalColorDef,
    #[serde(default)]
    pressed_border: OptionalColorDef,
}

#[derive(Debug, Clone, Deserialize)]
enum ColorDef {
    Rgb8(u8, u8, u8),
    Rgba8(u8, u8, u8, u8),
    Hex(String),
}

#[derive(Debug, Clone, Deserialize, Default)]
enum OptionalColorDef {
    #[default]
    None,
    Rgb8(u8, u8, u8),
    Rgba8(u8, u8, u8, u8),
    Hex(String),
}

impl OptionalColorDef {
    fn into_color(self) -> io::Result<Option<Color>> {
        match self {
            Self::None => Ok(None),
            Self::Rgb8(r, g, b) => Ok(Some(Color::from_rgb8(r, g, b))),
            Self::Rgba8(r, g, b, a) => Ok(Some(Color::from_rgba8(r, g, b, a))),
            Self::Hex(hex) => parse_hex_color(&hex).map(Some),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OptionalValue<T> {
    Value(T),
    Optional(Option<T>),
}

impl<T> Default for OptionalValue<T> {
    fn default() -> Self {
        Self::Optional(None)
    }
}

impl<T> OptionalValue<T> {
    fn into_option(self) -> Option<T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Optional(value) => value,
        }
    }
}

impl ColorDef {
    fn into_color(self) -> io::Result<Color> {
        match self {
            Self::Rgb8(r, g, b) => Ok(Color::from_rgb8(r, g, b)),
            Self::Rgba8(r, g, b, a) => Ok(Color::from_rgba8(r, g, b, a)),
            Self::Hex(hex) => parse_hex_color(&hex),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct BoxShadowDef {
    color: ColorDef,
    #[serde(default)]
    offset_x: f64,
    #[serde(default)]
    offset_y: f64,
    #[serde(default)]
    blur: f64,
}

impl BoxShadowDef {
    fn into_box_shadow(self) -> io::Result<BoxShadow> {
        Ok(
            BoxShadow::new(self.color.into_color()?, (self.offset_x, self.offset_y))
                .blur(self.blur),
        )
    }
}

impl StyleSetterDef {
    fn into_setter(self) -> io::Result<StyleSetter> {
        Ok(StyleSetter {
            layout: self.layout.into_layout(),
            colors: self.colors.into_color_style()?,
            text: self.text.into_text(),
            font_family: self.font_family.into_option(),
            box_shadow: self
                .box_shadow
                .into_option()
                .map(BoxShadowDef::into_box_shadow)
                .transpose()?,
            transition: self.transition.into_option(),
        })
    }
}

impl ColorStyleDef {
    fn into_color_style(self) -> io::Result<ColorStyle> {
        fn map_color(value: OptionalColorDef) -> io::Result<Option<Color>> {
            value.into_color()
        }

        Ok(ColorStyle {
            bg: map_color(self.bg)?,
            text: map_color(self.text_color)?,
            border: map_color(self.border)?,
            hover_bg: map_color(self.hover_bg)?,
            hover_text: map_color(self.hover_text)?,
            hover_border: map_color(self.hover_border)?,
            pressed_bg: map_color(self.pressed_bg)?,
            pressed_text: map_color(self.pressed_text)?,
            pressed_border: map_color(self.pressed_border)?,
        })
    }
}

fn parse_hex_color(hex: &str) -> io::Result<Color> {
    let hex = hex.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);

    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid hex color `{hex}`; expected #RRGGBB or #RRGGBBAA"),
        )
    };

    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| invalid())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| invalid())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| invalid())?;
            Ok(Color::from_rgb8(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| invalid())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| invalid())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| invalid())?;
            let a = u8::from_str_radix(&hex[6..8], 16).map_err(|_| invalid())?;
            Ok(Color::from_rgba8(r, g, b, a))
        }
        _ => Err(invalid()),
    }
}

fn stylesheet_from_ron_bytes(bytes: &[u8]) -> io::Result<StyleSheet> {
    let parsed: StyleSheetDef = ron::de::from_bytes(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse stylesheet RON: {error}"),
        )
    })?;

    let mut sheet = StyleSheet::default();
    for rule in parsed.rules {
        sheet.add_rule(StyleRule {
            selector: rule.selector.into(),
            setter: rule.setter.into_setter()?,
        });
    }

    Ok(sheet)
}

#[cfg(test)]
pub(crate) fn parse_stylesheet_ron_for_tests(ron_text: &str) -> io::Result<StyleSheet> {
    stylesheet_from_ron_bytes(ron_text.as_bytes())
}

/// Asset loader for stylesheet `.ron` files.
#[derive(Default, TypePath)]
pub struct StyleSheetRonLoader;

impl AssetLoader for StyleSheetRonLoader {
    type Asset = StyleSheet;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        stylesheet_from_ron_bytes(&bytes)
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}
