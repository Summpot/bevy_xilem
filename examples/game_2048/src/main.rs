use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy_xilem::{
    AppBevyXilemExt, BevyXilemPlugin, ProjectionCtx, StyleClass, UiEventQueue, UiRoot, UiView,
    apply_label_style, apply_widget_style,
    bevy_app::{App, PreUpdate, Startup},
    bevy_ecs::{hierarchy::ChildOf, prelude::*},
    bevy_input::{ButtonInput, keyboard::KeyCode},
    bevy_window::WindowResized,
    button, emit_ui_action, resolve_style, resolve_style_for_classes, run_app_with_window_options,
    xilem::{
        Color,
        masonry::layout::Length,
        style::Style as _,
        view::{
            CrossAxisAlignment, FlexExt as _, MainAxisAlignment, flex_col, flex_row, label, portal,
            sized_box,
        },
        winit::{dpi::LogicalSize, error::EventLoopError},
    },
    xilem_masonry::{
        Pod, ViewCtx, WidgetView,
        core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    },
};
use masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx,
        NewWidget, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent,
        Widget, WidgetMut, WidgetPod,
        keyboard::{Key, NamedKey},
    },
    kurbo::{Axis, Point, Size},
    layout::LenReq,
    vello::Scene,
};
use shared_utils::{drain_fluent_theme_toggle_events, init_logging, setup_fluent_theme_toggle};

const BOARD_SIDE: usize = 4;
const BOARD_LEN: usize = BOARD_SIDE * BOARD_SIDE;

#[derive(Resource, Debug, Clone, Copy)]
struct GameViewport {
    width: f64,
    height: f64,
}

