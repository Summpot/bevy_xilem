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
use serde::{
    Deserialize,
    de::{
        self, EnumAccess, IntoDeserializer, VariantAccess, Visitor,
        value::{MapAccessDeserializer, SeqAccessDeserializer},
    },
};
use xilem::{Color, style::Style as _};
use xilem_masonry::masonry::parley::{
    Alignment as ParleyTextAlign, FontFamily, GenericFamily, style::FontStack,
};
use xilem_masonry::masonry::properties::{
    Background, BorderColor, BorderWidth, BoxShadow, CornerRadius, LineBreaking, Padding,
};
use xilem_masonry::{
    WidgetView,
    view::{CrossAxisAlignment, Flex, Label, MainAxisAlignment, TextInput, sized_box, transformed},
};

use crate::UiEventQueue;

/// Marker component for CSS-like class names attached to an entity.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleClass(pub Vec<String>);

/// Marker component for entities whose style cache needs recomputation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct StyleDirty;

/// Transient interaction state used for pseudo-class styling.
///
/// This replaces frequently inserted/removed marker components like `Hovered`/`Pressed`.
/// Keeping state in a stable component avoids archetype churn when the pointer moves.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InteractionState {
    pub hovered: bool,
    pub pressed: bool,
}

/// Consolidated inline style overrides.
///
/// This is a "mega-component" that reduces archetype fragmentation vs inserting a
/// handful of smaller optional style components.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct InlineStyle {
    pub layout: LayoutStyle,
    pub colors: ColorStyle,
    pub text: TextStyle,
    pub transition: Option<StyleTransition>,
}

/// Inline layout style that can be attached to entities.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct LayoutStyle {
    pub padding: Option<f64>,
    pub gap: Option<f64>,
    pub corner_radius: Option<f64>,
    pub border_width: Option<f64>,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub scale: Option<f64>,
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
    pub text_align: Option<TextAlign>,
}

/// Main-axis content distribution for flex layouts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum JustifyContent {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Cross-axis alignment for flex layouts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum AlignItems {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

/// Text alignment for label-like UI components.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

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
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CurrentColorStyle {
    pub bg: Option<Color>,
    pub text: Option<Color>,
    pub border: Option<Color>,
    pub scale: f64,
}

/// Target color state derived from classes + inline style + pseudo state.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TargetColorStyle {
    pub bg: Option<Color>,
    pub text: Option<Color>,
    pub border: Option<Color>,
    pub scale: f64,
}

impl Default for CurrentColorStyle {
    fn default() -> Self {
        Self {
            bg: None,
            text: None,
            border: None,
            scale: 1.0,
        }
    }
}

impl Default for TargetColorStyle {
    fn default() -> Self {
        Self {
            bg: None,
            text: None,
            border: None,
            scale: 1.0,
        }
    }
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

/// Style payload value that can be either an explicit value or a token reference.
#[derive(Debug, Clone, PartialEq)]
pub enum StyleValue<T> {
    Value(T),
    Var(String),
}

impl<T> StyleValue<T> {
    #[must_use]
    pub fn value(value: T) -> Self {
        Self::Value(value)
    }

    #[must_use]
    pub fn var(name: impl Into<String>) -> Self {
        Self::Var(name.into())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutStyleValue {
    pub padding: Option<StyleValue<f64>>,
    pub gap: Option<StyleValue<f64>>,
    pub corner_radius: Option<StyleValue<f64>>,
    pub border_width: Option<StyleValue<f64>>,
    pub justify_content: Option<StyleValue<JustifyContent>>,
    pub align_items: Option<StyleValue<AlignItems>>,
    pub scale: Option<StyleValue<f64>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorStyleValue {
    pub bg: Option<StyleValue<Color>>,
    pub text: Option<StyleValue<Color>>,
    pub border: Option<StyleValue<Color>>,
    pub hover_bg: Option<StyleValue<Color>>,
    pub hover_text: Option<StyleValue<Color>>,
    pub hover_border: Option<StyleValue<Color>>,
    pub pressed_bg: Option<StyleValue<Color>>,
    pub pressed_text: Option<StyleValue<Color>>,
    pub pressed_border: Option<StyleValue<Color>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStyleValue {
    pub size: Option<StyleValue<f32>>,
    pub text_align: Option<StyleValue<TextAlign>>,
}

/// Token-aware style payload attached to stylesheet rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleSetterValue {
    pub layout: LayoutStyleValue,
    pub colors: ColorStyleValue,
    pub text: TextStyleValue,
    pub font_family: Option<StyleValue<Vec<String>>>,
    pub box_shadow: Option<StyleValue<BoxShadow>>,
    pub transition: Option<StyleValue<StyleTransition>>,
}

/// Token value stored in [`StyleSheet::tokens`].
#[derive(Debug, Clone, PartialEq)]
pub enum TokenValue {
    Color(Color),
    Float(f64),
    FontFamily(Vec<String>),
    BoxShadow(BoxShadow),
    Transition(StyleTransition),
}

impl From<LayoutStyle> for LayoutStyleValue {
    fn from(value: LayoutStyle) -> Self {
        Self {
            padding: value.padding.map(StyleValue::value),
            gap: value.gap.map(StyleValue::value),
            corner_radius: value.corner_radius.map(StyleValue::value),
            border_width: value.border_width.map(StyleValue::value),
            justify_content: value.justify_content.map(StyleValue::value),
            align_items: value.align_items.map(StyleValue::value),
            scale: value.scale.map(StyleValue::value),
        }
    }
}

impl From<ColorStyle> for ColorStyleValue {
    fn from(value: ColorStyle) -> Self {
        Self {
            bg: value.bg.map(StyleValue::value),
            text: value.text.map(StyleValue::value),
            border: value.border.map(StyleValue::value),
            hover_bg: value.hover_bg.map(StyleValue::value),
            hover_text: value.hover_text.map(StyleValue::value),
            hover_border: value.hover_border.map(StyleValue::value),
            pressed_bg: value.pressed_bg.map(StyleValue::value),
            pressed_text: value.pressed_text.map(StyleValue::value),
            pressed_border: value.pressed_border.map(StyleValue::value),
        }
    }
}

impl From<TextStyle> for TextStyleValue {
    fn from(value: TextStyle) -> Self {
        Self {
            size: value.size.map(StyleValue::value),
            text_align: value.text_align.map(StyleValue::value),
        }
    }
}

impl From<StyleSetter> for StyleSetterValue {
    fn from(value: StyleSetter) -> Self {
        Self {
            layout: value.layout.into(),
            colors: value.colors.into(),
            text: value.text.into(),
            font_family: value.font_family.map(StyleValue::value),
            box_shadow: value.box_shadow.map(StyleValue::value),
            transition: value.transition.map(StyleValue::value),
        }
    }
}

/// Selector + style payload.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRule {
    pub selector: Selector,
    pub setter: StyleSetterValue,
}

impl StyleRule {
    #[must_use]
    pub fn new(selector: Selector, setter: StyleSetter) -> Self {
        Self {
            selector,
            setter: setter.into(),
        }
    }

    #[must_use]
    pub fn new_with_values(selector: Selector, setter: StyleSetterValue) -> Self {
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
    pub tokens: HashMap<String, TokenValue>,
    pub rules: Vec<StyleRule>,
}

/// Baseline stylesheet tier populated from the embedded built-in theme.
#[derive(Resource, Debug, Clone, Default)]
pub struct BaseStyleSheet(pub StyleSheet);

/// Active user stylesheet tier loaded from external `.ron` assets.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveStyleSheet(pub StyleSheet);

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

/// Token names currently owned by the active stylesheet asset.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveStyleSheetTokenNames(pub HashSet<String>);

/// Registered named style variants parsed from a variant bundle.
#[derive(Resource, Debug, Clone, Default)]
pub struct RegisteredStyleVariants {
    pub default_variant: String,
    pub variants: HashMap<String, StyleSheet>,
}

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
            existing.setter = setter.into();
            return;
        }

        self.rules.push(StyleRule::class(class_name, setter));
    }

