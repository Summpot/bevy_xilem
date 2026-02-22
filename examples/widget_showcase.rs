mod utils;

use std::sync::Arc;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, BuiltinUiAction, ColorStyle, HasTooltip, LayoutStyle,
    ProjectionCtx, StyleClass, StyleSetter, StyleSheet, TextStyle, ToastKind, UiButton,
    UiColorPicker, UiColorPickerChanged, UiDatePicker, UiDatePickerChanged, UiEventQueue,
    UiFlexColumn, UiFlexRow, UiGroupBox, UiLabel, UiMenuBar, UiMenuBarItem, UiMenuItem,
    UiMenuItemSelected, UiRadioGroup, UiRadioGroupChanged, UiRoot, UiSpinner, UiSplitPane,
    UiTabBar, UiTabChanged, UiTable, UiToast, UiTreeNode, UiTreeNodeToggled, UiView,
    apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    resolve_style, resolve_style_for_classes, run_app_with_window_options, spawn_in_overlay_root,
    xilem::{
        Color,
        masonry::layout::Length,
        style::Style as _,
        view::{FlexExt as _, flex_col, label},
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use utils::init_logging;

// ---------------------------------------------------------------------------
// State resources
// ---------------------------------------------------------------------------

#[derive(Resource, Debug, Clone)]
struct ShowcaseState {
    last_event: String,
}

impl Default for ShowcaseState {
    fn default() -> Self {
        Self {
            last_event: "Interact with a widget to see events here.".to_string(),
        }
    }
}

/// Entity IDs we need to reference at runtime.
#[derive(Resource, Debug, Clone, Copy)]
struct ShowcaseRuntime {
    status_label: Entity,
    toast_info_btn: Entity,
    toast_success_btn: Entity,
    toast_warning_btn: Entity,
    toast_error_btn: Entity,
}

// ---------------------------------------------------------------------------
// Marker components (for custom projectors)
// ---------------------------------------------------------------------------

#[derive(Component, Debug, Clone, Copy, Default)]
struct ShowcaseRoot;

#[derive(Component, Debug, Clone, Copy, Default)]
struct StatusDisplay;

// ---------------------------------------------------------------------------
// Custom projectors
// ---------------------------------------------------------------------------

fn project_showcase_root(_: &ShowcaseRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|c| c.into_any_flex())
        .collect::<Vec<_>>();
    Arc::new(apply_widget_style(
        flex_col(children).gap(Length::px(16.0)),
        &style,
    ))
}

