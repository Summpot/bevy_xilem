mod utils;

use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ColorStyle, LayoutStyle, ProjectionCtx, StyleClass,
    StyleSetter, StyleSheet, StyleTransition, TextStyle, UiEventQueue, UiRoot, UiView,
    apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    button, button_with_child, checkbox, resolve_style, resolve_style_for_classes,
    run_app_with_window_options, slider,
    xilem::{
        Color,
        masonry::{dpi::LogicalSize, layout::AsUnit},
        style::Style as _,
        view::{
            CrossAxisAlignment, FlexExt as _, GridExt as _, flex_col, flex_row, grid, label, prose,
            sized_box,
        },
        winit::error::EventLoopError,
    },
};
use utils::init_logging;

#[allow(unexpected_cfgs)]
#[path = "chess_engine.rs"]
mod engine;

const TIMER_TICK_MS: u64 = 100;
const TIMER_TICK_SECS: f64 = TIMER_TICK_MS as f64 / 1000.0;
const BOARD_SIZE: usize = 8;
const BOARD_RECENT_MOVE_SHADE: u8 = 25;
const BOARD_TARGET_SHADE: u8 = 50;
const MOVELIST_WIDTH: i32 = 200;
const MOVELIST_HEIGHT: i32 = 260;

#[derive(Clone, Copy, Debug)]
enum Piece {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    White,
    Black,
}

#[derive(Clone, Copy, Debug)]
struct ColoredPiece {
    piece: Piece,
    side: Side,
}

type BoardView = [[Option<ColoredPiece>; BOARD_SIZE]; BOARD_SIZE];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerKind {
    Human,
    Engine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Uninitialized,
    Inactive,
    Ready,
    MoveAttempt,
    EngineThinking,
    EnginePlaying,
}

const PLAYER_FOR_ENGINE_FLAG: [PlayerKind; 2] = [PlayerKind::Human, PlayerKind::Engine];

#[derive(Debug, Clone)]
enum ChessEvent {
    ClickSquare { row: usize, col: usize },
    SetTimePerMove(f64),
    ToggleEngineWhite,
    ToggleEngineBlack,
    Rotate,
    NewGame,
    PrintMovelist,
}

#[derive(Resource)]
struct ChessGameResource {
    game: Arc<Mutex<engine::Game>>,
    rx: Option<Arc<Mutex<mpsc::Receiver<engine::Move>>>>,
    time_per_move: f64,
}

impl ChessGameResource {
    fn new(game: engine::Game) -> Self {
        Self {
            game: Arc::new(Mutex::new(game)),
            rx: None,
            time_per_move: 1.5,
        }
    }
}

#[derive(Resource)]
struct ChessUiResource {
    board: BoardView,
    selected: Option<(usize, usize)>,
    square_tags: engine::Board,
    status: String,
    engine_plays_white: bool,
    engine_plays_black: bool,
    rotated: bool,
    pending_move: Option<(usize, usize)>,
    movelist: Vec<String>,
}

impl ChessUiResource {
    fn from_game(game: &engine::Game) -> Self {
        Self {
            board: engine_to_board(engine::get_board(game)),
            selected: None,
            square_tags: [0; 64],
            status: "Tiny chess".into(),
            engine_plays_white: false,
            engine_plays_black: true,
            rotated: false,
            pending_move: None,
            movelist: Vec::new(),
        }
    }

