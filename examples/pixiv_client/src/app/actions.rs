use super::*;

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

pub(super) fn drain_ui_actions_and_dispatch(world: &mut World) {
    // Materialize built-in widget state changes before the app drains its own
    // action queues and mirrors resource state back into ECS-owned inputs.
    bevy_xilem::handle_widget_actions(world);

    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<AppAction>();

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
                    NavTab::Manga => tr(world, "pixiv.status.loading_manga", "Loading Manga feed…"),
                    NavTab::Novels => {
                        tr(world, "pixiv.status.loading_novels", "Loading Novels feed…")
                    }
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
                    NavTab::Manga => NetworkCommand::FetchManga,
                    NavTab::Novels => NetworkCommand::FetchNovels,
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
                set_status_key(world, "pixiv.overlay.title", "Illustration details");

                if let Some(illust) = world.get::<Illust>(entity) {
                    let high_res = illust
                        .meta_single_page
                        .as_ref()
                        .and_then(|meta| meta.original_image_url.clone())
                        .unwrap_or_else(|| illust.image_urls.large.clone());
                    if high_res.starts_with("https://") || high_res.starts_with("http://") {
                        let _ =
                            world
                                .resource::<ImageBridge>()
                                .cmd_tx
                                .send(ImageCommand::Download {
                                    entity,
                                    kind: ImageKind::HighRes,
                                    url: high_res,
                                });
                    }
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
                let Some(code) =
                    super::activation::extract_auth_code_from_input(&auth.auth_code_input)
                else {
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

    let text_input_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiTextInputChanged>();
    let ui_components = *world.resource::<PixivUiComponents>();

    for event in text_input_events {
        let value = event.action.value;
        match event.action.input {
            input if input == ui_components.search_input => {
                world.resource_mut::<UiState>().search_text = value;
            }
            input if input == ui_components.code_verifier_input => {
                world.resource_mut::<AuthState>().code_verifier_input = value;
            }
            input if input == ui_components.auth_code_input => {
                world.resource_mut::<AuthState>().auth_code_input = value;
            }
            input if input == ui_components.refresh_token_input => {
                world.resource_mut::<AuthState>().refresh_token_input = value;
            }
            _ => {}
        }
    }

    let combo_events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<UiComboBoxChanged>();

    for event in combo_events {
        if event.action.combo != ui_components.locale_combo {
            continue;
        }

        let next = parse_locale(event.action.value.as_str());
        world
            .resource_mut::<AppI18n>()
            .set_active_locale(next.clone());

        if let Some(mut combo) = world.get_mut::<UiComboBox>(ui_components.locale_combo)
            && !combo.options.is_empty()
        {
            combo.selected = event.action.selected.min(combo.options.len() - 1);
        }

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

    sync_bound_text_inputs(world);
}

pub(super) fn track_viewport_metrics(
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

    spawn_card_tween(world, entity, start, end, 420, EaseKind::ElasticOut);
}
