#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::{
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use bevy_asset::{AssetPlugin, Assets, Handle, RenderAssetUsages};
use bevy_embedded_assets::{EmbeddedAssetPlugin, PluginMode};
use bevy_image::Image as BevyImage;
use bevy_text::TextPlugin;
use bevy_xilem::{
    AppBevyXilemExt, AppI18n, BevyXilemPlugin, LUCIDE_FONT_FAMILY, OverlayConfig, OverlayPlacement,
    OverlayState, ProjectionCtx, ResolvedStyle, StyleClass, StyleSheet, StyleValue,
    SyncAssetSource, SyncTextSource, UiComboBox, UiComboBoxChanged, UiComboOption, UiEventQueue,
    UiRoot, UiTextInput, UiTextInputChanged, UiThemePicker, UiView, apply_direct_widget_style,
    apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup, Update},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    bevy_tasks::{AsyncComputeTaskPool, IoTaskPool, TaskPool},
    bevy_tween::{
        BevyTweenRegisterSystems,
        bevy_time_runner::{TimeContext, TimeRunner, TimeSpan},
        component_tween_system,
        interpolate::Interpolator,
        interpolation::EaseKind,
        tween::ComponentTween,
    },
    bevy_window::WindowResized,
    button_with_child, resolve_style, resolve_style_for_classes, resolve_style_for_entity_classes,
    run_app_with_window_options, spawn_in_overlay_root,
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
#[cfg(target_os = "macos")]
use bevy_xilem_activation::MacosBundleConfig;
use bevy_xilem_activation::{
    ActivationConfig, ActivationService, BootstrapOutcome, ProtocolRegistration, bootstrap,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use lucide_icons::Icon as LucideIcon;
use pixiv_client::{
    AuthSession, DecodedImageRgba, IdpUrlResponse, Illust, PixivApiClient, PixivContentKind,
    PixivResponse, build_browser_login_url, generate_pkce_code_verifier, pkce_s256_challenge,
};
use reqwest::Url;
use shared_utils::init_logging;
use unic_langid::LanguageIdentifier;
use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

const CARD_BASE_WIDTH: f64 = 270.0;
const CARD_MIN_WIDTH: f64 = 260.0;
const CARD_ROW_GAP: f64 = 10.0;
const MAX_CARD_COLUMNS: usize = 6;
const SIDEBAR_EXPANDED_WIDTH: f64 = 208.0;
const SIDEBAR_COLLAPSED_WIDTH: f64 = 100.0;
const RESPONSE_PANEL_HEIGHT: f64 = 180.0;
const PIXIV_AUTH_TOKEN_FALLBACK: &str = "https://oauth.secure.pixiv.net/auth/token";
const PIXIV_WEB_REDIRECT_FALLBACK: &str =
    "https://app-api.pixiv.net/web/v1/users/auth/pixiv/callback";
const PIXIV_ACTIVATION_APP_ID: &str = "bevy-xilem-example-pixiv-client";

mod actions;
mod activation;
mod network;
mod persistence;
mod ui;

use actions::{drain_ui_actions_and_dispatch, track_viewport_metrics};
use activation::poll_activation_messages;
use network::{apply_image_results, apply_network_results, spawn_image_tasks, spawn_network_tasks};
use ui::{
    project_auth_panel, project_detail_overlay, project_home_feed, project_illust_card,
    project_main_column, project_overlay_tag, project_overlay_tags, project_response_panel,
    project_root, project_search_panel, project_sidebar,
};

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

#[cfg(target_os = "macos")]
fn pixiv_macos_bundle_config() -> MacosBundleConfig {
    MacosBundleConfig::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Info.plist"))
}

fn sync_font_stack_for_locale(sheet: &mut StyleSheet, stack: Option<&[String]>) {
    for class_name in [
        "pixiv.root",
        "pixiv.sidebar",
        "pixiv.sidebar.title",
        "pixiv.sidebar.button",
        "pixiv.auth-panel",
        "pixiv.text-input",
        "pixiv.button",
        "pixiv.primary-btn",
        "pixiv.card",
        "pixiv.tag",
        "pixiv.overlay",
    ] {
        if let Some(existing) = sheet.get_class_values(class_name).cloned() {
            let mut updated = existing;
            updated.font_family = stack.map(|stack| StyleValue::value(stack.to_vec()));
            sheet.set_class_values(class_name, updated);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavTab {
    Home,
    Rankings,
    Manga,
    Novels,
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
struct PixivUiComponents {
    toggle_sidebar: Entity,
    locale_combo: Entity,
    code_verifier_input: Entity,
    auth_code_input: Entity,
    refresh_token_input: Entity,
    search_input: Entity,
    home_tab: Entity,
    rankings_tab: Entity,
    manga_tab: Entity,
    novels_tab: Entity,
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

#[derive(Component, Debug, Clone, Copy)]
struct IllustActionEntities {
    open_thumbnail: Entity,
    bookmark: Entity,
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
    FetchManga,
    FetchNovels,
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

#[derive(Resource)]
struct ActivationBridge {
    service: Mutex<ActivationService>,
    startup_uris: Vec<String>,
}

#[derive(Clone, Copy)]
struct CardAnimLens {
    start: CardAnimState,
    end: CardAnimState,
}

impl Interpolator for CardAnimLens {
    type Item = CardAnimState;

    fn interpolate(&self, target: &mut Self::Item, ratio: f32, _previous_value: f32) {
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
    ease: EaseKind,
) {
    let duration = Duration::from_millis(duration_ms);
    world.entity_mut(entity).insert((
        TimeSpan::try_from(Duration::ZERO..duration)
            .expect("card tween duration range should be valid"),
        ease,
        ComponentTween::new_target(entity, CardAnimLens { start, end }),
        TimeRunner::new(duration),
        TimeContext::<()>::default(),
    ));
}

fn ensure_task_pool_initialized() {
    let _ = IoTaskPool::get_or_init(TaskPool::new);
    let _ = AsyncComputeTaskPool::get_or_init(TaskPool::new);
}

fn register_bridge_fonts(app: &mut App) {
    app.register_xilem_font(SyncAssetSource::Bytes(include_bytes!(
        "../../../assets/fonts/NotoSans-Regular.ttf",
    )));
    app.register_xilem_font(SyncAssetSource::Bytes(include_bytes!(
        "../../../assets/fonts/NotoSansCJKsc-Regular.otf",
    )));
    app.register_xilem_font(SyncAssetSource::Bytes(include_bytes!(
        "../../../assets/fonts/NotoSansCJKjp-Regular.otf",
    )));
    app.register_xilem_font(SyncAssetSource::Bytes(include_bytes!(
        "../../../assets/fonts/NotoSansCJKtc-Regular.otf",
    )));
    app.register_xilem_font(SyncAssetSource::Bytes(include_bytes!(
        "../../../assets/fonts/NotoSansCJKkr-Regular.otf",
    )));
}

fn spawn_ui_component_entity(commands: &mut Commands, classes: &[&str]) -> Entity {
    commands
        .spawn((StyleClass(
            classes.iter().map(|class| (*class).to_string()).collect(),
        ),))
        .id()
}

fn spawn_bound_text_input(
    commands: &mut Commands,
    parent: Entity,
    value: impl Into<String>,
    placeholder: impl Into<String>,
) -> Entity {
    commands
        .spawn((
            UiTextInput::new(value).with_placeholder(placeholder),
            StyleClass(vec!["pixiv.text-input".to_string()]),
            ChildOf(parent),
        ))
        .id()
}

fn set_text_input_component_value(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut input) = world.get_mut::<UiTextInput>(entity)
        && input.value != value
    {
        input.value = value.to_string();
    }
}

fn set_text_input_component_placeholder(world: &mut World, entity: Entity, placeholder: &str) {
    if let Some(mut input) = world.get_mut::<UiTextInput>(entity)
        && input.placeholder != placeholder
    {
        input.placeholder = placeholder.to_string();
    }
}

fn sync_bound_text_inputs(world: &mut World) {
    let Some(ui_components) = world.get_resource::<PixivUiComponents>().copied() else {
        return;
    };
    let search_text = world
        .get_resource::<UiState>()
        .map(|ui| ui.search_text.clone())
        .unwrap_or_default();
    let (code_verifier_input, auth_code_input, refresh_token_input) = world
        .get_resource::<AuthState>()
        .map(|auth| {
            (
                auth.code_verifier_input.clone(),
                auth.auth_code_input.clone(),
                auth.refresh_token_input.clone(),
            )
        })
        .unwrap_or_else(|| (String::new(), String::new(), String::new()));
    let placeholders = [
        (
            ui_components.code_verifier_input,
            tr(world, "pixiv.auth.placeholder.pkce", "PKCE code_verifier"),
        ),
        (
            ui_components.auth_code_input,
            tr(world, "pixiv.auth.placeholder.code", "Auth code"),
        ),
        (
            ui_components.refresh_token_input,
            tr(
                world,
                "pixiv.auth.placeholder.refresh_token",
                "Refresh token",
            ),
        ),
        (
            ui_components.search_input,
            tr(world, "pixiv.search.placeholder", "Search illust keyword"),
        ),
    ];

    set_text_input_component_value(world, ui_components.search_input, &search_text);
    set_text_input_component_value(
        world,
        ui_components.code_verifier_input,
        &code_verifier_input,
    );
    set_text_input_component_value(world, ui_components.auth_code_input, &auth_code_input);
    set_text_input_component_value(
        world,
        ui_components.refresh_token_input,
        &refresh_token_input,
    );

    for (entity, placeholder) in placeholders {
        set_text_input_component_placeholder(world, entity, &placeholder);
    }
}

fn setup(mut commands: Commands, i18n: Res<AppI18n>) {
    ensure_task_pool_initialized();

    let restored_session = persistence::load_auth_session()
        .map_err(|error| {
            eprintln!("pixiv credential restore failed: {error}");
            error
        })
        .ok()
        .flatten();

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
        status_line: if restored_session.is_some() {
            "Booting Pixiv MVP… restored saved credentials, refreshing token…".to_string()
        } else {
            "Booting Pixiv MVP…".to_string()
        },
        ..UiState::default()
    });
    commands.insert_resource(AuthState {
        session: restored_session.clone(),
        refresh_token_input: restored_session
            .as_ref()
            .map(|session| session.refresh_token.clone())
            .unwrap_or_default(),
        ..AuthState::default()
    });
    commands.insert_resource(FeedOrder::default());
    commands.insert_resource(OverlayTags::default());
    commands.insert_resource(ResponsePanelState::default());
    commands.insert_resource(ViewportMetrics::default());
    commands.insert_resource(PixivApiClient::default());
    commands.insert_resource(Assets::<BevyImage>::default());

    let mut ui_components = PixivUiComponents {
        toggle_sidebar: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.sidebar"],
        ),
        locale_combo: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.sidebar"],
        ),
        code_verifier_input: Entity::PLACEHOLDER,
        auth_code_input: Entity::PLACEHOLDER,
        refresh_token_input: Entity::PLACEHOLDER,
        search_input: Entity::PLACEHOLDER,
        home_tab: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.subtle"],
        ),
        rankings_tab: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.subtle"],
        ),
        manga_tab: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.subtle"],
        ),
        novels_tab: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.subtle"],
        ),
        search_tab: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.subtle"],
        ),
        open_browser_login: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        exchange_auth_code: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        refresh_token: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        search_submit: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        copy_response: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.primary"],
        ),
        clear_response: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.warn"],
        ),
        close_overlay: spawn_ui_component_entity(
            &mut commands,
            &["pixiv.button", "pixiv.button.warn"],
        ),
    };
    let root = commands
        .spawn((
            UiRoot,
            PixivRoot,
            StyleClass(vec!["pixiv.root".to_string()]),
        ))
        .id();

    commands.spawn((UiThemePicker::fluent(), ChildOf(root)));

    let sidebar = commands
        .spawn((
            PixivSidebar,
            StyleClass(vec!["pixiv.sidebar".to_string()]),
            ChildOf(root),
        ))
        .id();

    let locale_options = vec![
        UiComboOption::new("en-US", "English"),
        UiComboOption::new("zh-CN", "简体中文"),
        UiComboOption::new("ja-JP", "日本語"),
    ];
    let active_locale_tag = i18n.active_locale.to_string();
    let selected_locale = locale_options
        .iter()
        .position(|option| {
            option
                .value
                .eq_ignore_ascii_case(active_locale_tag.as_str())
        })
        .unwrap_or(0);

    let mut locale_combo = UiComboBox::new(locale_options).with_placeholder("Language");
    locale_combo.selected = selected_locale;

    commands
        .entity(ui_components.locale_combo)
        .insert((locale_combo, ChildOf(sidebar)));

    let main_column = commands.spawn((PixivMainColumn, ChildOf(root))).id();

    let auth_panel = commands
        .spawn((
            PixivAuthPanel,
            StyleClass(vec!["pixiv.auth-panel".to_string()]),
            ChildOf(main_column),
        ))
        .id();
    commands.spawn((PixivResponsePanel, ChildOf(main_column)));
    let search_panel = commands
        .spawn((PixivSearchPanel, ChildOf(main_column)))
        .id();

    ui_components.code_verifier_input =
        spawn_bound_text_input(&mut commands, auth_panel, "", "PKCE code_verifier");
    ui_components.auth_code_input =
        spawn_bound_text_input(&mut commands, auth_panel, "", "Auth code");
    ui_components.refresh_token_input = spawn_bound_text_input(
        &mut commands,
        auth_panel,
        restored_session
            .as_ref()
            .map(|session| session.refresh_token.clone())
            .unwrap_or_default(),
        "Refresh token",
    );
    ui_components.search_input =
        spawn_bound_text_input(&mut commands, search_panel, "", "Search illust keyword");

    commands.insert_resource(ui_components);
    commands.queue(sync_bound_text_inputs);

    let home_feed = commands.spawn((PixivHomeFeed, ChildOf(main_column))).id();

    commands.queue(move |world: &mut World| {
        let detail_overlay = spawn_in_overlay_root(
            world,
            (
                PixivDetailOverlay,
                StyleClass(vec!["pixiv.overlay".to_string()]),
                OverlayState {
                    is_modal: true,
                    anchor: None,
                },
                OverlayConfig {
                    placement: OverlayPlacement::Center,
                    anchor: None,
                    auto_flip: false,
                },
            ),
        );

        let overlay_tags = world
            .spawn((PixivOverlayTags, ChildOf(detail_overlay)))
            .id();

        world.insert_resource(PixivUiTree {
            home_feed,
            overlay_tags,
        });
    });

    let _ = cmd_tx.send(NetworkCommand::DiscoverIdp);

    if let Some(session) = restored_session {
        let _ = cmd_tx.send(NetworkCommand::Refresh {
            refresh_token: session.refresh_token,
        });
    }
}