fn project_status_display(_: &StatusDisplay, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let state = ctx.world.resource::<ShowcaseState>();
    let text_style = resolve_style_for_classes(ctx.world, ["showcase.status.text"]);
    Arc::new(apply_widget_style(
        apply_label_style(label(state.last_event.clone()), &text_style),
        &style,
    ))
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn setup_showcase(mut commands: Commands) {
    // Root
    let root = commands
        .spawn((
            UiRoot,
            ShowcaseRoot,
            StyleClass(vec!["showcase.root".to_string()]),
        ))
        .id();

    // Title
    commands.spawn((
        UiLabel::new("Widget Showcase"),
        StyleClass(vec!["showcase.title".to_string()]),
        ChildOf(root),
    ));

    // Status display
    let status_label = commands
        .spawn((
            StatusDisplay,
            StyleClass(vec!["showcase.status".to_string()]),
            ChildOf(root),
        ))
        .id();

    // --- Radio Group ---
    let radio_section = commands
        .spawn((UiGroupBox::new("Radio Group"), ChildOf(root)))
        .id();
    commands.spawn((
        UiRadioGroup::new(["Apple", "Banana", "Cherry", "Date"]),
        ChildOf(radio_section),
    ));

    // --- Tab Bar ---
    let tab_section = commands
        .spawn((UiGroupBox::new("Tab Bar"), ChildOf(root)))
        .id();
    let tab_bar = commands
        .spawn((
            UiTabBar::new(["Details", "Settings", "Logs"]),
            ChildOf(tab_section),
        ))
        .id();
    commands.spawn((
        UiLabel::new("Details tab content: item information and metadata."),
        ChildOf(tab_bar),
    ));
    commands.spawn((
        UiLabel::new("Settings tab content: configuration options."),
        ChildOf(tab_bar),
    ));
    commands.spawn((
        UiLabel::new("Logs tab content: event history and diagnostics."),
        ChildOf(tab_bar),
    ));

    // --- Tree View ---
    let tree_section = commands
        .spawn((UiGroupBox::new("Tree View"), ChildOf(root)))
        .id();
    let root_node = commands
        .spawn((UiTreeNode::new("Root").expanded(), ChildOf(tree_section)))
        .id();
    let child1 = commands
        .spawn((UiTreeNode::new("Documents").expanded(), ChildOf(root_node)))
        .id();
    commands.spawn((UiTreeNode::new("report.pdf"), ChildOf(child1)));
    commands.spawn((UiTreeNode::new("notes.txt"), ChildOf(child1)));
    let child2 = commands
        .spawn((UiTreeNode::new("Projects"), ChildOf(root_node)))
        .id();
    commands.spawn((UiTreeNode::new("bevy_app"), ChildOf(child2)));
    commands.spawn((UiTreeNode::new("xilem_ui"), ChildOf(child2)));
    commands.spawn((UiTreeNode::new("readme.md"), ChildOf(root_node)));

    // --- Table ---
    let table_section = commands
        .spawn((UiGroupBox::new("Table / Data Grid"), ChildOf(root)))
        .id();
    commands.spawn((
        UiTable::new(["Name", "Role", "Status", "Score"])
            .with_row(["Alice Chen", "Engineer", "Active", "98"])
            .with_row(["Bob Smith", "Designer", "Away", "85"])
            .with_row(["Carol Davis", "Manager", "Active", "91"])
            .with_row(["Dave Wilson", "Lead", "Busy", "88"]),
        ChildOf(table_section),
    ));

    // --- Menu Bar ---
    let menu_section = commands
        .spawn((UiGroupBox::new("Menu Bar"), ChildOf(root)))
        .id();
    let menu_bar = commands.spawn((UiMenuBar, ChildOf(menu_section))).id();
    commands.spawn((
        UiMenuBarItem::new(
            "File",
            [
                UiMenuItem::new("New File", "file.new"),
                UiMenuItem::new("Open...", "file.open"),
                UiMenuItem::new("Save", "file.save"),
                UiMenuItem::new("Exit", "file.exit"),
            ],
        ),
        ChildOf(menu_bar),
    ));
    commands.spawn((
        UiMenuBarItem::new(
            "Edit",
            [
                UiMenuItem::new("Cut", "edit.cut"),
                UiMenuItem::new("Copy", "edit.copy"),
                UiMenuItem::new("Paste", "edit.paste"),
                UiMenuItem::new("Select All", "edit.select_all"),
            ],
        ),
        ChildOf(menu_bar),
    ));
    commands.spawn((
        UiMenuBarItem::new(
            "View",
            [
                UiMenuItem::new("Zoom In", "view.zoom_in"),
                UiMenuItem::new("Zoom Out", "view.zoom_out"),
                UiMenuItem::new("Reset Zoom", "view.zoom_reset"),
            ],
        ),
        ChildOf(menu_bar),
    ));

    // --- Spinner ---
    let spinner_section = commands
        .spawn((
            UiGroupBox::new("Spinner / Loading Indicator"),
            ChildOf(root),
        ))
        .id();
    let spinner_row = commands.spawn((UiFlexRow, ChildOf(spinner_section))).id();
    commands.spawn((UiSpinner::new(), ChildOf(spinner_row)));
    commands.spawn((
        UiSpinner::new().with_label("Processing…"),
        ChildOf(spinner_row),
    ));
    commands.spawn((
        UiSpinner::new().with_label("Uploading files…"),
        ChildOf(spinner_row),
    ));

    // --- Color Picker ---
    let color_section = commands
        .spawn((UiGroupBox::new("Color Picker"), ChildOf(root)))
        .id();
    commands.spawn((UiColorPicker::new(0x60, 0xA5, 0xFA), ChildOf(color_section)));

    // --- Date Picker ---
    let date_section = commands
        .spawn((UiGroupBox::new("Date Picker"), ChildOf(root)))
        .id();
    commands.spawn((UiDatePicker::new(2024, 6, 15), ChildOf(date_section)));

    // --- Split Pane ---
    let split_section = commands
        .spawn((UiGroupBox::new("Split Pane"), ChildOf(root)))
        .id();
    let split_pane = commands
        .spawn((UiSplitPane::new(0.4), ChildOf(split_section)))
        .id();
    commands.spawn((
        UiFlexColumn,
        StyleClass(vec!["showcase.split.panel".to_string()]),
        ChildOf(split_pane),
    ));
    commands.spawn((
        UiFlexColumn,
        StyleClass(vec!["showcase.split.panel".to_string()]),
        ChildOf(split_pane),
    ));

    // --- Toast ---
    let toast_section = commands
        .spawn((UiGroupBox::new("Toast Notifications"), ChildOf(root)))
        .id();
    let toast_btn_row = commands.spawn((UiFlexRow, ChildOf(toast_section))).id();
    let toast_info_btn = commands
        .spawn((UiButton::new("Info Toast"), ChildOf(toast_btn_row)))
        .id();
    let toast_success_btn = commands
        .spawn((UiButton::new("Success Toast"), ChildOf(toast_btn_row)))
        .id();
    let toast_warning_btn = commands
        .spawn((UiButton::new("Warning Toast"), ChildOf(toast_btn_row)))
        .id();
    let toast_error_btn = commands
        .spawn((UiButton::new("Error Toast"), ChildOf(toast_btn_row)))
        .id();

    // --- Tooltip ---
    let tooltip_section = commands
        .spawn((UiGroupBox::new("Tooltip"), ChildOf(root)))
        .id();
    let tooltip_row = commands.spawn((UiFlexRow, ChildOf(tooltip_section))).id();
    commands.spawn((
        UiButton::new("Hover me!"),
        HasTooltip::new("This is a tooltip shown on hover."),
        ChildOf(tooltip_row),
    ));
    commands.spawn((
        UiButton::new("I have a tip too"),
        HasTooltip::new("Tooltips work on any widget that can be hovered."),
        ChildOf(tooltip_row),
    ));

    commands.insert_resource(ShowcaseRuntime {
        status_label,
        toast_info_btn,
        toast_success_btn,
        toast_warning_btn,
        toast_error_btn,
    });
}

fn setup_showcase_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "showcase.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(24.0),
                gap: Some(16.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x0D, 0x12, 0x1E)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.title",
        StyleSetter {
            text: TextStyle { size: Some(26.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xE2, 0xE8, 0xF0)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.status",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(6.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(255, 255, 255, 12)),
                border: Some(Color::from_rgb8(0x38, 0x46, 0x64)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.status.text",
        StyleSetter {
            text: TextStyle { size: Some(13.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0x94, 0xA3, 0xB8)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.split.panel",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(12.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(255, 255, 255, 8)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );
}

// ---------------------------------------------------------------------------
// Event drain
// ---------------------------------------------------------------------------

fn drain_showcase_events(world: &mut World) {
    let rt = match world.get_resource::<ShowcaseRuntime>() {
        Some(rt) => *rt,
        None => return,
    };

    // Built-in button clicks (for toast spawning)
    let builtin_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<BuiltinUiAction>();

    for event in builtin_events {
        if !matches!(event.action, BuiltinUiAction::Clicked) {
            continue;
        }

        if event.entity == rt.toast_info_btn {
            spawn_in_overlay_root(
                world,
                (UiToast::new("Info: Widget showcase loaded successfully!"),),
            );
        } else if event.entity == rt.toast_success_btn {
            spawn_in_overlay_root(
                world,
                (UiToast::new("Success: Operation completed.").with_kind(ToastKind::Success),),
            );
        } else if event.entity == rt.toast_warning_btn {
            spawn_in_overlay_root(
                world,
                (
                    UiToast::new("Warning: Check your configuration.")
                        .with_kind(ToastKind::Warning),
                ),
            );
        } else if event.entity == rt.toast_error_btn {
            spawn_in_overlay_root(
                world,
                (UiToast::new("Error: Something went wrong!").with_kind(ToastKind::Error),),
            );
        }
    }

    // Radio group changes
    let radio_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiRadioGroupChanged>();
    for event in radio_events {
        let msg = format!("Radio: selected option index {}", event.action.selected);
        update_status(world, rt.status_label, msg);
    }

    // Tab changes
    let tab_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiTabChanged>();
    for event in tab_events {
        let msg = format!("Tab Bar: switched to tab {}", event.action.active);
        update_status(world, rt.status_label, msg);
    }

    // Tree node toggles
    let tree_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiTreeNodeToggled>();
    for event in tree_events {
        let state = if event.action.is_expanded {
            "expanded"
        } else {
            "collapsed"
        };
        let msg = format!("Tree Node {:?}: {state}", event.action.node);
        update_status(world, rt.status_label, msg);
    }

    // Menu item selections
    let menu_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiMenuItemSelected>();
    for event in menu_events {
        let msg = format!("Menu: selected \"{}\"", event.action.value);
        update_status(world, rt.status_label, msg);
    }

    // Color picker changes
    let color_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiColorPickerChanged>();
    for event in color_events {
        let msg = format!(
            "Color Picker: #{:02X}{:02X}{:02X}",
            event.action.r, event.action.g, event.action.b
        );
        update_status(world, rt.status_label, msg);
    }

    // Date picker changes
    let date_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiDatePickerChanged>();
    for event in date_events {
        let msg = format!(
            "Date Picker: {:04}-{:02}-{:02}",
            event.action.year, event.action.month, event.action.day
        );
        update_status(world, rt.status_label, msg);
    }
}

fn update_status(world: &mut World, _label_entity: Entity, text: String) {
    if let Some(mut state) = world.get_resource_mut::<ShowcaseState>() {
        state.last_event = text;
    }
}

// ---------------------------------------------------------------------------
// App builder + main
// ---------------------------------------------------------------------------

fn build_showcase_app() -> App {
    init_logging();

    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .register_ui_control::<bevy_xilem::UiButton>()
        .register_ui_control::<bevy_xilem::UiCheckbox>()
        .register_ui_control::<bevy_xilem::UiSlider>()
        .register_ui_control::<bevy_xilem::UiSwitch>()
        .register_ui_control::<bevy_xilem::UiTextInput>()
        .register_ui_control::<bevy_xilem::UiDialog>()
        .register_ui_control::<bevy_xilem::UiComboBox>()
        .register_ui_control::<bevy_xilem::UiRadioGroup>()
        .register_ui_control::<bevy_xilem::UiTabBar>()
        .register_ui_control::<bevy_xilem::UiTreeNode>()
        .register_ui_control::<bevy_xilem::UiTable>()
        .register_ui_control::<bevy_xilem::UiMenuBar>()
        .register_ui_control::<bevy_xilem::UiMenuBarItem>()
        .register_ui_control::<bevy_xilem::UiSpinner>()
        .register_ui_control::<bevy_xilem::UiColorPicker>()
        .register_ui_control::<bevy_xilem::UiGroupBox>()
        .register_ui_control::<bevy_xilem::UiSplitPane>()
        .register_ui_control::<bevy_xilem::UiDatePicker>()
        .load_style_sheet("themes/default_theme.ron")
        .insert_resource(ShowcaseState::default())
        .register_projector::<ShowcaseRoot>(project_showcase_root)
        .register_projector::<StatusDisplay>(project_status_display)
        .add_systems(Startup, (setup_showcase_styles, setup_showcase))
        .add_systems(PreUpdate, drain_showcase_events);
    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_showcase_app(), "Widget Showcase", |options| {
        options.with_initial_inner_size(LogicalSize::new(960.0, 720.0))
    })
}
