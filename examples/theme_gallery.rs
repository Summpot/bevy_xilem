mod utils;

use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, BuiltinUiAction, ColorStyle, LayoutStyle, ProjectionCtx,
    PseudoClass, Selector, StyleClass, StyleRule, StyleSetter, StyleSheet, StyleTransition,
    TextStyle, UiButton, UiComboBox, UiComboBoxChanged, UiComboOption, UiDialog, UiEventQueue,
    UiRoot, UiView, apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    button, resolve_style, resolve_style_for_entity_classes, run_app_with_window_options,
    spawn_in_overlay_root, switch,
    xilem::{
        Color,
        style::Style as _,
        view::{CrossAxisAlignment, FlexExt as _, MainAxisAlignment, flex_col, flex_row, label},
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use utils::init_logging;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    fn is_light(self) -> bool {
        matches!(self, Self::Light)
    }

    fn as_text(self) -> &'static str {
        match self {
            Self::Dark => "Dark mode",
            Self::Light => "Light mode",
        }
    }

    fn class_name(self) -> &'static str {
        match self {
            Self::Dark => "theme.dark",
            Self::Light => "theme.light",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonKind {
    Primary,
    Danger,
    Outline,
}

impl ButtonKind {
    fn status_class(self) -> &'static str {
        match self {
            Self::Primary => "gallery.status.primary",
            Self::Danger => "gallery.status.danger",
            Self::Outline => "gallery.status.outline",
        }
    }

    fn action_text(self) -> &'static str {
        match self {
            Self::Primary => "Primary pressed",
            Self::Danger => "Danger pressed",
            Self::Outline => "Outline pressed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeChoice {
    Compact,
    Comfortable,
    Spacious,
}

impl EdgeChoice {
    fn from_value(value: &str) -> Option<Self> {
        match value {
            "compact" => Some(Self::Compact),
            "comfortable" => Some(Self::Comfortable),
            "spacious" => Some(Self::Spacious),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Comfortable => "Comfortable",
            Self::Spacious => "Spacious",
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct GalleryState {
    theme: ThemeMode,
    last_action: Option<ButtonKind>,
    edge_choice: Option<EdgeChoice>,
}

impl Default for GalleryState {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            last_action: None,
            edge_choice: None,
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct GalleryRuntime {
    root: Entity,
    status_badge: Entity,
    edge_combo: Entity,
    show_dialog_button: Entity,
}

#[derive(Debug, Clone, Copy)]
enum GalleryEvent {
    SetLightMode(bool),
    Press(ButtonKind),
}

#[derive(Component, Debug, Clone, Copy)]
struct GalleryRoot;

#[derive(Component, Debug, Clone, Copy)]
struct GalleryHeader;

#[derive(Component, Debug, Clone, Copy)]
struct GalleryButtonRow;

#[derive(Component, Debug, Clone, Copy)]
struct GalleryButton {
    kind: ButtonKind,
    label: &'static str,
}

#[derive(Component, Debug, Clone, Copy)]
struct ActionBadge;

#[derive(Component, Debug, Clone, Copy)]
struct NestedShell;

#[derive(Component, Debug, Clone, Copy)]
struct NestedStack;

#[derive(Component, Debug, Clone, Copy)]
struct NestedTitle;

#[derive(Component, Debug, Clone, Copy)]
struct NestedNote;

#[derive(Component, Debug, Clone, Copy)]
struct BottomEdgeDemo;

fn root_classes(theme: ThemeMode) -> StyleClass {
    StyleClass(vec![
        "gallery.root".to_string(),
        theme.class_name().to_string(),
    ])
}

fn status_classes(kind: Option<ButtonKind>) -> StyleClass {
    let mut classes = vec!["gallery.status".to_string()];
    match kind {
        Some(kind) => classes.push(kind.status_class().to_string()),
        None => classes.push("gallery.status.idle".to_string()),
    }
    StyleClass(classes)
}

fn project_gallery_root(_: &GalleryRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_col(children).cross_axis_alignment(CrossAxisAlignment::Start),
        &style,
    ))
}

fn project_gallery_header(_: &GalleryHeader, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let caption_style =
        resolve_style_for_entity_classes(ctx.world, ctx.entity, ["gallery.caption"]);
    let hint_style = resolve_style_for_entity_classes(ctx.world, ctx.entity, ["gallery.hint"]);
    let state = *ctx.world.resource::<GalleryState>();

    Arc::new(apply_widget_style(
        flex_col((
            flex_row((
                apply_label_style(label("Theme"), &caption_style),
                switch(
                    ctx.entity,
                    state.theme.is_light(),
                    GalleryEvent::SetLightMode,
                ),
                apply_label_style(label(state.theme.as_text()), &caption_style),
            ))
            .main_axis_alignment(MainAxisAlignment::Start),
            apply_label_style(
                label("All visual state changes are tween-animated (hover, press, theme)."),
                &hint_style,
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start),
        &style,
    ))
}

fn project_gallery_button_row(_: &GalleryButtonRow, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_row(children).main_axis_alignment(MainAxisAlignment::Start),
        &style,
    ))
}

fn project_gallery_button(button_def: &GalleryButton, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let state = *ctx.world.resource::<GalleryState>();
    let fallback_text = if state.theme.is_light() {
        Color::from_rgb8(0x1E, 0x2A, 0x44)
    } else {
        Color::from_rgb8(0xE9, 0xF0, 0xFF)
    };
    let text_color = style.colors.text.unwrap_or(fallback_text);

    Arc::new(
        button(
            ctx.entity,
            GalleryEvent::Press(button_def.kind),
            button_def.label,
        )
        .padding(style.layout.padding)
        .corner_radius(style.layout.corner_radius)
        .border(
            style.colors.border.unwrap_or(Color::TRANSPARENT),
            style.layout.border_width,
        )
        .background_color(style.colors.bg.unwrap_or(Color::TRANSPARENT))
        .color(text_color),
    )
}

fn project_action_badge(_: &ActionBadge, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let state = *ctx.world.resource::<GalleryState>();

    let text = state
        .last_action
        .map(ButtonKind::action_text)
        .unwrap_or("Click any button above");

    let combo_text = state.edge_choice.map(EdgeChoice::label).unwrap_or("(none)");

    Arc::new(apply_widget_style(
        apply_label_style(label(format!("{text}\nEdge combo: {combo_text}")), &style),
        &style,
    ))
}

fn project_nested_shell(_: &NestedShell, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_col(children).cross_axis_alignment(CrossAxisAlignment::Start),
        &style,
    ))
}

fn project_nested_stack(_: &NestedStack, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_col(children).cross_axis_alignment(CrossAxisAlignment::Start),
        &style,
    ))
}