fn setup_styles(mut sheet: ResMut<StyleSheet>, i18n: Option<Res<AppI18n>>) {
    let font_stack = i18n
        .as_ref()
        .map(|current| current.get_font_stack())
        .filter(|stack| !stack.is_empty());

    sync_font_stack_for_locale(&mut sheet, font_stack.as_deref());
}

bevy_xilem::impl_ui_component_template!(PixivRoot, project_root);
bevy_xilem::impl_ui_component_template!(PixivSidebar, project_sidebar);
bevy_xilem::impl_ui_component_template!(PixivMainColumn, project_main_column);
bevy_xilem::impl_ui_component_template!(PixivAuthPanel, project_auth_panel);
bevy_xilem::impl_ui_component_template!(PixivResponsePanel, project_response_panel);
bevy_xilem::impl_ui_component_template!(PixivSearchPanel, project_search_panel);
bevy_xilem::impl_ui_component_template!(PixivHomeFeed, project_home_feed);
bevy_xilem::impl_ui_component_template!(PixivIllustCard, project_illust_card);
bevy_xilem::impl_ui_component_template!(PixivDetailOverlay, project_detail_overlay);
bevy_xilem::impl_ui_component_template!(PixivOverlayTags, project_overlay_tags);
bevy_xilem::impl_ui_component_template!(OverlayTag, project_overlay_tag);

