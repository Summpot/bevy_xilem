use std::{f32::consts::PI, process::Command, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use bevy_asset::{AssetPlugin, Assets, Handle, RenderAssetUsages};
use bevy_embedded_assets::{EmbeddedAssetPlugin, PluginMode};
use bevy_image::Image as BevyImage;
use bevy_text::TextPlugin;
use bevy_xilem::{
    AppBevyXilemExt, AppI18n, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx,
    ResolvedStyle, StyleClass, StyleSetter, StyleSheet, StyleTransition, SyncAssetSource,
    SyncTextSource, TextStyle, UiComboBox, UiComboBoxChanged, UiComboOption, UiEventQueue, UiRoot,
    UiView, apply_label_style, apply_text_input_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup, Update},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    bevy_tasks::{AsyncComputeTaskPool, IoTaskPool, TaskPool},
    bevy_tweening::{EaseMethod, Lens, Tween, TweenAnim},
    bevy_window::WindowResized,
    button, resolve_style, resolve_style_for_classes, resolve_style_for_entity_classes,
    run_app_with_window_options, text_input,
    xilem::{
        Color,
        masonry::layout::{Dim, Length},
        style::Style as _,
        view::{
            CrossAxisAlignment, FlexExt as _, MainAxisAlignment, flex_col, flex_row, image, label,
            portal, sized_box, virtual_scroll,
        },
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use pixiv_client::{
    AuthSession, DecodedImageRgba, IdpUrlResponse, Illust, PixivApiClient, PixivResponse,
    build_browser_login_url, generate_pkce_code_verifier, pkce_s256_challenge,
};
use reqwest::Url;
use unic_langid::LanguageIdentifier;
use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

const CARD_BASE_WIDTH: f64 = 270.0;
const CARD_BASE_HEIGHT: f64 = 310.0;
const CARD_MIN_WIDTH: f64 = 260.0;
const CARD_ROW_GAP: f64 = 10.0;
const MAX_CARD_COLUMNS: usize = 6;
const SIDEBAR_EXPANDED_WIDTH: f64 = 208.0;
const SIDEBAR_COLLAPSED_WIDTH: f64 = 72.0;
const RESPONSE_PANEL_HEIGHT: f64 = 180.0;
const PIXIV_AUTH_TOKEN_FALLBACK: &str = "https://oauth.secure.pixiv.net/auth/token";
const PIXIV_WEB_REDIRECT_FALLBACK: &str =
    "https://app-api.pixiv.net/web/v1/users/auth/pixiv/callback";

fn parse_locale(tag: &str) -> LanguageIdentifier {
    tag.parse()
        .unwrap_or_else(|_| panic!("locale `{tag}` should parse"))
}

fn locale_badge(locale: &LanguageIdentifier) -> &'static str {
    if locale.language.as_str() == "ja" {
        "日本語"
    } else if locale.language.as_str() == "zh"
        && locale
            .region
            .is_some_and(|region| region.as_str().eq_ignore_ascii_case("CN"))
    {
        "简体中文"
    } else {
        "English"
    }
}

fn tr(world: &World, key: &str, fallback: &str) -> String {
    let Some(i18n) = world.get_resource::<AppI18n>() else {
        return fallback.to_string();
    };

    let translated = i18n.translate(key);
    if translated != key {
        return translated;
    }

    if key.contains('.') {
        let normalized = key.replace('.', "-");
        let normalized_translated = i18n.translate(normalized.as_str());
        if normalized_translated != normalized {
            return normalized_translated;
        }
    }

    fallback.to_string()
}

fn set_status(world: &mut World, message: impl Into<String>) {
    world.resource_mut::<UiState>().status_line = message.into();
}

fn set_status_key(world: &mut World, key: &str, fallback: &str) {
    let message = {
        let world_ref: &World = world;
        tr(world_ref, key, fallback)
    };
    set_status(world, message);
}

fn sync_font_stack_for_locale(sheet: &mut StyleSheet, stack: Option<&[String]>) {
    for class_name in [
        "pixiv.root",
        "pixiv.button",
        "pixiv.primary-btn",
        "pixiv.card",
        "pixiv.tag",
        "pixiv.overlay",
    ] {
        if let Some(existing) = sheet.get_class(class_name).cloned() {
            let mut updated = existing;
            updated.font_family = stack.map(|stack| stack.to_vec());
            sheet.set_class(class_name, updated);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavTab {
    Home,
    Rankings,
    Search,
}

impl Default for NavTab {
    fn default() -> Self {
        Self::Home
    }
}

#[derive(Resource, Debug, Clone, Default)]
struct UiState {
    active_tab: NavTab,
    sidebar_collapsed: bool,
    search_text: String,
    selected_illust: Option<Entity>,
    status_line: String,
}

#[derive(Resource, Debug, Clone, Default)]
struct AuthState {
    idp_urls: Option<IdpUrlResponse>,
    session: Option<AuthSession>,
    code_verifier_input: String,
    auth_code_input: String,
    refresh_token_input: String,
}

#[derive(Resource, Default)]
struct FeedOrder(Vec<Entity>);

#[derive(Resource, Default)]
struct OverlayTags(Vec<Entity>);

#[derive(Resource, Debug, Clone, Default)]
struct ResponsePanelState {
    title: String,
    content: String,
}

#[derive(Resource, Debug, Clone, Copy)]
struct ViewportMetrics {
    width: f32,
    height: f32,
}

impl Default for ViewportMetrics {
    fn default() -> Self {
        Self {
            width: 1360.0,
            height: 860.0,
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct PixivControls {
    toggle_sidebar: Entity,
    locale_combo: Entity,
    home_tab: Entity,
    rankings_tab: Entity,
    search_tab: Entity,
    open_browser_login: Entity,
    exchange_auth_code: Entity,
    refresh_token: Entity,
    search_submit: Entity,
    copy_response: Entity,
    clear_response: Entity,
    close_overlay: Entity,
}

#[derive(Resource, Debug, Clone, Copy)]
struct PixivUiTree {
    home_feed: Entity,
    overlay_tags: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
struct PixivRoot;

#[derive(Component, Debug, Clone, Copy)]
struct PixivSidebar;

#[derive(Component, Debug, Clone, Copy)]
struct PixivMainColumn;

#[derive(Component, Debug, Clone, Copy)]
struct PixivAuthPanel;

#[derive(Component, Debug, Clone, Copy)]
struct PixivResponsePanel;

#[derive(Component, Debug, Clone, Copy)]
struct PixivSearchPanel;

#[derive(Component, Debug, Clone, Copy)]
struct PixivHomeFeed;

#[derive(Component, Debug, Clone, Copy)]
struct PixivIllustCard;

#[derive(Component, Debug, Clone, Copy)]
struct PixivDetailOverlay;

#[derive(Component, Debug, Clone, Copy)]
struct PixivOverlayTags;

#[derive(Component, Debug, Clone)]
struct OverlayTag {
    text: String,
}

#[derive(Component, Debug, Clone)]
struct IllustVisual {
    thumb_ui: Option<ImageData>,
    avatar_ui: Option<ImageData>,
    high_res_ui: Option<ImageData>,
    thumb_handle: Option<Handle<BevyImage>>,
    avatar_handle: Option<Handle<BevyImage>>,
    high_res_handle: Option<Handle<BevyImage>>,
}

impl Default for IllustVisual {
    fn default() -> Self {
        Self {
            thumb_ui: None,
            avatar_ui: None,
            high_res_ui: None,
            thumb_handle: None,
            avatar_handle: None,
            high_res_handle: None,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct CardAnimState {
    card_scale: f32,
    image_brightness: f32,
    heart_scale: f32,
}

impl Default for CardAnimState {
    fn default() -> Self {
        Self {
            card_scale: 1.0,
            image_brightness: 1.0,
            heart_scale: 1.0,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CardHoverFlag(bool);

#[derive(Debug, Clone, Copy)]
enum ImageKind {
    Thumb,
    Avatar,
    HighRes,
}

#[derive(Debug, Clone)]
enum AppAction {
    ToggleSidebar,
    SetTab(NavTab),
    SetSearchText(String),
    SubmitSearch,
    OpenIllust(Entity),
    CloseIllust,
    Bookmark(Entity),
    SearchByTag(String),
    SetAuthCode(String),
    SetCodeVerifier(String),
    SetRefreshToken(String),
    CopyResponseBody,
    ClearResponseBody,
    OpenBrowserLogin,
    ExchangeAuthCode,
    RefreshToken,
}

#[derive(Debug, Clone)]
enum NetworkCommand {
    DiscoverIdp,
    ExchangeCode { code: String, code_verifier: String },
    Refresh { refresh_token: String },
    FetchHome,
    FetchRanking,
    Search { word: String },
    Bookmark { illust_id: u64 },
}

#[derive(Debug, Clone)]
enum NetworkResult {
    IdpDiscovered(IdpUrlResponse),
    Authenticated(AuthSession),
    FeedLoaded {
        source: NavTab,
        payload: PixivResponse,
    },
    BookmarkDone {
        illust_id: u64,
    },
    Error {
        summary: String,
        details: String,
    },
}

#[derive(Debug, Clone)]
enum ImageCommand {
    Download {
        entity: Entity,
        kind: ImageKind,
        url: String,
    },
}

#[derive(Debug, Clone)]
enum ImageResult {
    Loaded {
        entity: Entity,
        kind: ImageKind,
        decoded: DecodedImageRgba,
    },
    Failed {
        entity: Entity,
        kind: ImageKind,
        error: String,
    },
}

#[derive(Resource)]
struct NetworkBridge {
    cmd_tx: Sender<NetworkCommand>,
    cmd_rx: Receiver<NetworkCommand>,
    result_tx: Sender<NetworkResult>,
    result_rx: Receiver<NetworkResult>,
}

#[derive(Resource)]
struct ImageBridge {
    cmd_tx: Sender<ImageCommand>,
    cmd_rx: Receiver<ImageCommand>,
    result_tx: Sender<ImageResult>,
    result_rx: Receiver<ImageResult>,
}

#[derive(Clone, Copy)]
struct CardAnimLens {
    start: CardAnimState,
    end: CardAnimState,
}

impl Lens<CardAnimState> for CardAnimLens {
    fn lerp(&mut self, mut target: Mut<'_, CardAnimState>, ratio: f32) {
        target.card_scale =
            self.start.card_scale + (self.end.card_scale - self.start.card_scale) * ratio;
        target.image_brightness = self.start.image_brightness
            + (self.end.image_brightness - self.start.image_brightness) * ratio;
        target.heart_scale =
            self.start.heart_scale + (self.end.heart_scale - self.start.heart_scale) * ratio;
    }
}

fn spawn_card_tween(
    world: &mut World,
    entity: Entity,
    start: CardAnimState,
    end: CardAnimState,
    duration_ms: u64,
    ease: EaseMethod,
) {
    let tween = Tween::new::<CardAnimState, _>(
        ease,
        Duration::from_millis(duration_ms),
        CardAnimLens { start, end },
    );
    world.entity_mut(entity).insert(TweenAnim::new(tween));
}

fn ensure_task_pool_initialized() {
    let _ = IoTaskPool::get_or_init(TaskPool::new);
    let _ = AsyncComputeTaskPool::get_or_init(TaskPool::new);
}

fn register_bridge_fonts(app: &mut App) {
    app.register_xilem_font(SyncAssetSource::FilePath("assets/fonts/Inter-Regular.otf"));
    app.register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSansCJKsc-Regular.otf",
    ));
    app.register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSansCJKjp-Regular.otf",
    ));
    app.register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSansCJKtc-Regular.otf",
    ));
    app.register_xilem_font(SyncAssetSource::FilePath(
        "assets/fonts/NotoSansCJKkr-Regular.otf",
    ));
}

fn ease_quadratic_in_out(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - ((-2.0 * t + 2.0).powi(2) / 2.0)
    }
}

fn ease_elastic_out(t: f32) -> f32 {
    if t == 0.0 {
        return 0.0;
    }
    if t == 1.0 {
        return 1.0;
    }
    let c4 = (2.0 * PI) / 3.0;
    2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}

fn extract_code_from_url(url: &Url, depth: u8) -> Option<String> {
    if depth == 0 {
        return None;
    }

    if let Some((_, code)) = url
        .query_pairs()
        .find(|(key, value)| key == "code" && !value.is_empty())
    {
        return Some(code.into_owned());
    }

    for (key, value) in url.query_pairs() {
        if matches!(key.as_ref(), "return_to" | "redirect" | "redirect_uri")
            && let Ok(nested_url) = Url::parse(value.as_ref())
            && let Some(code) = extract_code_from_url(&nested_url, depth - 1)
        {
            return Some(code);
        }
    }

    None
}

fn extract_auth_code_from_input(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(url) = Url::parse(trimmed) {
        if let Some(code) = extract_code_from_url(&url, 4) {
            return Some(code);
        }
        return None;
    }

    Some(trimmed.to_string())
}

fn summarize_error(details: &str) -> String {
    let first = details
        .lines()
        .next()
        .unwrap_or("network request failed")
        .trim();
    let mut summary = first.to_string();
    if summary.len() > 140 {
        summary.truncate(140);
        summary.push('…');
    }
    summary
}

fn open_in_system_browser(url: &str) -> Result<()> {
    if webbrowser::open(url).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg(url)
            .status()
            .context("failed to run `open`")?;
        if status.success() {
            return Ok(());
        }
        return Err(anyhow::anyhow!("`open` exited with status {status}"));
    }

    #[cfg(target_os = "linux")]
    {
        let status = Command::new("xdg-open")
            .arg(url)
            .status()
            .context("failed to run `xdg-open`")?;
        if status.success() {
            return Ok(());
        }
        return Err(anyhow::anyhow!("`xdg-open` exited with status {status}"));
    }

    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
            .context("failed to run `cmd /C start`")?;
        if status.success() {
            return Ok(());
        }
        return Err(anyhow::anyhow!(
            "`cmd /C start` exited with status {status}"
        ));
    }

    #[allow(unreachable_code)]
    Err(anyhow::anyhow!(
        "no browser launcher available on this platform"
    ))
}

fn spawn_control_entity(commands: &mut Commands, classes: &[&str]) -> Entity {
    commands
        .spawn((StyleClass(
            classes.iter().map(|class| (*class).to_string()).collect(),
        ),))
        .id()
}

fn compute_feed_layout(viewport_width: f64, sidebar_collapsed: bool) -> (usize, f64) {
    let sidebar_width = if sidebar_collapsed {
        SIDEBAR_COLLAPSED_WIDTH
    } else {
        SIDEBAR_EXPANDED_WIDTH
    };

    let available_width = (viewport_width - sidebar_width - 64.0).max(CARD_MIN_WIDTH);
    let columns = ((available_width / CARD_MIN_WIDTH).floor() as usize).clamp(1, MAX_CARD_COLUMNS);
    let spacing = CARD_ROW_GAP * columns.saturating_sub(1) as f64;
    let card_width = ((available_width - spacing) / columns as f64).max(180.0);

    (columns, card_width)
}

fn button_from_style(
    entity: Entity,
    action: AppAction,
    label_text: impl Into<String>,
    style: &ResolvedStyle,
) -> UiView {
    let text_color = style.colors.text.unwrap_or(Color::WHITE);
    Arc::new(
        button(entity, action, label_text.into())
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

fn action_button(
    world: &World,
    entity: Entity,
    action: AppAction,
    label_text: impl Into<String>,
) -> UiView {
    let style = resolve_style(world, entity);
    button_from_style(entity, action, label_text, &style)
}

fn setup(mut commands: Commands) {
    ensure_task_pool_initialized();

    let (cmd_tx, cmd_rx) = unbounded::<NetworkCommand>();
    let (result_tx, result_rx) = unbounded::<NetworkResult>();
    let (image_cmd_tx, image_cmd_rx) = unbounded::<ImageCommand>();
    let (image_result_tx, image_result_rx) = unbounded::<ImageResult>();

    commands.insert_resource(NetworkBridge {
        cmd_tx: cmd_tx.clone(),
        cmd_rx,
        result_tx,
        result_rx,
    });
    commands.insert_resource(ImageBridge {
        cmd_tx: image_cmd_tx,
        cmd_rx: image_cmd_rx,
        result_tx: image_result_tx,
        result_rx: image_result_rx,
    });

    commands.insert_resource(UiState {
        status_line: "Booting Pixiv MVP…".to_string(),
        ..UiState::default()
    });
    commands.insert_resource(AuthState::default());
    commands.insert_resource(FeedOrder::default());
    commands.insert_resource(OverlayTags::default());
    commands.insert_resource(ResponsePanelState::default());
    commands.insert_resource(ViewportMetrics::default());
    commands.insert_resource(PixivApiClient::default());
    commands.insert_resource(Assets::<BevyImage>::default());

    let controls = PixivControls {
        toggle_sidebar: spawn_control_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.sidebar"],
        ),
        locale_combo: spawn_control_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.sidebar"],
        ),
        home_tab: spawn_control_entity(&mut commands, &["pixiv.button", "pixiv.button.subtle"]),
        rankings_tab: spawn_control_entity(&mut commands, &["pixiv.button", "pixiv.button.subtle"]),
        search_tab: spawn_control_entity(&mut commands, &["pixiv.button", "pixiv.button.subtle"]),
        open_browser_login: spawn_control_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        exchange_auth_code: spawn_control_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        refresh_token: spawn_control_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        search_submit: spawn_control_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        copy_response: spawn_control_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        clear_response: spawn_control_entity(&mut commands, &["pixiv.button", "pixiv.button.warn"]),
        close_overlay: spawn_control_entity(&mut commands, &["pixiv.button", "pixiv.button.warn"]),
    };
    commands.insert_resource(controls);

    let root = commands
        .spawn((
            UiRoot,
            PixivRoot,
            StyleClass(vec!["pixiv.root".to_string()]),
        ))
        .id();

    let sidebar = commands
        .spawn((
            PixivSidebar,
            StyleClass(vec!["pixiv.sidebar".to_string()]),
            ChildOf(root),
        ))
        .id();

    commands.entity(controls.locale_combo).insert((
        UiComboBox::new(vec![
            UiComboOption::new("en-US", "English"),
            UiComboOption::new("zh-CN", "简体中文"),
            UiComboOption::new("ja-JP", "日本語"),
        ])
        .with_placeholder("Language"),
        ChildOf(sidebar),
    ));

    let main_column = commands.spawn((PixivMainColumn, ChildOf(root))).id();

    commands.spawn((
        PixivAuthPanel,
        StyleClass(vec!["pixiv.auth-panel".to_string()]),
        ChildOf(main_column),
    ));
    commands.spawn((PixivResponsePanel, ChildOf(main_column)));
    commands.spawn((PixivSearchPanel, ChildOf(main_column)));

    let home_feed = commands.spawn((PixivHomeFeed, ChildOf(main_column))).id();

    let detail_overlay = commands
        .spawn((
            PixivDetailOverlay,
            StyleClass(vec!["pixiv.overlay".to_string()]),
            ChildOf(main_column),
        ))
        .id();
    let overlay_tags = commands
        .spawn((PixivOverlayTags, ChildOf(detail_overlay)))
        .id();

    commands.insert_resource(PixivUiTree {
        home_feed,
        overlay_tags,
    });

    let _ = cmd_tx.send(NetworkCommand::DiscoverIdp);
}

fn setup_styles(mut sheet: ResMut<StyleSheet>, i18n: Option<Res<AppI18n>>) {
    let default_fonts = i18n
        .as_ref()
        .map(|current| current.get_font_stack())
        .filter(|stack| !stack.is_empty());

    sheet.set_class(
        "pixiv.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                gap: Some(10.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1E, 0x1E, 0x1E)),
                text: Some(Color::from_rgb8(0xEE, 0xEE, 0xEE)),
                ..ColorStyle::default()
            },
            font_family: default_fonts.clone(),
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.sidebar",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                gap: Some(8.0),
                border_width: Some(1.0),
                corner_radius: Some(8.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x16, 0x16, 0x16)),
                border: Some(Color::from_rgb8(0x2C, 0x2C, 0x2C)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.auth-panel",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(10.0),
                gap: Some(8.0),
                border_width: Some(1.0),
                corner_radius: Some(10.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x19, 0x19, 0x19)),
                border: Some(Color::from_rgb8(0x30, 0x30, 0x30)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                corner_radius: Some(10.0),
                border_width: Some(1.0),
                ..LayoutStyle::default()
            },
            text: TextStyle { size: Some(14.0) },
            font_family: default_fonts.clone(),
            transition: Some(StyleTransition { duration: 0.14 }),
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.button.primary",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x12, 0x89, 0xE4)),
                hover_bg: Some(Color::from_rgb8(0x2D, 0x9B, 0xEB)),
                pressed_bg: Some(Color::from_rgb8(0x0D, 0x73, 0xBF)),
                border: Some(Color::from_rgb8(0x2D, 0x9B, 0xEB)),
                text: Some(Color::from_rgb8(0xF7, 0xFB, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.button.subtle",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2A, 0x2A, 0x2A)),
                hover_bg: Some(Color::from_rgb8(0x36, 0x36, 0x36)),
                pressed_bg: Some(Color::from_rgb8(0x1E, 0x1E, 0x1E)),
                border: Some(Color::from_rgb8(0x40, 0x40, 0x40)),
                text: Some(Color::from_rgb8(0xE7, 0xE7, 0xE7)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.button.sidebar",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x20, 0x20, 0x20)),
                hover_bg: Some(Color::from_rgb8(0x2C, 0x2C, 0x2C)),
                pressed_bg: Some(Color::from_rgb8(0x16, 0x16, 0x16)),
                border: Some(Color::from_rgb8(0x36, 0x36, 0x36)),
                text: Some(Color::from_rgb8(0xE4, 0xE4, 0xE4)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.button.warn",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x5D, 0x2A, 0x2A)),
                hover_bg: Some(Color::from_rgb8(0x73, 0x34, 0x34)),
                pressed_bg: Some(Color::from_rgb8(0x49, 0x21, 0x21)),
                border: Some(Color::from_rgb8(0x8B, 0x45, 0x45)),
                text: Some(Color::from_rgb8(0xFF, 0xEF, 0xEF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.primary-btn",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                corner_radius: Some(6.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2A, 0x2A, 0x2A)),
                hover_bg: Some(Color::from_rgb8(0x36, 0x36, 0x36)),
                pressed_bg: Some(Color::from_rgb8(0x1E, 0x1E, 0x1E)),
                border: Some(Color::from_rgb8(0x40, 0x40, 0x40)),
                text: Some(Color::from_rgb8(0xE7, 0xE7, 0xE7)),
                ..ColorStyle::default()
            },
            font_family: default_fonts.clone(),
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.card",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                gap: Some(6.0),
                border_width: Some(1.0),
                corner_radius: Some(8.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x24, 0x24, 0x24)),
                border: Some(Color::from_rgb8(0x3A, 0x3A, 0x3A)),
                hover_bg: Some(Color::from_rgb8(0x2A, 0x2A, 0x2A)),
                ..ColorStyle::default()
            },
            text: TextStyle { size: Some(14.0) },
            font_family: default_fonts.clone(),
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.tag",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(4.0),
                corner_radius: Some(6.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2C, 0x2C, 0x2C)),
                hover_bg: Some(Color::from_rgb8(0x00, 0x96, 0xFA)),
                pressed_bg: Some(Color::from_rgb8(0x00, 0x7C, 0xD0)),
                text: Some(Color::from_rgb8(0xE4, 0xE4, 0xE4)),
                ..ColorStyle::default()
            },
            font_family: default_fonts.clone(),
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    sheet.set_class(
        "pixiv.overlay",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(12.0),
                gap: Some(8.0),
                border_width: Some(1.0),
                corner_radius: Some(10.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x12, 0x12, 0x12)),
                border: Some(Color::from_rgb8(0x3A, 0x3A, 0x3A)),
                ..ColorStyle::default()
            },
            font_family: default_fonts,
            ..StyleSetter::default()
        },
    );
}

fn empty_ui() -> UiView {
    Arc::new(label(""))
}

fn project_root(_: &PixivRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let root_style = resolve_style(ctx.world, ctx.entity);
    let ui = ctx.world.resource::<UiState>();
    let sidebar_width = if ui.sidebar_collapsed {
        SIDEBAR_COLLAPSED_WIDTH
    } else {
        SIDEBAR_EXPANDED_WIDTH
    };

    let mut children = ctx.children.into_iter();
    let sidebar = children.next().unwrap_or_else(empty_ui);
    let main_content = children.next().unwrap_or_else(empty_ui);

    Arc::new(apply_widget_style(
        flex_row((
            sized_box(sidebar)
                .dims((Length::px(sidebar_width), Dim::Stretch))
                .into_any_flex(),
            main_content.flex(1.0).into_any_flex(),
        ))
        .main_axis_alignment(MainAxisAlignment::Start)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .dims(Dim::Stretch),
        &root_style,
    ))
}

fn project_sidebar(_: &PixivSidebar, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let ui = ctx.world.resource::<UiState>();
    let controls = *ctx.world.resource::<PixivControls>();
    let mut sidebar_children = ctx.children.into_iter();
    let locale_combo_view = sidebar_children.next().unwrap_or_else(empty_ui);

    let mut items = Vec::new();
    items.push(
        action_button(
            ctx.world,
            controls.toggle_sidebar,
            AppAction::ToggleSidebar,
            if ui.sidebar_collapsed {
                format!("{} ▶", tr(ctx.world, "pixiv.sidebar.expand", "Expand"))
            } else {
                format!("◀ {}", tr(ctx.world, "pixiv.sidebar.collapse", "Collapse"))
            },
        )
        .into_any_flex(),
    );

    items.push(locale_combo_view.into_any_flex());

    if !ui.sidebar_collapsed {
        items.push(
            action_button(
                ctx.world,
                controls.home_tab,
                AppAction::SetTab(NavTab::Home),
                if ui.active_tab == NavTab::Home {
                    format!("● {}", tr(ctx.world, "pixiv.sidebar.home", "Home"))
                } else {
                    tr(ctx.world, "pixiv.sidebar.home", "Home")
                },
            )
            .into_any_flex(),
        );
        items.push(
            action_button(
                ctx.world,
                controls.rankings_tab,
                AppAction::SetTab(NavTab::Rankings),
                if ui.active_tab == NavTab::Rankings {
                    format!("● {}", tr(ctx.world, "pixiv.sidebar.rankings", "Rankings"))
                } else {
                    tr(ctx.world, "pixiv.sidebar.rankings", "Rankings")
                },
            )
            .into_any_flex(),
        );
        items.push(
            action_button(
                ctx.world,
                controls.search_tab,
                AppAction::SetTab(NavTab::Search),
                if ui.active_tab == NavTab::Search {
                    format!("● {}", tr(ctx.world, "pixiv.sidebar.search", "Search"))
                } else {
                    tr(ctx.world, "pixiv.sidebar.search", "Search")
                },
            )
            .into_any_flex(),
        );
    }

    Arc::new(apply_widget_style(
        flex_col(items)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .width(Dim::Stretch),
        &style,
    ))
}

fn project_main_column(_: &PixivMainColumn, ctx: ProjectionCtx<'_>) -> UiView {
    let ui = ctx.world.resource::<UiState>();
    let root_style = resolve_style_for_classes(ctx.world, ["pixiv.root"]);

    let mut children = Vec::new();
    children.push(apply_label_style(label(ui.status_line.clone()), &root_style).into_any_flex());
    children.extend(ctx.children.into_iter().map(|child| child.into_any_flex()));

    Arc::new(
        portal(
            flex_col(children)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .width(Dim::Stretch),
        )
        .dims(Dim::Stretch),
    )
}

fn project_auth_panel(_: &PixivAuthPanel, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let input_style = resolve_style_for_classes(ctx.world, ["pixiv.root"]);
    let auth = ctx.world.resource::<AuthState>();
    let controls = *ctx.world.resource::<PixivControls>();
    let auth_endpoint = auth
        .idp_urls
        .as_ref()
        .map(|i| i.auth_token_url.as_str())
        .map(std::borrow::ToOwned::to_owned)
        .unwrap_or_else(|| tr(ctx.world, "pixiv.auth.loading", "loading…"));

    let rows = vec![
        apply_label_style(
            label(format!(
                "{} {}",
                tr(ctx.world, "pixiv.auth.endpoint", "Auth endpoint:"),
                auth_endpoint
            )),
            &input_style,
        )
        .into_any_flex(),
        sized_box(apply_text_input_style(
            text_input(
                ctx.entity,
                auth.code_verifier_input.clone(),
                AppAction::SetCodeVerifier,
            )
            .placeholder(tr(
                ctx.world,
                "pixiv.auth.placeholder.pkce",
                "PKCE code_verifier",
            )),
            &input_style,
        ))
        .width(Dim::Stretch)
        .into_any_flex(),
        sized_box(apply_text_input_style(
            text_input(
                ctx.entity,
                auth.auth_code_input.clone(),
                AppAction::SetAuthCode,
            )
            .placeholder(tr(ctx.world, "pixiv.auth.placeholder.code", "Auth code")),
            &input_style,
        ))
        .width(Dim::Stretch)
        .into_any_flex(),
        action_button(
            ctx.world,
            controls.open_browser_login,
            AppAction::OpenBrowserLogin,
            tr(
                ctx.world,
                "pixiv.auth.open_browser_login",
                "Open Browser Login",
            ),
        )
        .into_any_flex(),
        action_button(
            ctx.world,
            controls.exchange_auth_code,
            AppAction::ExchangeAuthCode,
            tr(ctx.world, "pixiv.auth.login_auth_code", "Login (auth_code)"),
        )
        .into_any_flex(),
        sized_box(apply_text_input_style(
            text_input(
                ctx.entity,
                auth.refresh_token_input.clone(),
                AppAction::SetRefreshToken,
            )
            .placeholder(tr(
                ctx.world,
                "pixiv.auth.placeholder.refresh_token",
                "Refresh token",
            )),
            &input_style,
        ))
        .width(Dim::Stretch)
        .into_any_flex(),
        action_button(
            ctx.world,
            controls.refresh_token,
            AppAction::RefreshToken,
            tr(ctx.world, "pixiv.auth.refresh_token", "Refresh Token"),
        )
        .into_any_flex(),
    ];

    Arc::new(apply_widget_style(
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .width(Dim::Stretch),
        &style,
    ))
}

fn project_response_panel(_: &PixivResponsePanel, ctx: ProjectionCtx<'_>) -> UiView {
    let controls = *ctx.world.resource::<PixivControls>();
    let panel = ctx.world.resource::<ResponsePanelState>();

    if panel.content.trim().is_empty() {
        return empty_ui();
    }

    let lines = panel
        .content
        .lines()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    let lines = Arc::new(lines);
    let line_count = i64::try_from(lines.len()).unwrap_or(i64::MAX);

    Arc::new(
        flex_col((
            label(panel.title.clone()).into_any_flex(),
            flex_row((
                action_button(
                    ctx.world,
                    controls.copy_response,
                    AppAction::CopyResponseBody,
                    tr(ctx.world, "pixiv.response.copy", "Copy Response Body"),
                )
                .into_any_flex(),
                action_button(
                    ctx.world,
                    controls.clear_response,
                    AppAction::ClearResponseBody,
                    tr(ctx.world, "pixiv.response.clear", "Clear"),
                )
                .into_any_flex(),
            ))
            .into_any_flex(),
            sized_box(virtual_scroll(0..line_count, {
                let lines = Arc::clone(&lines);
                move |_, idx| {
                    let row_idx = usize::try_from(idx).unwrap_or(0);
                    Arc::new(label(lines.get(row_idx).cloned().unwrap_or_default())) as UiView
                }
            }))
            .dims((Dim::Stretch, Length::px(RESPONSE_PANEL_HEIGHT)))
            .into_any_flex(),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .width(Dim::Stretch),
    )
}

fn project_search_panel(_: &PixivSearchPanel, ctx: ProjectionCtx<'_>) -> UiView {
    let ui = ctx.world.resource::<UiState>();
    if ui.active_tab != NavTab::Search {
        return empty_ui();
    }

    let controls = *ctx.world.resource::<PixivControls>();
    let input_style = resolve_style_for_classes(ctx.world, ["pixiv.root"]);

    Arc::new(
        flex_row((
            apply_text_input_style(
                text_input(ctx.entity, ui.search_text.clone(), AppAction::SetSearchText)
                    .placeholder(tr(
                        ctx.world,
                        "pixiv.search.placeholder",
                        "Search illust keyword",
                    )),
                &input_style,
            )
            .flex(1.0),
            action_button(
                ctx.world,
                controls.search_submit,
                AppAction::SubmitSearch,
                tr(ctx.world, "pixiv.search.submit", "Search"),
            )
            .into_any_flex(),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .width(Dim::Stretch),
    )
}

fn project_home_feed(_: &PixivHomeFeed, ctx: ProjectionCtx<'_>) -> UiView {
    if ctx.children.is_empty() {
        return Arc::new(label(tr(
            ctx.world,
            "pixiv.feed.empty",
            "No data yet. Login first, then switch tabs.",
        )));
    }

    Arc::new(
        flex_col(
            ctx.children
                .into_iter()
                .map(|child| child.into_any_flex())
                .collect::<Vec<_>>(),
        )
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .width(Dim::Stretch),
    )
}

fn illust_thumbnail_view(world: &World, visual: &IllustVisual) -> UiView {
    if let Some(image_data) = visual.thumb_ui.clone() {
        Arc::new(image(image_data))
    } else {
        Arc::new(label(tr(
            world,
            "pixiv.feed.thumbnail_loading",
            "thumbnail loading…",
        )))
    }
}

fn illust_avatar_view(visual: &IllustVisual) -> UiView {
    if let Some(image_data) = visual.avatar_ui.clone() {
        Arc::new(
            sized_box(image(image_data))
                .fixed_height(Length::px(28.0))
                .fixed_width(Length::px(28.0)),
        )
    } else {
        Arc::new(label("👤"))
    }
}

fn illust_author_row(author: &str, avatar: UiView, style: &ResolvedStyle) -> UiView {
    Arc::new(flex_row((
        avatar.into_any_flex(),
        apply_label_style(label(author.to_string()), style).into_any_flex(),
    )))
}

fn illust_stats_view(illust: &Illust, style: &ResolvedStyle) -> UiView {
    Arc::new(apply_label_style(
        label(format!(
            "👁 {}   ❤ {}",
            illust.total_view, illust.total_bookmarks
        )),
        style,
    ))
}

fn project_illust_card(_: &PixivIllustCard, ctx: ProjectionCtx<'_>) -> UiView {
    let Some(illust) = ctx.world.get::<Illust>(ctx.entity) else {
        return empty_ui();
    };

    let visual = ctx
        .world
        .get::<IllustVisual>(ctx.entity)
        .cloned()
        .unwrap_or_default();
    let anim = ctx
        .world
        .get::<CardAnimState>(ctx.entity)
        .copied()
        .unwrap_or_default();
    let style = resolve_style(ctx.world, ctx.entity);
    let primary_button_style = resolve_style_for_entity_classes(
        ctx.world,
        ctx.entity,
        ["pixiv.button", "pixiv.button.primary"],
    );
    let subtle_button_style = resolve_style_for_entity_classes(
        ctx.world,
        ctx.entity,
        ["pixiv.button", "pixiv.button.subtle"],
    );

    let ui = ctx.world.resource::<UiState>();
    let viewport = ctx.world.resource::<ViewportMetrics>();
    let (_, card_width) = compute_feed_layout(viewport.width as f64, ui.sidebar_collapsed);

    let image_height = (card_width * 0.58 * anim.card_scale as f64).max(120.0);
    let heart = if illust.is_bookmarked { "♥" } else { "♡" };

    let heart_button = sized_box(button_from_style(
        ctx.entity,
        AppAction::Bookmark(ctx.entity),
        heart,
        &subtle_button_style,
    ))
    .fixed_width(Length::px((46.0_f32 * anim.heart_scale) as f64));

    Arc::new(
        sized_box(apply_widget_style(
            flex_col(vec![
                sized_box(illust_thumbnail_view(ctx.world, &visual))
                    .dims((Dim::Stretch, Length::px(image_height)))
                    .into_any_flex(),
                apply_label_style(label(illust.title.clone()), &style).into_any_flex(),
                illust_author_row(&illust.user.name, illust_avatar_view(&visual), &style)
                    .into_any_flex(),
                illust_stats_view(illust, &style).into_any_flex(),
                flex_row((
                    button_from_style(
                        ctx.entity,
                        AppAction::OpenIllust(ctx.entity),
                        tr(ctx.world, "pixiv.feed.open", "Open"),
                        &primary_button_style,
                    )
                    .into_any_flex(),
                    heart_button.into_any_flex(),
                ))
                .main_axis_alignment(MainAxisAlignment::SpaceBetween)
                .into_any_flex(),
            ]),
            &style,
        ))
        .fixed_width(Length::px((card_width * anim.card_scale as f64).max(180.0)))
        .fixed_height(Length::px(
            ((CARD_BASE_HEIGHT * (card_width / CARD_BASE_WIDTH)) * anim.card_scale as f64)
                .max(200.0),
        )),
    )
}

fn project_detail_overlay(_: &PixivDetailOverlay, ctx: ProjectionCtx<'_>) -> UiView {
    let ui = ctx.world.resource::<UiState>();
    let Some(entity) = ui.selected_illust else {
        return empty_ui();
    };

    let Some(illust) = ctx.world.get::<Illust>(entity) else {
        return empty_ui();
    };
    let style = resolve_style(ctx.world, ctx.entity);
    let controls = *ctx.world.resource::<PixivControls>();
    let visual = ctx
        .world
        .get::<IllustVisual>(entity)
        .cloned()
        .unwrap_or_default();

    let hero: UiView = if let Some(high_res) = visual.high_res_ui {
        Arc::new(sized_box(image(high_res)).fixed_height(Length::px(280.0)))
    } else {
        Arc::new(label(tr(
            ctx.world,
            "pixiv.feed.high_res_loading",
            "high-res loading…",
        )))
    };

    let tags = ctx.children.into_iter().next().unwrap_or_else(empty_ui);

    Arc::new(apply_widget_style(
        flex_col((
            action_button(
                ctx.world,
                controls.close_overlay,
                AppAction::CloseIllust,
                tr(ctx.world, "pixiv.overlay.close", "Close"),
            )
            .into_any_flex(),
            hero.into_any_flex(),
            label(illust.title.clone()).into_any_flex(),
            label(format!(
                "{} {}",
                tr(ctx.world, "pixiv.overlay.author", "Author:"),
                illust.user.name
            ))
            .into_any_flex(),
            label(format!(
                "{} {}  {} {}  {} {}",
                tr(ctx.world, "pixiv.overlay.views", "Views"),
                illust.total_view,
                tr(ctx.world, "pixiv.overlay.bookmarks", "Bookmarks"),
                illust.total_bookmarks,
                tr(ctx.world, "pixiv.overlay.comments", "Comments"),
                illust.total_comments
            ))
            .into_any_flex(),
            tags.into_any_flex(),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .width(Dim::Stretch),
        &style,
    ))
}

fn project_overlay_tags(_: &PixivOverlayTags, ctx: ProjectionCtx<'_>) -> UiView {
    if ctx.children.is_empty() {
        return empty_ui();
    }

    let rows = ctx
        .children
        .chunks(4)
        .map(|chunk| {
            flex_row(
                chunk
                    .iter()
                    .cloned()
                    .map(|child| child.into_any_flex())
                    .collect::<Vec<_>>(),
            )
            .into_any_flex()
        })
        .collect::<Vec<_>>();

    Arc::new(flex_col(rows).cross_axis_alignment(CrossAxisAlignment::Stretch))
}

fn project_overlay_tag(tag: &OverlayTag, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    button_from_style(
        ctx.entity,
        AppAction::SearchByTag(tag.text.clone()),
        tag.text.clone(),
        &style,
    )
}

fn drain_ui_actions_and_dispatch(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<AppAction>();
    if events.is_empty() {
        return;
    }

    for event in events {
        match event.action {
            AppAction::ToggleSidebar => {
                let collapsed = {
                    let mut ui = world.resource_mut::<UiState>();
                    ui.sidebar_collapsed = !ui.sidebar_collapsed;
                    ui.sidebar_collapsed
                };

                if collapsed {
                    set_status_key(world, "pixiv.status.sidebar_collapsed", "Sidebar collapsed");
                } else {
                    set_status_key(world, "pixiv.status.sidebar_expanded", "Sidebar expanded");
                }
            }
            AppAction::SetTab(tab) => {
                let status_line = match tab {
                    NavTab::Home => tr(world, "pixiv.status.loading_home", "Loading Home feed…"),
                    NavTab::Rankings => tr(
                        world,
                        "pixiv.status.loading_rankings",
                        "Loading Rankings feed…",
                    ),
                    NavTab::Search => tr(
                        world,
                        "pixiv.status.search_ready",
                        "Search tab ready. Enter keywords and press Search.",
                    ),
                };

                {
                    let mut ui = world.resource_mut::<UiState>();
                    ui.active_tab = tab;
                    ui.status_line = status_line;
                }

                let cmd = match tab {
                    NavTab::Home => NetworkCommand::FetchHome,
                    NavTab::Rankings => NetworkCommand::FetchRanking,
                    NavTab::Search => continue,
                };
                let _ = world.resource::<NetworkBridge>().cmd_tx.send(cmd);
            }
            AppAction::SetSearchText(value) => {
                world.resource_mut::<UiState>().search_text = value;
            }
            AppAction::SubmitSearch => {
                let query = world.resource::<UiState>().search_text.clone();

                if query.trim().is_empty() {
                    set_status_key(
                        world,
                        "pixiv.status.search_keyword_required",
                        "Please enter a search keyword first.",
                    );
                    continue;
                }

                set_status(
                    world,
                    format!(
                        "{} ‘{}’…",
                        tr(world, "pixiv.status.searching", "Searching for"),
                        query.trim()
                    ),
                );
                let _ = world
                    .resource::<NetworkBridge>()
                    .cmd_tx
                    .send(NetworkCommand::Search { word: query });
            }
            AppAction::OpenIllust(entity) => {
                world.resource_mut::<UiState>().selected_illust = Some(entity);
                prepare_overlay_tags(world, entity);

                if let Some(illust) = world.get::<Illust>(entity) {
                    let high_res = illust
                        .meta_single_page
                        .as_ref()
                        .and_then(|meta| meta.original_image_url.clone())
                        .unwrap_or_else(|| illust.image_urls.large.clone());
                    let _ = world
                        .resource::<ImageBridge>()
                        .cmd_tx
                        .send(ImageCommand::Download {
                            entity,
                            kind: ImageKind::HighRes,
                            url: high_res,
                        });
                }
            }
            AppAction::CloseIllust => {
                world.resource_mut::<UiState>().selected_illust = None;
                clear_overlay_tags(world);
            }
            AppAction::Bookmark(entity) => {
                let illust_id = if let Some(mut illust) = world.get_mut::<Illust>(entity) {
                    illust.is_bookmarked = !illust.is_bookmarked;
                    Some(illust.id)
                } else {
                    None
                };

                if let Some(id) = illust_id {
                    trigger_bookmark_pulse(world, entity);
                    let _ = world
                        .resource::<NetworkBridge>()
                        .cmd_tx
                        .send(NetworkCommand::Bookmark { illust_id: id });
                }
            }
            AppAction::SearchByTag(tag) => {
                {
                    let mut ui = world.resource_mut::<UiState>();
                    ui.search_text = tag.clone();
                    ui.active_tab = NavTab::Search;
                }
                let _ = world
                    .resource::<NetworkBridge>()
                    .cmd_tx
                    .send(NetworkCommand::Search { word: tag });
            }
            AppAction::SetAuthCode(value) => {
                world.resource_mut::<AuthState>().auth_code_input = value;
            }
            AppAction::SetCodeVerifier(value) => {
                world.resource_mut::<AuthState>().code_verifier_input = value;
            }
            AppAction::SetRefreshToken(value) => {
                world.resource_mut::<AuthState>().refresh_token_input = value;
            }
            AppAction::CopyResponseBody => {
                let body = world.resource::<ResponsePanelState>().content.clone();
                if body.trim().is_empty() {
                    set_status_key(
                        world,
                        "pixiv.status.no_response_to_copy",
                        "No response body to copy.",
                    );
                    continue;
                }

                match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(body)) {
                    Ok(_) => {
                        set_status_key(
                            world,
                            "pixiv.status.response_copied",
                            "Response body copied to clipboard.",
                        );
                    }
                    Err(err) => {
                        set_status(
                            world,
                            format!(
                                "{}: {err}",
                                tr(world, "pixiv.status.copy_failed", "Clipboard copy failed")
                            ),
                        );
                    }
                }
            }
            AppAction::ClearResponseBody => {
                *world.resource_mut::<ResponsePanelState>() = ResponsePanelState::default();
                set_status_key(
                    world,
                    "pixiv.status.response_panel_cleared",
                    "Response panel cleared.",
                );
            }
            AppAction::OpenBrowserLogin => {
                let (idp_urls, verifier) = {
                    let mut auth = world.resource_mut::<AuthState>();
                    let idp_urls = auth.idp_urls.clone();

                    if auth.code_verifier_input.trim().is_empty() {
                        auth.code_verifier_input = generate_pkce_code_verifier();
                    }

                    (idp_urls, auth.code_verifier_input.clone())
                };

                let redirect_uri = idp_urls
                    .as_ref()
                    .map(|idp| idp.auth_token_redirect_url.as_str())
                    .unwrap_or(PIXIV_WEB_REDIRECT_FALLBACK);
                let challenge = pkce_s256_challenge(&verifier);

                match build_browser_login_url(&challenge) {
                    Ok(login_url) => match open_in_system_browser(&login_url) {
                        Ok(_) => {
                            let message = if idp_urls.is_some() {
                                format!(
                                    "{} {redirect_uri}.",
                                    tr(
                                        world,
                                        "pixiv.status.browser_opened_ready",
                                        "Browser login page opened. Official callback should look like pixiv://account/login?code=...&via=login. Token exchange uses redirect_uri from /idp-urls (current:)"
                                    )
                                )
                            } else {
                                tr(
                                    world,
                                    "pixiv.status.browser_opened_fallback",
                                    "Browser login page opened. /idp-urls is not ready yet, so token exchange will use fallback redirect_uri. If Login fails, wait for IdP discovery and retry.",
                                )
                            };
                            set_status(world, message);
                        }
                        Err(err) => {
                            set_status(
                                world,
                                format!(
                                    "{}: {err}. {}: {login_url}",
                                    tr(
                                        world,
                                        "pixiv.status.browser_open_failed",
                                        "Could not open browser automatically"
                                    ),
                                    tr(
                                        world,
                                        "pixiv.status.open_url_manually",
                                        "Open this URL manually"
                                    )
                                ),
                            );
                        }
                    },
                    Err(err) => {
                        set_status(
                            world,
                            format!(
                                "{}: {err}",
                                tr(
                                    world,
                                    "pixiv.status.build_login_url_failed",
                                    "Failed to build browser login URL"
                                )
                            ),
                        );
                    }
                }
            }
            AppAction::ExchangeAuthCode => {
                let auth = world.resource::<AuthState>();
                let Some(code) = extract_auth_code_from_input(&auth.auth_code_input) else {
                    set_status_key(
                        world,
                        "pixiv.status.auth_code_missing",
                        "Auth code is missing. Please paste a raw code or a callback URL containing `code=`.",
                    );
                    continue;
                };
                let _ =
                    world
                        .resource::<NetworkBridge>()
                        .cmd_tx
                        .send(NetworkCommand::ExchangeCode {
                            code,
                            code_verifier: auth.code_verifier_input.clone(),
                        });
            }
            AppAction::RefreshToken => {
                let refresh_token = world.resource::<AuthState>().refresh_token_input.clone();
                set_status_key(world, "pixiv.status.refreshing_token", "Refreshing token…");
                let _ = world
                    .resource::<NetworkBridge>()
                    .cmd_tx
                    .send(NetworkCommand::Refresh { refresh_token });
            }
        }
    }

    let combo_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiComboBoxChanged>();
    let controls = *world.resource::<PixivControls>();

    for event in combo_events {
        if event.action.combo != controls.locale_combo {
            continue;
        }

        let next = parse_locale(event.action.value.as_str());
        world
            .resource_mut::<AppI18n>()
            .set_active_locale(next.clone());

        {
            let font_stack = {
                let i18n = world.resource::<AppI18n>();
                let stack = i18n.get_font_stack();
                (!stack.is_empty()).then_some(stack)
            };
            let mut style_sheet = world.resource_mut::<StyleSheet>();
            sync_font_stack_for_locale(&mut style_sheet, font_stack.as_deref());
        }

        let status_prefix = tr(
            world,
            "pixiv.status.locale_switched",
            "Language switched to",
        );
        set_status(world, format!("{status_prefix} {}", locale_badge(&next)));
    }
}

fn track_viewport_metrics(
    mut resize_events: MessageReader<WindowResized>,
    mut viewport: ResMut<ViewportMetrics>,
) {
    for event in resize_events.read() {
        viewport.width = event.width;
        viewport.height = event.height;
    }
}

fn clear_overlay_tags(world: &mut World) {
    let entities = std::mem::take(&mut world.resource_mut::<OverlayTags>().0);
    for entity in entities {
        if world.get_entity(entity).is_ok() {
            world.entity_mut(entity).despawn();
        }
    }
}

fn prepare_overlay_tags(world: &mut World, illust_entity: Entity) {
    clear_overlay_tags(world);

    let tags_parent = world.resource::<PixivUiTree>().overlay_tags;

    let tags = world
        .get::<Illust>(illust_entity)
        .map(|illust| illust.tags.clone())
        .unwrap_or_default();

    let mut spawned = Vec::new();
    for tag in tags {
        let entity = world
            .spawn((
                OverlayTag {
                    text: tag
                        .translated_name
                        .clone()
                        .unwrap_or_else(|| tag.name.clone()),
                },
                StyleClass(vec!["pixiv.tag".to_string()]),
                ChildOf(tags_parent),
            ))
            .id();
        spawned.push(entity);
    }

    world.resource_mut::<OverlayTags>().0 = spawned;
}

fn trigger_bookmark_pulse(world: &mut World, entity: Entity) {
    let current = world
        .get::<CardAnimState>(entity)
        .copied()
        .unwrap_or_default();

    let mut start = current;
    start.heart_scale = 1.28;
    world.entity_mut(entity).insert(start);

    let mut end = start;
    end.heart_scale = 1.0;

    spawn_card_tween(
        world,
        entity,
        start,
        end,
        420,
        EaseMethod::CustomFunction(ease_elastic_out),
    );
}

fn animate_card_hover(world: &mut World) {
    let entities = {
        let mut q = world.query::<(
            Entity,
            Option<&bevy_xilem::Hovered>,
            &CardHoverFlag,
            &CardAnimState,
            &Illust,
        )>();
        q.iter(world)
            .map(|(entity, hovered, hover_flag, anim, _)| {
                (entity, hovered.is_some(), hover_flag.0, *anim)
            })
            .collect::<Vec<_>>()
    };

    for (entity, hovered_now, hovered_before, anim) in entities {
        if hovered_now == hovered_before {
            continue;
        }

        world.entity_mut(entity).insert(CardHoverFlag(hovered_now));

        let mut end = anim;
        if hovered_now {
            end.card_scale = 1.02;
            end.image_brightness = 1.08;
        } else {
            end.card_scale = 1.0;
            end.image_brightness = 1.0;
        }

        spawn_card_tween(
            world,
            entity,
            anim,
            end,
            150,
            EaseMethod::CustomFunction(ease_quadratic_in_out),
        );
    }
}

fn spawn_network_tasks(world: &mut World) {
    let cmd_rx = world.resource::<NetworkBridge>().cmd_rx.clone();
    let result_tx = world.resource::<NetworkBridge>().result_tx.clone();
    let client = world.resource::<PixivApiClient>().clone();
    let auth = world.resource::<AuthState>().clone();

    while let Ok(cmd) = cmd_rx.try_recv() {
        let client = client.clone();
        let auth = auth.clone();
        let result_tx = result_tx.clone();

        AsyncComputeTaskPool::get()
            .spawn(async move {
                let result = match run_network_command(&client, &auth, cmd) {
                    Ok(r) => r,
                    Err(err) => {
                        let details = err.to_string();
                        let summary = summarize_error(&details);
                        NetworkResult::Error { summary, details }
                    }
                };
                let _ = result_tx.send(result);
            })
            .detach();
    }
}

fn run_network_command(
    client: &PixivApiClient,
    auth: &AuthState,
    cmd: NetworkCommand,
) -> Result<NetworkResult> {
    match cmd {
        NetworkCommand::DiscoverIdp => {
            let idp = client.discover_idp_urls()?;
            Ok(NetworkResult::IdpDiscovered(idp))
        }
        NetworkCommand::ExchangeCode {
            code,
            code_verifier,
        } => {
            let idp = auth.idp_urls.as_ref();
            let auth_token_url = idp
                .map(|value| value.auth_token_url.as_str())
                .unwrap_or(PIXIV_AUTH_TOKEN_FALLBACK);
            let redirect_uri = idp
                .map(|value| value.auth_token_redirect_url.as_str())
                .unwrap_or(PIXIV_WEB_REDIRECT_FALLBACK);
            let response = client.exchange_authorization_code(
                auth_token_url,
                &code_verifier,
                &code,
                redirect_uri,
            )?;
            Ok(NetworkResult::Authenticated(response.into()))
        }
        NetworkCommand::Refresh { refresh_token } => {
            let auth_token_url = auth
                .idp_urls
                .as_ref()
                .map(|value| value.auth_token_url.as_str())
                .unwrap_or(PIXIV_AUTH_TOKEN_FALLBACK);
            let response = client.refresh_access_token(auth_token_url, &refresh_token)?;
            Ok(NetworkResult::Authenticated(response.into()))
        }
        NetworkCommand::FetchHome => {
            let token = auth
                .session
                .as_ref()
                .map(|s| s.access_token.clone())
                .ok_or_else(|| anyhow::anyhow!("not authenticated"))?;
            let payload = client.recommended_illusts(&token)?;
            Ok(NetworkResult::FeedLoaded {
                source: NavTab::Home,
                payload,
            })
        }
        NetworkCommand::FetchRanking => {
            let token = auth
                .session
                .as_ref()
                .map(|s| s.access_token.clone())
                .ok_or_else(|| anyhow::anyhow!("not authenticated"))?;
            let payload = client.ranking_illusts(&token, "day")?;
            Ok(NetworkResult::FeedLoaded {
                source: NavTab::Rankings,
                payload,
            })
        }
        NetworkCommand::Search { word } => {
            let token = auth
                .session
                .as_ref()
                .map(|s| s.access_token.clone())
                .ok_or_else(|| anyhow::anyhow!("not authenticated"))?;
            let payload = client.search_illusts(&token, &word)?;
            Ok(NetworkResult::FeedLoaded {
                source: NavTab::Search,
                payload,
            })
        }
        NetworkCommand::Bookmark { illust_id } => {
            let token = auth
                .session
                .as_ref()
                .map(|s| s.access_token.clone())
                .ok_or_else(|| anyhow::anyhow!("not authenticated"))?;
            client.bookmark_illust(&token, illust_id)?;
            Ok(NetworkResult::BookmarkDone { illust_id })
        }
    }
}

fn apply_network_results(world: &mut World) {
    let result_rx = world.resource::<NetworkBridge>().result_rx.clone();
    let image_cmd_tx = world.resource::<ImageBridge>().cmd_tx.clone();

    while let Ok(result) = result_rx.try_recv() {
        match result {
            NetworkResult::IdpDiscovered(idp) => {
                world.resource_mut::<AuthState>().idp_urls = Some(idp);
                set_status_key(
                    world,
                    "pixiv.status.idp_discovered",
                    "IdP endpoint discovered. Enter auth_code or refresh token.",
                );
            }
            NetworkResult::Authenticated(session) => {
                world.resource_mut::<AuthState>().session = Some(session.clone());
                set_status_key(
                    world,
                    "pixiv.status.authenticated_loading_home",
                    "Authenticated. Loading home feed…",
                );
                *world.resource_mut::<ResponsePanelState>() = ResponsePanelState::default();

                if world.resource::<AuthState>().refresh_token_input.is_empty() {
                    world.resource_mut::<AuthState>().refresh_token_input =
                        session.refresh_token.clone();
                }

                let _ = world
                    .resource::<NetworkBridge>()
                    .cmd_tx
                    .send(NetworkCommand::FetchHome);
            }
            NetworkResult::FeedLoaded { source, payload } => {
                let home_feed = world.resource::<PixivUiTree>().home_feed;
                world.resource_mut::<UiState>().active_tab = source;
                let message = format!(
                    "{} {} ({source:?})",
                    tr(
                        world,
                        "pixiv.status.loaded_illustrations",
                        "Loaded illustrations",
                    ),
                    payload.illusts.len()
                );
                set_status(world, message);

                for entity in std::mem::take(&mut world.resource_mut::<FeedOrder>().0) {
                    if world.get_entity(entity).is_ok() {
                        world.entity_mut(entity).despawn();
                    }
                }

                let mut new_order = Vec::new();
                for illust in payload.illusts {
                    let entity = world
                        .spawn((
                            PixivIllustCard,
                            illust.clone(),
                            IllustVisual::default(),
                            CardAnimState::default(),
                            CardHoverFlag(false),
                            StyleClass(vec!["pixiv.card".to_string()]),
                            ChildOf(home_feed),
                        ))
                        .id();

                    let _ = image_cmd_tx.send(ImageCommand::Download {
                        entity,
                        kind: ImageKind::Thumb,
                        url: illust.image_urls.square_medium.clone(),
                    });
                    let _ = image_cmd_tx.send(ImageCommand::Download {
                        entity,
                        kind: ImageKind::Avatar,
                        url: illust.user.profile_image_urls.medium.clone(),
                    });

                    new_order.push(entity);
                }

                world.resource_mut::<FeedOrder>().0 = new_order;
            }
            NetworkResult::BookmarkDone { illust_id } => {
                set_status(
                    world,
                    format!(
                        "{} #{illust_id}",
                        tr(
                            world,
                            "pixiv.status.bookmark_synced",
                            "Bookmark synced for illust",
                        )
                    ),
                );
            }
            NetworkResult::Error { summary, details } => {
                let status_message = format!(
                    "{}: {summary}",
                    tr(world, "pixiv.status.network_error", "Network error")
                );
                set_status(world, status_message);
                *world.resource_mut::<ResponsePanelState>() = ResponsePanelState {
                    title: tr(
                        world,
                        "pixiv.status.response_detail_title",
                        "Last network response body / detail",
                    ),
                    content: details,
                };
            }
        }
    }
}

fn spawn_image_tasks(world: &mut World) {
    let cmd_rx = world.resource::<ImageBridge>().cmd_rx.clone();
    let result_tx = world.resource::<ImageBridge>().result_tx.clone();
    let client = world.resource::<PixivApiClient>().clone();

    while let Ok(cmd) = cmd_rx.try_recv() {
        let client = client.clone();
        let result_tx = result_tx.clone();

        AsyncComputeTaskPool::get()
            .spawn(async move {
                let result = match cmd {
                    ImageCommand::Download { entity, kind, url } => {
                        match client.download_image_rgba8(&url) {
                            Ok(decoded) => ImageResult::Loaded {
                                entity,
                                kind,
                                decoded,
                            },
                            Err(err) => ImageResult::Failed {
                                entity,
                                kind,
                                error: err.to_string(),
                            },
                        }
                    }
                };

                let _ = result_tx.send(result);
            })
            .detach();
    }
}

fn apply_image_results(world: &mut World) {
    let result_rx = world.resource::<ImageBridge>().result_rx.clone();

    while let Ok(result) = result_rx.try_recv() {
        match result {
            ImageResult::Loaded {
                entity,
                kind,
                decoded,
            } => {
                if world.get_entity(entity).is_err() {
                    continue;
                }

                let DecodedImageRgba {
                    width,
                    height,
                    rgba8,
                } = decoded;

                let ui_data = ImageData {
                    data: Blob::new(Arc::new(rgba8.clone())),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width,
                    height,
                };

                let Some(rgba_image) = image::RgbaImage::from_raw(width, height, rgba8) else {
                    set_status(
                        world,
                        format!(
                            "{} {entity:?}",
                            tr(
                                world,
                                "pixiv.status.image_decode_buffer_mismatch",
                                "Image decode buffer size mismatch for entity",
                            )
                        ),
                    );
                    continue;
                };
                let bevy_image = BevyImage::from_dynamic(
                    image::DynamicImage::ImageRgba8(rgba_image),
                    true,
                    RenderAssetUsages::default(),
                );

                let handle = world.resource_mut::<Assets<BevyImage>>().add(bevy_image);

                let mut visual = world
                    .get::<IllustVisual>(entity)
                    .cloned()
                    .unwrap_or_default();
                match kind {
                    ImageKind::Thumb => {
                        visual.thumb_ui = Some(ui_data);
                        visual.thumb_handle = Some(handle);
                    }
                    ImageKind::Avatar => {
                        visual.avatar_ui = Some(ui_data);
                        visual.avatar_handle = Some(handle);
                    }
                    ImageKind::HighRes => {
                        visual.high_res_ui = Some(ui_data);
                        visual.high_res_handle = Some(handle);
                    }
                }

                world.entity_mut(entity).insert(visual);
            }
            ImageResult::Failed {
                entity,
                kind,
                error,
            } => {
                let which = match kind {
                    ImageKind::Thumb => "thumb",
                    ImageKind::Avatar => "avatar",
                    ImageKind::HighRes => "high-res",
                };
                if world.get_entity(entity).is_ok() {
                    set_status(
                        world,
                        format!(
                            "{} ({which}): {error}",
                            tr(world, "pixiv.status.image_load_failed", "Image load failed")
                        ),
                    );
                }
            }
        }
    }
}

fn build_app() -> App {
    ensure_task_pool_initialized();

    let mut app = App::new();
    register_bridge_fonts(&mut app);

    app.add_plugins((
        EmbeddedAssetPlugin {
            mode: PluginMode::ReplaceDefault,
        },
        AssetPlugin::default(),
        TextPlugin::default(),
        BevyXilemPlugin,
    ))
    .register_ui_control::<bevy_xilem::UiButton>()
    .register_ui_control::<bevy_xilem::UiTextInput>()
    .register_ui_control::<bevy_xilem::UiDialog>()
    .register_ui_control::<bevy_xilem::UiComboBox>()
    .register_ui_control::<bevy_xilem::UiToast>()
    .load_style_sheet("themes/default_theme.ron")
    .insert_resource(AppI18n::new(parse_locale("en-US")))
    .register_i18n_bundle(
        "en-US",
        SyncTextSource::FilePath("assets/locales/en-US/main.ftl"),
        vec![
            "Inter",
            "Noto Sans CJK SC",
            "Noto Sans CJK JP",
            "Noto Sans CJK TC",
            "Noto Sans CJK KR",
            "sans-serif",
        ],
    )
    .register_i18n_bundle(
        "zh-CN",
        SyncTextSource::FilePath("assets/locales/zh-CN/main.ftl"),
        vec![
            "Inter",
            "Noto Sans CJK SC",
            "Noto Sans CJK JP",
            "Noto Sans CJK TC",
            "Noto Sans CJK KR",
            "sans-serif",
        ],
    )
    .register_i18n_bundle(
        "ja-JP",
        SyncTextSource::FilePath("assets/locales/ja-JP/main.ftl"),
        vec![
            "Inter",
            "Noto Sans CJK JP",
            "Noto Sans CJK SC",
            "Noto Sans CJK TC",
            "Noto Sans CJK KR",
            "sans-serif",
        ],
    )
    .register_projector::<PixivRoot>(project_root)
    .register_projector::<PixivSidebar>(project_sidebar)
    .register_projector::<PixivMainColumn>(project_main_column)
    .register_projector::<PixivAuthPanel>(project_auth_panel)
    .register_projector::<PixivResponsePanel>(project_response_panel)
    .register_projector::<PixivSearchPanel>(project_search_panel)
    .register_projector::<PixivHomeFeed>(project_home_feed)
    .register_projector::<PixivIllustCard>(project_illust_card)
    .register_projector::<PixivDetailOverlay>(project_detail_overlay)
    .register_projector::<PixivOverlayTags>(project_overlay_tags)
    .register_projector::<OverlayTag>(project_overlay_tag)
    .add_systems(Startup, (setup_styles, setup))
    .add_systems(PreUpdate, drain_ui_actions_and_dispatch)
    .add_systems(
        Update,
        (
            track_viewport_metrics,
            spawn_network_tasks,
            apply_network_results,
            spawn_image_tasks,
            apply_image_results,
            animate_card_hover,
        ),
    );
    app
}

pub fn run() -> std::result::Result<(), EventLoopError> {
    run_app_with_window_options(build_app(), "Pixiv Desktop", |options| {
        options.with_initial_inner_size(LogicalSize::new(1360.0, 860.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_xilem::bevy_ecs::schedule::Schedule;

    #[test]
    fn feed_layout_scales_with_viewport_width() {
        let (narrow_columns, _) = compute_feed_layout(900.0, false);
        let (wide_columns, _) = compute_feed_layout(1700.0, false);

        assert!(wide_columns >= narrow_columns);
        assert!(wide_columns > 1);
    }

    #[test]
    fn collapsed_sidebar_yields_more_card_space() {
        let (expanded_columns, expanded_card_width) = compute_feed_layout(1360.0, false);
        let (collapsed_columns, collapsed_card_width) = compute_feed_layout(1360.0, true);

        assert!(collapsed_columns >= expanded_columns);
        assert!(collapsed_card_width >= expanded_card_width);
    }

    #[test]
    fn auth_code_can_be_extracted_from_nested_redirect() {
        let nested = "https://example.com/callback?redirect_uri=https%3A%2F%2Fapp.example.com%2Fauth%3Fcode%3Dabc123";
        assert_eq!(
            extract_auth_code_from_input(nested).as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn setup_builds_componentized_ui_tree() {
        let mut world = World::new();
        let mut schedule = Schedule::default();
        schedule.add_systems(setup);
        schedule.run(&mut world);

        let tree = *world.resource::<PixivUiTree>();
        let controls = *world.resource::<PixivControls>();
        assert!(world.get::<PixivHomeFeed>(tree.home_feed).is_some());
        assert!(world.get::<PixivOverlayTags>(tree.overlay_tags).is_some());
        assert!(world.get::<UiComboBox>(controls.locale_combo).is_some());
    }

    #[test]
    fn ensure_task_pool_initializes_io_pool() {
        ensure_task_pool_initialized();
        let _ = IoTaskPool::get();
    }

    #[test]
    fn pixiv_locale_ids_do_not_use_dot_namespace() {
        let locales = [
            (
                "en-US",
                include_str!("../../../assets/locales/en-US/main.ftl"),
            ),
            (
                "zh-CN",
                include_str!("../../../assets/locales/zh-CN/main.ftl"),
            ),
            (
                "ja-JP",
                include_str!("../../../assets/locales/ja-JP/main.ftl"),
            ),
        ];

        for (locale, content) in locales {
            assert!(
                !content
                    .lines()
                    .map(str::trim_start)
                    .any(|line| line.starts_with("pixiv.")),
                "{locale} locale still contains dot-separated pixiv message IDs"
            );
        }
    }
}
