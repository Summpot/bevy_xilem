use std::{f32::consts::PI, process::Command, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use bevy_asset::{Assets, Handle, RenderAssetUsages};
use bevy_image::Image as BevyImage;
use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx, StyleClass,
    StyleSetter, StyleSheet, StyleTransition, TextStyle, UiEventQueue, UiRoot, UiView,
    apply_label_style, apply_text_input_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup, Update},
    bevy_ecs::prelude::*,
    bevy_tasks::{AsyncComputeTaskPool, TaskPool},
    bevy_tweening::{EaseMethod, Lens, Tween, TweenAnim},
    button, resolve_style, resolve_style_for_classes, run_app_with_window_options, text_input,
    xilem::{
        Color,
        masonry::layout::Length,
        view::{
            CrossAxisAlignment, FlexExt as _, MainAxisAlignment, flex_col, flex_row, image, label,
            sized_box, virtual_scroll,
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
use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

const CARD_BASE_WIDTH: f64 = 270.0;
const CARD_BASE_HEIGHT: f64 = 310.0;
const CARDS_PER_ROW: usize = 3;
const AUTH_PANEL_WIDTH: f64 = 640.0;
const RESPONSE_PANEL_HEIGHT: f64 = 180.0;
const PIXIV_AUTH_TOKEN_FALLBACK: &str = "https://oauth.secure.pixiv.net/auth/token";
const PIXIV_WEB_REDIRECT_FALLBACK: &str =
    "https://app-api.pixiv.net/web/v1/users/auth/pixiv/callback";

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

#[derive(Component, Debug, Clone, Copy)]
struct PixivRoot;

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
    let _ = AsyncComputeTaskPool::get_or_init(TaskPool::new);
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
    commands.insert_resource(PixivApiClient::default());
    commands.insert_resource(Assets::<BevyImage>::default());

    commands.spawn((
        UiRoot,
        PixivRoot,
        StyleClass(vec!["pixiv.root".to_string()]),
    ));

    let _ = cmd_tx.send(NetworkCommand::DiscoverIdp);
}

fn setup_styles(mut sheet: ResMut<StyleSheet>) {
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
        "pixiv.primary-btn",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                corner_radius: Some(6.0),
                border_width: Some(0.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x00, 0x96, 0xFA)),
                hover_bg: Some(Color::from_rgb8(0x14, 0xA2, 0xFA)),
                pressed_bg: Some(Color::from_rgb8(0x00, 0x7C, 0xD0)),
                text: Some(Color::WHITE),
                ..ColorStyle::default()
            },
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
            ..StyleSetter::default()
        },
    );
}