    #[must_use]
    pub fn get_class(&self, class_name: &str) -> Option<StyleSetter> {
        self.get_class_values(class_name)
            .map(|setter| resolve_setter_values(setter, &self.tokens))
    }

    #[must_use]
    pub fn get_class_values(&self, class_name: &str) -> Option<&StyleSetterValue> {
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

fn upsert_rule_by_selector(sheet: &mut StyleSheet, incoming: StyleRule) {
    if let Some(existing) = sheet
        .rules
        .iter_mut()
        .find(|rule| rule.selector == incoming.selector)
    {
        *existing = incoming;
    } else {
        sheet.rules.push(incoming);
    }
}

fn upsert_rules_by_selector(sheet: &mut StyleSheet, incoming: Vec<StyleRule>) {
    for rule in incoming {
        upsert_rule_by_selector(sheet, rule);
    }
}

fn merge_sheet_inplace(sheet: &mut StyleSheet, incoming: StyleSheet) {
    for (name, token) in incoming.tokens {
        sheet.tokens.insert(name, token);
    }
    upsert_rules_by_selector(sheet, incoming.rules);
}

fn apply_base_stylesheet(world: &mut World, new_base: StyleSheet) {
    world.init_resource::<BaseStyleSheet>();
    world.init_resource::<StyleSheet>();
    world.init_resource::<ActiveStyleSheetTokenNames>();

    let (previous_base_selectors, previous_base_tokens) = {
        let previous = &world.resource::<BaseStyleSheet>().0;
        (
            previous
                .rules
                .iter()
                .map(|rule| rule.selector.clone())
                .collect::<HashSet<_>>(),
            previous.tokens.keys().cloned().collect::<HashSet<_>>(),
        )
    };

    world.resource_mut::<BaseStyleSheet>().0 = new_base.clone();

    let active_selectors = world
        .get_resource::<ActiveStyleSheetSelectors>()
        .map(|selectors| selectors.0.clone())
        .unwrap_or_default();
    let active_tokens = world
        .get_resource::<ActiveStyleSheetTokenNames>()
        .map(|names| names.0.clone())
        .unwrap_or_default();

    let mut runtime_sheet = world.resource_mut::<StyleSheet>();
    runtime_sheet.rules.retain(|rule| {
        !previous_base_selectors.contains(&rule.selector)
            || active_selectors.contains(&rule.selector)
    });
    runtime_sheet
        .tokens
        .retain(|name, _| !previous_base_tokens.contains(name) || active_tokens.contains(name));

    for (name, token) in new_base.tokens {
        if !active_tokens.contains(name.as_str()) {
            runtime_sheet.tokens.insert(name, token);
        }
    }

    let filtered_rules = new_base
        .rules
        .into_iter()
        .filter(|rule| !active_selectors.contains(&rule.selector))
        .collect::<Vec<_>>();
    upsert_rules_by_selector(&mut runtime_sheet, filtered_rules);
}

/// Embedded baseline Fluent-inspired dark theme used with zero filesystem configuration.
pub const BUILTIN_FLUENT_DARK_THEME_RON: &str = include_str!("theme/fluent_dark.ron");

/// Embedded Fluent-inspired light palette overrides.
pub const BUILTIN_FLUENT_LIGHT_THEME_RON: &str = include_str!("theme/fluent_light.ron");

/// Embedded Fluent theme bundle containing multiple named variants.
pub const BUILTIN_FLUENT_THEME_RON: &str = include_str!("theme/fluent_theme.ron");

/// Built-in Fluent theme variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FluentThemeVariant {
    Dark,
    Light,
    HighContrast,
}

impl FluentThemeVariant {
    #[must_use]
    pub const fn as_name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::HighContrast => "high-contrast",
        }
    }
}

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
    registry.register_type_aliases::<UiScrollView>();
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

/// Parse stylesheet RON text into a runtime [`StyleSheet`].
pub fn parse_stylesheet_ron(ron_text: &str) -> io::Result<StyleSheet> {
    stylesheet_from_ron_bytes(ron_text.as_bytes())
}

/// Parse a multi-variant stylesheet bundle RON into registered variants.
pub fn parse_stylesheet_variants_ron(ron_text: &str) -> io::Result<RegisteredStyleVariants> {
    stylesheet_variants_from_ron_bytes(ron_text.as_bytes())
}

/// Parse + register named stylesheet variants in the world.
pub fn register_stylesheet_variants_ron(world: &mut World, ron_text: &str) -> io::Result<()> {
    let variants = parse_stylesheet_variants_ron(ron_text)?;
    world.insert_resource(variants);
    Ok(())
}

/// Install a previously registered stylesheet variant by name.
pub fn install_registered_style_variant(world: &mut World, variant_name: &str) -> io::Result<()> {
    let variants = world
        .get_resource::<RegisteredStyleVariants>()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "style variants are not registered in this world",
            )
        })?
        .clone();

    let selected = variants
        .variants
        .get(variant_name)
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("style variant `{variant_name}` is not registered"),
            )
        })?;

    apply_base_stylesheet(world, selected);
    Ok(())
}

/// Register all embedded Fluent variants from the bundled multi-variant theme file.
pub fn register_embedded_fluent_theme_variants(world: &mut World) -> io::Result<()> {
    register_stylesheet_variants_ron(world, BUILTIN_FLUENT_THEME_RON)
}

/// Install an embedded Fluent variant by string name.
pub fn install_embedded_fluent_theme_variant_by_name(
    world: &mut World,
    variant_name: &str,
) -> io::Result<()> {
    register_embedded_fluent_theme_variants(world)?;
    install_registered_style_variant(world, variant_name)
}

/// Install the embedded Fluent-inspired dark baseline stylesheet.
pub fn install_embedded_fluent_dark_theme(world: &mut World) -> io::Result<()> {
    install_embedded_fluent_theme_variant_by_name(world, FluentThemeVariant::Dark.as_name())
}

/// Install one of the embedded Fluent theme variants.
pub fn install_embedded_fluent_theme_variant(
    world: &mut World,
    variant: FluentThemeVariant,
) -> io::Result<()> {
    install_embedded_fluent_theme_variant_by_name(world, variant.as_name())
}

/// Install the embedded Fluent-inspired light theme.
///
/// This keeps selector/rule coverage identical to Fluent dark by first installing
/// the dark baseline and then applying light token overrides.
pub fn install_embedded_fluent_light_theme(world: &mut World) -> io::Result<()> {
    install_embedded_fluent_theme_variant_by_name(world, FluentThemeVariant::Light.as_name())
}