    fn movelist_text(&self) -> String {
        self.movelist
            .chunks(2)
            .enumerate()
            .map(|(idx, chunk)| match chunk {
                [a, b] => format!("{:>3}. {:>7}  {}", idx + 1, a, b),
                [a] => format!("{:>3}. {:>7}", idx + 1, a),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Resource)]
struct ChessFlowResource {
    phase: Phase,
    players: [PlayerKind; 2],
    time_elapsed: [f64; 2],
    turn: usize,
    last_tick_instant: Instant,
    tick_accumulator: Duration,
}

impl Default for ChessFlowResource {
    fn default() -> Self {
        Self {
            phase: Phase::Uninitialized,
            players: [PlayerKind::Human, PlayerKind::Engine],
            time_elapsed: [0.0, 0.0],
            turn: 0,
            last_tick_instant: Instant::now(),
            tick_accumulator: Duration::ZERO,
        }
    }
}

fn formatted_clock(secs: f64) -> String {
    let total = secs.round() as u64;
    let minutes = total / 60;
    let seconds = total % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn dimmed(color: Color, amount: u8) -> Color {
    let rgba = color.to_rgba8();
    let r = rgba.r;
    let g = rgba.g;
    let b = rgba.b;
    let a = rgba.a;

    Color::from_rgba8(
        r.saturating_sub(amount),
        g.saturating_sub(amount),
        b.saturating_sub(amount),
        a,
    )
}

fn with_chess_resources<R>(
    world: &mut World,
    f: impl FnOnce(&mut ChessGameResource, &mut ChessUiResource, &mut ChessFlowResource) -> R,
) -> R {
    let mut f = Some(f);
    world.resource_scope(|world, mut game: Mut<ChessGameResource>| {
        world.resource_scope(|world, mut ui: Mut<ChessUiResource>| {
            world.resource_scope(|_world, mut flow: Mut<ChessFlowResource>| {
                let f = f.take().expect("closure should run exactly once");
                f(&mut game, &mut ui, &mut flow)
            })
        })
    })
}

fn apply_event(
    event: ChessEvent,
    game_res: &mut ChessGameResource,
    ui: &mut ChessUiResource,
    flow: &mut ChessFlowResource,
) {
    match event {
        ChessEvent::ClickSquare { row, col } => handle_square_click(row, col, game_res, ui, flow),
        ChessEvent::SetTimePerMove(value) => {
            game_res.time_per_move = value;
        }
        ChessEvent::ToggleEngineWhite => {
            ui.engine_plays_white = !ui.engine_plays_white;
            flow.players[0] = PLAYER_FOR_ENGINE_FLAG[ui.engine_plays_white as usize];
            flow.phase = Phase::Uninitialized;
        }
        ChessEvent::ToggleEngineBlack => {
            ui.engine_plays_black = !ui.engine_plays_black;
            flow.players[1] = PLAYER_FOR_ENGINE_FLAG[ui.engine_plays_black as usize];
            flow.phase = Phase::Uninitialized;
        }
        ChessEvent::Rotate => {
            ui.rotated = !ui.rotated;
        }
        ChessEvent::NewGame => reset_game(game_res, ui, flow),
        ChessEvent::PrintMovelist => {
            if let Ok(game) = game_res.game.lock() {
                engine::print_move_list(&game);
            }
        }
    }
}

fn handle_square_click(
    row: usize,
    col: usize,
    game_res: &mut ChessGameResource,
    ui: &mut ChessUiResource,
    flow: &mut ChessFlowResource,
) {
    let idx = row * BOARD_SIZE + col;
    let clicked = (row, col);

    match ui.selected {
        None => {
            if ui.board[row][col].is_some() {
                ui.selected = Some(clicked);
                ui.pending_move = None;
                ui.square_tags = [0; 64];

                for mov in engine::tag(&mut game_res.game.lock().unwrap(), idx as i64) {
                    ui.square_tags[mov.di as usize] = 1;
                }
                ui.square_tags[idx] = -1;
                flow.phase = Phase::Ready;
            }
        }
        Some(prev) if prev != clicked => {
            let from_idx = prev.0 * BOARD_SIZE + prev.1;
            ui.pending_move = Some((from_idx, idx));
            ui.selected = None;
            flow.phase = Phase::MoveAttempt;
        }
        Some(_) => {
            ui.selected = None;
            ui.pending_move = None;
            ui.square_tags = [0; 64];
        }
    }
}

fn reset_game(
    game_res: &mut ChessGameResource,
    ui: &mut ChessUiResource,
    flow: &mut ChessFlowResource,
) {
    if let Ok(mut game) = game_res.game.lock() {
        engine::reset_game(&mut game);
        ui.board = engine_to_board(engine::get_board(&game));
        ui.square_tags = [0; 64];
        ui.selected = None;
        ui.pending_move = None;
        game_res.rx = None;
        flow.phase = Phase::Uninitialized;
        flow.time_elapsed = [0.0, 0.0];
        ui.movelist.clear();
    }
}

fn tick_game(
    game_res: &mut ChessGameResource,
    ui: &mut ChessUiResource,
    flow: &mut ChessFlowResource,
) {
    let now = Instant::now();
    let elapsed = now.saturating_duration_since(flow.last_tick_instant);
    flow.last_tick_instant = now;
    flow.tick_accumulator += elapsed;

    let step = Duration::from_millis(TIMER_TICK_MS);
    while flow.tick_accumulator >= step {
        flow.tick_accumulator -= step;
        tick_once(game_res, ui, flow);
    }
}

fn tick_once(
    game_res: &mut ChessGameResource,
    ui: &mut ChessUiResource,
    flow: &mut ChessFlowResource,
) {
    if matches!(
        flow.phase,
        Phase::Ready | Phase::MoveAttempt | Phase::EngineThinking | Phase::EnginePlaying
    ) {
        flow.time_elapsed[flow.turn] += TIMER_TICK_SECS;
    }

    if let Ok(game) = game_res.game.try_lock() {
        ui.board = engine_to_board(engine::get_board(&game));
    }

    match flow.phase {
        Phase::Uninitialized => {
            if let Ok(game) = game_res.game.lock() {
                flow.turn = game.move_counter as usize % 2;
                flow.phase = match flow.players[flow.turn] {
                    PlayerKind::Human => Phase::Ready,
                    PlayerKind::Engine => Phase::EngineThinking,
                };
            }
        }
        Phase::MoveAttempt => {
            if let Some((from_idx, to_idx)) = ui.pending_move.take() {
                let from = from_idx as i8;
                let to = to_idx as i8;

                let mut game = game_res.game.lock().unwrap();
                let valid = engine::move_is_valid2(&mut game, from as i64, to as i64);

                ui.square_tags = [0; 64];

                if from_idx == to_idx || !valid {
                    ui.status = "Invalid move.".into();
                } else {
                    let flag = engine::do_move(&mut game, from, to, false);
                    let notation = engine::move_to_str(&game, from, to, flag);
                    ui.movelist.push(notation.clone());
                    ui.status = notation;
                    ui.square_tags[from_idx] = 2;
                    ui.square_tags[to_idx] = 2;
                }
            }
            flow.phase = Phase::Uninitialized;
        }
        Phase::EngineThinking => {
            flow.phase = Phase::EnginePlaying;

            if let Ok(mut game) = game_res.game.try_lock() {
                game.secs_per_move = game_res.time_per_move as f32;
            }

            let (tx, rx) = mpsc::channel();
            game_res.rx = Some(Arc::new(Mutex::new(rx)));
            let game_clone = Arc::clone(&game_res.game);

            thread::spawn(move || {
                let chess_move = engine::reply(&mut game_clone.lock().unwrap());
                let _ = tx.send(chess_move);
            });
        }
        Phase::EnginePlaying => {
            let maybe_move = game_res
                .rx
                .as_ref()
                .and_then(|rx| rx.lock().ok().and_then(|locked| locked.try_recv().ok()));

            if let Some(mv) = maybe_move {
                let mut game = game_res.game.lock().unwrap();

                ui.square_tags = [0; 64];
                ui.square_tags[mv.src as usize] = 2;
                ui.square_tags[mv.dst as usize] = 2;

                let flag = engine::do_move(&mut game, mv.src as i8, mv.dst as i8, false);
                let notation = engine::move_to_str(&game, mv.src as i8, mv.dst as i8, flag);

                ui.movelist.push(notation.clone());
                ui.status = format!("{notation} (scr: {})", mv.score);

                game_res.rx = None;
                flow.phase = match mv.state {
                    engine::STATE_CHECKMATE => {
                        ui.status = "Checkmate, game terminated!".into();
                        Phase::Inactive
                    }
                    _ if mv.score.abs() > engine::KING_VALUE_DIV_2 as i64 => {
                        let turns = mv.checkmate_in / 2 + if mv.score > 0 { -1 } else { 1 };
                        ui.status.push_str(&format!(" Checkmate in {}", turns));
                        Phase::Uninitialized
                    }
                    _ => Phase::Uninitialized,
                };
            }
        }
        Phase::Ready | Phase::Inactive => {}
    }
}

#[derive(Component, Debug, Clone, Copy)]
struct ChessRootView;

#[derive(Component, Debug, Clone, Copy)]
struct ChessControlsPanel;

#[derive(Component, Debug, Clone, Copy)]
struct ChessBoardPanel;

fn build_chess_board_view(world: &World, ui: &ChessUiResource, action_entity: Entity) -> UiView {
    let board_style = resolve_style_for_classes(world, ["chess.board"]);
    let cell_style = resolve_style_for_classes(world, ["chess.cell"]);
    let light_cell_style = resolve_style_for_classes(world, ["chess.cell.light"]);
    let dark_cell_style = resolve_style_for_classes(world, ["chess.cell.dark"]);
    let piece_style = resolve_style_for_classes(world, ["chess.cell-piece"]);

    let light_bg = light_cell_style
        .colors
        .bg
        .unwrap_or(Color::from_rgb8(0xFF, 0xFF, 0xFF));
    let dark_bg = dark_cell_style
        .colors
        .bg
        .unwrap_or(Color::from_rgb8(0xCD, 0xCD, 0xCD));

    let mut cells = Vec::with_capacity(BOARD_SIZE * BOARD_SIZE);

    for row in 0..BOARD_SIZE {
        for col in 0..BOARD_SIZE {
            let idx = row * BOARD_SIZE + col;

            let (draw_row, draw_col) = if ui.rotated {
                (row, col)
            } else {
                (BOARD_SIZE - 1 - row, BOARD_SIZE - 1 - col)
            };

            let shade = match ui.square_tags[idx] {
                2 => BOARD_RECENT_MOVE_SHADE,
                1 => BOARD_TARGET_SHADE,
                _ => 0,
            };

            let color = if (row + col) % 2 == 0 {
                dimmed(light_bg, shade)
            } else {
                dimmed(dark_bg, shade)
            };

            let label_text = ui.board[row][col].map(piece_unicode).unwrap_or(" ");

            let label_piece = apply_label_style(
                label(label_text).font(chess_piece_font_family()),
                &piece_style,
            );

            let cell = button_with_child(
                action_entity,
                ChessEvent::ClickSquare { row, col },
                label_piece,
            )
            .padding(cell_style.layout.padding)
            .corner_radius(cell_style.layout.corner_radius)
            .border(
                cell_style.colors.border.unwrap_or(Color::TRANSPARENT),
                cell_style.layout.border_width,
            )
            .background_color(color)
            .grid_pos(draw_col as i32, draw_row as i32);

            cells.push(cell);
        }
    }

    Arc::new(apply_widget_style(
        flex_col((grid(cells, BOARD_SIZE as i32, BOARD_SIZE as i32).flex(1.0),)),
        &board_style,
    ))
}

fn build_chess_controls_view(
    world: &World,
    game_res: &ChessGameResource,
    ui: &ChessUiResource,
    flow: &ChessFlowResource,
    action_entity: Entity,
) -> UiView {
    let controls_style = resolve_style_for_classes(world, ["chess.controls"]);
    let status_style = resolve_style_for_classes(world, ["chess.status"]);
    let clock_style = resolve_style_for_classes(world, ["chess.clock"]);
    let time_per_move_style = resolve_style_for_classes(world, ["chess.time-per-move"]);
    let toggle_style = resolve_style_for_classes(world, ["chess.toggle"]);
    let action_button_style = resolve_style_for_classes(world, ["chess.action-button"]);
    let movelist_style = resolve_style_for_classes(world, ["chess.movelist"]);

    let movelist_text = ui.movelist_text();

    let rotate_button = apply_widget_style(
        button(action_entity, ChessEvent::Rotate, "Rotate"),
        &action_button_style,
    );

    let new_game_button = apply_widget_style(
        button(action_entity, ChessEvent::NewGame, "New game"),
        &action_button_style,
    );

    let print_movelist_button = apply_widget_style(
        button(action_entity, ChessEvent::PrintMovelist, "Print movelist"),
        &action_button_style,
    );

    Arc::new(apply_widget_style(
        flex_col((
            apply_label_style(label(ui.status.clone()), &status_style),
            apply_label_style(
                label(format!("White: {}", formatted_clock(flow.time_elapsed[0]))),
                &clock_style,
            ),
            apply_label_style(
                label(format!("Black: {}", formatted_clock(flow.time_elapsed[1]))),
                &clock_style,
            ),
            apply_label_style(
                label(format!("{:.2} sec/move", game_res.time_per_move)),
                &time_per_move_style,
            ),
            slider(
                action_entity,
                0.1,
                5.0,
                game_res.time_per_move,
                ChessEvent::SetTimePerMove,
            ),
            apply_widget_style(
                checkbox(
                    action_entity,
                    "Engine plays white",
                    ui.engine_plays_white,
                    |_| ChessEvent::ToggleEngineWhite,
                )
                .text_size(toggle_style.text.size),
                &toggle_style,
            ),
            apply_widget_style(
                checkbox(
                    action_entity,
                    "Engine plays black",
                    ui.engine_plays_black,
                    |_| ChessEvent::ToggleEngineBlack,
                )
                .text_size(toggle_style.text.size),
                &toggle_style,
            ),
            rotate_button,
            new_game_button,
            print_movelist_button,
            apply_widget_style(
                sized_box(prose(movelist_text))
                    .width(MOVELIST_WIDTH.px())
                    .height(MOVELIST_HEIGHT.px()),
                &movelist_style,
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start),
        &controls_style,
    ))
}

fn project_chess_root(_: &ChessRootView, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    Arc::new(apply_widget_style(
        flex_row(
            ctx.children
                .into_iter()
                .map(|child| child.into_any_flex())
                .collect::<Vec<_>>(),
        )
        .cross_axis_alignment(CrossAxisAlignment::Start),
        &style,
    ))
}

fn project_chess_controls_panel(_: &ChessControlsPanel, ctx: ProjectionCtx<'_>) -> UiView {
    let game_res = ctx.world.resource::<ChessGameResource>();
    let ui = ctx.world.resource::<ChessUiResource>();
    let flow = ctx.world.resource::<ChessFlowResource>();
    build_chess_controls_view(ctx.world, &game_res, &ui, &flow, ctx.entity)
}

fn project_chess_board_panel(_: &ChessBoardPanel, ctx: ProjectionCtx<'_>) -> UiView {
    let ui = ctx.world.resource::<ChessUiResource>();
    build_chess_board_view(ctx.world, &ui, ctx.entity)
}

fn setup_chess_world(mut commands: Commands) {
    let root = commands
        .spawn((
            UiRoot,
            ChessRootView,
            StyleClass(vec!["chess.root".to_string()]),
        ))
        .id();

    commands.spawn((ChessControlsPanel, ChildOf(root)));
    commands.spawn((ChessBoardPanel, ChildOf(root)));
}

fn setup_chess_styles(mut style_sheet: ResMut<StyleSheet>) {
    style_sheet.set_class(
        "chess.root",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(8.0),
                gap: Some(8.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x1A, 0x1A, 0x1A)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.controls",
        StyleSetter {
            layout: LayoutStyle {
                gap: Some(6.0),
                padding: Some(6.0),
                corner_radius: Some(8.0),
                border_width: Some(1.0),
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x24, 0x24, 0x24)),
                border: Some(Color::from_rgb8(0x3F, 0x3F, 0x46)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.status",
        StyleSetter {
            text: TextStyle { size: Some(16.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xE4, 0xE4, 0xE7)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.clock",
        StyleSetter {
            text: TextStyle { size: Some(14.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xD4, 0xD4, 0xD8)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.time-per-move",
        StyleSetter {
            text: TextStyle { size: Some(14.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xA1, 0xA1, 0xAA)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.toggle",
        StyleSetter {
            text: TextStyle { size: Some(14.0) },
            layout: LayoutStyle {
                padding: Some(2.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x2B, 0x2B, 0x2F)),
                hover_bg: Some(Color::from_rgb8(0x3A, 0x3A, 0x40)),
                pressed_bg: Some(Color::from_rgb8(0x22, 0x22, 0x25)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.15 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.action-button",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                border_width: Some(0.0),
                corner_radius: Some(8.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x3F, 0x3F, 0x46)),
                hover_bg: Some(Color::from_rgb8(0x52, 0x52, 0x5B)),
                pressed_bg: Some(Color::from_rgb8(0x27, 0x27, 0x2A)),
                ..ColorStyle::default()
            },
            transition: Some(StyleTransition { duration: 0.1 }),
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.action-label",
        StyleSetter {
            text: TextStyle { size: Some(14.0) },
            colors: ColorStyle {
                text: Some(Color::from_rgb8(0xFA, 0xFA, 0xFA)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.movelist",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(6.0),
                border_width: Some(1.0),
                corner_radius: Some(8.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x11, 0x11, 0x13)),
                border: Some(Color::from_rgb8(0x3F, 0x3F, 0x46)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.board",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(4.0),
                border_width: Some(1.0),
                corner_radius: Some(8.0),
                ..LayoutStyle::default()
            },
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0x24, 0x24, 0x24)),
                border: Some(Color::from_rgb8(0x3F, 0x3F, 0x46)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.cell",
        StyleSetter {
            layout: LayoutStyle {
                padding: Some(0.0),
                corner_radius: Some(0.0),
                ..LayoutStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.cell.light",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.cell.dark",
        StyleSetter {
            colors: ColorStyle {
                bg: Some(Color::from_rgb8(0xCD, 0xCD, 0xCD)),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );

    style_sheet.set_class(
        "chess.cell-piece",
        StyleSetter {
            text: TextStyle { size: Some(48.0) },
            colors: ColorStyle {
                text: Some(Color::BLACK),
                ..ColorStyle::default()
            },
            ..StyleSetter::default()
        },
    );
}

fn drain_events_and_tick(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<ChessEvent>();

    with_chess_resources(world, |game_res, ui, flow| {
        for event in events {
            apply_event(event.action, game_res, ui, flow);
        }
        tick_game(game_res, ui, flow);
    });
}

fn build_bevy_chess_app() -> App {
    init_logging();

    let game = engine::new_game();
    let ui = ChessUiResource::from_game(&game);

    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .register_ui_control::<bevy_xilem::UiButton>()
        .register_ui_control::<bevy_xilem::UiDialog>()
        .register_ui_control::<bevy_xilem::UiComboBox>()
        .load_style_sheet("themes/default_theme.ron")
        .insert_resource(ChessGameResource::new(game))
        .insert_resource(ui)
        .insert_resource(ChessFlowResource::default())
        .register_projector::<ChessRootView>(project_chess_root)
        .register_projector::<ChessControlsPanel>(project_chess_controls_panel)
        .register_projector::<ChessBoardPanel>(project_chess_board_panel)
        .add_systems(Startup, (setup_chess_styles, setup_chess_world));

    app.add_systems(PreUpdate, drain_events_and_tick);

    app
}

fn engine_to_board(engine_board: engine::Board) -> BoardView {
    use Piece::*;
    use Side::{Black, White};

    let mut board = [[None; BOARD_SIZE]; BOARD_SIZE];

    for (i, &val) in engine_board.iter().enumerate() {
        let piece_side = match val {
            1 => Some((Pawn, White)),
            2 => Some((Knight, White)),
            3 => Some((Bishop, White)),
            4 => Some((Rook, White)),
            5 => Some((Queen, White)),
            6 => Some((King, White)),
            -1 => Some((Pawn, Black)),
            -2 => Some((Knight, Black)),
            -3 => Some((Bishop, Black)),
            -4 => Some((Rook, Black)),
            -5 => Some((Queen, Black)),
            -6 => Some((King, Black)),
            _ => None,
        };

        if let Some((piece, side)) = piece_side {
            board[i / BOARD_SIZE][i % BOARD_SIZE] = Some(ColoredPiece { piece, side });
        }
    }

    board
}

fn piece_unicode(piece: ColoredPiece) -> &'static str {
    use Piece::*;
    use Side::{Black, White};

    match (piece.piece, piece.side) {
        (King, White) => "♔",
        (Queen, White) => "♕",
        (Rook, White) => "♖",
        (Bishop, White) => "♗",
        (Knight, White) => "♘",
        (Pawn, White) => "♙",
        (King, Black) => "♚",
        (Queen, Black) => "♛",
        (Rook, Black) => "♜",
        (Bishop, Black) => "♝",
        (Knight, Black) => "♞",
        (Pawn, Black) => "♟",
    }
}

fn chess_piece_font_family() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        return "Apple Symbols";
    }

    #[cfg(target_os = "windows")]
    {
        return "Segoe UI Symbol";
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        "DejaVu Sans"
    }
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_bevy_chess_app(), "Xilem Chess GUI", |options| {
        options
            .with_resizable(true)
            .with_min_inner_size(LogicalSize::new(640.0, 560.0))
            .with_initial_inner_size(LogicalSize::new(1024.0, 760.0))
    })
}