fn build_app(mut activation_service: Option<ActivationService>) -> App {
    ensure_task_pool_initialized();
    init_logging();

    let mut app = App::new();
    register_bridge_fonts(&mut app);

    if let Some(mut service) = activation_service.take() {
        let startup_uris = service.take_startup_uris();
        app.insert_resource(ActivationBridge {
            service: Mutex::new(service),
            startup_uris,
        });
    }

    app.add_plugins((
        EmbeddedAssetPlugin {
            mode: PluginMode::ReplaceDefault,
        },
        AssetPlugin::default(),
        TextPlugin::default(),
        BevyXilemPlugin,
    ))
    .load_style_sheet_ron(include_str!("../assets/themes/pixiv_client.ron"))
    .insert_resource(AppI18n::new(parse_locale("en-US")))
    .register_i18n_bundle(
        "en-US",
        SyncTextSource::String(include_str!("../assets/locales/en-US/main.ftl")),
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
        SyncTextSource::String(include_str!("../assets/locales/zh-CN/main.ftl")),
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
        SyncTextSource::String(include_str!("../assets/locales/ja-JP/main.ftl")),
        vec![
            "Inter",
            "Noto Sans CJK JP",
            "Noto Sans CJK SC",
            "Noto Sans CJK TC",
            "Noto Sans CJK KR",
            "sans-serif",
        ],
    )
    .register_ui_component::<PixivRoot>()
    .register_ui_component::<PixivSidebar>()
    .register_ui_component::<PixivMainColumn>()
    .register_ui_component::<PixivAuthPanel>()
    .register_ui_component::<PixivResponsePanel>()
    .register_ui_component::<PixivSearchPanel>()
    .register_ui_component::<PixivHomeFeed>()
    .register_ui_component::<PixivIllustCard>()
    .register_ui_component::<PixivDetailOverlay>()
    .register_ui_component::<PixivOverlayTags>()
    .register_ui_component::<OverlayTag>()
    .add_tween_systems(Update, component_tween_system::<CardAnimLens>())
    .add_systems(Startup, (setup_styles, setup))
    .add_systems(
        Update,
        (
            drain_ui_actions_and_dispatch
                .after(bevy_xilem::handle_widget_actions)
                .after(bevy_xilem::handle_overlay_actions),
            poll_activation_messages,
            track_viewport_metrics,
            spawn_network_tasks,
            apply_network_results,
            spawn_image_tasks,
            apply_image_results,
        )
            .chain(),
    );
    app
}