/// Merge a baseline stylesheet RON into the base + runtime tiers.
///
/// Runtime insertion preserves active stylesheet precedence by skipping selectors
/// and tokens currently owned by the active stylesheet asset.
pub fn merge_base_stylesheet_ron(world: &mut World, ron_text: &str) -> io::Result<()> {
    let parsed = parse_stylesheet_ron(ron_text)?;

    world.init_resource::<BaseStyleSheet>();
    world.init_resource::<StyleSheet>();
    world.init_resource::<ActiveStyleSheetTokenNames>();

    let incoming_tokens = parsed.tokens;
    let incoming_rules = parsed.rules;

    {
        let mut base_sheet = world.resource_mut::<BaseStyleSheet>();
        for (name, token) in &incoming_tokens {
            base_sheet.0.tokens.insert(name.clone(), token.clone());
        }
        upsert_rules_by_selector(&mut base_sheet.0, incoming_rules.clone());
    }

    let active_selectors = world
        .get_resource::<ActiveStyleSheetSelectors>()
        .map(|selectors| selectors.0.clone())
        .unwrap_or_default();
    let active_tokens = world
        .get_resource::<ActiveStyleSheetTokenNames>()
        .map(|names| names.0.clone())
        .unwrap_or_default();

    let filtered = incoming_rules
        .into_iter()
        .filter(|rule| !active_selectors.contains(&rule.selector))
        .collect::<Vec<_>>();

    {
        let mut runtime_sheet = world.resource_mut::<StyleSheet>();
        for (name, token) in incoming_tokens {
            if !active_tokens.contains(name.as_str()) {
                runtime_sheet.tokens.insert(name, token);
            }
        }
        upsert_rules_by_selector(&mut runtime_sheet, filtered);
    }

    Ok(())
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

    world.init_resource::<ActiveStyleSheet>();
    world.init_resource::<ActiveStyleSheetSelectors>();
    world.init_resource::<ActiveStyleSheetTokenNames>();
    world.resource_mut::<ActiveStyleSheet>().0 = loaded_stylesheet.clone();

    let incoming_selectors = loaded_stylesheet
        .rules
        .iter()
        .map(|rule| rule.selector.clone())
        .collect::<HashSet<_>>();
    let incoming_token_names = loaded_stylesheet
        .tokens
        .keys()
        .cloned()
        .collect::<HashSet<_>>();

    let previous_asset_selectors = world
        .get_resource::<ActiveStyleSheetSelectors>()
        .map(|selectors| selectors.0.clone())
        .unwrap_or_default();
    let previous_asset_token_names = world
        .get_resource::<ActiveStyleSheetTokenNames>()
        .map(|names| names.0.clone())
        .unwrap_or_default();

    let mut runtime_sheet = world.resource_mut::<StyleSheet>();
    runtime_sheet
        .rules
        .retain(|rule| !previous_asset_selectors.contains(&rule.selector));
    runtime_sheet
        .tokens
        .retain(|name, _| !previous_asset_token_names.contains(name));
    runtime_sheet.rules.extend(loaded_stylesheet.rules);
    runtime_sheet.tokens.extend(loaded_stylesheet.tokens);

    world.resource_mut::<ActiveStyleSheetSelectors>().0 = incoming_selectors;
    world.resource_mut::<ActiveStyleSheetTokenNames>().0 = incoming_token_names;
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResolvedLayoutStyle {
    pub padding: f64,
    pub gap: f64,
    pub corner_radius: f64,
    pub border_width: f64,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub scale: f64,
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
    pub text_align: TextAlign,
}

impl Default for ResolvedTextStyle {
    fn default() -> Self {
        Self {
            size: theme::TEXT_SIZE_NORMAL,
            text_align: TextAlign::Start,
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

fn merge_layout_values(dst: &mut LayoutStyleValue, src: &LayoutStyleValue) {
    if src.padding.is_some() {
        dst.padding = src.padding.clone();
    }
    if src.gap.is_some() {
        dst.gap = src.gap.clone();
    }
    if src.corner_radius.is_some() {
        dst.corner_radius = src.corner_radius.clone();
    }
    if src.border_width.is_some() {
        dst.border_width = src.border_width.clone();
    }
    if src.justify_content.is_some() {
        dst.justify_content = src.justify_content.clone();
    }
    if src.align_items.is_some() {
        dst.align_items = src.align_items.clone();
    }
    if src.scale.is_some() {
        dst.scale = src.scale.clone();
    }
}

fn merge_colors_values(dst: &mut ColorStyleValue, src: &ColorStyleValue) {
    if src.bg.is_some() {
        dst.bg = src.bg.clone();
    }
    if src.text.is_some() {
        dst.text = src.text.clone();
    }
    if src.border.is_some() {
        dst.border = src.border.clone();
    }
    if src.hover_bg.is_some() {
        dst.hover_bg = src.hover_bg.clone();
    }
    if src.hover_text.is_some() {
        dst.hover_text = src.hover_text.clone();
    }
    if src.hover_border.is_some() {
        dst.hover_border = src.hover_border.clone();
    }
    if src.pressed_bg.is_some() {
        dst.pressed_bg = src.pressed_bg.clone();
    }
    if src.pressed_text.is_some() {
        dst.pressed_text = src.pressed_text.clone();
    }
    if src.pressed_border.is_some() {
        dst.pressed_border = src.pressed_border.clone();
    }
}

fn merge_text_values(dst: &mut TextStyleValue, src: &TextStyleValue) {
    if src.size.is_some() {
        dst.size = src.size.clone();
    }
    if src.text_align.is_some() {
        dst.text_align = src.text_align.clone();
    }
}

fn merge_value_setter(dst: &mut StyleSetterValue, setter: &StyleSetterValue) {
    merge_layout_values(&mut dst.layout, &setter.layout);
    merge_colors_values(&mut dst.colors, &setter.colors);
    merge_text_values(&mut dst.text, &setter.text);
    if setter.font_family.is_some() {
        dst.font_family = setter.font_family.clone();
    }
    if setter.box_shadow.is_some() {
        dst.box_shadow = setter.box_shadow.clone();
    }
    if setter.transition.is_some() {
        dst.transition = setter.transition.clone();
    }
}

fn merge_inline_layout_values(dst: &mut LayoutStyleValue, src: &LayoutStyle) {
    if let Some(padding) = src.padding {
        dst.padding = Some(StyleValue::value(padding));
    }
    if let Some(gap) = src.gap {
        dst.gap = Some(StyleValue::value(gap));
    }
    if let Some(corner_radius) = src.corner_radius {
        dst.corner_radius = Some(StyleValue::value(corner_radius));
    }
    if let Some(border_width) = src.border_width {
        dst.border_width = Some(StyleValue::value(border_width));
    }
    if let Some(justify_content) = src.justify_content {
        dst.justify_content = Some(StyleValue::value(justify_content));
    }
    if let Some(align_items) = src.align_items {
        dst.align_items = Some(StyleValue::value(align_items));
    }
    if let Some(scale) = src.scale {
        dst.scale = Some(StyleValue::value(scale));
    }
}

fn merge_inline_color_values(dst: &mut ColorStyleValue, src: &ColorStyle) {
    if let Some(bg) = src.bg {
        dst.bg = Some(StyleValue::value(bg));
    }
    if let Some(text) = src.text {
        dst.text = Some(StyleValue::value(text));
    }
    if let Some(border) = src.border {
        dst.border = Some(StyleValue::value(border));
    }
    if let Some(hover_bg) = src.hover_bg {
        dst.hover_bg = Some(StyleValue::value(hover_bg));
    }
    if let Some(hover_text) = src.hover_text {
        dst.hover_text = Some(StyleValue::value(hover_text));
    }
    if let Some(hover_border) = src.hover_border {
        dst.hover_border = Some(StyleValue::value(hover_border));
    }
    if let Some(pressed_bg) = src.pressed_bg {
        dst.pressed_bg = Some(StyleValue::value(pressed_bg));
    }
    if let Some(pressed_text) = src.pressed_text {
        dst.pressed_text = Some(StyleValue::value(pressed_text));
    }
    if let Some(pressed_border) = src.pressed_border {
        dst.pressed_border = Some(StyleValue::value(pressed_border));
    }
}

fn merge_inline_text_values(dst: &mut TextStyleValue, src: &TextStyle) {
    if let Some(size) = src.size {
        dst.size = Some(StyleValue::value(size));
    }
    if let Some(text_align) = src.text_align {
        dst.text_align = Some(StyleValue::value(text_align));
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
        Selector::PseudoClass(PseudoClass::Hovered) => world
            .get::<InteractionState>(entity)
            .is_some_and(|state| state.hovered),
        Selector::PseudoClass(PseudoClass::Pressed) => world
            .get::<InteractionState>(entity)
            .is_some_and(|state| state.pressed),
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
        Selector::PseudoClass(PseudoClass::Hovered) => entity
            .and_then(|entity| world.get::<InteractionState>(entity))
            .is_some_and(|state| state.hovered),
        Selector::PseudoClass(PseudoClass::Pressed) => entity
            .and_then(|entity| world.get::<InteractionState>(entity))
            .is_some_and(|state| state.pressed),
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
) -> StyleSetterValue {
    let mut merged = StyleSetterValue::default();
    let Some(sheet) = world.get_resource::<StyleSheet>() else {
        return merged;
    };

    let class_set = class_names.into_iter().collect::<HashSet<_>>();
    let has_class = |class_name: &str| class_set.contains(class_name);

    for rule in &sheet.rules {
        if selector_matches_class_context(world, entity, &rule.selector, &has_class) {
            merge_value_setter(&mut merged, &rule.setter);
        }
    }

    merged
}

fn merged_for_entity(world: &World, entity: Entity) -> (StyleSetterValue, bool) {
    let mut merged = StyleSetterValue::default();
    let mut matched_rule = false;

    if let Some(sheet) = world.get_resource::<StyleSheet>() {
        for rule in &sheet.rules {
            if selector_matches_entity(world, entity, &rule.selector) {
                merge_value_setter(&mut merged, &rule.setter);
                matched_rule = true;
            }
        }
    }

    if let Some(layout) = world.get::<LayoutStyle>(entity) {
        merge_inline_layout_values(&mut merged.layout, layout);
    }
    if let Some(colors) = world.get::<ColorStyle>(entity) {
        merge_inline_color_values(&mut merged.colors, colors);
    }
    if let Some(text) = world.get::<TextStyle>(entity) {
        merge_inline_text_values(&mut merged.text, text);
    }
    if let Some(transition) = world.get::<StyleTransition>(entity) {
        merged.transition = Some(StyleValue::value(*transition));
    }

    // Consolidated inline overrides (preferred).
    if let Some(inline) = world.get::<InlineStyle>(entity) {
        merge_inline_layout_values(&mut merged.layout, &inline.layout);
        merge_inline_color_values(&mut merged.colors, &inline.colors);
        merge_inline_text_values(&mut merged.text, &inline.text);
        if let Some(transition) = inline.transition {
            merged.transition = Some(StyleValue::value(transition));
        }
    }

    (merged, matched_rule)
}

fn target_colors(world: &World, entity: Entity, colors: &ColorStyle) -> ResolvedColorStyle {
    let (hovered, pressed) = world
        .get::<InteractionState>(entity)
        .map(|state| (state.hovered, state.pressed))
        .unwrap_or((false, false));

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
        justify_content: layout.justify_content.unwrap_or_default(),
        align_items: layout.align_items.unwrap_or_default(),
        scale: layout.scale.unwrap_or(1.0),
    }
}

fn to_resolved_text(text: &TextStyle) -> ResolvedTextStyle {
    ResolvedTextStyle {
        size: text.size.unwrap_or(theme::TEXT_SIZE_NORMAL),
        text_align: text.text_align.unwrap_or_default(),
    }
}

fn warn_missing_or_invalid_token(token: &str, field: &str, expected: &str) {
    tracing::warn!(
        token,
        field,
        expected,
        "style token missing or has incompatible type; applying fallback"
    );
}

fn resolve_f64_value(
    tokens: &HashMap<String, TokenValue>,
    value: &StyleValue<f64>,
    field: &str,
) -> f64 {
    match value {
        StyleValue::Value(value) => *value,
        StyleValue::Var(token) => match tokens.get(token) {
            Some(TokenValue::Float(value)) => *value,
            _ => {
                warn_missing_or_invalid_token(token, field, "Float");
                0.0
            }
        },
    }
}

fn resolve_f32_value(
    tokens: &HashMap<String, TokenValue>,
    value: &StyleValue<f32>,
    field: &str,
) -> f32 {
    match value {
        StyleValue::Value(value) => *value,
        StyleValue::Var(token) => match tokens.get(token) {
            Some(TokenValue::Float(value)) => *value as f32,
            _ => {
                warn_missing_or_invalid_token(token, field, "Float");
                0.0
            }
        },
    }
}

fn resolve_color_value(
    tokens: &HashMap<String, TokenValue>,
    value: &StyleValue<Color>,
    field: &str,
) -> Color {
    match value {
        StyleValue::Value(value) => *value,
        StyleValue::Var(token) => match tokens.get(token) {
            Some(TokenValue::Color(value)) => *value,
            _ => {
                warn_missing_or_invalid_token(token, field, "Color");
                Color::TRANSPARENT
            }
        },
    }
}

fn resolve_font_family_value(
    tokens: &HashMap<String, TokenValue>,
    value: &StyleValue<Vec<String>>,
    field: &str,
) -> Option<Vec<String>> {
    match value {
        StyleValue::Value(value) => Some(value.clone()),
        StyleValue::Var(token) => match tokens.get(token) {
            Some(TokenValue::FontFamily(value)) => Some(value.clone()),
            _ => {
                warn_missing_or_invalid_token(token, field, "FontFamily");
                None
            }
        },
    }
}

fn resolve_box_shadow_value(
    tokens: &HashMap<String, TokenValue>,
    value: &StyleValue<BoxShadow>,
    field: &str,
) -> BoxShadow {
    match value {
        StyleValue::Value(value) => *value,
        StyleValue::Var(token) => match tokens.get(token) {
            Some(TokenValue::BoxShadow(value)) => *value,
            _ => {
                warn_missing_or_invalid_token(token, field, "BoxShadow");
                BoxShadow::default()
            }
        },
    }
}

fn resolve_transition_value(
    tokens: &HashMap<String, TokenValue>,
    value: &StyleValue<StyleTransition>,
    field: &str,
) -> StyleTransition {
    match value {
        StyleValue::Value(value) => *value,
        StyleValue::Var(token) => match tokens.get(token) {
            Some(TokenValue::Transition(value)) => *value,
            Some(TokenValue::Float(value)) => StyleTransition {
                duration: *value as f32,
            },
            _ => {
                warn_missing_or_invalid_token(token, field, "Transition|Float");
                StyleTransition { duration: 0.0 }
            }
        },
    }
}

fn resolve_enum_value<T: Copy + Default>(
    _tokens: &HashMap<String, TokenValue>,
    value: &StyleValue<T>,
    _field: &str,
) -> T {
    match value {
        StyleValue::Value(value) => *value,
        StyleValue::Var(_token) => {
            tracing::warn!(
                field = _field,
                "style enum values currently only support literal values; token reference ignored"
            );
            T::default()
        }
    }
}

fn resolve_layout_style(
    layout: &LayoutStyleValue,
    tokens: &HashMap<String, TokenValue>,
) -> LayoutStyle {
    LayoutStyle {
        padding: layout
            .padding
            .as_ref()
            .map(|value| resolve_f64_value(tokens, value, "layout.padding")),
        gap: layout
            .gap
            .as_ref()
            .map(|value| resolve_f64_value(tokens, value, "layout.gap")),
        corner_radius: layout
            .corner_radius
            .as_ref()
            .map(|value| resolve_f64_value(tokens, value, "layout.corner_radius")),
        border_width: layout
            .border_width
            .as_ref()
            .map(|value| resolve_f64_value(tokens, value, "layout.border_width")),
        justify_content: layout
            .justify_content
            .as_ref()
            .map(|value| resolve_enum_value(tokens, value, "layout.justify_content")),
        align_items: layout
            .align_items
            .as_ref()
            .map(|value| resolve_enum_value(tokens, value, "layout.align_items")),
        scale: layout
            .scale
            .as_ref()
            .map(|value| resolve_f64_value(tokens, value, "layout.scale")),
    }
}

fn resolve_color_style(
    colors: &ColorStyleValue,
    tokens: &HashMap<String, TokenValue>,
) -> ColorStyle {
    ColorStyle {
        bg: colors
            .bg
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.bg")),
        text: colors
            .text
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.text")),
        border: colors
            .border
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.border")),
        hover_bg: colors
            .hover_bg
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.hover_bg")),
        hover_text: colors
            .hover_text
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.hover_text")),
        hover_border: colors
            .hover_border
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.hover_border")),
        pressed_bg: colors
            .pressed_bg
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.pressed_bg")),
        pressed_text: colors
            .pressed_text
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.pressed_text")),
        pressed_border: colors
            .pressed_border
            .as_ref()
            .map(|value| resolve_color_value(tokens, value, "colors.pressed_border")),
    }
}

fn resolve_text_style(text: &TextStyleValue, tokens: &HashMap<String, TokenValue>) -> TextStyle {
    TextStyle {
        size: text
            .size
            .as_ref()
            .map(|value| resolve_f32_value(tokens, value, "text.size")),
        text_align: text
            .text_align
            .as_ref()
            .map(|value| resolve_enum_value(tokens, value, "text.text_align")),
    }
}

fn resolve_setter_values(
    setter: &StyleSetterValue,
    tokens: &HashMap<String, TokenValue>,
) -> StyleSetter {
    StyleSetter {
        layout: resolve_layout_style(&setter.layout, tokens),
        colors: resolve_color_style(&setter.colors, tokens),
        text: resolve_text_style(&setter.text, tokens),
        font_family: setter
            .font_family
            .as_ref()
            .and_then(|value| resolve_font_family_value(tokens, value, "font_family")),
        box_shadow: setter
            .box_shadow
            .as_ref()
            .map(|value| resolve_box_shadow_value(tokens, value, "box_shadow")),
        transition: setter
            .transition
            .as_ref()
            .map(|value| resolve_transition_value(tokens, value, "transition")),
    }
}

fn has_any_style_source(world: &World, entity: Entity, matched_rule: bool) -> bool {
    matched_rule
        || world.get::<StyleClass>(entity).is_some()
        || world.get::<InlineStyle>(entity).is_some()
        || world.get::<LayoutStyle>(entity).is_some()
        || world.get::<ColorStyle>(entity).is_some()
        || world.get::<TextStyle>(entity).is_some()
        || world.get::<StyleTransition>(entity).is_some()
}

fn resolved_from_merged(
    world: &World,
    entity: Entity,
    merged: &StyleSetterValue,
    tokens: &HashMap<String, TokenValue>,
    include_current_override: bool,
) -> ResolvedStyle {
    let merged = resolve_setter_values(merged, tokens);
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

    let mut layout = to_resolved_layout(&merged.layout);
    if include_current_override && let Some(current) = world.get::<CurrentColorStyle>(entity) {
        layout.scale = current.scale;
    }

    ResolvedStyle {
        layout,
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

    let empty_tokens = HashMap::new();
    let tokens = world
        .get_resource::<StyleSheet>()
        .map(|sheet| &sheet.tokens)
        .unwrap_or(&empty_tokens);

    Some(resolved_from_merged(world, entity, &merged, tokens, false))
}

/// Resolve final style for an entity.
///
/// Cascading order:
/// 1. class styles from [`StyleSheet`] and [`StyleClass`]
/// 2. inline overrides from [`InlineStyle`] (or legacy inline components)
/// 3. pseudo classes from [`InteractionState`]
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
            style.layout.scale = current.scale;
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
    let empty_tokens = HashMap::new();
    let tokens = world
        .get_resource::<StyleSheet>()
        .map(|sheet| &sheet.tokens)
        .unwrap_or(&empty_tokens);
    let merged = resolve_setter_values(&merged, tokens);

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
/// This is useful when a UI component's visual style is class-driven, but hover/pressed
/// state is tracked on an ECS entity via [`InteractionState`].
#[must_use]
pub fn resolve_style_for_entity_classes<'a>(
    world: &World,
    entity: Entity,
    class_names: impl IntoIterator<Item = &'a str>,
) -> ResolvedStyle {
    let merged = merged_from_class_names(world, Some(entity), class_names);
    let empty_tokens = HashMap::new();
    let tokens = world
        .get_resource::<StyleSheet>()
        .map(|sheet| &sheet.tokens)
        .unwrap_or(&empty_tokens);
    resolved_from_merged(world, entity, &merged, tokens, false)
}

/// Map style-level justify-content to Masonry flex main-axis alignment.
#[must_use]
pub fn map_main_axis_alignment(justify_content: JustifyContent) -> MainAxisAlignment {
    match justify_content {
        JustifyContent::Start => MainAxisAlignment::Start,
        JustifyContent::Center => MainAxisAlignment::Center,
        JustifyContent::End => MainAxisAlignment::End,
        JustifyContent::SpaceBetween => MainAxisAlignment::SpaceBetween,
    }
}

/// Map style-level align-items to Masonry flex cross-axis alignment.
#[must_use]
pub fn map_cross_axis_alignment(align_items: AlignItems) -> CrossAxisAlignment {
    match align_items {
        AlignItems::Start => CrossAxisAlignment::Start,
        AlignItems::Center => CrossAxisAlignment::Center,
        AlignItems::End => CrossAxisAlignment::End,
        AlignItems::Stretch => CrossAxisAlignment::Stretch,
    }
}

fn map_text_alignment(text_align: TextAlign) -> ParleyTextAlign {
    match text_align {
        TextAlign::Start => ParleyTextAlign::Start,
        TextAlign::Center => ParleyTextAlign::Center,
        TextAlign::End => ParleyTextAlign::End,
    }
}

pub trait StyleFlexAlignmentExt {
    fn with_style_alignment(self, style: &ResolvedStyle) -> Self;
}

impl<Seq, State, Action> StyleFlexAlignmentExt for Flex<Seq, State, Action> {
    fn with_style_alignment(self, style: &ResolvedStyle) -> Self {
        self.main_axis_alignment(map_main_axis_alignment(style.layout.justify_content))
            .cross_axis_alignment(map_cross_axis_alignment(style.layout.align_items))
    }
}

/// Apply style-derived flex axis alignment on a flex view.
pub fn apply_flex_alignment<V>(view: V, style: &ResolvedStyle) -> V
where
    V: StyleFlexAlignmentExt,
{
    view.with_style_alignment(style)
}

/// Apply box/layout styling on any widget view.
pub fn apply_widget_style<V>(view: V, style: &ResolvedStyle) -> impl WidgetView<(), ()>
where
    V: WidgetView<(), ()>,
{
    let scale = style.layout.scale.max(0.01);
    transformed(
        sized_box(view)
            .padding(style.layout.padding)
            .corner_radius(style.layout.corner_radius)
            .border(
                style.colors.border.unwrap_or(Color::TRANSPARENT),
                style.layout.border_width,
            )
            .background_color(style.colors.bg.unwrap_or(Color::TRANSPARENT))
            .box_shadow(style.box_shadow.unwrap_or_default()),
    )
    .scale(scale)
}

/// Apply style directly on the target widget.
///
/// This should be preferred for interactive UI components to ensure visual bounds
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
    let scale = style.layout.scale.max(0.01);
    transformed(
        view.padding(style.layout.padding)
            .corner_radius(style.layout.corner_radius)
            .border(
                style.colors.border.unwrap_or(Color::TRANSPARENT),
                style.layout.border_width,
            )
            .background_color(style.colors.bg.unwrap_or(Color::TRANSPARENT))
            .box_shadow(style.box_shadow.unwrap_or_default()),
    )
    .scale(scale)
}

fn to_target_component(style: &ResolvedStyle) -> TargetColorStyle {
    TargetColorStyle {
        bg: style.colors.bg,
        text: style.colors.text,
        border: style.colors.border,
        scale: style.layout.scale,
    }
}

fn to_current_component(colors: TargetColorStyle) -> CurrentColorStyle {
    CurrentColorStyle {
        bg: colors.bg,
        text: colors.text,
        border: colors.border,
        scale: colors.scale,
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

/// Consume interaction events and synchronize [`InteractionState`].
pub fn sync_ui_interaction_markers(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiInteractionEvent>();

    for event in events {
        if world.get_entity(event.entity).is_err() {
            continue;
        }

        let before = world
            .get::<InteractionState>(event.entity)
            .copied()
            .unwrap_or_default();

        let mut after = before;
        match event.action {
            UiInteractionEvent::PointerEntered => after.hovered = true,
            UiInteractionEvent::PointerLeft => after.hovered = false,
            UiInteractionEvent::PointerPressed => after.pressed = true,
            UiInteractionEvent::PointerReleased => after.pressed = false,
        }

        if after != before {
            world.entity_mut(event.entity).insert(after);
            world.entity_mut(event.entity).insert(StyleDirty);
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
            Changed<InlineStyle>,
            Changed<LayoutStyle>,
            Changed<ColorStyle>,
            Changed<TextStyle>,
            Changed<StyleTransition>,
            Changed<InteractionState>,
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
                With<InlineStyle>,
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
                        && world.get::<InlineStyle>(*entity).is_none()
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

                let target = to_target_component(&resolved);
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
        target.layout.scale = lerp_f64(self.start.layout.scale, self.end.layout.scale, t);
        target.layout.justify_content = if t < 1.0 {
            self.start.layout.justify_content
        } else {
            self.end.layout.justify_content
        };
        target.layout.align_items = if t < 1.0 {
            self.start.layout.align_items
        } else {
            self.end.layout.align_items
        };

        target.colors.bg = lerp_optional_color(self.start.colors.bg, self.end.colors.bg, t);
        target.colors.text = lerp_optional_color(self.start.colors.text, self.end.colors.text, t);
        target.colors.border =
            lerp_optional_color(self.start.colors.border, self.end.colors.border, t);

        target.text.size = lerp_f32(self.start.text.size, self.end.text.size, t);
        target.text.text_align = if t < 1.0 {
            self.start.text.text_align
        } else {
            self.end.text.text_align
        };
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
        target.scale = lerp_f64(self.start.scale, self.end.scale, ratio);
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
    let mut styled = view
        .text_size(style.text.size)
        .text_alignment(map_text_alignment(style.text.text_align));
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
    let mut styled = view
        .text_size(style.text.size)
        .text_alignment(map_text_alignment(style.text.text_align));
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
    tokens: HashMap<String, TokenDef>,
    #[serde(default)]
    rules: Vec<StyleRuleDef>,
}

#[derive(Debug, Deserialize)]
struct StyleSheetVariantsDef {
    default_variant: String,
    #[serde(default)]
    variants: HashMap<String, StyleSheetDef>,
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
    font_family: OptionalStyleValueDef<Vec<String>>,
    #[serde(default)]
    box_shadow: OptionalStyleValueDef<BoxShadowDef>,
    #[serde(default)]
    transition: OptionalStyleValueDef<StyleTransition>,
}

#[derive(Debug, Clone)]
struct OptionalLiteralValueDef<T>(Option<T>);

impl<T> Default for OptionalLiteralValueDef<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<'de, T> Deserialize<'de> for OptionalLiteralValueDef<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(Some(T::deserialize(deserializer)?)))
    }
}

impl<T> OptionalLiteralValueDef<T> {
    fn into_option(self) -> Option<T> {
        self.0
    }
}

#[derive(Debug, Default, Deserialize)]
struct LayoutStyleDef {
    #[serde(default)]
    padding: OptionalStyleValueDef<f64>,
    #[serde(default)]
    gap: OptionalStyleValueDef<f64>,
    #[serde(default)]
    corner_radius: OptionalStyleValueDef<f64>,
    #[serde(default)]
    border_width: OptionalStyleValueDef<f64>,
    #[serde(default)]
    justify_content: OptionalLiteralValueDef<JustifyContent>,
    #[serde(default)]
    align_items: OptionalLiteralValueDef<AlignItems>,
    #[serde(default)]
    scale: OptionalStyleValueDef<f64>,
}

impl LayoutStyleDef {
    fn into_layout_values(self) -> io::Result<LayoutStyleValue> {
        Ok(LayoutStyleValue {
            padding: into_style_value(self.padding.into_option(), Ok)?,
            gap: into_style_value(self.gap.into_option(), Ok)?,
            corner_radius: into_style_value(self.corner_radius.into_option(), Ok)?,
            border_width: into_style_value(self.border_width.into_option(), Ok)?,
            justify_content: self.justify_content.into_option().map(StyleValue::Value),
            align_items: self.align_items.into_option().map(StyleValue::Value),
            scale: into_style_value(self.scale.into_option(), Ok)?,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct TextStyleDef {
    #[serde(default)]
    size: OptionalStyleValueDef<f32>,
    #[serde(default)]
    text_align: OptionalLiteralValueDef<TextAlign>,
}

impl TextStyleDef {
    fn into_text_values(self) -> io::Result<TextStyleValue> {
        Ok(TextStyleValue {
            size: into_style_value(self.size.into_option(), Ok)?,
            text_align: self.text_align.into_option().map(StyleValue::Value),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct ColorStyleDef {
    #[serde(default)]
    bg: OptionalStyleValueDef<ColorDef>,
    #[serde(default, rename = "text")]
    text_color: OptionalStyleValueDef<ColorDef>,
    #[serde(default)]
    border: OptionalStyleValueDef<ColorDef>,
    #[serde(default)]
    hover_bg: OptionalStyleValueDef<ColorDef>,
    #[serde(default)]
    hover_text: OptionalStyleValueDef<ColorDef>,
    #[serde(default)]
    hover_border: OptionalStyleValueDef<ColorDef>,
    #[serde(default)]
    pressed_bg: OptionalStyleValueDef<ColorDef>,
    #[serde(default)]
    pressed_text: OptionalStyleValueDef<ColorDef>,
    #[serde(default)]
    pressed_border: OptionalStyleValueDef<ColorDef>,
}

#[derive(Debug, Clone, Deserialize)]
enum ColorDef {
    Rgb(f32, f32, f32),
    Rgba(f32, f32, f32, f32),
    Rgb8(u8, u8, u8),
    Rgba8(u8, u8, u8, u8),
    Hex(String),
}

#[derive(Debug, Clone, Deserialize)]
enum TokenDef {
    Color(ColorDef),
    Float(f64),
    FontFamily(Vec<String>),
    BoxShadow(BoxShadowDef),
    Transition(StyleTransition),
}

impl TokenDef {
    fn into_token_value(self) -> io::Result<TokenValue> {
        match self {
            Self::Color(color) => Ok(TokenValue::Color(color.into_color()?)),
            Self::Float(value) => Ok(TokenValue::Float(value)),
            Self::FontFamily(value) => Ok(TokenValue::FontFamily(value)),
            Self::BoxShadow(value) => Ok(TokenValue::BoxShadow(value.into_box_shadow()?)),
            Self::Transition(value) => Ok(TokenValue::Transition(value)),
        }
    }
}

#[derive(Debug, Clone)]
enum StyleValueDef<T> {
    Value(T),
    Var(String),
}

impl<'de, T> Deserialize<'de> for StyleValueDef<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        fn literal_to_style_value<'de, T, U, E>(value: U) -> Result<StyleValueDef<T>, E>
        where
            T: Deserialize<'de>,
            U: IntoDeserializer<'de, de::value::Error>,
            E: de::Error,
        {
            T::deserialize(value.into_deserializer())
                .map(StyleValueDef::Value)
                .map_err(de::Error::custom)
        }

        struct StyleValueVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> Visitor<'de> for StyleValueVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = StyleValueDef<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a style literal value or Var(\"token-name\") reference")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_style_value::<T, _, E>(value)
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_style_value::<T, _, E>(value)
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_style_value::<T, _, E>(value)
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_style_value::<T, _, E>(value)
            }

            fn visit_char<E>(self, value: char) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_style_value::<T, _, E>(value)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_style_value::<T, _, E>(value)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_style_value::<T, _, E>(value)
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let de = SeqAccessDeserializer::new(seq);
                let values = Vec::<ron::Value>::deserialize(de).map_err(de::Error::custom)?;

                if let Ok(value) = T::deserialize(ron::Value::Seq(values.clone())) {
                    return Ok(StyleValueDef::Value(value));
                }

                if let [ron::Value::String(token)] = values.as_slice() {
                    return Ok(StyleValueDef::Var(token.clone()));
                }

                if let [ron::Value::String(tag), ron::Value::String(token)] = values.as_slice()
                    && tag == "Var"
                {
                    return Ok(StyleValueDef::Var(token.clone()));
                }

                Err(de::Error::custom(
                    "invalid style value sequence; expected literal sequence value or Var(\"token\")",
                ))
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let de = MapAccessDeserializer::new(map);
                let raw = ron::Value::deserialize(de).map_err(de::Error::custom)?;

                if let Ok(value) = T::deserialize(raw.clone()) {
                    return Ok(StyleValueDef::Value(value));
                }

                if let ron::Value::Map(entries) = &raw
                    && entries.len() == 1
                    && let Some((ron::Value::String(tag), value)) = entries.iter().next()
                    && tag == "Var"
                    && let ron::Value::String(token) = value
                {
                    return Ok(StyleValueDef::Var(token.clone()));
                }

                Err(de::Error::custom(
                    "invalid style value map; expected literal map value or Var(\"token\")",
                ))
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: EnumAccess<'de>,
            {
                let (variant, variant_access) = data.variant::<String>()?;
                match variant.as_str() {
                    "Var" => Ok(StyleValueDef::Var(
                        variant_access.newtype_variant::<String>()?,
                    )),
                    "Value" => Ok(StyleValueDef::Value(variant_access.newtype_variant::<T>()?)),
                    _ => Err(de::Error::unknown_variant(&variant, &["Var", "Value"])),
                }
            }
        }

        deserializer.deserialize_any(StyleValueVisitor(std::marker::PhantomData))
    }
}

#[derive(Debug, Clone)]
enum OptionalStyleValueDef<T> {
    Style(StyleValueDef<T>),
    None,
}

impl<T> Default for OptionalStyleValueDef<T> {
    fn default() -> Self {
        Self::None
    }
}

impl<'de, T> Deserialize<'de> for OptionalStyleValueDef<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        fn literal_to_optional_style<'de, T, U, E>(value: U) -> Result<OptionalStyleValueDef<T>, E>
        where
            T: Deserialize<'de>,
            U: IntoDeserializer<'de, de::value::Error>,
            E: de::Error,
        {
            T::deserialize(value.into_deserializer())
                .map(StyleValueDef::Value)
                .map(OptionalStyleValueDef::Style)
                .map_err(de::Error::custom)
        }

        struct OptionalStyleValueVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> Visitor<'de> for OptionalStyleValueVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = OptionalStyleValueDef<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an optional style value literal, Var(\"token-name\"), or None")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(OptionalStyleValueDef::None)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(OptionalStyleValueDef::None)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                StyleValueDef::<T>::deserialize(deserializer).map(OptionalStyleValueDef::Style)
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_optional_style::<T, _, E>(value)
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_optional_style::<T, _, E>(value)
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_optional_style::<T, _, E>(value)
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_optional_style::<T, _, E>(value)
            }

            fn visit_char<E>(self, value: char) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_optional_style::<T, _, E>(value)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_optional_style::<T, _, E>(value)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                literal_to_optional_style::<T, _, E>(value)
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let de = SeqAccessDeserializer::new(seq);
                StyleValueDef::<T>::deserialize(de)
                    .map(OptionalStyleValueDef::Style)
                    .map_err(de::Error::custom)
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let de = MapAccessDeserializer::new(map);
                StyleValueDef::<T>::deserialize(de)
                    .map(OptionalStyleValueDef::Style)
                    .map_err(de::Error::custom)
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: EnumAccess<'de>,
            {
                let (variant, variant_access) = data.variant::<String>()?;
                match variant.as_str() {
                    "None" => {
                        variant_access.unit_variant()?;
                        Ok(OptionalStyleValueDef::None)
                    }
                    "Some" => Ok(OptionalStyleValueDef::Style(
                        variant_access.newtype_variant::<StyleValueDef<T>>()?,
                    )),
                    "Var" => Ok(OptionalStyleValueDef::Style(StyleValueDef::Var(
                        variant_access.newtype_variant::<String>()?,
                    ))),
                    "Value" => Ok(OptionalStyleValueDef::Style(StyleValueDef::Value(
                        variant_access.newtype_variant::<T>()?,
                    ))),
                    _ => Err(de::Error::unknown_variant(
                        &variant,
                        &["None", "Some", "Var", "Value"],
                    )),
                }
            }
        }

        deserializer.deserialize_any(OptionalStyleValueVisitor(std::marker::PhantomData))
    }
}

impl<T> OptionalStyleValueDef<T> {
    fn into_option(self) -> Option<StyleValueDef<T>> {
        match self {
            Self::Style(value) => Some(value),
            Self::None => None,
        }
    }
}

fn into_style_value<T, U>(
    value: Option<StyleValueDef<T>>,
    map: impl FnOnce(T) -> io::Result<U>,
) -> io::Result<Option<StyleValue<U>>> {
    match value {
        None => Ok(None),
        Some(StyleValueDef::Value(value)) => Ok(Some(StyleValue::Value(map(value)?))),
        Some(StyleValueDef::Var(name)) => Ok(Some(StyleValue::Var(name))),
    }
}

impl ColorDef {
    fn into_color(self) -> io::Result<Color> {
        match self {
            Self::Rgb(r, g, b) => Ok(Color::from_rgb8(
                float_color_component_to_u8(r),
                float_color_component_to_u8(g),
                float_color_component_to_u8(b),
            )),
            Self::Rgba(r, g, b, a) => Ok(Color::from_rgba8(
                float_color_component_to_u8(r),
                float_color_component_to_u8(g),
                float_color_component_to_u8(b),
                float_color_component_to_u8(a),
            )),
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
    fn into_setter(self) -> io::Result<StyleSetterValue> {
        Ok(StyleSetterValue {
            layout: self.layout.into_layout_values()?,
            colors: self.colors.into_color_style_values()?,
            text: self.text.into_text_values()?,
            font_family: into_style_value(self.font_family.into_option(), Ok)?,
            box_shadow: into_style_value(
                self.box_shadow.into_option(),
                BoxShadowDef::into_box_shadow,
            )?,
            transition: into_style_value(self.transition.into_option(), Ok)?,
        })
    }
}

impl ColorStyleDef {
    fn into_color_style_value(
        value: Option<StyleValueDef<ColorDef>>,
    ) -> io::Result<Option<StyleValue<Color>>> {
        match value {
            None => Ok(None),
            Some(StyleValueDef::Value(value)) => Ok(Some(StyleValue::Value(value.into_color()?))),
            Some(StyleValueDef::Var(name)) => {
                // `Hex("#RRGGBB[AA]")` may be deserialized as a bare string payload on some
                // enum paths, which can be misclassified as `Var("#...")`. Recover by
                // recognizing hex literals here and treating them as literal colors.
                let trimmed = name.trim();
                if trimmed.starts_with('#')
                    && let Ok(color) = parse_hex_color(trimmed)
                {
                    return Ok(Some(StyleValue::Value(color)));
                }

                Ok(Some(StyleValue::Var(name)))
            }
        }
    }

    fn into_color_style_values(self) -> io::Result<ColorStyleValue> {
        Ok(ColorStyleValue {
            bg: Self::into_color_style_value(self.bg.into_option())?,
            text: Self::into_color_style_value(self.text_color.into_option())?,
            border: Self::into_color_style_value(self.border.into_option())?,
            hover_bg: Self::into_color_style_value(self.hover_bg.into_option())?,
            hover_text: Self::into_color_style_value(self.hover_text.into_option())?,
            hover_border: Self::into_color_style_value(self.hover_border.into_option())?,
            pressed_bg: Self::into_color_style_value(self.pressed_bg.into_option())?,
            pressed_text: Self::into_color_style_value(self.pressed_text.into_option())?,
            pressed_border: Self::into_color_style_value(self.pressed_border.into_option())?,
        })
    }
}

fn float_color_component_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
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

fn stylesheet_from_def(parsed: StyleSheetDef) -> io::Result<StyleSheet> {
    let mut sheet = StyleSheet::default();
    for (name, token) in parsed.tokens {
        sheet.tokens.insert(name, token.into_token_value()?);
    }

    for rule in parsed.rules {
        sheet.add_rule(StyleRule::new_with_values(
            rule.selector.into(),
            rule.setter.into_setter()?,
        ));
    }

    Ok(sheet)
}

fn stylesheet_from_ron_bytes(bytes: &[u8]) -> io::Result<StyleSheet> {
    let parsed: StyleSheetDef = ron::de::from_bytes(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse stylesheet RON: {error}"),
        )
    })?;

    stylesheet_from_def(parsed)
}

fn stylesheet_variants_from_ron_bytes(bytes: &[u8]) -> io::Result<RegisteredStyleVariants> {
    let parsed: StyleSheetVariantsDef = ron::de::from_bytes(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse stylesheet variants RON: {error}"),
        )
    })?;

    if parsed.variants.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stylesheet variants RON must define at least one variant",
        ));
    }

    let default_variant = parsed.default_variant;
    let mut raw_variants = HashMap::new();
    for (name, def) in parsed.variants {
        raw_variants.insert(name, stylesheet_from_def(def)?);
    }

    let default_sheet = raw_variants.get(&default_variant).cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("stylesheet variants RON default_variant `{default_variant}` is not defined"),
        )
    })?;

    let mut variants = HashMap::new();
    for (name, variant_sheet) in raw_variants {
        if name == default_variant {
            variants.insert(name, default_sheet.clone());
            continue;
        }

        let mut merged = default_sheet.clone();
        merge_sheet_inplace(&mut merged, variant_sheet);
        variants.insert(name, merged);
    }

    Ok(RegisteredStyleVariants {
        default_variant,
        variants,
    })
}

#[cfg(test)]
pub(crate) fn parse_stylesheet_ron_for_tests(ron_text: &str) -> io::Result<StyleSheet> {
    parse_stylesheet_ron(ron_text)
}

#[cfg(test)]
pub(crate) fn parse_stylesheet_variants_ron_for_tests(
    ron_text: &str,
) -> io::Result<RegisteredStyleVariants> {
    parse_stylesheet_variants_ron(ron_text)
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
