use std::collections::HashMap;

use bevy_ecs::{entity::Entity, prelude::*};
use bevy_time::Time;
use masonry::theme;
use xilem::{Color, style::Style as _};
use xilem_masonry::{
    WidgetView,
    view::{Label, TextInput, sized_box},
};

use crate::UiEventQueue;

/// Marker component for CSS-like class names attached to an entity.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleClass(pub Vec<String>);

/// Inline layout style that can be attached to entities.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
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
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
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
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct StyleTransition {
    /// Duration in seconds.
    pub duration: f32,
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

/// A named style rule used by [`StyleSheet`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StyleRule {
    pub layout: LayoutStyle,
    pub colors: ColorStyle,
    pub text: TextStyle,
    pub transition: Option<StyleTransition>,
}

/// Global class-based style table.
#[derive(Resource, Debug, Clone, Default)]
pub struct StyleSheet {
    pub classes: HashMap<String, StyleRule>,
}

impl StyleSheet {
    #[must_use]
    pub fn with_class(mut self, class_name: impl Into<String>, rule: StyleRule) -> Self {
        self.classes.insert(class_name.into(), rule);
        self
    }

    pub fn set_class(&mut self, class_name: impl Into<String>, rule: StyleRule) {
        self.classes.insert(class_name.into(), rule);
    }

    #[must_use]
    pub fn get_class(&self, class_name: &str) -> Option<&StyleRule> {
        self.classes.get(class_name)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct CascadedStyle {
    layout: LayoutStyle,
    colors: ColorStyle,
    text: TextStyle,
    transition: Option<StyleTransition>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResolvedStyle {
    pub layout: ResolvedLayoutStyle,
    pub colors: ResolvedColorStyle,
    pub text: ResolvedTextStyle,
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

fn merge_rule(dst: &mut CascadedStyle, rule: &StyleRule) {
    merge_layout(&mut dst.layout, &rule.layout);
    merge_colors(&mut dst.colors, &rule.colors);
    merge_text(&mut dst.text, &rule.text);
    if rule.transition.is_some() {
        dst.transition = rule.transition;
    }
}

fn merged_from_class_names<'a>(
    world: &World,
    class_names: impl IntoIterator<Item = &'a str>,
) -> CascadedStyle {
    let mut cascaded = CascadedStyle::default();
    let Some(sheet) = world.get_resource::<StyleSheet>() else {
        return cascaded;
    };

    for class_name in class_names {
        if let Some(rule) = sheet.get_class(class_name) {
            merge_rule(&mut cascaded, rule);
        }
    }

    cascaded
}

fn merged_for_entity(world: &World, entity: Entity) -> CascadedStyle {
    let mut cascaded = CascadedStyle::default();

    if let Some(class_component) = world.get::<StyleClass>(entity) {
        let class_cascaded =
            merged_from_class_names(world, class_component.0.iter().map(String::as_str));
        cascaded = class_cascaded;
    }

    if let Some(layout) = world.get::<LayoutStyle>(entity) {
        merge_layout(&mut cascaded.layout, layout);
    }
    if let Some(colors) = world.get::<ColorStyle>(entity) {
        merge_colors(&mut cascaded.colors, colors);
    }
    if let Some(text) = world.get::<TextStyle>(entity) {
        merge_text(&mut cascaded.text, text);
    }
    if let Some(transition) = world.get::<StyleTransition>(entity) {
        cascaded.transition = Some(*transition);
    }

    cascaded
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

/// Resolve final style for an entity.
///
/// Cascading order:
/// 1. class styles from [`StyleSheet`] and [`StyleClass`]
/// 2. inline components [`LayoutStyle`], [`ColorStyle`], [`TextStyle`]
/// 3. pseudo classes [`Hovered`] / [`Pressed`]
/// 4. animated override from [`CurrentColorStyle`] when present
#[must_use]
pub fn resolve_style(world: &World, entity: Entity) -> ResolvedStyle {
    let cascaded = merged_for_entity(world, entity);
    let mut colors = target_colors(world, entity, &cascaded.colors);

    if let Some(current) = world.get::<CurrentColorStyle>(entity) {
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
        layout: to_resolved_layout(&cascaded.layout),
        colors,
        text: to_resolved_text(&cascaded.text),
        transition: cascaded.transition,
    }
}

/// Resolve style from class names only, without inline entity overrides.
#[must_use]
pub fn resolve_style_for_classes<'a>(
    world: &World,
    class_names: impl IntoIterator<Item = &'a str>,
) -> ResolvedStyle {
    let cascaded = merged_from_class_names(world, class_names);
    ResolvedStyle {
        layout: to_resolved_layout(&cascaded.layout),
        colors: ResolvedColorStyle {
            bg: cascaded.colors.bg,
            text: cascaded.colors.text,
            border: cascaded.colors.border,
        },
        text: to_resolved_text(&cascaded.text),
        transition: cascaded.transition,
    }
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
}

/// Apply text + box styling to a label view.
pub fn apply_label_style(view: Label, style: &ResolvedStyle) -> impl WidgetView<(), ()> {
    view.text_size(style.text.size)
        .color(style.colors.text.unwrap_or(Color::WHITE))
}

/// Apply text + box styling to a text input view.
pub fn apply_text_input_style(
    view: TextInput<(), ()>,
    style: &ResolvedStyle,
) -> impl WidgetView<(), ()> {
    let mut styled = view.text_size(style.text.size);
    if let Some(text_color) = style.colors.text {
        styled = styled.text_color(text_color);
    }
    styled
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
                world.entity_mut(event.entity).insert(Hovered);
            }
            UiInteractionEvent::PointerLeft => {
                world.entity_mut(event.entity).remove::<Hovered>();
                world.entity_mut(event.entity).remove::<Pressed>();
            }
            UiInteractionEvent::PointerPressed => {
                world.entity_mut(event.entity).insert(Pressed);
            }
            UiInteractionEvent::PointerReleased => {
                world.entity_mut(event.entity).remove::<Pressed>();
            }
        }
    }
}