fn project_root(_: &PixivRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let root_style = resolve_style(ctx.world, ctx.entity);
    let ui = ctx.world.resource::<UiState>();
    let auth = ctx.world.resource::<AuthState>();

    let sidebar_style = resolve_style_for_classes(ctx.world, ["pixiv.sidebar"]);
    let btn_style = resolve_style_for_classes(ctx.world, ["pixiv.primary-btn"]);

    let sidebar = {
        let mut items = Vec::new();
        items.push(
            apply_widget_style(
                button(
                    ctx.entity,
                    AppAction::ToggleSidebar,
                    if ui.sidebar_collapsed { ">>" } else { "<<" },
                ),
                &btn_style,
            )
            .into_any_flex(),
        );

        if !ui.sidebar_collapsed {
            items.push(
                apply_widget_style(
                    button(ctx.entity, AppAction::SetTab(NavTab::Home), "Home"),
                    &btn_style,
                )
                .into_any_flex(),
            );
            items.push(
                apply_widget_style(
                    button(ctx.entity, AppAction::SetTab(NavTab::Rankings), "Rankings"),
                    &btn_style,
                )
                .into_any_flex(),
            );
            items.push(
                apply_widget_style(
                    button(ctx.entity, AppAction::SetTab(NavTab::Search), "Search"),
                    &btn_style,
                )
                .into_any_flex(),
            );
        }

        Arc::new(apply_widget_style(
            flex_col(items).cross_axis_alignment(CrossAxisAlignment::Start),
            &sidebar_style,
        )) as UiView
    };

    let auth_panel = {
        let mut rows = Vec::new();

        rows.push(
            apply_label_style(
                label(format!(
                    "Auth endpoint: {}",
                    auth.idp_urls
                        .as_ref()
                        .map(|i| i.auth_token_url.as_str())
                        .unwrap_or("loading…")
                )),
                &root_style,
            )
            .into_any_flex(),
        );

        rows.push(
            sized_box(apply_text_input_style(
                text_input(
                    ctx.entity,
                    auth.code_verifier_input.clone(),
                    AppAction::SetCodeVerifier,
                )
                .placeholder("PKCE code_verifier"),
                &root_style,
            ))
            .fixed_width(Length::px(AUTH_PANEL_WIDTH))
            .into_any_flex(),
        );
        rows.push(
            sized_box(apply_text_input_style(
                text_input(
                    ctx.entity,
                    auth.auth_code_input.clone(),
                    AppAction::SetAuthCode,
                )
                .placeholder("Auth code"),
                &root_style,
            ))
            .fixed_width(Length::px(AUTH_PANEL_WIDTH))
            .into_any_flex(),
        );
        rows.push(
            apply_widget_style(
                button(
                    ctx.entity,
                    AppAction::OpenBrowserLogin,
                    "Open Browser Login",
                ),
                &btn_style,
            )
            .into_any_flex(),
        );
        rows.push(
            apply_widget_style(
                button(ctx.entity, AppAction::ExchangeAuthCode, "Login (auth_code)"),
                &btn_style,
            )
            .into_any_flex(),
        );
        rows.push(
            sized_box(apply_text_input_style(
                text_input(
                    ctx.entity,
                    auth.refresh_token_input.clone(),
                    AppAction::SetRefreshToken,
                )
                .placeholder("Refresh token"),
                &root_style,
            ))
            .fixed_width(Length::px(AUTH_PANEL_WIDTH))
            .into_any_flex(),
        );
        rows.push(
            apply_widget_style(
                button(ctx.entity, AppAction::RefreshToken, "Refresh Token"),
                &btn_style,
            )
            .into_any_flex(),
        );

        Arc::new(
            sized_box(flex_col(rows).cross_axis_alignment(CrossAxisAlignment::Start))
                .fixed_width(Length::px(AUTH_PANEL_WIDTH + 10.0)),
        ) as UiView
    };

    let response_panel = build_response_panel(ctx.world, ctx.entity);

    let grid = build_feed_grid(ctx.world, ctx.entity);

    let search_bar = if ui.active_tab == NavTab::Search {
        let search_line = flex_row((
            apply_text_input_style(
                text_input(ctx.entity, ui.search_text.clone(), AppAction::SetSearchText)
                    .placeholder("Search illust keyword"),
                &root_style,
            )
            .flex(1.0),
            apply_widget_style(
                button(ctx.entity, AppAction::SubmitSearch, "Search"),
                &btn_style,
            ),
        ));
        Arc::new(search_line) as UiView
    } else {
        Arc::new(label("")) as UiView
    };

    let status_label = Arc::new(apply_label_style(
        label(ui.status_line.clone()),
        &root_style,
    )) as UiView;

    let main_content = Arc::new(
        flex_col((
            status_label.into_any_flex(),
            auth_panel.into_any_flex(),
            response_panel.into_any_flex(),
            search_bar.into_any_flex(),
            grid.into_any_flex(),
            build_detail_overlay(ctx.world, ctx.entity).into_any_flex(),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start),
    ) as UiView;

    Arc::new(apply_widget_style(
        flex_row((sidebar.into_any_flex(), main_content.into_any_flex()))
            .main_axis_alignment(MainAxisAlignment::Start),
        &root_style,
    ))
}

fn build_response_panel(world: &World, dispatcher: Entity) -> UiView {
    let panel = world.resource::<ResponsePanelState>();
    let btn_style = resolve_style_for_classes(world, ["pixiv.primary-btn"]);

    if panel.content.trim().is_empty() {
        return Arc::new(label(""));
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
                apply_widget_style(
                    button(
                        dispatcher,
                        AppAction::CopyResponseBody,
                        "Copy Response Body",
                    ),
                    &btn_style,
                )
                .into_any_flex(),
                apply_widget_style(
                    button(dispatcher, AppAction::ClearResponseBody, "Clear"),
                    &btn_style,
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
            .fixed_height(Length::px(RESPONSE_PANEL_HEIGHT))
            .into_any_flex(),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
}

fn build_feed_grid(world: &World, dispatcher: Entity) -> UiView {
    let order = world.resource::<FeedOrder>().0.clone();
    if order.is_empty() {
        return Arc::new(label("No data yet. Login first, then switch tabs."));
    }

    let row_views = order
        .chunks(CARDS_PER_ROW)
        .map(|chunk| {
            let cards = chunk
                .iter()
                .filter_map(|entity| build_illust_card(world, dispatcher, *entity))
                .map(|view| view.into_any_flex())
                .collect::<Vec<_>>();
            Arc::new(flex_row(cards).cross_axis_alignment(CrossAxisAlignment::Start)) as UiView
        })
        .collect::<Vec<_>>();

    let rows = Arc::new(row_views);
    let row_count = i64::try_from(rows.len()).unwrap_or(i64::MAX);

    Arc::new(
        sized_box(virtual_scroll(0..row_count, {
            let rows = Arc::clone(&rows);
            move |_, idx| {
                let row_idx = usize::try_from(idx).unwrap_or(0);
                rows.get(row_idx)
                    .cloned()
                    .unwrap_or_else(|| Arc::new(label("")))
            }
        }))
        .fixed_height(Length::px(520.0)),
    )
}

fn build_illust_card(world: &World, _dispatcher: Entity, entity: Entity) -> Option<UiView> {
    let illust = world.get::<Illust>(entity)?;
    let visual = world
        .get::<IllustVisual>(entity)
        .cloned()
        .unwrap_or_default();
    let anim = world
        .get::<CardAnimState>(entity)
        .copied()
        .unwrap_or_default();
    let style = resolve_style(world, entity);
    let button_style = resolve_style_for_classes(world, ["pixiv.primary-btn"]);

    let image_view: UiView = if let Some(image_data) = visual.thumb_ui {
        Arc::new(image(image_data))
    } else {
        Arc::new(label("thumbnail loading…"))
    };

    let avatar_view: UiView = if let Some(image_data) = visual.avatar_ui {
        Arc::new(
            sized_box(image(image_data))
                .fixed_height(Length::px(28.0))
                .fixed_width(Length::px(28.0)),
        )
    } else {
        Arc::new(label("👤"))
    };

    let mut card_children = Vec::new();
    card_children.push(
        sized_box(image_view)
            .fixed_height(Length::px((160.0_f32 * anim.card_scale) as f64))
            .into_any_flex(),
    );

    card_children.push(apply_label_style(label(illust.title.clone()), &style).into_any_flex());

    card_children.push(
        flex_row((
            avatar_view.into_any_flex(),
            apply_label_style(label(illust.user.name.clone()), &style).into_any_flex(),
        ))
        .into_any_flex(),
    );

    card_children.push(
        apply_label_style(
            label(format!(
                "👁 {}   ❤ {}",
                illust.total_view, illust.total_bookmarks
            )),
            &style,
        )
        .into_any_flex(),
    );

    let heart = if illust.is_bookmarked { "♥" } else { "♡" };
    let heart_button = sized_box(button(entity, AppAction::Bookmark(entity), heart))
        .fixed_width(Length::px((46.0_f32 * anim.heart_scale) as f64));

    card_children.push(
        flex_row((
            apply_widget_style(
                button(entity, AppAction::OpenIllust(entity), "Open"),
                &button_style,
            )
            .into_any_flex(),
            heart_button.into_any_flex(),
        ))
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .into_any_flex(),
    );

    Some(Arc::new(
        sized_box(apply_widget_style(flex_col(card_children), &style))
            .fixed_width(Length::px(
                (CARD_BASE_WIDTH * anim.card_scale as f64).max(140.0),
            ))
            .fixed_height(Length::px(
                (CARD_BASE_HEIGHT * anim.card_scale as f64).max(180.0),
            )),
    ))
}

fn build_detail_overlay(world: &World, dispatcher: Entity) -> UiView {
    let ui = world.resource::<UiState>();
    let Some(entity) = ui.selected_illust else {
        return Arc::new(label(""));
    };

    let Some(illust) = world.get::<Illust>(entity) else {
        return Arc::new(label(""));
    };
    let style = resolve_style_for_classes(world, ["pixiv.overlay"]);
    let btn_style = resolve_style_for_classes(world, ["pixiv.primary-btn"]);
    let visual = world
        .get::<IllustVisual>(entity)
        .cloned()
        .unwrap_or_default();

    let hero: UiView = if let Some(high_res) = visual.high_res_ui {
        Arc::new(sized_box(image(high_res)).fixed_height(Length::px(280.0)))
    } else {
        Arc::new(label("high-res loading…"))
    };

    let tag_entities = world.resource::<OverlayTags>().0.clone();
    let mut tag_rows = Vec::new();
    for chunk in tag_entities.chunks(4) {
        let tag_buttons = chunk
            .iter()
            .filter_map(|entity| {
                let tag = world.get::<OverlayTag>(*entity)?;
                let tag_style = resolve_style(world, *entity);
                Some(
                    apply_widget_style(
                        button(
                            *entity,
                            AppAction::SearchByTag(tag.text.clone()),
                            tag.text.clone(),
                        ),
                        &tag_style,
                    )
                    .into_any_flex(),
                )
            })
            .collect::<Vec<_>>();
        tag_rows.push(flex_row(tag_buttons).into_any_flex());
    }

    Arc::new(apply_widget_style(
        flex_col((
            apply_widget_style(
                button(dispatcher, AppAction::CloseIllust, "Close"),
                &btn_style,
            )
            .into_any_flex(),
            hero.into_any_flex(),
            label(illust.title.clone()).into_any_flex(),
            label(format!("Author: {}", illust.user.name)).into_any_flex(),
            label(format!(
                "Views {}  Bookmarks {}  Comments {}",
                illust.total_view, illust.total_bookmarks, illust.total_comments
            ))
            .into_any_flex(),
            flex_col(tag_rows).into_any_flex(),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start),
        &style,
    ))
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
                let mut ui = world.resource_mut::<UiState>();
                ui.sidebar_collapsed = !ui.sidebar_collapsed;
            }
            AppAction::SetTab(tab) => {
                world.resource_mut::<UiState>().active_tab = tab;
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
                    world.resource_mut::<UiState>().status_line =
                        "No response body to copy.".to_string();
                    continue;
                }

                match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(body)) {
                    Ok(_) => {
                        world.resource_mut::<UiState>().status_line =
                            "Response body copied to clipboard.".to_string();
                    }
                    Err(err) => {
                        world.resource_mut::<UiState>().status_line =
                            format!("Clipboard copy failed: {err}");
                    }
                }
            }
            AppAction::ClearResponseBody => {
                *world.resource_mut::<ResponsePanelState>() = ResponsePanelState::default();
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
                            world.resource_mut::<UiState>().status_line = if idp_urls.is_some() {
                                format!(
                                    "Browser login page opened. Official callback should look like pixiv://account/login?code=...&via=login. Token exchange uses redirect_uri from /idp-urls (current: {redirect_uri})."
                                )
                            } else {
                                "Browser login page opened. /idp-urls is not ready yet, so token exchange will use fallback redirect_uri. If Login fails, wait for IdP discovery and retry.".to_string()
                            };
                        }
                        Err(err) => {
                            world.resource_mut::<UiState>().status_line = format!(
                                "Could not open browser automatically: {err}. Open this URL manually: {login_url}"
                            );
                        }
                    },
                    Err(err) => {
                        world.resource_mut::<UiState>().status_line =
                            format!("Failed to build browser login URL: {err}");
                    }
                }
            }
            AppAction::ExchangeAuthCode => {
                let auth = world.resource::<AuthState>();
                let Some(code) = extract_auth_code_from_input(&auth.auth_code_input) else {
                    world.resource_mut::<UiState>().status_line = "Auth code is missing. Please paste a raw code or a callback URL containing `code=`.".to_string();
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
                let auth = world.resource::<AuthState>();
                let _ = world
                    .resource::<NetworkBridge>()
                    .cmd_tx
                    .send(NetworkCommand::Refresh {
                        refresh_token: auth.refresh_token_input.clone(),
                    });
            }
        }
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
                world.resource_mut::<UiState>().status_line =
                    "IdP endpoint discovered. Enter auth_code or refresh token.".to_string();
            }
            NetworkResult::Authenticated(session) => {
                world.resource_mut::<AuthState>().session = Some(session.clone());
                world.resource_mut::<UiState>().status_line =
                    "Authenticated. Loading home feed…".to_string();
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
                world.resource_mut::<UiState>().active_tab = source;
                world.resource_mut::<UiState>().status_line = format!(
                    "Loaded {} illustrations ({source:?})",
                    payload.illusts.len()
                );

                for entity in std::mem::take(&mut world.resource_mut::<FeedOrder>().0) {
                    if world.get_entity(entity).is_ok() {
                        world.entity_mut(entity).despawn();
                    }
                }

                let mut new_order = Vec::new();
                for illust in payload.illusts {
                    let entity = world
                        .spawn((
                            illust.clone(),
                            IllustVisual::default(),
                            CardAnimState::default(),
                            CardHoverFlag(false),
                            StyleClass(vec!["pixiv.card".to_string()]),
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
                world.resource_mut::<UiState>().status_line =
                    format!("Bookmark synced for illust #{illust_id}");
            }
            NetworkResult::Error { summary, details } => {
                world.resource_mut::<UiState>().status_line = format!("Network error: {summary}");
                *world.resource_mut::<ResponsePanelState>() = ResponsePanelState {
                    title: "Last network response body / detail".to_string(),
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
                    world.resource_mut::<UiState>().status_line =
                        format!("Image decode buffer size mismatch for entity {entity:?}");
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
                    world.resource_mut::<UiState>().status_line =
                        format!("Image load failed ({which}): {error}");
                }
            }
        }
    }
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .register_projector::<PixivRoot>(project_root)
        .add_systems(Startup, (setup_styles, setup))
        .add_systems(PreUpdate, drain_ui_actions_and_dispatch)
        .add_systems(
            Update,
            (
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