fn project_nested_title(_: &NestedTitle, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    Arc::new(apply_widget_style(
        apply_label_style(label("Descendant selector demo"), &style),
        &style,
    ))
}

fn project_nested_note(_: &NestedNote, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    Arc::new(apply_widget_style(
        apply_label_style(
            label("This text is styled by ancestor classes + nested descendant rules."),
            &style,
        ),
        &style,
    ))
}

fn project_bottom_edge_demo(_: &BottomEdgeDemo, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_col(children).cross_axis_alignment(CrossAxisAlignment::Start),
        &style,
    ))
}

fn setup_gallery_world(mut commands: Commands) {
    let root = commands
        .spawn((UiRoot, GalleryRoot, root_classes(ThemeMode::Dark)))
        .id();

    commands.spawn((
        GalleryHeader,
        StyleClass(vec!["gallery.header".to_string()]),
        ChildOf(root),
    ));

    let button_row = commands
        .spawn((
            GalleryButtonRow,
            StyleClass(vec!["gallery.button-row".to_string()]),
            ChildOf(root),
        ))
        .id();

    commands.spawn((
        GalleryButton {
            kind: ButtonKind::Primary,
            label: "Primary",
        },
        StyleClass(vec![
            "gallery.button".to_string(),
            "gallery.button.primary".to_string(),
        ]),
        ChildOf(button_row),
    ));

    commands.spawn((
        GalleryButton {
            kind: ButtonKind::Danger,
            label: "Danger",
        },
        StyleClass(vec![
            "gallery.button".to_string(),
            "gallery.button.danger".to_string(),
        ]),
        ChildOf(button_row),
    ));

    commands.spawn((
        GalleryButton {
            kind: ButtonKind::Outline,
            label: "Outline",
        },
        StyleClass(vec![
            "gallery.button".to_string(),
            "gallery.button.outline".to_string(),
        ]),
        ChildOf(button_row),
    ));

    let status_badge = commands
        .spawn((ActionBadge, status_classes(None), ChildOf(root)))
        .id();

    let nested_shell = commands
        .spawn((
            NestedShell,
            StyleClass(vec!["gallery.nested-shell".to_string()]),
            ChildOf(root),
        ))
        .id();

    let nested_stack = commands
        .spawn((
            NestedStack,
            StyleClass(vec!["gallery.nested-stack".to_string()]),
            ChildOf(nested_shell),
        ))
        .id();

    commands.spawn((
        NestedTitle,
        StyleClass(vec![
            "gallery.descendant-target".to_string(),
            "gallery.nested-title".to_string(),
        ]),
        ChildOf(nested_stack),
    ));

    commands.spawn((
        NestedNote,
        StyleClass(vec![
            "gallery.descendant-target".to_string(),
            "gallery.nested-note".to_string(),
        ]),
        ChildOf(nested_stack),
    ));

    let bottom_demo = commands
        .spawn((
            BottomEdgeDemo,
            StyleClass(vec!["gallery.bottom-edge-demo".to_string()]),
            ChildOf(root),
        ))
        .id();

    commands.spawn((
        bevy_xilem::UiLabel::new(
            "Bottom-edge ComboBox demo: open it near the window bottom, it should flip upward.",
        ),
        StyleClass(vec!["gallery.edge-hint".to_string()]),
        ChildOf(bottom_demo),
    ));

    let edge_combo = commands
        .spawn((
            UiComboBox::new(vec![
                UiComboOption::new("compact", "Compact"),
                UiComboOption::new("comfortable", "Comfortable"),
                UiComboOption::new("spacious", "Spacious"),
            ])
            .with_placeholder("Density")
            .with_overlay_placement(bevy_xilem::OverlayPlacement::BottomStart)
            .with_overlay_auto_flip(true),
            StyleClass(vec!["gallery.edge-combo".to_string()]),
            ChildOf(bottom_demo),
        ))
        .id();

    let show_dialog_button = commands
        .spawn((
            UiButton::new("Open dialog (click backdrop to close)"),
            StyleClass(vec!["gallery.overlay-button".to_string()]),
            ChildOf(bottom_demo),
        ))
        .id();

    commands.insert_resource(GalleryRuntime {
        root,
        status_badge,
        edge_combo,
        show_dialog_button,
    });
}