pub fn run() -> std::result::Result<(), EventLoopError> {
    let mut protocol = ProtocolRegistration::new("pixiv", "Pixiv OAuth callback", None);
    #[cfg(target_os = "macos")]
    {
        protocol = protocol.with_macos_bundle(pixiv_macos_bundle_config());
    }

    let activation_config = ActivationConfig::new(PIXIV_ACTIVATION_APP_ID).with_protocol(protocol);

    let activation_service = match bootstrap(activation_config) {
        Ok(BootstrapOutcome::Primary(service)) => Some(service),
        Ok(BootstrapOutcome::SecondaryForwarded) => return Ok(()),
        Err(error) => {
            eprintln!("activation bootstrap failed: {error}");
            None
        }
    };

    run_app_with_window_options(build_app(activation_service), "Pixiv Desktop", |options| {
        options.with_initial_inner_size(LogicalSize::new(1360.0, 860.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_xilem::bevy_ecs::schedule::Schedule;

    fn mock_illust(title: &str) -> Illust {
        Illust {
            id: 1,
            title: title.to_string(),
            image_urls: pixiv_client::ImageUrls {
                medium: "https://example.com/m.jpg".to_string(),
                large: "https://example.com/l.jpg".to_string(),
                square_medium: "https://example.com/s.jpg".to_string(),
            },
            user: pixiv_client::User {
                id: 9,
                name: "artist".to_string(),
                account: Some("artist_account".to_string()),
                profile_image_urls: pixiv_client::ProfileImageUrls {
                    medium: "https://example.com/avatar.jpg".to_string(),
                },
            },
            tags: Vec::new(),
            total_view: 0,
            total_bookmarks: 0,
            total_comments: 0,
            is_bookmarked: false,
            page_count: 1,
            meta_single_page: None,
            content_kind: pixiv_client::PixivContentKind::Illust,
            description: None,
        }
    }

    #[test]
    fn feed_layout_scales_with_viewport_width() {
        let (narrow_columns, _) = ui::compute_feed_layout(900.0, false);
        let (wide_columns, _) = ui::compute_feed_layout(1700.0, false);

        assert!(wide_columns >= narrow_columns);
        assert!(wide_columns > 1);
    }

    #[test]
    fn collapsed_sidebar_yields_more_card_space() {
        let (expanded_columns, expanded_card_width) = ui::compute_feed_layout(1360.0, false);
        let (collapsed_columns, collapsed_card_width) = ui::compute_feed_layout(1360.0, true);

        assert!(collapsed_columns >= expanded_columns);
        assert!(collapsed_card_width >= expanded_card_width);
    }

    #[test]
    fn card_height_estimator_reflects_title_length() {
        let mut world = World::new();

        let short = world
            .spawn((mock_illust("short"), IllustVisual::default()))
            .id();
        let long = world
            .spawn((
                mock_illust(
                    "a very long illustration title that should wrap to multiple lines in cards",
                ),
                IllustVisual::default(),
            ))
            .id();

        let short_h = ui::estimate_illust_card_height(&world, short, 280.0);
        let long_h = ui::estimate_illust_card_height(&world, long, 280.0);

        assert!(long_h > short_h);
    }

    #[test]
    fn locale_combo_event_applies_even_without_app_action_events() {
        let mut world = World::new();
        world.insert_resource(UiEventQueue::default());
        world.insert_resource(AppI18n::new(parse_locale("en-US")));
        world.insert_resource(StyleSheet::default());
        world.insert_resource(UiState::default());

        let locale_combo = world
            .spawn((UiComboBox::new(vec![
                UiComboOption::new("en-US", "English"),
                UiComboOption::new("zh-CN", "简体中文"),
                UiComboOption::new("ja-JP", "日本語"),
            ]),))
            .id();
        world.insert_resource(PixivUiComponents {
            toggle_sidebar: Entity::PLACEHOLDER,
            locale_combo,
            code_verifier_input: Entity::PLACEHOLDER,
            auth_code_input: Entity::PLACEHOLDER,
            refresh_token_input: Entity::PLACEHOLDER,
            search_input: Entity::PLACEHOLDER,
            home_tab: Entity::PLACEHOLDER,
            rankings_tab: Entity::PLACEHOLDER,
            manga_tab: Entity::PLACEHOLDER,
            novels_tab: Entity::PLACEHOLDER,
            search_tab: Entity::PLACEHOLDER,
            open_browser_login: Entity::PLACEHOLDER,
            exchange_auth_code: Entity::PLACEHOLDER,
            refresh_token: Entity::PLACEHOLDER,
            search_submit: Entity::PLACEHOLDER,
            copy_response: Entity::PLACEHOLDER,
            clear_response: Entity::PLACEHOLDER,
            close_overlay: Entity::PLACEHOLDER,
        });

        world.resource::<UiEventQueue>().push_typed(
            locale_combo,
            UiComboBoxChanged {
                combo: locale_combo,
                selected: 1,
                value: "zh-CN".to_string(),
            },
        );

        drain_ui_actions_and_dispatch(&mut world);

        assert_eq!(
            world.resource::<AppI18n>().active_locale,
            parse_locale("zh-CN")
        );
        assert_eq!(
            world
                .get::<UiComboBox>(locale_combo)
                .and_then(UiComboBox::clamped_selected),
            Some(1)
        );
    }

    #[test]
    fn auth_code_can_be_extracted_from_nested_redirect() {
        let nested = "https://example.com/callback?redirect_uri=https%3A%2F%2Fapp.example.com%2Fauth%3Fcode%3Dabc123";
        assert_eq!(
            activation::extract_auth_code_from_input(nested).as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn pixiv_custom_scheme_auth_code_is_supported() {
        let uri = "pixiv://account/login?code=from_protocol&via=login";
        assert_eq!(
            activation::extract_auth_code_from_input(uri).as_deref(),
            Some("from_protocol")
        );
        assert!(activation::is_pixiv_callback_uri(uri));
    }

    #[test]
    fn info_plist_keeps_expected_bundle_identifier() {
        let plist = include_str!("../Info.plist");
        assert!(
            plist.contains("<string>dev.summpot.example-pixiv-client</string>"),
            "Info.plist should keep the Pixiv app bundle identifier stable"
        );
    }

    #[test]
    fn app_actions_emitted_in_preupdate_are_drained_in_update() {
        let mut app = App::new();
        app.insert_resource(UiEventQueue::default());
        app.insert_resource(UiState::default());
        app.insert_resource(PixivUiComponents {
            toggle_sidebar: Entity::PLACEHOLDER,
            locale_combo: Entity::PLACEHOLDER,
            code_verifier_input: Entity::PLACEHOLDER,
            auth_code_input: Entity::PLACEHOLDER,
            refresh_token_input: Entity::PLACEHOLDER,
            search_input: Entity::PLACEHOLDER,
            home_tab: Entity::PLACEHOLDER,
            rankings_tab: Entity::PLACEHOLDER,
            manga_tab: Entity::PLACEHOLDER,
            novels_tab: Entity::PLACEHOLDER,
            search_tab: Entity::PLACEHOLDER,
            open_browser_login: Entity::PLACEHOLDER,
            exchange_auth_code: Entity::PLACEHOLDER,
            refresh_token: Entity::PLACEHOLDER,
            search_submit: Entity::PLACEHOLDER,
            copy_response: Entity::PLACEHOLDER,
            clear_response: Entity::PLACEHOLDER,
            close_overlay: Entity::PLACEHOLDER,
        });

        app.add_systems(PreUpdate, |queue: Res<UiEventQueue>| {
            queue.push_typed(
                Entity::PLACEHOLDER,
                AppAction::SetSearchText("same-frame text".to_string()),
            );
        });
        app.add_systems(Update, drain_ui_actions_and_dispatch);

        app.update();

        assert_eq!(
            app.world().resource::<UiState>().search_text,
            "same-frame text"
        );
    }

    #[test]
    fn setup_builds_componentized_ui_tree() {
        let mut world = World::new();
        world.insert_resource(AppI18n::new(parse_locale("en-US")));
        let mut schedule = Schedule::default();
        schedule.add_systems(setup);
        schedule.run(&mut world);

        let tree = *world.resource::<PixivUiTree>();
        let ui_components = *world.resource::<PixivUiComponents>();
        assert!(world.get::<PixivHomeFeed>(tree.home_feed).is_some());
        assert!(world.get::<PixivOverlayTags>(tree.overlay_tags).is_some());
        let overlay_parent = world
            .get::<ChildOf>(tree.overlay_tags)
            .map(ChildOf::parent)
            .expect("overlay tags should be parented to detail overlay");
        let overlay_state = world
            .get::<OverlayState>(overlay_parent)
            .expect("detail overlay should carry OverlayState");
        assert!(overlay_state.is_modal);
        assert!(
            world
                .get::<UiComboBox>(ui_components.locale_combo)
                .is_some()
        );
        assert!(
            world
                .get::<UiTextInput>(ui_components.code_verifier_input)
                .is_some()
        );
        assert!(
            world
                .get::<UiTextInput>(ui_components.auth_code_input)
                .is_some()
        );
        assert!(
            world
                .get::<UiTextInput>(ui_components.refresh_token_input)
                .is_some()
        );
        assert!(
            world
                .get::<UiTextInput>(ui_components.search_input)
                .is_some()
        );
        assert!(world.get_entity(ui_components.manga_tab).is_ok());
        assert!(world.get_entity(ui_components.novels_tab).is_ok());
        assert_eq!(
            world
                .get::<UiComboBox>(ui_components.locale_combo)
                .and_then(UiComboBox::clamped_selected),
            Some(0)
        );
    }

    #[test]
    fn ensure_task_pool_initializes_io_pool() {
        ensure_task_pool_initialized();
        let _ = IoTaskPool::get();
    }

    #[test]
    fn pixiv_locale_ids_do_not_use_dot_namespace() {
        let locales = [
            ("en-US", include_str!("../assets/locales/en-US/main.ftl")),
            ("zh-CN", include_str!("../assets/locales/zh-CN/main.ftl")),
            ("ja-JP", include_str!("../assets/locales/ja-JP/main.ftl")),
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

    #[test]
    fn text_input_events_and_programmatic_updates_stay_in_sync() {
        let mut world = World::new();
        world.insert_resource(UiEventQueue::default());
        world.insert_resource(AppI18n::new(parse_locale("en-US")));
        world.insert_resource(StyleSheet::default());
        world.insert_resource(UiState::default());
        world.insert_resource(AuthState::default());

        let code_verifier_input = world
            .spawn((UiTextInput::new("").with_placeholder("PKCE code_verifier"),))
            .id();
        let auth_code_input = world
            .spawn((UiTextInput::new("").with_placeholder("Auth code"),))
            .id();
        let refresh_token_input = world
            .spawn((UiTextInput::new("").with_placeholder("Refresh token"),))
            .id();
        let search_input = world
            .spawn((UiTextInput::new("").with_placeholder("Search illust keyword"),))
            .id();

        world.insert_resource(PixivUiComponents {
            toggle_sidebar: Entity::PLACEHOLDER,
            locale_combo: Entity::PLACEHOLDER,
            code_verifier_input,
            auth_code_input,
            refresh_token_input,
            search_input,
            home_tab: Entity::PLACEHOLDER,
            rankings_tab: Entity::PLACEHOLDER,
            manga_tab: Entity::PLACEHOLDER,
            novels_tab: Entity::PLACEHOLDER,
            search_tab: Entity::PLACEHOLDER,
            open_browser_login: Entity::PLACEHOLDER,
            exchange_auth_code: Entity::PLACEHOLDER,
            refresh_token: Entity::PLACEHOLDER,
            search_submit: Entity::PLACEHOLDER,
            copy_response: Entity::PLACEHOLDER,
            clear_response: Entity::PLACEHOLDER,
            close_overlay: Entity::PLACEHOLDER,
        });

        world.resource::<UiEventQueue>().push_typed(
            search_input,
            UiTextInputChanged {
                input: search_input,
                value: "same-frame keyword".to_string(),
            },
        );

        drain_ui_actions_and_dispatch(&mut world);

        assert_eq!(
            world.resource::<UiState>().search_text,
            "same-frame keyword"
        );
        assert_eq!(
            world
                .get::<UiTextInput>(search_input)
                .expect("search input should exist")
                .value,
            "same-frame keyword"
        );

        world.resource::<UiEventQueue>().push_typed(
            Entity::PLACEHOLDER,
            AppAction::SetSearchText("猫咪".to_string()),
        );

        drain_ui_actions_and_dispatch(&mut world);

        assert_eq!(world.resource::<UiState>().search_text, "猫咪");
        assert_eq!(
            world
                .get::<UiTextInput>(search_input)
                .expect("search input should exist")
                .value,
            "猫咪"
        );
    }

    #[test]
    fn drain_dispatch_consumes_pending_widget_text_actions_before_sync() {
        let mut world = World::new();
        world.insert_resource(UiEventQueue::default());
        world.insert_resource(AppI18n::new(parse_locale("en-US")));
        world.insert_resource(StyleSheet::default());
        world.insert_resource(UiState::default());
        world.insert_resource(AuthState::default());

        let code_verifier_input = world
            .spawn((UiTextInput::new("").with_placeholder("PKCE code_verifier"),))
            .id();
        let auth_code_input = world
            .spawn((UiTextInput::new("").with_placeholder("Auth code"),))
            .id();
        let refresh_token_input = world
            .spawn((UiTextInput::new("").with_placeholder("Refresh token"),))
            .id();
        let search_input = world
            .spawn((UiTextInput::new("").with_placeholder("Search illust keyword"),))
            .id();

        world.insert_resource(PixivUiComponents {
            toggle_sidebar: Entity::PLACEHOLDER,
            locale_combo: Entity::PLACEHOLDER,
            code_verifier_input,
            auth_code_input,
            refresh_token_input,
            search_input,
            home_tab: Entity::PLACEHOLDER,
            rankings_tab: Entity::PLACEHOLDER,
            manga_tab: Entity::PLACEHOLDER,
            novels_tab: Entity::PLACEHOLDER,
            search_tab: Entity::PLACEHOLDER,
            open_browser_login: Entity::PLACEHOLDER,
            exchange_auth_code: Entity::PLACEHOLDER,
            refresh_token: Entity::PLACEHOLDER,
            search_submit: Entity::PLACEHOLDER,
            copy_response: Entity::PLACEHOLDER,
            clear_response: Entity::PLACEHOLDER,
            close_overlay: Entity::PLACEHOLDER,
        });

        world.resource::<UiEventQueue>().push_typed(
            search_input,
            bevy_xilem::WidgetUiAction::SetTextInput {
                input: search_input,
                value: "same-frame widget action".to_string(),
            },
        );

        drain_ui_actions_and_dispatch(&mut world);

        assert_eq!(
            world.resource::<UiState>().search_text,
            "same-frame widget action"
        );
        assert_eq!(
            world
                .get::<UiTextInput>(search_input)
                .expect("search input should exist")
                .value,
            "same-frame widget action"
        );
    }

    #[test]
    fn embedded_pixiv_theme_ron_parses() {
        bevy_xilem::parse_stylesheet_ron(include_str!("../assets/themes/pixiv_client.ron"))
            .expect("embedded pixiv_client stylesheet should parse");
    }

    #[test]
    fn pixiv_primary_button_uses_neutral_fluent_tokens() {
        let sheet =
            bevy_xilem::parse_stylesheet_ron(include_str!("../assets/themes/pixiv_client.ron"))
                .expect("embedded pixiv_client stylesheet should parse");

        let button = sheet
            .get_class_values("pixiv.button")
            .expect("pixiv.button class should exist");
        let primary = sheet
            .get_class_values("pixiv.button.primary")
            .expect("pixiv.button.primary class should exist");

        let corner_radius = match button.layout.corner_radius.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.button corner_radius should come from a theme token"),
        };
        let primary_bg = match primary.colors.bg.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.button.primary bg should come from a theme token"),
        };
        let primary_border = match primary.colors.border.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.button.primary border should come from a theme token"),
        };

        assert_eq!(corner_radius, "radius-md");
        assert_eq!(primary_bg, "surface-panel");
        assert_eq!(primary_border, "border-default");
    }

    #[test]
    fn pixiv_text_input_uses_neutral_fluent_tokens() {
        let sheet =
            bevy_xilem::parse_stylesheet_ron(include_str!("../assets/themes/pixiv_client.ron"))
                .expect("embedded pixiv_client stylesheet should parse");

        let input = sheet
            .get_class_values("pixiv.text-input")
            .expect("pixiv.text-input class should exist");

        let bg = match input.colors.bg.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.text-input bg should come from a theme token"),
        };
        let border = match input.colors.border.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.text-input border should come from a theme token"),
        };

        assert_eq!(bg, "surface-subtle");
        assert_eq!(border, "border-default");
    }

    #[test]
    fn pixiv_warn_button_uses_fluent_tokens() {
        let sheet =
            bevy_xilem::parse_stylesheet_ron(include_str!("../assets/themes/pixiv_client.ron"))
                .expect("embedded pixiv_client stylesheet should parse");
        let warn = sheet
            .get_class_values("pixiv.button.warn")
            .expect("pixiv.button.warn class should exist");

        let bg = match warn.colors.bg.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.button.warn bg should come from a theme token"),
        };
        let hover_bg = match warn.colors.hover_bg.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.button.warn hover_bg should come from a theme token"),
        };
        let pressed_bg = match warn.colors.pressed_bg.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.button.warn pressed_bg should come from a theme token"),
        };
        let border = match warn.colors.border.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.button.warn border should come from a theme token"),
        };
        let text = match warn.colors.text.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.button.warn text should come from a theme token"),
        };

        assert_eq!(bg, "status-error-bg");
        assert_eq!(hover_bg, "status-error-border");
        assert_eq!(pressed_bg, "surface-overlay-item-pressed");
        assert_eq!(border, "status-error-border");
        assert_eq!(text, "text-primary");
    }

    #[test]
    fn sync_font_stack_for_locale_preserves_tokenized_fields() {
        let mut sheet =
            bevy_xilem::parse_stylesheet_ron(include_str!("../assets/themes/pixiv_client.ron"))
                .expect("embedded pixiv_client stylesheet should parse");

        // Pixiv sheet intentionally carries class rules with token refs but no local token map.
        assert!(sheet.tokens.is_empty());

        let stack = vec!["Inter".to_string(), "sans-serif".to_string()];
        sync_font_stack_for_locale(&mut sheet, Some(&stack));

        let root = sheet
            .get_class_values("pixiv.root")
            .expect("pixiv.root class should exist");

        let padding_token = match root.layout.padding.as_ref() {
            Some(bevy_xilem::StyleValue::Var(token)) => token.as_str(),
            _ => panic!("pixiv.root padding should remain tokenized"),
        };
        assert_eq!(padding_token, "space-lg");

        let font_family = match root.font_family.as_ref() {
            Some(bevy_xilem::StyleValue::Value(value)) => value,
            _ => panic!("font family should be written as a literal style value"),
        };
        assert_eq!(font_family, &stack);

        let sidebar_button = sheet
            .get_class_values("pixiv.sidebar.button")
            .expect("pixiv.sidebar.button class should exist");
        let sidebar_font_family = match sidebar_button.font_family.as_ref() {
            Some(bevy_xilem::StyleValue::Value(value)) => value,
            _ => panic!("sidebar button font family should be written as a literal style value"),
        };
        assert_eq!(sidebar_font_family, &stack);
    }

    #[test]
    fn locale_combo_initial_selection_follows_active_locale() {
        let mut world = World::new();
        world.insert_resource(AppI18n::new(parse_locale("ja-JP")));

        let mut schedule = Schedule::default();
        schedule.add_systems(setup);
        schedule.run(&mut world);

        let ui_components = *world.resource::<PixivUiComponents>();
        let combo = world
            .get::<UiComboBox>(ui_components.locale_combo)
            .expect("locale combo should exist");
        let selected = combo
            .clamped_selected()
            .expect("locale combo should select active locale");

        assert_eq!(combo.options[selected].value, "ja-JP");
    }

    #[test]
    fn pixiv_ui_drops_legacy_unicode_icons() {
        let app_source = include_str!("app.rs");
        let ui_source = include_str!("app/ui.rs");
        let source = format!("{app_source}\n{ui_source}");
        for codepoint in [0x25B6, 0x25C0, 0x1F464, 0x1F441, 0x2764, 0x2665, 0x2661] {
            let legacy_icon = char::from_u32(codepoint)
                .expect("valid unicode codepoint")
                .to_string();
            assert!(
                !source.contains(&legacy_icon),
                "legacy unicode icon `{legacy_icon}` should be replaced by lucide"
            );
        }
        assert!(
            source.contains("LucideIcon"),
            "pixiv client should use lucide icons in app UI"
        );
    }
}
