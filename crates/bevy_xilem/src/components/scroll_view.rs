use bevy_ecs::{entity::Entity, prelude::*};
use bevy_math::Vec2;

use crate::{
    ProjectionCtx, StyleClass, UiLabel, UiView, components::UiComponentTemplate,
    templates::ensure_template_part,
};

/// Scroll axis used by [`UiScrollView`] interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAxis {
    Horizontal,
    Vertical,
}

/// Built-in portal-backed scroll container.
///
/// This component stores logical scroll state (`scroll_offset`) together with
/// viewport/content extents. Projectors can use this state both for rendering
/// and for virtualization decisions.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiScrollView {
    pub scroll_offset: Vec2,
    pub content_size: Vec2,
    pub viewport_size: Vec2,
    pub show_horizontal_scrollbar: bool,
    pub show_vertical_scrollbar: bool,
}

impl Default for UiScrollView {
    fn default() -> Self {
        Self {
            scroll_offset: Vec2::ZERO,
            content_size: Vec2::new(960.0, 960.0),
            viewport_size: Vec2::new(420.0, 280.0),
            show_horizontal_scrollbar: false,
            show_vertical_scrollbar: true,
        }
    }
}

impl UiScrollView {
    #[must_use]
    pub fn new(viewport_size: Vec2, content_size: Vec2) -> Self {
        Self {
            viewport_size,
            content_size,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_horizontal_scrollbar(mut self, enabled: bool) -> Self {
        self.show_horizontal_scrollbar = enabled;
        self
    }

    #[must_use]
    pub fn with_vertical_scrollbar(mut self, enabled: bool) -> Self {
        self.show_vertical_scrollbar = enabled;
        self
    }

    #[must_use]
    pub fn max_scroll_offset(self) -> Vec2 {
        Vec2::new(
            (self.content_size.x - self.viewport_size.x).max(0.0),
            (self.content_size.y - self.viewport_size.y).max(0.0),
        )
    }

    pub fn clamp_scroll_offset(&mut self) {
        let max = self.max_scroll_offset();
        self.scroll_offset.x = self.scroll_offset.x.clamp(0.0, max.x);
        self.scroll_offset.y = self.scroll_offset.y.clamp(0.0, max.y);
    }

    /// Virtualization helper: visible content rectangle in content-space.
    #[must_use]
    pub fn visible_rect(self) -> (Vec2, Vec2) {
        let start = self.scroll_offset.max(Vec2::ZERO);
        let end = start + self.viewport_size.max(Vec2::ZERO);
        (start, end)
    }
}

/// Emitted when a [`UiScrollView`] offset changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiScrollViewChanged {
    pub scroll_view: Entity,
    pub scroll_offset: Vec2,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartScrollViewport;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartScrollBarVertical;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartScrollBarHorizontal;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartScrollThumbVertical;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartScrollThumbHorizontal;

impl UiComponentTemplate for UiScrollView {
    fn expand(world: &mut World, entity: Entity) {
        let _viewport = ensure_template_part::<PartScrollViewport, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.scroll_view.viewport".to_string()]),
            )
        });

        let _vertical_bar = ensure_template_part::<PartScrollBarVertical, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["template.scroll_view.scrollbar.vertical".to_string()]),
            )
        });

        let _vertical_thumb =
            ensure_template_part::<PartScrollThumbVertical, _>(world, entity, || {
                (
                    UiLabel::new(""),
                    StyleClass(vec!["template.scroll_view.thumb.vertical".to_string()]),
                )
            });

        let _horizontal_bar =
            ensure_template_part::<PartScrollBarHorizontal, _>(world, entity, || {
                (
                    UiLabel::new(""),
                    StyleClass(vec![
                        "template.scroll_view.scrollbar.horizontal".to_string(),
                    ]),
                )
            });

        let _horizontal_thumb =
            ensure_template_part::<PartScrollThumbHorizontal, _>(world, entity, || {
                (
                    UiLabel::new(""),
                    StyleClass(vec!["template.scroll_view.thumb.horizontal".to_string()]),
                )
            });
    }

    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::widgets::project_scroll_view(component, ctx)
    }
}