fn setup_gallery_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "gallery.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(20.0),
                gap: Some(12.0),
                corner_radius: Some(14.0),
                border_width: Some(1.0),
            },
            transition: Some(StyleTransition { duration: 0.22 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("gallery.root"),
            Selector::class("theme.dark"),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x12, 0x15, 0x20)),
                border: Some(Color::from_rgb8(0x2A, 0x31, 0x45)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("gallery.root"),
            Selector::class("theme.light"),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xF4, 0xF7, 0xFF)),
                border: Some(Color::from_rgb8(0xD1, 0xDD, 0xF6)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.set_class(
        "gallery.header",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                gap: Some(8.0),
                corner_radius: Some(10.0),
                border_width: Some(1.0),
            },
            transition: Some(StyleTransition { duration: 0.22 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.dark"),
            Selector::class("gallery.header"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1A, 0x1F, 0x2E)),
                border: Some(Color::from_rgb8(0x2F, 0x3B, 0x57)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.light"),
            Selector::class("gallery.header"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
                border: Some(Color::from_rgb8(0xD9, 0xE4, 0xFB)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.set_class(
        "gallery.caption",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            transition: Some(StyleTransition { duration: 0.22 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.hint",
        StyleSetter {
            text: TextStyle { size: Some(13.0) },
            transition: Some(StyleTransition { duration: 0.22 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.dark"),
            Selector::class("gallery.caption"),
        ),
        StyleSetter {
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xE5, 0xEB, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.light"),
            Selector::class("gallery.caption"),
        ),
        StyleSetter {
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0x1E, 0x2A, 0x44)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.dark"),
            Selector::class("gallery.hint"),
        ),
        StyleSetter {
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0x9E, 0xAF, 0xD5)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.light"),
            Selector::class("gallery.hint"),
        ),
        StyleSetter {
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0x5D, 0x6C, 0x8B)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.set_class(
        "gallery.button-row",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(10.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.14 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.button.primary",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2B, 0x6C, 0xF2)),
                border: Some(Color::from_rgb8(0x2B, 0x6C, 0xF2)),
                text: Some(Color::from_rgb8(0xF8, 0xFB, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.button.danger",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xD6, 0x45, 0x45)),
                border: Some(Color::from_rgb8(0xD6, 0x45, 0x45)),
                text: Some(Color::from_rgb8(0xFF, 0xF1, 0xF1)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.button.outline",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::TRANSPARENT),
                border: Some(Color::from_rgb8(0x58, 0x71, 0xA6)),
                text: Some(Color::from_rgb8(0xE3, 0xEB, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.light"),
            Selector::class("gallery.button.outline"),
        ),
        StyleSetter {
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0x2D, 0x43, 0x70)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("gallery.button.primary"),
            Selector::pseudo(PseudoClass::Hovered),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x3C, 0x7B, 0xFB)),
                border: Some(Color::from_rgb8(0x3C, 0x7B, 0xFB)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("gallery.button.primary"),
            Selector::pseudo(PseudoClass::Pressed),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1D, 0x56, 0xD6)),
                border: Some(Color::from_rgb8(0x1D, 0x56, 0xD6)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("gallery.button.danger"),
            Selector::pseudo(PseudoClass::Hovered),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xE2, 0x5A, 0x5A)),
                border: Some(Color::from_rgb8(0xE2, 0x5A, 0x5A)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("gallery.button.danger"),
            Selector::pseudo(PseudoClass::Pressed),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xAF, 0x2F, 0x2F)),
                border: Some(Color::from_rgb8(0xAF, 0x2F, 0x2F)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("gallery.button.outline"),
            Selector::pseudo(PseudoClass::Hovered),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(0x4E, 0x66, 0x99, 0x30)),
                border: Some(Color::from_rgb8(0x6A, 0x86, 0xC3)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::and(vec![
            Selector::class("gallery.button.outline"),
            Selector::pseudo(PseudoClass::Pressed),
        ]),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(0x3E, 0x56, 0x85, 0x40)),
                border: Some(Color::from_rgb8(0x4F, 0x69, 0x9A)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.set_class(
        "gallery.status",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            text: TextStyle { size: Some(14.0) },
            transition: Some(StyleTransition { duration: 0.16 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.status.idle",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2C, 0x32, 0x44)),
                border: Some(Color::from_rgb8(0x45, 0x4F, 0x6B)),
                text: Some(Color::from_rgb8(0xD8, 0xDF, 0xF2)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.status.primary",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1C, 0x45, 0x98)),
                border: Some(Color::from_rgb8(0x2A, 0x5C, 0xC3)),
                text: Some(Color::from_rgb8(0xE6, 0xEE, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.status.danger",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x7B, 0x24, 0x24)),
                border: Some(Color::from_rgb8(0xB8, 0x36, 0x36)),
                text: Some(Color::from_rgb8(0xFF, 0xE7, 0xE7)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.status.outline",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2C, 0x36, 0x50)),
                border: Some(Color::from_rgb8(0x6B, 0x82, 0xB8)),
                text: Some(Color::from_rgb8(0xE5, 0xEC, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.nested-shell",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                gap: Some(8.0),
                corner_radius: Some(10.0),
                border_width: Some(1.0),
            },
            transition: Some(StyleTransition { duration: 0.22 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.dark"),
            Selector::class("gallery.nested-shell"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x16, 0x1C, 0x2E)),
                border: Some(Color::from_rgb8(0x2F, 0x3D, 0x61)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.light"),
            Selector::class("gallery.nested-shell"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xFA, 0xFC, 0xFF)),
                border: Some(Color::from_rgb8(0xD6, 0xE1, 0xFA)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.set_class(
        "gallery.nested-stack",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                gap: Some(6.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
            },
            transition: Some(StyleTransition { duration: 0.22 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("gallery.nested-shell"),
            Selector::class("gallery.nested-stack"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(0x00, 0x00, 0x00, 0x20)),
                border: Some(Color::from_rgb8(0x56, 0x6A, 0x97)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.set_class(
        "gallery.descendant-target",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                corner_radius: Some(6.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.22 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("gallery.nested-shell"),
            Selector::class("gallery.descendant-target"),
        ),
        StyleSetter {
            colors: ColorStyle {
                border: Some(Color::from_rgb8(0x56, 0x6B, 0x9A)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.dark"),
            Selector::class("gallery.descendant-target"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1A, 0x25, 0x3E)),
                text: Some(Color::from_rgb8(0xE4, 0xEC, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.add_rule(StyleRule::new(
        Selector::descendant(
            Selector::class("theme.light"),
            Selector::class("gallery.descendant-target"),
        ),
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xEE, 0xF3, 0xFF)),
                text: Some(Color::from_rgb8(0x2A, 0x3A, 0x5E)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    ));

    style_sheet.set_class(
        "gallery.nested-title",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.nested-note",
        StyleSetter {
            text: TextStyle { size: Some(13.0) },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.bottom-edge-demo",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                gap: Some(8.0),
                corner_radius: Some(10.0),
                border_width: Some(1.0),
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x17, 0x1F, 0x31)),
                border: Some(Color::from_rgb8(0x37, 0x4A, 0x72)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.edge-hint",
        StyleSetter {
            text: TextStyle { size: Some(13.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xC5, 0xD3, 0xF3)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.edge-combo",
        StyleSetter {
            text: TextStyle { size: Some(15.0) },
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x25, 0x33, 0x4F)),
                hover_bg: Some(Color::from_rgb8(0x2E, 0x3E, 0x5F)),
                pressed_bg: Some(Color::from_rgb8(0x1D, 0x2B, 0x44)),
                border: Some(Color::from_rgb8(0x4F, 0x66, 0x95)),
                text: Some(Color::from_rgb8(0xDF, 0xE9, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "gallery.overlay-button",
        StyleSetter {
            text: TextStyle { size: Some(14.0) },
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(8.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x6E, 0x48, 0xE8)),
                hover_bg: Some(Color::from_rgb8(0x7D, 0x58, 0xF2)),
                pressed_bg: Some(Color::from_rgb8(0x5B, 0x3A, 0xC6)),
                text: Some(Color::from_rgb8(0xF5, 0xF1, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "overlay.dialog.backdrop",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(0x00, 0x00, 0x00, 0xA0)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );
}

fn drain_gallery_events(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<GalleryEvent>();

    if events.is_empty() {
        return;
    }

    for event in events {
        match event.action {
            GalleryEvent::SetLightMode(is_light) => {
                let theme = if is_light {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                };

                world.resource_mut::<GalleryState>().theme = theme;

                let root = world.resource::<GalleryRuntime>().root;
                world.entity_mut(root).insert(root_classes(theme));
            }
            GalleryEvent::Press(kind) => {
                world.resource_mut::<GalleryState>().last_action = Some(kind);

                let status_badge = world.resource::<GalleryRuntime>().status_badge;
                world
                    .entity_mut(status_badge)
                    .insert(status_classes(Some(kind)));
            }
        }
    }

    let builtin_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<BuiltinUiAction>();

    let show_dialog_button = world.resource::<GalleryRuntime>().show_dialog_button;

    for event in builtin_events {
        if !matches!(event.action, BuiltinUiAction::Clicked) {
            continue;
        }

        if event.entity != show_dialog_button {
            continue;
        }

        spawn_in_overlay_root(
            world,
            (UiDialog::new(
                "Overlay dialog",
                "Click the dimmed backdrop to close.\nOpen the bottom combo and click outside to dismiss.",
            ),),
        );
    }

    let combo_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiComboBoxChanged>();
    let edge_combo = world.resource::<GalleryRuntime>().edge_combo;

    for event in combo_events {
        if event.action.combo != edge_combo {
            continue;
        }

        world.resource_mut::<GalleryState>().edge_choice =
            EdgeChoice::from_value(event.action.value.as_str());
    }
}

fn build_theme_gallery_app() -> App {
    init_logging();

    // Hot reload demo:
    // while this example is running, edit `assets/themes/default_theme.ron`
    // (for example `overlay.dialog.dismiss` or `gallery.button.primary`) and save.
    // The style asset is reloaded automatically and transitions animate to new values.

    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .load_style_sheet("themes/default_theme.ron")
        .insert_resource(GalleryState::default())
        .register_projector::<GalleryRoot>(project_gallery_root)
        .register_projector::<GalleryHeader>(project_gallery_header)
        .register_projector::<GalleryButtonRow>(project_gallery_button_row)
        .register_projector::<GalleryButton>(project_gallery_button)
        .register_projector::<ActionBadge>(project_action_badge)
        .register_projector::<NestedShell>(project_nested_shell)
        .register_projector::<NestedStack>(project_nested_stack)
        .register_projector::<NestedTitle>(project_nested_title)
        .register_projector::<NestedNote>(project_nested_note)
        .register_projector::<BottomEdgeDemo>(project_bottom_edge_demo)
        .add_systems(Startup, (setup_gallery_styles, setup_gallery_world))
        .add_systems(PreUpdate, drain_gallery_events);
    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_theme_gallery_app(), "Theme Gallery", |options| {
        options.with_initial_inner_size(LogicalSize::new(920.0, 600.0))
    })
}