impl Default for GameViewport {
    fn default() -> Self {
        Self {
            width: 1040.0,
            height: 720.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct GameLayoutMetrics {
    tile_size: f64,
    side_panel_width: f64,
    ui_component_button_width: f64,
    ui_component_button_height: f64,
}

impl GameLayoutMetrics {
    fn from_viewport(viewport: GameViewport) -> Self {
        let side_panel_width = (viewport.width * 0.30).clamp(220.0, 320.0);
        let board_width_budget = (viewport.width - side_panel_width - 150.0).max(220.0);
        let board_height_budget = (viewport.height - 240.0).max(220.0);

        let tile_from_width = board_width_budget / 4.4;
        let tile_from_height = board_height_budget / 4.4;
        let tile_size = tile_from_width.min(tile_from_height).clamp(44.0, 92.0);

        let ui_component_button_width = (side_panel_width * 0.86).clamp(120.0, 228.0);
        let ui_component_button_height = (tile_size * 0.64).clamp(42.0, 58.0);

        Self {
            tile_size,
            side_panel_width,
            ui_component_button_width,
            ui_component_button_height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MoveDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameEvent {
    Move(MoveDirection),
    Undo,
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyboardAction {
    key: KeyCode,
    pressed: bool,
}

fn keycode_from_key(key: &Key) -> Option<KeyCode> {
    match key {
        Key::Named(NamedKey::ArrowUp) => Some(KeyCode::ArrowUp),
        Key::Named(NamedKey::ArrowDown) => Some(KeyCode::ArrowDown),
        Key::Named(NamedKey::ArrowLeft) => Some(KeyCode::ArrowLeft),
        Key::Named(NamedKey::ArrowRight) => Some(KeyCode::ArrowRight),
        Key::Character(c) if c.eq_ignore_ascii_case("w") => Some(KeyCode::KeyW),
        Key::Character(c) if c.eq_ignore_ascii_case("s") => Some(KeyCode::KeyS),
        Key::Character(c) if c.eq_ignore_ascii_case("a") => Some(KeyCode::KeyA),
        Key::Character(c) if c.eq_ignore_ascii_case("d") => Some(KeyCode::KeyD),
        Key::Character(c) if c.eq_ignore_ascii_case("z") => Some(KeyCode::KeyZ),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn seeded(seed: u64) -> Self {
        let fallback = 0x9E37_79B9_7F4A_7C15;
        Self {
            state: if seed == 0 { fallback } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_usize(&mut self, max: usize) -> usize {
        if max <= 1 {
            0
        } else {
            (self.next_u64() as usize) % max
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MoveComputation {
    tiles: [u16; BOARD_LEN],
    moved: bool,
    score_delta: u32,
}

#[derive(Debug, Clone, Copy)]
struct GameSnapshot {
    tiles: [u16; BOARD_LEN],
    score: u32,
    moves: u32,
    won_once: bool,
    game_over: bool,
    rng: XorShift64,
}

#[derive(Debug)]
struct Game2048 {
    tiles: [u16; BOARD_LEN],
    score: u32,
    best_score: u32,
    moves: u32,
    won_once: bool,
    game_over: bool,
    rng: XorShift64,
    last_snapshot: Option<GameSnapshot>,
}

impl Game2048 {
    fn new(seed: u64) -> Self {
        let mut game = Self {
            tiles: [0; BOARD_LEN],
            score: 0,
            best_score: 0,
            moves: 0,
            won_once: false,
            game_over: false,
            rng: XorShift64::seeded(seed),
            last_snapshot: None,
        };

        game.spawn_random_tile();
        game.spawn_random_tile();
        game
    }

    fn restart(&mut self) {
        self.tiles = [0; BOARD_LEN];
        self.score = 0;
        self.moves = 0;
        self.won_once = false;
        self.game_over = false;
        self.last_snapshot = None;
        self.spawn_random_tile();
        self.spawn_random_tile();
    }

    fn snapshot(&self) -> GameSnapshot {
        GameSnapshot {
            tiles: self.tiles,
            score: self.score,
            moves: self.moves,
            won_once: self.won_once,
            game_over: self.game_over,
            rng: self.rng,
        }
    }

    fn restore_snapshot(&mut self, snapshot: GameSnapshot) {
        self.tiles = snapshot.tiles;
        self.score = snapshot.score;
        self.moves = snapshot.moves;
        self.won_once = snapshot.won_once;
        self.game_over = snapshot.game_over;
        self.rng = snapshot.rng;
    }

    fn undo(&mut self) -> bool {
        let Some(snapshot) = self.last_snapshot.take() else {
            return false;
        };

        self.restore_snapshot(snapshot);
        self.best_score = self.best_score.max(self.score);
        true
    }

    fn max_tile(&self) -> u16 {
        *self.tiles.iter().max().unwrap_or(&0)
    }

    fn apply_move(&mut self, direction: MoveDirection, spawn_tile: bool) -> bool {
        if self.game_over {
            return false;
        }

        let snapshot_before_move = self.snapshot();
        let computation = compute_move(self.tiles, direction);
        if !computation.moved {
            self.game_over = !can_move(&self.tiles);
            return false;
        }

        self.last_snapshot = Some(snapshot_before_move);
        self.tiles = computation.tiles;
        self.score += computation.score_delta;
        self.best_score = self.best_score.max(self.score);
        self.moves += 1;

        if spawn_tile {
            self.spawn_random_tile();
        }

        self.recompute_flags();
        true
    }

    fn try_move(&mut self, direction: MoveDirection) -> bool {
        self.apply_move(direction, true)
    }

    fn recompute_flags(&mut self) {
        if self.max_tile() >= 2048 {
            self.won_once = true;
        }
        self.game_over = !can_move(&self.tiles);
    }

    fn spawn_random_tile(&mut self) -> bool {
        let empty_indices = self
            .tiles
            .iter()
            .enumerate()
            .filter_map(|(idx, value)| (*value == 0).then_some(idx))
            .collect::<Vec<_>>();

        if empty_indices.is_empty() {
            return false;
        }

        let target_index = empty_indices[self.rng.next_usize(empty_indices.len())];
        let value = if self.rng.next_usize(10) == 0 { 4 } else { 2 };
        self.tiles[target_index] = value;
        true
    }
}

fn compute_move(tiles: [u16; BOARD_LEN], direction: MoveDirection) -> MoveComputation {
    let mut next = tiles;
    let mut moved = false;
    let mut score_delta = 0;

    for line in 0..BOARD_SIDE {
        let mut source = [0_u16; BOARD_SIDE];
        for offset in 0..BOARD_SIDE {
            source[offset] = tiles[directional_index(direction, line, offset)];
        }

        let (merged, line_score) = slide_and_merge_line(source);
        score_delta += line_score;
        moved |= merged != source;

        for offset in 0..BOARD_SIDE {
            next[directional_index(direction, line, offset)] = merged[offset];
        }
    }

    MoveComputation {
        tiles: next,
        moved,
        score_delta,
    }
}

fn directional_index(direction: MoveDirection, line: usize, offset: usize) -> usize {
    match direction {
        MoveDirection::Left => line * BOARD_SIDE + offset,
        MoveDirection::Right => line * BOARD_SIDE + (BOARD_SIDE - 1 - offset),
        MoveDirection::Up => offset * BOARD_SIDE + line,
        MoveDirection::Down => (BOARD_SIDE - 1 - offset) * BOARD_SIDE + line,
    }
}

fn slide_and_merge_line(line: [u16; BOARD_SIDE]) -> ([u16; BOARD_SIDE], u32) {
    let mut compact = [0_u16; BOARD_SIDE];
    let mut compact_len = 0;

    for value in line {
        if value != 0 {
            compact[compact_len] = value;
            compact_len += 1;
        }
    }

    let mut output = [0_u16; BOARD_SIDE];
    let mut output_idx = 0;
    let mut source_idx = 0;
    let mut score_delta = 0;

    while source_idx < compact_len {
        if source_idx + 1 < compact_len && compact[source_idx] == compact[source_idx + 1] {
            let merged_value = compact[source_idx] * 2;
            output[output_idx] = merged_value;
            score_delta += u32::from(merged_value);
            source_idx += 2;
        } else {
            output[output_idx] = compact[source_idx];
            source_idx += 1;
        }

        output_idx += 1;
    }

    (output, score_delta)
}

fn can_move(tiles: &[u16; BOARD_LEN]) -> bool {
    if tiles.iter().any(|value| *value == 0) {
        return true;
    }

    for row in 0..BOARD_SIDE {
        for col in 0..BOARD_SIDE {
            let index = row * BOARD_SIDE + col;
            let value = tiles[index];

            if col + 1 < BOARD_SIDE && tiles[index + 1] == value {
                return true;
            }

            if row + 1 < BOARD_SIDE && tiles[index + BOARD_SIDE] == value {
                return true;
            }
        }
    }

    false
}

fn tile_value_class(value: u16) -> &'static str {
    match value {
        0 => "g2048.tile.empty",
        2 => "g2048.tile.v2",
        4 => "g2048.tile.v4",
        8 => "g2048.tile.v8",
        16 => "g2048.tile.v16",
        32 => "g2048.tile.v32",
        64 => "g2048.tile.v64",
        128 => "g2048.tile.v128",
        256 => "g2048.tile.v256",
        512 => "g2048.tile.v512",
        1024 => "g2048.tile.v1024",
        2048 => "g2048.tile.v2048",
        _ => "g2048.tile.super",
    }
}

fn status_message(game: &Game2048) -> (&'static str, &'static str) {
    if game.game_over {
        (
            "💥 No legal moves left. Hit New Game to try again.",
            "g2048.status.over",
        )
    } else if game.won_once {
        (
            "🎉 You reached 2048! Keep going for a higher score.",
            "g2048.status.won",
        )
    } else {
        (
            "Goal: combine tiles to reach 2048 (and keep playing after).",
            "g2048.status.running",
        )
    }
}

fn seed_from_clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0xA5A5_A5A5_5A5A_5A5A)
}

#[derive(Resource, Debug)]
struct Game2048State {
    game: Game2048,
}

impl Default for Game2048State {
    fn default() -> Self {
        Self {
            game: Game2048::new(seed_from_clock()),
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
struct GameRoot;

#[derive(Component, Debug, Clone, Copy)]
struct HeaderBlock;

#[derive(Component, Debug, Clone, Copy)]
struct ScoreStrip;

#[derive(Debug, Clone, Copy)]
enum ScoreKind {
    Score,
    Best,
    Moves,
    Peak,
}

impl ScoreKind {
    fn title(self) -> &'static str {
        match self {
            Self::Score => "SCORE",
            Self::Best => "BEST",
            Self::Moves => "MOVES",
            Self::Peak => "MAX",
        }
    }

    fn value(self, game: &Game2048) -> String {
        match self {
            Self::Score => game.score.to_string(),
            Self::Best => game.best_score.to_string(),
            Self::Moves => game.moves.to_string(),
            Self::Peak => game.max_tile().to_string(),
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
struct ScoreCard {
    kind: ScoreKind,
}

#[derive(Component, Debug, Clone, Copy)]
struct StatusLine;

#[derive(Component, Debug, Clone, Copy)]
struct GameFlowRow;

#[derive(Component, Debug, Clone, Copy)]
struct BoardContainer;

#[derive(Component, Debug, Clone, Copy)]
struct BoardRow;

#[derive(Component, Debug, Clone, Copy)]
struct TileCell {
    index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
struct UiComponentsPad;

#[derive(Component, Debug, Clone, Copy)]
struct SidePanel;

#[derive(Component, Debug, Clone, Copy)]
struct UiComponentsRow;

#[derive(Component, Debug, Clone, Copy)]
struct UiComponentButton {
    action: GameEvent,
    label: &'static str,
}

#[derive(Component, Debug, Clone, Copy)]
struct HintLine;

struct HotkeyCaptureWidget<W: Widget + FromDynWidget + ?Sized> {
    entity: Entity,
    child: WidgetPod<W>,
}

impl<W: Widget + FromDynWidget + ?Sized> HotkeyCaptureWidget<W> {
    fn new(entity: Entity, child: NewWidget<W>) -> Self {
        Self {
            entity,
            child: child.to_pod(),
        }
    }

    fn set_entity(this: &mut WidgetMut<'_, Self>, entity: Entity) {
        this.widget.entity = entity;
    }

    fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for HotkeyCaptureWidget<W> {
    type Action = ();

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if matches!(event, PointerEvent::Down(..)) {
            ctx.request_focus();
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(event) = event
            && let Some(key) = keycode_from_key(&event.key)
        {
            emit_ui_action(
                self.entity,
                KeyboardAction {
                    key,
                    pressed: event.state.is_down(),
                },
            );
            ctx.submit_action::<Self::Action>(());
            ctx.request_render();
        }
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<f64>,
    ) -> f64 {
        ctx.redirect_measurement(&mut self.child, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _scene: &mut Scene) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

struct HotkeyCaptureView<Child> {
    entity: Entity,
    child: Child,
}

const HOTKEY_CAPTURE_CHILD_VIEW_ID: ViewId = ViewId::new(0x2048_0001);

fn hotkey_capture<Child>(entity: Entity, child: Child) -> HotkeyCaptureView<Child>
where
    Child: WidgetView<(), ()>,
{
    HotkeyCaptureView { entity, child }
}

impl<Child> ViewMarker for HotkeyCaptureView<Child> where Child: WidgetView<(), ()> {}

impl<Child> View<(), (), ViewCtx> for HotkeyCaptureView<Child>
where
    Child: WidgetView<(), ()>,
{
    type Element = Pod<HotkeyCaptureWidget<Child::Widget>>;
    type ViewState = Child::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut (),
    ) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = ctx.with_id(HOTKEY_CAPTURE_CHILD_VIEW_ID, |ctx| {
            self.child.build(ctx, &mut ())
        });
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(HotkeyCaptureWidget::new(self.entity, child.new_widget))
            }),
            child_state,
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut (),
    ) {
        if self.entity != prev.entity {
            HotkeyCaptureWidget::set_entity(&mut element, self.entity);
        }

        ctx.with_id(HOTKEY_CAPTURE_CHILD_VIEW_ID, |ctx| {
            self.child.rebuild(
                &prev.child,
                view_state,
                ctx,
                HotkeyCaptureWidget::child_mut(&mut element),
                &mut (),
            );
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(HOTKEY_CAPTURE_CHILD_VIEW_ID, |ctx| {
            self.child.teardown(
                view_state,
                ctx,
                HotkeyCaptureWidget::child_mut(&mut element),
            );
        });
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut (),
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(HOTKEY_CAPTURE_CHILD_VIEW_ID) => self.child.message(
                view_state,
                message,
                HotkeyCaptureWidget::child_mut(&mut element),
                &mut (),
            ),
            None => match message.take_message::<()>() {
                Some(_) => MessageResult::Action(()),
                None => MessageResult::Stale,
            },
            Some(_) => MessageResult::Stale,
        }
    }
}

fn project_game_root(_: &GameRoot, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    let content = apply_widget_style(
        flex_col(children)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::Start),
        &style,
    );

    Arc::new(hotkey_capture(ctx.entity, portal(content)))
}

fn project_header_block(_: &HeaderBlock, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let title_style = resolve_style_for_classes(ctx.world, ["g2048.title"]);
    let subtitle_style = resolve_style_for_classes(ctx.world, ["g2048.subtitle"]);

    Arc::new(apply_widget_style(
        flex_col((
            apply_label_style(label("2048"), &title_style),
            apply_label_style(
                label("Playable demo · scoring · keep playing after 2048"),
                &subtitle_style,
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center),
        &style,
    ))
}

fn project_score_strip(_: &ScoreStrip, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_row(children).main_axis_alignment(MainAxisAlignment::Center),
        &style,
    ))
}

fn project_score_card(score_card: &ScoreCard, ctx: ProjectionCtx<'_>) -> UiView {
    let state = ctx.world.resource::<Game2048State>();
    let style = resolve_style(ctx.world, ctx.entity);
    let caption_style = resolve_style_for_classes(ctx.world, ["g2048.score-caption"]);
    let value_style = resolve_style_for_classes(ctx.world, ["g2048.score-value"]);

    Arc::new(apply_widget_style(
        flex_col((
            apply_label_style(label(score_card.kind.title()), &caption_style),
            apply_label_style(label(score_card.kind.value(&state.game)), &value_style),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center),
        &style,
    ))
}

fn project_status_line(_: &StatusLine, ctx: ProjectionCtx<'_>) -> UiView {
    let state = ctx.world.resource::<Game2048State>();
    let (message, class_name) = status_message(&state.game);
    let style = resolve_style_for_classes(ctx.world, ["g2048.status", class_name]);

    Arc::new(apply_widget_style(
        apply_label_style(label(message), &style),
        &style,
    ))
}

fn project_game_flow_row(_: &GameFlowRow, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_row(children)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Center),
        &style,
    ))
}

fn project_board_container(_: &BoardContainer, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let rows = ctx
        .children
        .into_iter()
        .map(|row| row.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_col(rows).cross_axis_alignment(CrossAxisAlignment::Center),
        &style,
    ))
}

fn project_side_panel(_: &SidePanel, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let viewport = *ctx.world.resource::<GameViewport>();
    let metrics = GameLayoutMetrics::from_viewport(viewport);
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(
        sized_box(apply_widget_style(
            flex_col(children)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_alignment(MainAxisAlignment::Start),
            &style,
        ))
        .fixed_width(Length::px(metrics.side_panel_width)),
    )
}

fn project_board_row(_: &BoardRow, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let cells = ctx
        .children
        .into_iter()
        .map(|cell| cell.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_row(cells).main_axis_alignment(MainAxisAlignment::Center),
        &style,
    ))
}

fn project_tile_cell(tile: &TileCell, ctx: ProjectionCtx<'_>) -> UiView {
    let state = ctx.world.resource::<Game2048State>();
    let viewport = *ctx.world.resource::<GameViewport>();
    let metrics = GameLayoutMetrics::from_viewport(viewport);
    let value = state.game.tiles[tile.index];
    let class_name = tile_value_class(value);
    let style = resolve_style_for_classes(ctx.world, ["g2048.tile", class_name]);

    let text = if value == 0 {
        String::new()
    } else {
        value.to_string()
    };

    Arc::new(
        sized_box(apply_widget_style(
            apply_label_style(label(text), &style),
            &style,
        ))
        .fixed_width(Length::px(metrics.tile_size))
        .fixed_height(Length::px(metrics.tile_size)),
    )
}

fn project_ui_components_pad(_: &UiComponentsPad, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let rows = ctx
        .children
        .into_iter()
        .map(|row| row.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_alignment(MainAxisAlignment::Start),
        &style,
    ))
}

fn project_ui_components_row(_: &UiComponentsRow, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    let buttons = ctx
        .children
        .into_iter()
        .map(|button| button.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(apply_widget_style(
        flex_row(buttons).main_axis_alignment(MainAxisAlignment::Start),
        &style,
    ))
}

fn project_ui_component_button(button_info: &UiComponentButton, ctx: ProjectionCtx<'_>) -> UiView {
    let viewport = *ctx.world.resource::<GameViewport>();
    let metrics = GameLayoutMetrics::from_viewport(viewport);
    let style = resolve_style(ctx.world, ctx.entity);
    let text_color = style
        .colors
        .text
        .unwrap_or(Color::from_rgb8(0xF9, 0xF6, 0xF2));

    Arc::new(
        sized_box(
            button(ctx.entity, button_info.action, button_info.label)
                .padding(style.layout.padding)
                .corner_radius(style.layout.corner_radius)
                .border(
                    style.colors.border.unwrap_or(Color::TRANSPARENT),
                    style.layout.border_width,
                )
                .background_color(style.colors.bg.unwrap_or(Color::TRANSPARENT))
                .color(text_color),
        )
        .fixed_width(Length::px(metrics.ui_component_button_width))
        .fixed_height(Length::px(metrics.ui_component_button_height)),
    )
}

fn project_hint_line(_: &HintLine, ctx: ProjectionCtx<'_>) -> UiView {
    let style = resolve_style(ctx.world, ctx.entity);
    Arc::new(apply_widget_style(
        apply_label_style(
            label(
                "UI Components: Arrow keys / WASD / on-screen buttons. Press Z to undo one move.",
            ),
            &style,
        ),
        &style,
    ))
}

fn spawn_ui_component_button(
    commands: &mut Commands,
    parent: Entity,
    label_text: &'static str,
    action: GameEvent,
    variant_class: &'static str,
) {
    commands.spawn((
        UiComponentButton {
            action,
            label: label_text,
        },
        StyleClass(vec![
            "g2048.ui-component.button".to_string(),
            variant_class.to_string(),
        ]),
        ChildOf(parent),
    ));
}

fn setup_game_world(mut commands: Commands) {
    let root = commands
        .spawn((UiRoot, GameRoot, StyleClass(vec!["g2048.root".to_string()])))
        .id();

    commands.spawn((
        HeaderBlock,
        StyleClass(vec!["g2048.header".to_string()]),
        ChildOf(root),
    ));

    let score_strip = commands
        .spawn((
            ScoreStrip,
            StyleClass(vec!["g2048.score-strip".to_string()]),
            ChildOf(root),
        ))
        .id();

    for kind in [
        ScoreKind::Score,
        ScoreKind::Best,
        ScoreKind::Moves,
        ScoreKind::Peak,
    ] {
        commands.spawn((
            ScoreCard { kind },
            StyleClass(vec!["g2048.score-card".to_string()]),
            ChildOf(score_strip),
        ));
    }

    let flow = commands
        .spawn((
            GameFlowRow,
            StyleClass(vec!["g2048.flow".to_string()]),
            ChildOf(root),
        ))
        .id();

    let board = commands
        .spawn((
            BoardContainer,
            StyleClass(vec!["g2048.board".to_string()]),
            ChildOf(flow),
        ))
        .id();

    for row in 0..BOARD_SIDE {
        let row_entity = commands
            .spawn((
                BoardRow,
                StyleClass(vec!["g2048.board-row".to_string()]),
                ChildOf(board),
            ))
            .id();

        for col in 0..BOARD_SIDE {
            let index = row * BOARD_SIDE + col;
            commands.spawn((
                TileCell { index },
                StyleClass(vec!["g2048.tile-host".to_string()]),
                ChildOf(row_entity),
            ));
        }
    }

    let side_panel = commands
        .spawn((
            SidePanel,
            StyleClass(vec!["g2048.side-panel".to_string()]),
            ChildOf(flow),
        ))
        .id();

    commands.spawn((
        StatusLine,
        StyleClass(vec!["g2048.status-host".to_string()]),
        ChildOf(side_panel),
    ));

    let ui_components = commands
        .spawn((
            UiComponentsPad,
            StyleClass(vec!["g2048.ui-components".to_string()]),
            ChildOf(side_panel),
        ))
        .id();

    let row_top = commands
        .spawn((
            UiComponentsRow,
            StyleClass(vec!["g2048.ui-component-row".to_string()]),
            ChildOf(ui_components),
        ))
        .id();
    spawn_ui_component_button(
        &mut commands,
        row_top,
        "↑ Up",
        GameEvent::Move(MoveDirection::Up),
        "g2048.ui-component.button.primary",
    );

    let row_middle = commands
        .spawn((
            UiComponentsRow,
            StyleClass(vec!["g2048.ui-component-row".to_string()]),
            ChildOf(ui_components),
        ))
        .id();
    spawn_ui_component_button(
        &mut commands,
        row_middle,
        "← Left",
        GameEvent::Move(MoveDirection::Left),
        "g2048.ui-component.button.primary",
    );
    spawn_ui_component_button(
        &mut commands,
        row_middle,
        "↓ Down",
        GameEvent::Move(MoveDirection::Down),
        "g2048.ui-component.button.primary",
    );
    spawn_ui_component_button(
        &mut commands,
        row_middle,
        "→ Right",
        GameEvent::Move(MoveDirection::Right),
        "g2048.ui-component.button.primary",
    );

    let row_bottom = commands
        .spawn((
            UiComponentsRow,
            StyleClass(vec!["g2048.ui-component-row".to_string()]),
            ChildOf(ui_components),
        ))
        .id();
    spawn_ui_component_button(
        &mut commands,
        row_bottom,
        "↶ Undo",
        GameEvent::Undo,
        "g2048.ui-component.button.secondary",
    );
    spawn_ui_component_button(
        &mut commands,
        row_bottom,
        "↺ New Game",
        GameEvent::Restart,
        "g2048.ui-component.button.danger",
    );

    commands.spawn((
        HintLine,
        StyleClass(vec!["g2048.hint".to_string()]),
        ChildOf(side_panel),
    ));
}

fn drain_game_events(world: &mut World) {
    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<GameEvent>();

    if events.is_empty() {
        return;
    }

    let mut state = world.resource_mut::<Game2048State>();
    for event in events {
        match event.action {
            GameEvent::Move(direction) => {
                state.game.try_move(direction);
            }
            GameEvent::Undo => {
                state.game.undo();
            }
            GameEvent::Restart => {
                state.game.restart();
            }
        }
    }
}

fn track_game_viewport(
    mut window_resized: MessageReader<WindowResized>,
    mut viewport: ResMut<GameViewport>,
) {
    for event in window_resized.read() {
        viewport.width = (event.width as f64).max(1.0);
        viewport.height = (event.height as f64).max(1.0);
    }
}

fn sync_keyboard_input(world: &mut World) {
    if world.get_resource::<ButtonInput<KeyCode>>().is_none() {
        world.insert_resource(ButtonInput::<KeyCode>::default());
    }

    let events = world
        .resource_mut::<UiEventQueue>()
        .drain_actions::<KeyboardAction>();

    if events.is_empty() {
        world.resource_mut::<ButtonInput<KeyCode>>().clear();
        return;
    }

    let mut input = world.resource_mut::<ButtonInput<KeyCode>>();
    input.clear();
    for event in events {
        if event.action.pressed {
            input.press(event.action.key);
        } else {
            input.release(event.action.key);
        }
    }
}

fn apply_keyboard_game_input(world: &mut World) {
    let Some(input) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };

    let mut pending_move = None;
    let mut pending_undo = false;

    if input.just_pressed(KeyCode::ArrowUp) || input.just_pressed(KeyCode::KeyW) {
        pending_move = Some(MoveDirection::Up);
    } else if input.just_pressed(KeyCode::ArrowDown) || input.just_pressed(KeyCode::KeyS) {
        pending_move = Some(MoveDirection::Down);
    } else if input.just_pressed(KeyCode::ArrowLeft) || input.just_pressed(KeyCode::KeyA) {
        pending_move = Some(MoveDirection::Left);
    } else if input.just_pressed(KeyCode::ArrowRight) || input.just_pressed(KeyCode::KeyD) {
        pending_move = Some(MoveDirection::Right);
    }

    if input.just_pressed(KeyCode::KeyZ) {
        pending_undo = true;
    }

    if pending_move.is_none() && !pending_undo {
        return;
    }

    let mut state = world.resource_mut::<Game2048State>();
    if pending_undo {
        state.game.undo();
    }
    if let Some(direction) = pending_move {
        state.game.try_move(direction);
    }
}

bevy_xilem::impl_ui_component_template!(GameRoot, project_game_root);
bevy_xilem::impl_ui_component_template!(HeaderBlock, project_header_block);
bevy_xilem::impl_ui_component_template!(ScoreStrip, project_score_strip);
bevy_xilem::impl_ui_component_template!(ScoreCard, project_score_card);
bevy_xilem::impl_ui_component_template!(StatusLine, project_status_line);
bevy_xilem::impl_ui_component_template!(GameFlowRow, project_game_flow_row);
bevy_xilem::impl_ui_component_template!(BoardContainer, project_board_container);
bevy_xilem::impl_ui_component_template!(BoardRow, project_board_row);
bevy_xilem::impl_ui_component_template!(TileCell, project_tile_cell);
bevy_xilem::impl_ui_component_template!(SidePanel, project_side_panel);
bevy_xilem::impl_ui_component_template!(UiComponentsPad, project_ui_components_pad);
bevy_xilem::impl_ui_component_template!(UiComponentsRow, project_ui_components_row);
bevy_xilem::impl_ui_component_template!(UiComponentButton, project_ui_component_button);
bevy_xilem::impl_ui_component_template!(HintLine, project_hint_line);

fn build_2048_app() -> App {
    init_logging();

    let mut app = App::new();
    app.add_plugins(BevyXilemPlugin)
        .load_style_sheet_ron(include_str!("../assets/themes/game_2048.ron"))
        .insert_resource(ButtonInput::<KeyCode>::default())
        .insert_resource(GameViewport::default())
        .insert_resource(Game2048State::default())
        .register_ui_component::<GameRoot>()
        .register_ui_component::<HeaderBlock>()
        .register_ui_component::<ScoreStrip>()
        .register_ui_component::<ScoreCard>()
        .register_ui_component::<StatusLine>()
        .register_ui_component::<GameFlowRow>()
        .register_ui_component::<BoardContainer>()
        .register_ui_component::<BoardRow>()
        .register_ui_component::<TileCell>()
        .register_ui_component::<SidePanel>()
        .register_ui_component::<UiComponentsPad>()
        .register_ui_component::<UiComponentsRow>()
        .register_ui_component::<UiComponentButton>()
        .register_ui_component::<HintLine>()
        .add_systems(Startup, (setup_game_world, setup_fluent_theme_toggle))
        .add_systems(
            PreUpdate,
            (drain_fluent_theme_toggle_events, track_game_viewport),
        )
        .add_systems(
            PreUpdate,
            (
                sync_keyboard_input,
                apply_keyboard_game_input,
                drain_game_events,
            )
                .chain(),
        );
    app
}

fn main() -> Result<(), EventLoopError> {
    run_app_with_window_options(build_2048_app(), "2048 Game", |options| {
        options.with_initial_inner_size(LogicalSize::new(1040.0, 720.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_line_follows_2048_rules() {
        let (merged, score) = slide_and_merge_line([2, 2, 2, 2]);
        assert_eq!(merged, [4, 4, 0, 0]);
        assert_eq!(score, 8);

        let (merged, score) = slide_and_merge_line([2, 2, 4, 4]);
        assert_eq!(merged, [4, 8, 0, 0]);
        assert_eq!(score, 12);

        let (merged, score) = slide_and_merge_line([4, 0, 4, 4]);
        assert_eq!(merged, [8, 4, 0, 0]);
        assert_eq!(score, 8);
    }

    #[test]
    fn compute_move_left_scores_and_moves_tiles() {
        let mut tiles = [0_u16; BOARD_LEN];
        tiles[0] = 2;
        tiles[1] = 2;
        tiles[2] = 2;
        tiles[3] = 2;

        let result = compute_move(tiles, MoveDirection::Left);
        assert!(result.moved);
        assert_eq!(&result.tiles[0..4], &[4, 4, 0, 0]);
        assert_eq!(result.score_delta, 8);
    }

    #[test]
    fn reaching_2048_does_not_end_game() {
        let mut game = Game2048::new(42);
        game.tiles = [
            1024, 1024, 0, 0, // merge into 2048
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        game.score = 0;
        game.won_once = false;
        game.game_over = false;

        let moved = game.apply_move(MoveDirection::Left, false);
        assert!(moved);
        assert!(game.won_once);
        assert!(!game.game_over);
        assert_eq!(game.max_tile(), 2048);
    }

    #[test]
    fn undo_restores_previous_state_once() {
        let mut game = Game2048::new(9);
        game.tiles = [2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        game.score = 16;
        game.moves = 3;
        game.won_once = false;
        game.game_over = false;

        let before = game.snapshot();
        let moved = game.apply_move(MoveDirection::Left, false);
        assert!(moved);
        assert_eq!(game.tiles[0], 4);
        assert_eq!(game.score, 20);
        assert!(game.undo());

        assert_eq!(game.tiles, before.tiles);
        assert_eq!(game.score, before.score);
        assert_eq!(game.moves, before.moves);
        assert!(!game.undo());
    }

    #[test]
    fn key_mapping_supports_arrow_wasd_and_undo() {
        assert_eq!(
            keycode_from_key(&Key::Named(NamedKey::ArrowUp)),
            Some(KeyCode::ArrowUp)
        );
        assert_eq!(
            keycode_from_key(&Key::Character("w".into())),
            Some(KeyCode::KeyW)
        );
        assert_eq!(
            keycode_from_key(&Key::Character("A".into())),
            Some(KeyCode::KeyA)
        );
        assert_eq!(
            keycode_from_key(&Key::Character("z".into())),
            Some(KeyCode::KeyZ)
        );
        assert_eq!(keycode_from_key(&Key::Character("x".into())), None);
    }

    #[test]
    fn keyboard_action_pipeline_applies_move_immediately() {
        let mut world = World::new();
        let sender = world.spawn_empty().id();
        world.insert_resource(UiEventQueue::default());
        world.insert_resource(ButtonInput::<KeyCode>::default());

        let mut state = Game2048State {
            game: Game2048::new(123),
        };
        state.game.tiles = [2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        state.game.score = 0;
        state.game.moves = 0;
        state.game.game_over = false;
        world.insert_resource(state);

        world.resource::<UiEventQueue>().push_typed(
            sender,
            KeyboardAction {
                key: KeyCode::ArrowLeft,
                pressed: true,
            },
        );

        sync_keyboard_input(&mut world);
        apply_keyboard_game_input(&mut world);

        let state = world.resource::<Game2048State>();
        assert_eq!(state.game.tiles[0], 4);
        assert_eq!(state.game.score, 4);
        assert_eq!(state.game.moves, 1);
    }

    #[test]
    fn full_board_without_merges_is_game_over() {
        let mut game = Game2048::new(7);
        game.tiles = [2, 4, 2, 4, 4, 2, 4, 2, 2, 4, 2, 4, 4, 2, 4, 2];

        game.recompute_flags();
        assert!(game.game_over);
        assert!(!can_move(&game.tiles));
    }

    #[test]
    fn layout_metrics_shrink_with_smaller_viewports() {
        let large = GameLayoutMetrics::from_viewport(GameViewport {
            width: 1200.0,
            height: 820.0,
        });
        let small = GameLayoutMetrics::from_viewport(GameViewport {
            width: 520.0,
            height: 620.0,
        });

        assert!(small.tile_size < large.tile_size);
        assert!(small.side_panel_width <= large.side_panel_width);
        assert!(small.ui_component_button_width <= large.ui_component_button_width);
        assert!(small.ui_component_button_height <= large.ui_component_button_height);
        assert!(small.tile_size >= 44.0);
    }

    #[test]
    fn embedded_theme_ron_parses() {
        bevy_xilem::parse_stylesheet_ron(include_str!("../assets/themes/game_2048.ron"))
            .expect("embedded game_2048 stylesheet should parse");
    }
}
