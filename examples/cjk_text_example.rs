mod utils;

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, StyleClass, StyleSetter, StyleSheet,
    TextStyle, UiFlexColumn, UiLabel, UiRoot,
    bevy_app::{App, Startup},
    bevy_asset::{AssetPlugin, AssetServer, Handle},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    bevy_tasks::{IoTaskPool, TaskPool},
    bevy_text::{Font, TextPlugin},
    run_app_with_window_options,
    xilem::{
        Color,
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use utils::init_logging;

#[derive(Resource, Default)]
struct DemoFontHandles {
    handles: Vec<Handle<Font>>,
}

fn ensure_task_pool_initialized() {
    let _ = IoTaskPool::get_or_init(TaskPool::new);
}

fn register_bridge_fonts(app: &mut App) {
    app.register_xilem_font_path("assets/fonts/Inter-Regular.otf")
        .expect("failed to register Inter-Regular.otf into Xilem font bridge");
    app.register_xilem_font_path("assets/fonts/NotoSansCJKsc-Regular.otf")
        .expect("failed to register NotoSansCJKsc-Regular.otf into Xilem font bridge");
    app.register_xilem_font_path("assets/fonts/NotoSansCJKjp-Regular.otf")
        .expect("failed to register NotoSansCJKjp-Regular.otf into Xilem font bridge");
}

fn load_demo_fonts(asset_server: Res<AssetServer>, mut font_handles: ResMut<DemoFontHandles>) {
    if !font_handles.handles.is_empty() {
        return;
    }

    font_handles
        .handles
        .push(asset_server.load("fonts/Inter-Regular.otf"));
    font_handles
        .handles
        .push(asset_server.load("fonts/NotoSansCJKsc-Regular.otf"));
    font_handles
        .handles
        .push(asset_server.load("fonts/NotoSansCJKjp-Regular.otf"));
}

fn setup_cjk_world(mut commands: Commands) {
    let root = commands
        .spawn((
            UiRoot,
            UiFlexColumn,
            StyleClass(vec!["cjk.root".to_string()]),
        ))
        .id();

    commands.spawn((
        UiLabel::new("bevy_xilem Font Bridge (CJK)"),
        StyleClass(vec!["cjk.title".to_string()]),
        ChildOf(root),
    ));

    commands.spawn((
        UiLabel::new("Xilem Client: 骨 (SC) and 骨 (JP/TC variants if applicable), こんにちは!"),
        StyleClass(vec!["cjk-text".to_string()]),
        ChildOf(root),
    ));

    commands.spawn((
        UiLabel::new(
            "Fallback stack check: Inter → Noto Sans CJK SC → Noto Sans CJK JP → sans-serif",
        ),
        StyleClass(vec!["cjk-text".to_string()]),
        ChildOf(root),
    ));
}

fn setup_cjk_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "cjk.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(20.0),
                gap: Some(10.0),
                corner_radius: Some(12.0),
                border_width: Some(1.0),
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x16, 0x1B, 0x28)),
                border: Some(Color::from_rgb8(0x2F, 0x3F, 0x5D)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "cjk.title",
        StyleSetter {
            text: TextStyle { size: Some(22.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xE3, 0xED, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "cjk-text",
        StyleSetter {
            text: TextStyle { size: Some(26.0) },
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

fn build_cjk_app() -> App {
    init_logging();

    ensure_task_pool_initialized();

    let mut app = App::new();
    register_bridge_fonts(&mut app);

    app.add_plugins((
        AssetPlugin::default(),
        TextPlugin::default(),
        BevyXilemPlugin,
    ))
    .load_style_sheet("themes/default_theme.ron")
    .init_resource::<DemoFontHandles>()
    .add_systems(
        Startup,
        (setup_cjk_styles, setup_cjk_world, load_demo_fonts),
    );

    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_cjk_app(), "CJK Font Bridge", |options| {
        options.with_initial_inner_size(LogicalSize::new(920.0, 420.0))
    })
}
