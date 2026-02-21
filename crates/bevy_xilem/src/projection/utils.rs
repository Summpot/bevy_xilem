use crate::{ecs::LocalizeText, i18n::AppI18n, styling::ResolvedStyle};
use bevy_ecs::prelude::*;

pub(crate) fn translate_text(world: &World, key: Option<&str>, fallback: &str) -> String {
    match key {
        Some(key) => world.get_resource::<AppI18n>().map_or_else(
            || {
                if fallback.is_empty() {
                    key.to_string()
                } else {
                    fallback.to_string()
                }
            },
            |i18n| i18n.translate(key),
        ),
        None => fallback.to_string(),
    }
}

pub(crate) fn transparentize(color: xilem::Color) -> xilem::Color {
    let rgba = color.to_rgba8();
    xilem::Color::from_rgba8(rgba.r, rgba.g, rgba.b, 0)
}

pub(crate) fn hide_style_without_collapsing_layout(style: &mut ResolvedStyle) {
    style.colors.bg = Some(
        style
            .colors
            .bg
            .map_or(xilem::Color::TRANSPARENT, transparentize),
    );
    style.colors.border = Some(
        style
            .colors
            .border
            .map_or(xilem::Color::TRANSPARENT, transparentize),
    );
    style.colors.text = Some(
        style
            .colors
            .text
            .map_or(xilem::Color::TRANSPARENT, transparentize),
    );
    style.box_shadow = None;
}

pub(crate) fn estimate_text_width_px(text: &str, font_size: f32) -> f64 {
    let units = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_whitespace() {
                0.34
            } else if ch.is_ascii() {
                0.56
            } else {
                1.0
            }
        })
        .sum::<f64>();

    (units * font_size as f64).max(font_size as f64 * 2.0)
}

pub(crate) fn estimate_wrapped_lines(text: &str, font_size: f32, max_line_width: f64) -> usize {
    let max_line_width = max_line_width.max(font_size as f64 * 2.0);
    let mut total = 0_usize;

    for raw_line in text.lines() {
        let logical_line = if raw_line.is_empty() { " " } else { raw_line };
        let width = estimate_text_width_px(logical_line, font_size);
        let wrapped = (width / max_line_width).ceil() as usize;
        total += wrapped.max(1);
    }

    total.max(1)
}

pub(crate) fn app_i18n_font_stack(world: &World) -> Option<Vec<String>> {
    world
        .get_resource::<AppI18n>()
        .map(AppI18n::get_font_stack)
        .filter(|stack| !stack.is_empty())
}

pub(crate) fn localized_font_stack(world: &World, entity: Entity) -> Option<Vec<String>> {
    if world.get::<LocalizeText>(entity).is_none() {
        return None;
    }

    app_i18n_font_stack(world)
}
