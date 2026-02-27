use std::sync::Arc;

use bevy_embedded_assets::{EmbeddedAssetPlugin, PluginMode};
use bevy_xilem::{
    AppBevyXilemExt, AppI18n, BevyXilemPlugin, BuiltinUiAction, ColorStyle, HasTooltip,
    LayoutStyle, LocalizeText, ProjectionCtx, StyleClass, StyleSetter, StyleSheet, StyleTransition,
    SyncAssetSource, SyncTextSource, TextStyle, ToastKind, UiButton, UiCheckbox, UiCheckboxChanged,
    UiColorPicker, UiColorPickerChanged, UiComboBox, UiComboBoxChanged, UiComboOption,
    UiDatePicker, UiDatePickerChanged, UiDialog, UiEventQueue, UiFlexColumn, UiFlexRow, UiGroupBox,
    UiLabel, UiMenuBar, UiMenuBarItem, UiMenuItem, UiMenuItemSelected, UiRadioGroup,
    UiRadioGroupChanged, UiRoot, UiScrollView, UiScrollViewChanged, UiSlider, UiSliderChanged,
    UiSpinner, UiSplitPane, UiTabBar, UiTabChanged, UiTable, UiTextInput, UiTextInputChanged,
    UiToast, UiTreeNode, UiTreeNodeToggled, UiView, apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_asset::AssetPlugin,
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    bevy_math::Vec2,
    bevy_text::TextPlugin,
    resolve_style, resolve_style_for_classes, run_app_with_window_options, spawn_in_overlay_root,
    xilem::{
        Color,
        masonry::layout::{Dim, Length},
        style::Style as _,
        view::{FlexExt as _, flex_col, label},
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use shared_utils::init_logging;
use unic_langid::LanguageIdentifier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    fn class_name(self) -> &'static str {
        match self {
            Self::Dark => "theme.dark",
            Self::Light => "theme.light",
        }
    }

    fn from_combo_value(value: &str) -> Option<Self> {
        match value {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }
}

#[derive(Resource, Debug, Clone)]
struct ShowcaseState {
    last_event: String,
    theme: ThemeMode,
}

impl Default for ShowcaseState {
    fn default() -> Self {
        Self {
            last_event: "Interact with any page to see events here.".to_string(),
            theme: ThemeMode::Dark,
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct ShowcaseRuntime {
    root: Entity,
    status_label: Entity,
    pages_tab_bar: Entity,
    sidebar_components_btn: Entity,
    sidebar_theming_btn: Entity,
    sidebar_localization_btn: Entity,
    components_combo: Entity,
    dialog_demo_btn: Entity,
    theme_mode_combo: Entity,
    locale_combo: Entity,
    toast_info_btn: Entity,
    toast_success_btn: Entity,
    toast_warning_btn: Entity,
    toast_error_btn: Entity,
    theme_primary_btn: Entity,
    theme_danger_btn: Entity,
    theme_outline_btn: Entity,
}

#[derive(Component, Debug, Clone, Copy, Default)]
struct ShowcaseRoot;

#[derive(Component, Debug, Clone, Copy, Default)]
struct StatusDisplay;

fn parse_locale(tag: &str) -> LanguageIdentifier {
    tag.parse()
        .unwrap_or_else(|_| panic!("locale `{tag}` should parse"))
}

fn cjk_fallback_font_stack() -> Vec<&'static str> {
    vec![
        "Inter",
        "Noto Sans CJK SC",
        "NotoSansCJKsc",
        "Noto Sans CJK JP",
        "NotoSansCJKjp",
        "PingFang SC",
        "Hiragino Sans",
        "Apple SD Gothic Neo",
        "sans-serif",
    ]
}

fn zh_cjk_fallback_font_stack() -> Vec<&'static str> {
    vec![
        "Inter",
        "Noto Sans CJK SC",
        "NotoSansCJKsc",
        "Noto Sans CJK JP",
        "NotoSansCJKjp",
        "PingFang SC",
        "Hiragino Sans",
        "Apple SD Gothic Neo",
        "sans-serif",
    ]
}

fn ja_cjk_fallback_font_stack() -> Vec<&'static str> {
    vec![
        "Inter",
        "Noto Sans CJK JP",
        "NotoSansCJKjp",
        "Noto Sans CJK SC",
        "NotoSansCJKsc",
        "Hiragino Sans",
        "PingFang SC",
        "Apple SD Gothic Neo",
        "sans-serif",
    ]
}

fn root_classes(theme: ThemeMode) -> StyleClass {
    StyleClass(vec![
        "showcase.root".to_string(),
        theme.class_name().to_string(),
    ])
}

fn project_showcase_root(_: &ShowcaseRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_col(children)
            .gap(Length::px(14.0))
            .width(Dim::Stretch)
            .height(Dim::Stretch),
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

fn setup_showcase(mut commands: Commands) {
    let root = commands
        .spawn((UiRoot, ShowcaseRoot, root_classes(ThemeMode::Dark)))
        .id();

    commands.spawn((
        UiLabel::new("UI Showcase (Components / Theming / Localization & CJK)"),
        StyleClass(vec!["showcase.title".to_string()]),
        ChildOf(root),
    ));

    let status_label = commands
        .spawn((
            StatusDisplay,
            StyleClass(vec!["showcase.status".to_string()]),
            ChildOf(root),
        ))
        .id();

    let body = commands
        .spawn((
            UiFlexRow,
            StyleClass(vec!["showcase.body".to_string()]),
            ChildOf(root),
        ))
        .id();

    let sidebar = commands
        .spawn((
            UiGroupBox::new("Navigation"),
            StyleClass(vec!["showcase.sidebar".to_string()]),
            ChildOf(body),
        ))
        .id();

    let sidebar_components_btn = commands
        .spawn((
            UiButton::new("Components"),
            StyleClass(vec![
                "showcase.sidebar.button".to_string(),
                "showcase.sidebar.button.active".to_string(),
            ]),
            ChildOf(sidebar),
        ))
        .id();
    let sidebar_theming_btn = commands
        .spawn((
            UiButton::new("Theming"),
            StyleClass(vec!["showcase.sidebar.button".to_string()]),
            ChildOf(sidebar),
        ))
        .id();
    let sidebar_localization_btn = commands
        .spawn((
            UiButton::new("Localization & CJK"),
            StyleClass(vec!["showcase.sidebar.button".to_string()]),
            ChildOf(sidebar),
        ))
        .id();
    commands.spawn((
        UiLabel::new("Use the tabs on the right to switch pages."),
        ChildOf(sidebar),
    ));

    let pages = commands
        .spawn((
            UiTabBar::new(["Components", "Theming", "Localization & CJK"]),
            StyleClass(vec!["showcase.pages".to_string()]),
            ChildOf(body),
        ))
        .id();

    let components_page = commands
        .spawn((
            UiScrollView::new(Vec2::new(760.0, 520.0), Vec2::new(760.0, 3000.0))
                .with_vertical_scrollbar(true)
                .with_horizontal_scrollbar(false),
            StyleClass(vec!["showcase.page.scroll".to_string()]),
            ChildOf(pages),
        ))
        .id();

    let components_col = commands
        .spawn((
            UiFlexColumn,
            StyleClass(vec!["showcase.page.column".to_string()]),
            ChildOf(components_page),
        ))
        .id();

    let radio_section = commands
        .spawn((UiGroupBox::new("Radio Group"), ChildOf(components_col)))
        .id();
    commands.spawn((
        UiRadioGroup::new(["Apple", "Banana", "Cherry", "Date"]),
        ChildOf(radio_section),
    ));

    let tab_section = commands
        .spawn((UiGroupBox::new("Nested Tab Bar"), ChildOf(components_col)))
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

    let tree_section = commands
        .spawn((UiGroupBox::new("Tree View"), ChildOf(components_col)))
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

    let forms_section = commands
        .spawn((UiGroupBox::new("Form Inputs"), ChildOf(components_col)))
        .id();
    let forms_col = commands.spawn((UiFlexColumn, ChildOf(forms_section))).id();
    commands.spawn((
        UiCheckbox::new("Enable desktop notifications", false),
        ChildOf(forms_col),
    ));
    commands.spawn((
        UiSlider::new(0.0, 100.0, 42.0).with_step(5.0),
        ChildOf(forms_col),
    ));
    commands.spawn((
        UiTextInput::new("".to_string()).with_placeholder("Type to edit this field"),
        ChildOf(forms_col),
    ));
    let components_combo = commands
        .spawn((
            UiComboBox::new(vec![
                UiComboOption::new("rust", "Rust"),
                UiComboOption::new("go", "Go"),
                UiComboOption::new("zig", "Zig"),
            ])
            .with_placeholder("Pick a language"),
            ChildOf(forms_col),
        ))
        .id();

    let dialog_section = commands
        .spawn((
            UiGroupBox::new("Dialog (Modal Overlay)"),
            ChildOf(components_col),
        ))
        .id();
    commands.spawn((
        UiLabel::new("Click to open a modal dialog. Dismiss via button or outside click."),
        ChildOf(dialog_section),
    ));
    let dialog_demo_btn = commands
        .spawn((UiButton::new("Open Dialog"), ChildOf(dialog_section)))
        .id();

    let table_section = commands
        .spawn((
            UiGroupBox::new("Table / Data Grid"),
            ChildOf(components_col),
        ))
        .id();
    commands.spawn((
        UiTable::new(["Name", "Role", "Status", "Score"])
            .with_row(["Alice Chen", "Engineer", "Active", "98"])
            .with_row(["Bob Smith", "Designer", "Away", "85"])
            .with_row(["Carol Davis", "Manager", "Active", "91"])
            .with_row(["Dave Wilson", "Lead", "Busy", "88"]),
        ChildOf(table_section),
    ));

    let menu_section = commands
        .spawn((UiGroupBox::new("Menu Bar"), ChildOf(components_col)))
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

    let spinner_section = commands
        .spawn((
            UiGroupBox::new("Spinner / Loading Indicator"),
            ChildOf(components_col),
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

    let color_section = commands
        .spawn((UiGroupBox::new("Color Picker"), ChildOf(components_col)))
        .id();
    commands.spawn((UiColorPicker::new(0x60, 0xA5, 0xFA), ChildOf(color_section)));

    let date_section = commands
        .spawn((UiGroupBox::new("Date Picker"), ChildOf(components_col)))
        .id();
    commands.spawn((UiDatePicker::new(2024, 6, 15), ChildOf(date_section)));

    let split_section = commands
        .spawn((UiGroupBox::new("Split Pane"), ChildOf(components_col)))
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

    let scroll_section = commands
        .spawn((
            UiGroupBox::new("Scroll View (Portal + ECS Scrollbars)"),
            ChildOf(components_col),
        ))
        .id();
    let scroll_view = commands
        .spawn((
            UiScrollView::new(Vec2::new(640.0, 220.0), Vec2::new(640.0, 1600.0))
                .with_vertical_scrollbar(true)
                .with_horizontal_scrollbar(false),
            ChildOf(scroll_section),
        ))
        .id();

    for i in 1..=60 {
        commands.spawn((
            UiLabel::new(format!(
                "Scrollable row #{i:02}  •  Drag the thumb or use mouse wheel"
            )),
            ChildOf(scroll_view),
        ));
    }

    let toast_section = commands
        .spawn((
            UiGroupBox::new("Toast Notifications"),
            ChildOf(components_col),
        ))
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

    let tooltip_section = commands
        .spawn((UiGroupBox::new("Tooltip"), ChildOf(components_col)))
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

    let theming_page = commands
        .spawn((
            UiScrollView::new(Vec2::new(760.0, 520.0), Vec2::new(760.0, 1300.0))
                .with_vertical_scrollbar(true)
                .with_horizontal_scrollbar(false),
            StyleClass(vec!["showcase.page.scroll".to_string()]),
            ChildOf(pages),
        ))
        .id();

    let theming_col = commands
        .spawn((
            UiFlexColumn,
            StyleClass(vec!["showcase.page.column".to_string()]),
            ChildOf(theming_page),
        ))
        .id();

    let theme_mode_section = commands
        .spawn((UiGroupBox::new("Theme Mode"), ChildOf(theming_col)))
        .id();
    let theme_mode_combo = commands
        .spawn((
            UiComboBox::new(vec![
                UiComboOption::new("dark", "Dark"),
                UiComboOption::new("light", "Light"),
            ])
            .with_placeholder("Choose dark/light theme"),
            StyleClass(vec!["showcase.theme.combo".to_string()]),
            ChildOf(theme_mode_section),
        ))
        .id();

    let theme_buttons_section = commands
        .spawn((UiGroupBox::new("Button Styles"), ChildOf(theming_col)))
        .id();
    let theme_buttons_row = commands
        .spawn((UiFlexRow, ChildOf(theme_buttons_section)))
        .id();
    let theme_primary_btn = commands
        .spawn((
            UiButton::new("Primary"),
            StyleClass(vec![
                "showcase.theme.button".to_string(),
                "showcase.theme.button.primary".to_string(),
            ]),
            ChildOf(theme_buttons_row),
        ))
        .id();
    let theme_danger_btn = commands
        .spawn((
            UiButton::new("Danger"),
            StyleClass(vec![
                "showcase.theme.button".to_string(),
                "showcase.theme.button.danger".to_string(),
            ]),
            ChildOf(theme_buttons_row),
        ))
        .id();
    let theme_outline_btn = commands
        .spawn((
            UiButton::new("Outline"),
            StyleClass(vec![
                "showcase.theme.button".to_string(),
                "showcase.theme.button.outline".to_string(),
            ]),
            ChildOf(theme_buttons_row),
        ))
        .id();

    let transitions_section = commands
        .spawn((
            UiGroupBox::new("Theme + Transition Notes"),
            ChildOf(theming_col),
        ))
        .id();
    commands.spawn((
        UiLabel::new("• Root background transitions animate on theme change."),
        ChildOf(transitions_section),
    ));
    commands.spawn((
        UiLabel::new("• Button hover/press states are style-driven."),
        ChildOf(transitions_section),
    ));
    commands.spawn((
        UiLabel::new("• Theme page content was merged from the old theme gallery."),
        ChildOf(transitions_section),
    ));

    let localization_page = commands
        .spawn((
            UiScrollView::new(Vec2::new(760.0, 520.0), Vec2::new(760.0, 1300.0))
                .with_vertical_scrollbar(true)
                .with_horizontal_scrollbar(false),
            StyleClass(vec!["showcase.page.scroll".to_string()]),
            ChildOf(pages),
        ))
        .id();

    let localization_col = commands
        .spawn((
            UiFlexColumn,
            StyleClass(vec!["showcase.page.column".to_string()]),
            ChildOf(localization_page),
        ))
        .id();

    let locale_section = commands
        .spawn((UiGroupBox::new("Locale"), ChildOf(localization_col)))
        .id();
    let locale_combo = commands
        .spawn((
            UiComboBox::new(vec![
                UiComboOption::new("en-US", "English"),
                UiComboOption::new("zh-CN", "简体中文"),
                UiComboOption::new("ja-JP", "日本語"),
            ])
            .with_placeholder("Language"),
            StyleClass(vec!["showcase.locale.combo".to_string()]),
            ChildOf(locale_section),
        ))
        .id();

    let i18n_section = commands
        .spawn((
            UiGroupBox::new("Localized Strings"),
            ChildOf(localization_col),
        ))
        .id();
    commands.spawn((
        UiLabel::new("hello_world"),
        LocalizeText::new("hello_world"),
        StyleClass(vec!["showcase.locale.title".to_string()]),
        ChildOf(i18n_section),
    ));
    commands.spawn((
        UiLabel::new("han_unification_test"),
        LocalizeText::new("han_unification_test"),
        StyleClass(vec!["showcase.locale.han".to_string()]),
        ChildOf(i18n_section),
    ));

    let cjk_section = commands
        .spawn((
            UiGroupBox::new("CJK Font Bridge"),
            ChildOf(localization_col),
        ))
        .id();
    commands.spawn((
        UiLabel::new("Xilem Client: 骨 (SC) and 骨 (JP/TC variants if applicable), こんにちは!"),
        StyleClass(vec!["showcase.cjk.text".to_string()]),
        ChildOf(cjk_section),
    ));
    commands.spawn((
        UiLabel::new("Fallback stack: Inter → Noto Sans CJK SC → Noto Sans CJK JP → sans-serif"),
        StyleClass(vec!["showcase.cjk.text".to_string()]),
        ChildOf(cjk_section),
    ));

    commands.insert_resource(ShowcaseRuntime {
        root,
        status_label,
        pages_tab_bar: pages,
        sidebar_components_btn,
        sidebar_theming_btn,
        sidebar_localization_btn,
        components_combo,
        dialog_demo_btn,
        theme_mode_combo,
        locale_combo,
        toast_info_btn,
        toast_success_btn,
        toast_warning_btn,
        toast_error_btn,
        theme_primary_btn,
        theme_danger_btn,
        theme_outline_btn,
    });
}

fn setup_showcase_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "showcase.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(18.0),
                gap: Some(12.0),
                ..LayoutStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.22 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "theme.light",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xF4, 0xF7, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.title",
        StyleSetter {
            text: TextStyle {
                size: Some(24.0),
                ..Default::default()
            },
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
            text: TextStyle {
                size: Some(13.0),
                ..Default::default()
            },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0x94, 0xA3, 0xB8)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.body",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(12.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.sidebar",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                gap: Some(8.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(255, 255, 255, 10)),
                border: Some(Color::from_rgb8(0x3D, 0x4D, 0x70)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.sidebar.button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                corner_radius: Some(6.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(255, 255, 255, 18)),
                hover_bg: Some(Color::from_rgba8(255, 255, 255, 34)),
                pressed_bg: Some(Color::from_rgba8(255, 255, 255, 42)),
                border: Some(Color::from_rgb8(0x4B, 0x5C, 0x82)),
                text: Some(Color::from_rgb8(0xE8, 0xEE, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.sidebar.button.active",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2F, 0x66, 0xE5)),
                hover_bg: Some(Color::from_rgb8(0x3C, 0x73, 0xF1)),
                pressed_bg: Some(Color::from_rgb8(0x25, 0x57, 0xC5)),
                border: Some(Color::from_rgb8(0x2F, 0x66, 0xE5)),
                text: Some(Color::from_rgb8(0xF8, 0xFB, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.pages",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(4.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.page.scroll",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgba8(0x00, 0x00, 0x00, 0x20)),
                border: Some(Color::from_rgb8(0x39, 0x49, 0x6C)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.page.column",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                gap: Some(10.0),
                ..LayoutStyle::default()
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

    style_sheet.set_class(
        "showcase.theme.combo",
        StyleSetter {
            text: TextStyle {
                size: Some(15.0),
                ..Default::default()
            },
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
        "showcase.theme.button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.theme.button.primary",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2B, 0x6C, 0xF2)),
                hover_bg: Some(Color::from_rgb8(0x3C, 0x7B, 0xFB)),
                pressed_bg: Some(Color::from_rgb8(0x1D, 0x56, 0xD6)),
                border: Some(Color::from_rgb8(0x2B, 0x6C, 0xF2)),
                text: Some(Color::from_rgb8(0xF8, 0xFB, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.theme.button.danger",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xD6, 0x45, 0x45)),
                hover_bg: Some(Color::from_rgb8(0xE2, 0x5A, 0x5A)),
                pressed_bg: Some(Color::from_rgb8(0xAF, 0x2F, 0x2F)),
                border: Some(Color::from_rgb8(0xD6, 0x45, 0x45)),
                text: Some(Color::from_rgb8(0xFF, 0xF1, 0xF1)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.theme.button.outline",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::TRANSPARENT),
                hover_bg: Some(Color::from_rgba8(0x4E, 0x66, 0x99, 0x30)),
                pressed_bg: Some(Color::from_rgba8(0x3E, 0x56, 0x85, 0x40)),
                border: Some(Color::from_rgb8(0x58, 0x71, 0xA6)),
                text: Some(Color::from_rgb8(0xE3, 0xEB, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.locale.combo",
        StyleSetter {
            text: TextStyle {
                size: Some(16.0),
                ..Default::default()
            },
            layout: LayoutStyle {
                padding: Some(10.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1F, 0x2A, 0x40)),
                hover_bg: Some(Color::from_rgb8(0x27, 0x35, 0x52)),
                pressed_bg: Some(Color::from_rgb8(0x17, 0x23, 0x38)),
                border: Some(Color::from_rgb8(0x4A, 0x5F, 0x8A)),
                text: Some(Color::from_rgb8(0xDE, 0xE8, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.locale.title",
        StyleSetter {
            text: TextStyle {
                size: Some(26.0),
                ..Default::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.locale.han",
        StyleSetter {
            text: TextStyle {
                size: Some(44.0),
                ..Default::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "showcase.cjk.text",
        StyleSetter {
            text: TextStyle {
                size: Some(24.0),
                ..Default::default()
            },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xF3, 0xF7, 0xFF)),
                ..ColorStyle::default()
            },
            font_family: Some(vec![
                "Inter".into(),
                "Noto Sans CJK SC".into(),
                "NotoSansCJKsc".into(),
                "Noto Sans CJK JP".into(),
                "NotoSansCJKjp".into(),
                "PingFang SC".into(),
                "Hiragino Sans".into(),
                "Apple SD Gothic Neo".into(),
                "sans-serif".into(),
            ]),
            ..StyleSetter::default()
        },
    );
}

fn drain_showcase_events(world: &mut World) {
    let rt = match world.get_resource::<ShowcaseRuntime>() {
        Some(rt) => *rt,
        None => return,
    };

    let builtin_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<BuiltinUiAction>();

    for event in builtin_events {
        if !matches!(event.action, BuiltinUiAction::Clicked) {
            continue;
        }

        if event.entity == rt.sidebar_components_btn {
            set_showcase_page(world, rt, 0);
            update_status(
                world,
                rt.status_label,
                "Sidebar: switched to Components".to_string(),
            );
        } else if event.entity == rt.sidebar_theming_btn {
            set_showcase_page(world, rt, 1);
            update_status(
                world,
                rt.status_label,
                "Sidebar: switched to Theming".to_string(),
            );
        } else if event.entity == rt.sidebar_localization_btn {
            set_showcase_page(world, rt, 2);
            update_status(
                world,
                rt.status_label,
                "Sidebar: switched to Localization & CJK".to_string(),
            );
        } else if event.entity == rt.toast_info_btn {
            spawn_in_overlay_root(
                world,
                (
                    UiToast::new("Info: Components page interaction successful.")
                        .with_duration(2.8)
                        .with_min_width(260.0)
                        .with_max_width(440.0),
                ),
            );
        } else if event.entity == rt.toast_success_btn {
            spawn_in_overlay_root(
                world,
                (UiToast::new("Success: UI action completed.")
                    .with_kind(ToastKind::Success)
                    .with_duration(2.4)
                    .with_show_close_button(false),),
            );
        } else if event.entity == rt.toast_warning_btn {
            spawn_in_overlay_root(
                world,
                (UiToast::new("Warning: Double-check this config.")
                    .with_kind(ToastKind::Warning)
                    .with_duration(3.6),),
            );
        } else if event.entity == rt.toast_error_btn {
            spawn_in_overlay_root(
                world,
                (UiToast::new("Error: Simulated failure toast.")
                    .with_kind(ToastKind::Error)
                    .with_duration(0.0)
                    .with_min_width(320.0),),
            );
        } else if event.entity == rt.theme_primary_btn {
            update_status(
                world,
                rt.status_label,
                "Theme demo: Primary pressed".to_string(),
            );
        } else if event.entity == rt.theme_danger_btn {
            update_status(
                world,
                rt.status_label,
                "Theme demo: Danger pressed".to_string(),
            );
        } else if event.entity == rt.theme_outline_btn {
            update_status(
                world,
                rt.status_label,
                "Theme demo: Outline pressed".to_string(),
            );
        } else if event.entity == rt.dialog_demo_btn {
            spawn_in_overlay_root(
                world,
                (UiDialog::new(
                    "Modal Dialog Demo",
                    "This UiDialog is rendered in the overlay layer.\n\nTry dismissing it via the close button or by clicking outside.",
                ),),
            );
            update_status(
                world,
                rt.status_label,
                "Dialog demo: opened modal overlay".to_string(),
            );
        }
    }

    let checkbox_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiCheckboxChanged>();
    for event in checkbox_events {
        let msg = format!(
            "Checkbox {:?}: {}",
            event.action.checkbox,
            if event.action.checked {
                "checked"
            } else {
                "unchecked"
            }
        );
        update_status(world, rt.status_label, msg);
    }

    let radio_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiRadioGroupChanged>();
    for event in radio_events {
        let msg = format!("Radio: selected option index {}", event.action.selected);
        update_status(world, rt.status_label, msg);
    }

    let tab_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiTabChanged>();
    for event in tab_events {
        if event.action.bar == rt.pages_tab_bar {
            set_showcase_page(world, rt, event.action.active);
        }
        let msg = format!("Tab: switched to index {}", event.action.active);
        update_status(world, rt.status_label, msg);
    }

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

    let menu_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiMenuItemSelected>();
    for event in menu_events {
        let msg = format!("Menu: selected \"{}\"", event.action.value);
        update_status(world, rt.status_label, msg);
    }

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

    let scroll_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiScrollViewChanged>();
    for event in scroll_events {
        let msg = format!(
            "Scroll View {:?}: offset=({:.1}, {:.1})",
            event.action.scroll_view, event.action.scroll_offset.x, event.action.scroll_offset.y
        );
        update_status(world, rt.status_label, msg);
    }

    let slider_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiSliderChanged>();
    for event in slider_events {
        let msg = format!(
            "Slider {:?}: value={:.2}",
            event.action.slider, event.action.value
        );
        update_status(world, rt.status_label, msg);
    }

    let text_input_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiTextInputChanged>();
    for event in text_input_events {
        let msg = format!(
            "TextInput {:?}: \"{}\"",
            event.action.input, event.action.value
        );
        update_status(world, rt.status_label, msg);
    }

    let combo_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiComboBoxChanged>();

    for event in combo_events {
        if event.action.combo == rt.components_combo {
            update_status(
                world,
                rt.status_label,
                format!(
                    "Components Combo: selected {} ({})",
                    event.action.selected, event.action.value
                ),
            );
            continue;
        }

        if event.action.combo == rt.theme_mode_combo {
            if let Some(theme) = ThemeMode::from_combo_value(event.action.value.as_str()) {
                world.resource_mut::<ShowcaseState>().theme = theme;
                world.entity_mut(rt.root).insert(root_classes(theme));
                update_status(
                    world,
                    rt.status_label,
                    format!("Theme switched to {}", event.action.value),
                );
            }
            continue;
        }

        if event.action.combo == rt.locale_combo {
            let next_locale = parse_locale(event.action.value.as_str());
            world
                .resource_mut::<AppI18n>()
                .set_active_locale(next_locale.clone());
            update_status(
                world,
                rt.status_label,
                format!("Locale switched to {}", next_locale),
            );
            continue;
        }
    }
}

fn update_status(world: &mut World, _label_entity: Entity, text: String) {
    if let Some(mut state) = world.get_resource_mut::<ShowcaseState>() {
        state.last_event = text;
    }
}

fn set_showcase_page(world: &mut World, rt: ShowcaseRuntime, page: usize) {
    if let Some(mut tabs) = world.get_mut::<UiTabBar>(rt.pages_tab_bar) {
        tabs.active = page.min(tabs.tabs.len().saturating_sub(1));
    }

    let buttons = [
        (rt.sidebar_components_btn, 0usize),
        (rt.sidebar_theming_btn, 1usize),
        (rt.sidebar_localization_btn, 2usize),
    ];

    for (button, index) in buttons {
        let classes = if index == page {
            vec![
                "showcase.sidebar.button".to_string(),
                "showcase.sidebar.button.active".to_string(),
            ]
        } else {
            vec!["showcase.sidebar.button".to_string()]
        };
        world.entity_mut(button).insert(StyleClass(classes));
    }
}

bevy_xilem::impl_ui_component_template!(ShowcaseRoot, project_showcase_root);
bevy_xilem::impl_ui_component_template!(StatusDisplay, project_status_display);

fn build_showcase_app() -> App {
    init_logging();

    let mut app = App::new();
    app.add_plugins((
        EmbeddedAssetPlugin {
            mode: PluginMode::ReplaceDefault,
        },
        AssetPlugin::default(),
        TextPlugin::default(),
        BevyXilemPlugin,
    ))
    .insert_resource(AppI18n::new(parse_locale("en-US")))
    .register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSans-Regular.ttf",
    ))
    .register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSansCJKsc-Regular.otf",
    ))
    .register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSansCJKjp-Regular.otf",
    ))
    .register_i18n_bundle(
        "en-US",
        SyncTextSource::FilePath("assets/locales/en-US/main.ftl"),
        cjk_fallback_font_stack(),
    )
    .register_i18n_bundle(
        "zh-CN",
        SyncTextSource::FilePath("assets/locales/zh-CN/main.ftl"),
        zh_cjk_fallback_font_stack(),
    )
    .register_i18n_bundle(
        "ja-JP",
        SyncTextSource::FilePath("assets/locales/ja-JP/main.ftl"),
        ja_cjk_fallback_font_stack(),
    )
    .insert_resource(ShowcaseState::default())
    .register_ui_component::<ShowcaseRoot>()
    .register_ui_component::<StatusDisplay>()
    .add_systems(Startup, (setup_showcase_styles, setup_showcase))
    .add_systems(PreUpdate, drain_showcase_events);

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_showcase_app(), "UI Showcase", |options| {
        options.with_initial_inner_size(LogicalSize::new(1220.0, 780.0))
    })
}
