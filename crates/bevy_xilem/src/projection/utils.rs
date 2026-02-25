use std::sync::Arc;

use super::core::UiView;
use crate::{ecs::LocalizeText, i18n::AppI18n, styling::ResolvedStyle};
use bevy_ecs::prelude::*;
use masonry::{
    kurbo::{Affine, BezPath, Circle, Stroke},
    layout::{Dim, Length},
    peniko::Fill,
};
use xilem_masonry::style::Style as _;
use xilem_masonry::view::{canvas, sized_box};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VectorIcon {
    ChevronDown,
    ChevronUp,
    ChevronRight,
    RadioOff,
    RadioOn,
}

pub(crate) fn vector_icon(icon: VectorIcon, size_px: f64, color: xilem::Color) -> UiView {
    Arc::new(
        sized_box(canvas(move |_, _, scene, size| {
            let width = size.width.max(1.0);
            let height = size.height.max(1.0);

            match icon {
                VectorIcon::ChevronDown => {
                    let mut path = BezPath::new();
                    path.move_to((width * 0.22, height * 0.34));
                    path.line_to((width * 0.50, height * 0.66));
                    path.line_to((width * 0.78, height * 0.34));
                    scene.stroke(
                        &Stroke::new((size_px * 0.14).max(1.6)),
                        Affine::IDENTITY,
                        color,
                        None,
                        &path,
                    );
                }
                VectorIcon::ChevronUp => {
                    let mut path = BezPath::new();
                    path.move_to((width * 0.22, height * 0.66));
                    path.line_to((width * 0.50, height * 0.34));
                    path.line_to((width * 0.78, height * 0.66));
                    scene.stroke(
                        &Stroke::new((size_px * 0.14).max(1.6)),
                        Affine::IDENTITY,
                        color,
                        None,
                        &path,
                    );
                }
                VectorIcon::ChevronRight => {
                    let mut path = BezPath::new();
                    path.move_to((width * 0.34, height * 0.22));
                    path.line_to((width * 0.66, height * 0.50));
                    path.line_to((width * 0.34, height * 0.78));
                    scene.stroke(
                        &Stroke::new((size_px * 0.14).max(1.6)),
                        Affine::IDENTITY,
                        color,
                        None,
                        &path,
                    );
                }
                VectorIcon::RadioOff => {
                    let radius = width.min(height) * 0.40;
                    let circle = Circle::new((width * 0.5, height * 0.5), radius);
                    scene.stroke(
                        &Stroke::new((size_px * 0.11).max(1.4)),
                        Affine::IDENTITY,
                        color,
                        None,
                        &circle,
                    );
                }
                VectorIcon::RadioOn => {
                    let outer_radius = width.min(height) * 0.40;
                    let outer = Circle::new((width * 0.5, height * 0.5), outer_radius);
                    scene.stroke(
                        &Stroke::new((size_px * 0.11).max(1.4)),
                        Affine::IDENTITY,
                        color,
                        None,
                        &outer,
                    );

                    let inner_radius = outer_radius * 0.45;
                    let inner = Circle::new((width * 0.5, height * 0.5), inner_radius);
                    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &inner);
                }
            }
        }))
        .width(Dim::Fixed(Length::px(size_px)))
        .height(Dim::Fixed(Length::px(size_px))),
    )
}

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