/// Compute and store target/current style states used by transition animation.
pub fn sync_style_targets(world: &mut World) {
    let entities = {
        let mut query = world.query_filtered::<Entity, Or<(
            With<StyleClass>,
            With<LayoutStyle>,
            With<ColorStyle>,
            With<TextStyle>,
            With<StyleTransition>,
            With<Hovered>,
            With<Pressed>,
        )>>();
        query.iter(world).collect::<Vec<_>>()
    };

    let snapshots = {
        let world_ref: &World = world;
        entities
            .into_iter()
            .map(|entity| {
                let cascaded = merged_for_entity(world_ref, entity);
                let target = target_colors(world_ref, entity, &cascaded.colors);
                (entity, cascaded.transition, target)
            })
            .collect::<Vec<_>>()
    };

    for (entity, transition, target) in snapshots {
        match transition {
            Some(transition) => {
                world.entity_mut(entity).insert(transition);

                if let Some(mut target_component) = world.get_mut::<TargetColorStyle>(entity) {
                    *target_component = TargetColorStyle {
                        bg: target.bg,
                        text: target.text,
                        border: target.border,
                    };
                } else {
                    world.entity_mut(entity).insert(TargetColorStyle {
                        bg: target.bg,
                        text: target.text,
                        border: target.border,
                    });
                }

                if world.get::<CurrentColorStyle>(entity).is_none() {
                    world.entity_mut(entity).insert(CurrentColorStyle {
                        bg: target.bg,
                        text: target.text,
                        border: target.border,
                    });
                }
            }
            None => {
                world.entity_mut(entity).remove::<TargetColorStyle>();
                world.entity_mut(entity).remove::<CurrentColorStyle>();
            }
        }
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

fn lerp_optional_color(current: Option<Color>, target: Option<Color>, t: f32) -> Option<Color> {
    match (current, target) {
        (Some(current), Some(target)) => Some(lerp_color(current, target, t)),
        (None, Some(target)) => Some(target),
        (Some(current), None) => {
            if t >= 1.0 {
                None
            } else {
                Some(current)
            }
        }
        (None, None) => None,
    }
}

/// Lerp [`CurrentColorStyle`] towards [`TargetColorStyle`] each frame.
pub fn animate_style_transitions(world: &mut World) {
    let delta_secs = world.resource::<Time>().delta_secs();

    let mut query = world.query::<(&StyleTransition, &TargetColorStyle, &mut CurrentColorStyle)>();
    for (transition, target, mut current) in query.iter_mut(world) {
        let t = if transition.duration <= f32::EPSILON {
            1.0
        } else {
            (delta_secs / transition.duration).clamp(0.0, 1.0)
        };

        current.bg = lerp_optional_color(current.bg, target.bg, t);
        current.text = lerp_optional_color(current.text, target.text, t);
        current.border = lerp_optional_color(current.border, target.border, t);
    }
}
