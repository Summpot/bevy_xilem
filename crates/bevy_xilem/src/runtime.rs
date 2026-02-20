use std::{fmt::Debug, sync::Arc};

use bevy_ecs::{
    entity::Entity,
    message::MessageReader,
    prelude::{Added, FromWorld, NonSendMut, Query, Res, ResMut, With, World},
};
use bevy_input::{
    ButtonState,
    mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel},
};
#[cfg(test)]
use bevy_math::Vec2;
use bevy_time::Time;
use bevy_window::{
    CursorLeft, CursorMoved, PrimaryWindow, Window, WindowResized, WindowScaleFactorChanged,
};
use masonry::layout::{Dim, UnitPoint};
use masonry::{
    app::{RenderRoot, RenderRootOptions, WindowSizePolicy},
    core::{
        Handled, PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo,
        PointerScrollEvent, PointerState, PointerType, PointerUpdate, ScrollDelta, Widget,
        WidgetId, WidgetRef, WindowEvent,
    },
    dpi::{PhysicalPosition, PhysicalSize},
    peniko::Color,
    theme::default_property_set,
    vello::{Renderer, wgpu},
    widgets::Passthrough,
};
use masonry_winit::app::{ExistingWindowMetrics, ExternalWindowSurface, existing_window_metrics};
use xilem::style::Style as _;
use xilem_core::{ProxyError, RawProxy, SendMessage, View, ViewId};
use xilem_masonry::{
    ViewCtx,
    view::{label, zstack},
};

use crate::{
    MasonryWidgetId,
    events::{UiEventQueue, install_global_ui_event_queue},
    overlay::OverlayPointerRoutingState,
    projection::{UiAnyView, UiView},
    synthesize::SynthesizedUiViews,
};

#[derive(Debug)]
struct NoopProxy;

impl RawProxy for NoopProxy {
    fn send_message(&self, _path: Arc<[ViewId]>, message: SendMessage) -> Result<(), ProxyError> {
        Err(ProxyError::DriverFinished(message))
    }

    fn dyn_debug(&self) -> &dyn Debug {
        self
    }
}

type RuntimeViewState = <UiAnyView as View<(), (), ViewCtx>>::ViewState;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointerTraceEvent {
    Move,
    Leave,
    Down,
    Up,
    Scroll,
}

/// Headless Masonry runtime owned by Bevy.
///
/// This runtime keeps ownership of the retained Masonry tree and drives it via
/// explicit Bevy-system input injection + synthesis-time rebuilds.
pub struct MasonryRuntime {
    pub root_widget_id: WidgetId,
    pub render_root: RenderRoot,
    view_ctx: ViewCtx,
    view_state: RuntimeViewState,
    current_view: UiView,
    active_window: Option<Entity>,
    window_scale_factor: f64,
    pointer_info: PointerInfo,
    pointer_state: PointerState,
    viewport_width: f64,
    viewport_height: f64,
    window_surface: Option<ExternalWindowSurface>,
    renderer: Option<Renderer>,
    #[cfg(test)]
    pointer_trace: Vec<PointerTraceEvent>,
}

impl FromWorld for MasonryRuntime {
    fn from_world(world: &mut World) -> Self {
        world.init_resource::<UiEventQueue>();
        let queue = world.resource::<UiEventQueue>().shared_queue();
        install_global_ui_event_queue(queue);

        let mut view_ctx = ViewCtx::new(
            Arc::new(NoopProxy),
            Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime should initialize")),
        );

        let initial_view: UiView = Arc::new(label("bevy_xilem: waiting for synthesized root"));
        let (initial_root_widget, view_state) =
            <UiAnyView as View<(), (), ViewCtx>>::build(initial_view.as_ref(), &mut view_ctx, ());

        let options = RenderRootOptions {
            default_properties: Arc::new(default_property_set()),
            use_system_fonts: true,
            size_policy: WindowSizePolicy::User,
            size: PhysicalSize::new(1024, 768),
            scale_factor: 1.0,
            test_font: None,
        };
        let initial_viewport = (options.size.width as f64, options.size.height as f64);

        let mut render_root =
            RenderRoot::new(initial_root_widget.new_widget.erased(), |_| {}, options);

        if let Some(fallback) = focus_fallback_widget(&render_root) {
            let _ = render_root.set_focus_fallback(Some(fallback));
        }

        let root_widget_id = render_root.get_layer_root(0).id();

        Self {
            root_widget_id,
            render_root,
            view_ctx,
            view_state,
            current_view: initial_view,
            active_window: None,
            window_scale_factor: 1.0,
            pointer_info: PointerInfo {
                pointer_id: Some(PointerId::new(1).expect("pointer id 1 should be valid")),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
            pointer_state: PointerState::default(),
            viewport_width: initial_viewport.0,
            viewport_height: initial_viewport.1,
            window_surface: None,
            renderer: None,
            #[cfg(test)]
            pointer_trace: Vec::new(),
        }
    }
}

fn focus_fallback_widget(render_root: &RenderRoot) -> Option<WidgetId> {
    render_root
        .get_layer_root(0)
        .downcast::<Passthrough>()
        .map(|root| root.inner().inner_id())
}

fn parse_entity_scope_widget_id(widget: WidgetRef<'_, dyn Widget>) -> Option<(Entity, WidgetId)> {
    let debug = widget.get_debug_text()?;
    let bits = debug.strip_prefix("entity_scope=")?.parse::<u64>().ok()?;
    let entity = Entity::try_from_bits(bits)?;
    Some((entity, widget.id()))
}

fn collect_entity_scope_widget_ids(
    widget: WidgetRef<'_, dyn Widget>,
    out: &mut Vec<(Entity, WidgetId)>,
) {
    for child in widget.children() {
        collect_entity_scope_widget_ids(child, out);
    }

    let Some((entity, widget_id)) = parse_entity_scope_widget_id(widget) else {
        return;
    };

    if let Some(index) = out
        .iter()
        .position(|(existing_entity, _)| *existing_entity == entity)
    {
        out[index] = (entity, widget_id);
    } else {
        out.push((entity, widget_id));
    }
}

/// Synchronize per-entity Masonry widget identity onto ECS components.
pub fn sync_masonry_widget_ids(world: &mut World) {
    let mappings = {
        let Some(runtime) = world.get_non_send_resource::<MasonryRuntime>() else {
            return;
        };

        let root = runtime.render_root.get_layer_root(0);
        let mut mappings = Vec::new();
        collect_entity_scope_widget_ids(root, &mut mappings);
        mappings
    };

    let existing = {
        let mut query = world.query_filtered::<Entity, With<MasonryWidgetId>>();
        query.iter(world).collect::<Vec<_>>()
    };

    for entity in existing {
        let still_mapped = mappings
            .iter()
            .any(|(mapped_entity, _)| *mapped_entity == entity);
        if !still_mapped && world.get_entity(entity).is_ok() {
            world.entity_mut(entity).remove::<MasonryWidgetId>();
        }
    }

    for (entity, widget_id) in mappings {
        if world.get_entity(entity).is_err() {
            continue;
        }

        let needs_update = world
            .get::<MasonryWidgetId>(entity)
            .is_none_or(|current| current.0 != widget_id);

        if needs_update {
            world.entity_mut(entity).insert(MasonryWidgetId(widget_id));
        }
    }
}

impl MasonryRuntime {
    #[must_use]
    pub fn is_attached_to_window(&self, window: Entity) -> bool {
        self.active_window == Some(window)
    }

    pub fn attach_to_window(&mut self, window: Entity, metrics: ExistingWindowMetrics) {
        self.sync_window_metrics(window, metrics);
    }

    #[must_use]
    pub fn viewport_size(&self) -> (f64, f64) {
        (self.viewport_width.max(1.0), self.viewport_height.max(1.0))
    }

    #[must_use]
    pub fn get_hit_path(
        &self,
        physical_pos: masonry::kurbo::Point,
    ) -> Vec<masonry::core::WidgetId> {
        self.render_root.get_hit_path(physical_pos)
    }

    #[cfg(test)]
    pub(crate) fn pointer_position_for_tests(&self) -> Vec2 {
        Vec2::new(
            self.pointer_state.position.x as f32,
            self.pointer_state.position.y as f32,
        )
    }

    #[cfg(test)]
    pub(crate) fn pointer_trace_for_tests(&self) -> &[PointerTraceEvent] {
        &self.pointer_trace
    }

    #[cfg(test)]
    pub(crate) fn clear_pointer_trace_for_tests(&mut self) {
        self.pointer_trace.clear();
    }

    pub fn rebuild_root_view(&mut self, next_view: UiView) {
        self.render_root.edit_base_layer(|mut root| {
            let mut root = root.downcast::<Passthrough>();
            <UiAnyView as View<(), (), ViewCtx>>::rebuild(
                next_view.as_ref(),
                self.current_view.as_ref(),
                &mut self.view_state,
                &mut self.view_ctx,
                root.reborrow_mut(),
                (),
            );
            self.root_widget_id = root.widget.inner_id();
        });

        self.current_view = next_view;

        if let Some(fallback) = focus_fallback_widget(&self.render_root) {
            let _ = self.render_root.set_focus_fallback(Some(fallback));
        }
    }

    fn accepts_window(&mut self, window: Entity) -> bool {
        match self.active_window {
            Some(active) => active == window,
            None => {
                self.active_window = Some(window);
                true
            }
        }
    }

    pub fn handle_cursor_moved(&mut self, window: Entity, x: f32, y: f32) -> Handled {
        if !self.accepts_window(window) {
            return Handled::No;
        }

        self.pointer_state.position = PhysicalPosition {
            x: x as f64,
            y: y as f64,
        };

        #[cfg(test)]
        self.pointer_trace.push(PointerTraceEvent::Move);

        self.render_root
            .handle_pointer_event(PointerEvent::Move(PointerUpdate {
                pointer: self.pointer_info.clone(),
                current: self.pointer_state.clone(),
                coalesced: vec![],
                predicted: vec![],
            }))
    }

    pub fn handle_cursor_left(&mut self, window: Entity) -> Handled {
        if !self.accepts_window(window) {
            return Handled::No;
        }

        #[cfg(test)]
        self.pointer_trace.push(PointerTraceEvent::Leave);

        self.render_root
            .handle_pointer_event(PointerEvent::Leave(self.pointer_info.clone()))
    }

    pub fn handle_mouse_button(
        &mut self,
        window: Entity,
        button: MouseButton,
        state: ButtonState,
    ) -> Handled {
        if !self.accepts_window(window) {
            return Handled::No;
        }

        let Some(button) = map_mouse_button(button) else {
            return Handled::No;
        };

        match state {
            ButtonState::Pressed => {
                self.pointer_state.buttons.insert(button);
                #[cfg(test)]
                self.pointer_trace.push(PointerTraceEvent::Down);
                self.render_root
                    .handle_pointer_event(PointerEvent::Down(PointerButtonEvent {
                        pointer: self.pointer_info.clone(),
                        button: Some(button),
                        state: self.pointer_state.clone(),
                    }))
            }
            ButtonState::Released => {
                self.pointer_state.buttons.remove(button);
                #[cfg(test)]
                self.pointer_trace.push(PointerTraceEvent::Up);
                self.render_root
                    .handle_pointer_event(PointerEvent::Up(PointerButtonEvent {
                        pointer: self.pointer_info.clone(),
                        button: Some(button),
                        state: self.pointer_state.clone(),
                    }))
            }
        }
    }

    pub fn handle_mouse_wheel(
        &mut self,
        window: Entity,
        unit: MouseScrollUnit,
        x: f32,
        y: f32,
    ) -> Handled {
        if !self.accepts_window(window) {
            return Handled::No;
        }

        let factor = if unit == MouseScrollUnit::Line {
            MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR
        } else {
            1.0
        };

        #[cfg(test)]
        self.pointer_trace.push(PointerTraceEvent::Scroll);

        self.render_root
            .handle_pointer_event(PointerEvent::Scroll(PointerScrollEvent {
                pointer: self.pointer_info.clone(),
                delta: ScrollDelta::PixelDelta(PhysicalPosition {
                    x: (x * factor) as f64,
                    y: (y * factor) as f64,
                }),
                state: self.pointer_state.clone(),
            }))
    }

    pub fn handle_window_resized(&mut self, window: Entity, width: f32, height: f32) -> Handled {
        if !self.accepts_window(window) {
            return Handled::No;
        }

        self.viewport_width = width.max(1.0) as f64;
        self.viewport_height = height.max(1.0) as f64;

        let scale = self.window_scale_factor.max(f64::EPSILON);
        let physical_width = (self.viewport_width * scale).round().max(1.0) as u32;
        let physical_height = (self.viewport_height * scale).round().max(1.0) as u32;

        self.render_root
            .handle_window_event(WindowEvent::Resize(PhysicalSize::new(
                physical_width,
                physical_height,
            )))
    }

    pub fn handle_window_scale_factor_changed(
        &mut self,
        window: Entity,
        scale_factor: f64,
    ) -> Handled {
        if !self.accepts_window(window) {
            return Handled::No;
        }

        self.window_scale_factor = scale_factor.max(f64::EPSILON);
        let _ = self
            .render_root
            .handle_window_event(WindowEvent::Rescale(self.window_scale_factor));

        let physical_width = (self.viewport_width * self.window_scale_factor)
            .round()
            .max(1.0) as u32;
        let physical_height = (self.viewport_height * self.window_scale_factor)
            .round()
            .max(1.0) as u32;

        self.render_root
            .handle_window_event(WindowEvent::Resize(PhysicalSize::new(
                physical_width,
                physical_height,
            )))
    }

    pub fn ensure_external_surface(&mut self, window: Arc<xilem::winit::window::Window>) -> bool {
        if self.window_surface.is_some() {
            return true;
        }

        match ExternalWindowSurface::new(window, wgpu::PresentMode::AutoVsync) {
            Ok(surface) => {
                self.window_surface = Some(surface);
                true
            }
            Err(error) => {
                tracing::error!("failed to initialize external Masonry surface: {error}");
                false
            }
        }
    }

    pub fn paint_frame(&mut self, delta: std::time::Duration) {
        let _ = self
            .render_root
            .handle_window_event(WindowEvent::AnimFrame(delta));
        let logical_size = self.render_root.size();
        let (scene, _tree_update) = self.render_root.redraw();

        let Some(surface) = self.window_surface.as_mut() else {
            return;
        };

        let _ = surface.sync_window_metrics();
        surface.render_scene(
            &mut self.renderer,
            scene,
            logical_size.width.max(1),
            logical_size.height.max(1),
            Color::BLACK,
        );
    }

    fn sync_window_metrics(&mut self, window: Entity, metrics: ExistingWindowMetrics) {
        let window_changed = self.active_window != Some(window);
        if window_changed {
            self.active_window = Some(window);
            self.window_surface = None;
            self.renderer = None;
        }

        let next_scale = metrics.scale_factor.max(f64::EPSILON);
        let next_viewport_width = metrics.logical_size.width.max(1.0);
        let next_viewport_height = metrics.logical_size.height.max(1.0);
        let needs_rescale = (self.window_scale_factor - next_scale).abs() > f64::EPSILON;
        let needs_resize = (self.viewport_width - next_viewport_width).abs() > f64::EPSILON
            || (self.viewport_height - next_viewport_height).abs() > f64::EPSILON;

        self.window_scale_factor = next_scale;
        self.viewport_width = next_viewport_width;
        self.viewport_height = next_viewport_height;

        if window_changed || needs_rescale {
            let _ = self
                .render_root
                .handle_window_event(WindowEvent::Rescale(self.window_scale_factor));
        }

        if window_changed || needs_resize || needs_rescale {
            let _ = self
                .render_root
                .handle_window_event(WindowEvent::Resize(PhysicalSize::new(
                    metrics.physical_size.width.max(1),
                    metrics.physical_size.height.max(1),
                )));
        }
    }
}

fn compose_runtime_root(roots: &[UiView]) -> UiView {
    match roots {
        [] => Arc::new(label("bevy_xilem: no synthesized root")),
        [root] => root.clone(),
        _ => Arc::new(
            zstack(roots.to_vec())
                .alignment(UnitPoint::TOP_LEFT)
                .width(Dim::Stretch)
                .height(Dim::Stretch),
        ),
    }
}

fn map_mouse_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Auxiliary),
        MouseButton::Back => Some(PointerButton::X1),
        MouseButton::Forward => Some(PointerButton::X2),
        MouseButton::Other(_) => None,
    }
}

/// PreUpdate input bridge: consume Bevy window/input messages and inject them into Masonry.
pub fn inject_bevy_input_into_masonry(
    runtime: Option<NonSendMut<MasonryRuntime>>,
    mut overlay_routing: ResMut<OverlayPointerRoutingState>,
    primary_window_query: Query<&Window, With<PrimaryWindow>>,
    primary_window_entity_query: Query<Entity, With<PrimaryWindow>>,
    mut cursor_moved: MessageReader<CursorMoved>,
    mut cursor_left: MessageReader<CursorLeft>,
    mut mouse_button_input: MessageReader<MouseButtonInput>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mut window_resized: MessageReader<WindowResized>,
    mut window_scale_factor_changed: MessageReader<WindowScaleFactorChanged>,
) {
    let Some(mut runtime) = runtime else {
        return;
    };

    let Some(primary_window_entity) = primary_window_entity_query.iter().next() else {
        return;
    };

    let Ok(primary_window) = primary_window_query.get(primary_window_entity) else {
        return;
    };

    for event in cursor_moved.read() {
        if event.window != primary_window_entity {
            continue;
        }

        let Some(pointer_position) = primary_window.physical_cursor_position() else {
            continue;
        };

        runtime.handle_cursor_moved(
            primary_window_entity,
            pointer_position.x,
            pointer_position.y,
        );
        tracing::trace!(
            "Input Injection - Bevy Physical Cursor Moved: ({}, {}). Injected into Masonry.",
            pointer_position.x,
            pointer_position.y
        );
    }

    for event in cursor_left.read() {
        if event.window != primary_window_entity {
            continue;
        }

        runtime.handle_cursor_left(primary_window_entity);
    }

    for event in mouse_button_input.read() {
        if event.window != primary_window_entity {
            continue;
        }

        let suppressed = match event.state {
            ButtonState::Pressed => {
                overlay_routing.take_suppressed_press(primary_window_entity, event.button)
            }
            ButtonState::Released => {
                overlay_routing.take_suppressed_release(primary_window_entity, event.button)
            }
        };

        if suppressed {
            continue;
        }

        let Some(pointer_position) = primary_window.physical_cursor_position() else {
            tracing::debug!(
                "skipping mouse button input because primary cursor is outside window {:?}",
                primary_window_entity
            );
            continue;
        };

        runtime.handle_cursor_moved(
            primary_window_entity,
            pointer_position.x,
            pointer_position.y,
        );

        runtime.handle_mouse_button(primary_window_entity, event.button, event.state);
        tracing::trace!(
            "Input Injection - Mouse Button: {:?} {:?} at Physical ({}, {})",
            event.button,
            event.state,
            pointer_position.x,
            pointer_position.y
        );
    }

    for event in mouse_wheel.read() {
        if event.window != primary_window_entity {
            continue;
        }

        let Some(pointer_position) = primary_window.physical_cursor_position() else {
            tracing::debug!(
                "skipping mouse wheel input because primary cursor is outside window {:?}",
                primary_window_entity
            );
            continue;
        };

        runtime.handle_cursor_moved(
            primary_window_entity,
            pointer_position.x,
            pointer_position.y,
        );
        runtime.handle_mouse_wheel(primary_window_entity, event.unit, event.x, event.y);
        tracing::trace!(
            "Input Injection - Mouse Wheel: {:?} ({}, {}) at Physical cursor ({}, {})",
            event.unit,
            event.x,
            event.y,
            pointer_position.x,
            pointer_position.y
        );
    }

    for event in window_resized.read() {
        if event.window != primary_window_entity {
            continue;
        }

        runtime.handle_window_resized(
            primary_window_entity,
            primary_window.width(),
            primary_window.height(),
        );
        tracing::trace!(
            "Window Resize - Bevy Logical Size: {}x{}, Injected into Masonry.",
            primary_window.width(),
            primary_window.height()
        );
    }

    for event in window_scale_factor_changed.read() {
        if event.window != primary_window_entity {
            continue;
        }

        runtime.handle_window_scale_factor_changed(
            primary_window_entity,
            primary_window.scale_factor() as f64,
        );
        tracing::trace!(
            "Window Scale Factor - Bevy Scale: {}, Injected into Masonry.",
            primary_window.scale_factor()
        );
    }
}

/// Attach Masonry runtime viewport state to the primary Bevy winit window once available.
pub fn initialize_masonry_runtime_from_primary_window(
    runtime: Option<NonSendMut<MasonryRuntime>>,
    added_primary_window_query: Query<Entity, (With<PrimaryWindow>, Added<PrimaryWindow>)>,
    primary_window_query: Query<Entity, With<PrimaryWindow>>,
) {
    let Some(mut runtime) = runtime else {
        return;
    };

    let primary_window_entity = added_primary_window_query
        .iter()
        .next()
        .or_else(|| primary_window_query.iter().next());

    let Some(primary_window_entity) = primary_window_entity else {
        return;
    };

    if runtime.is_attached_to_window(primary_window_entity) {
        return;
    }

    let Some(metrics) = bevy_winit::WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        winit_windows
            .get_window(primary_window_entity)
            .map(|window| existing_window_metrics(window))
    }) else {
        return;
    };

    runtime.attach_to_window(primary_window_entity, metrics);

    tracing::trace!(
        "Runtime Init - Primary Window Logic Size: {}x{}, Scale: {}",
        metrics.logical_size.width,
        metrics.logical_size.height,
        metrics.scale_factor
    );

    // Prime Masonry's layout root with an explicit initial logical resize so hit-testing
    // never starts from a zero-sized root, even before the first window-resize message.
    runtime.handle_window_resized(
        primary_window_entity,
        metrics.logical_size.width as f32,
        metrics.logical_size.height as f32,
    );
    tracing::trace!(
        "Runtime Init - Priming Masonry Resize: {}x{}",
        metrics.logical_size.width,
        metrics.logical_size.height
    );
}

/// PostUpdate rebuild step: diff synthesized root against retained Masonry tree.
pub fn rebuild_masonry_runtime(world: &mut World) {
    let Some(roots) = world
        .get_resource::<SynthesizedUiViews>()
        .map(|views| views.roots.clone())
    else {
        return;
    };

    let next_root = compose_runtime_root(&roots);

    let Some(mut runtime) = world.get_non_send_resource_mut::<MasonryRuntime>() else {
        return;
    };

    runtime.rebuild_root_view(next_root);
}

/// Last-stage paint pass: submit Masonry scene through Vello and present to the primary window.
pub fn paint_masonry_ui(
    runtime: Option<NonSendMut<MasonryRuntime>>,
    primary_window_query: Query<Entity, With<PrimaryWindow>>,
    time: Res<Time>,
) {
    let Some(mut runtime) = runtime else {
        return;
    };

    let Some(primary_window_entity) = primary_window_query.iter().next() else {
        return;
    };

    let Some((window, metrics)) = bevy_winit::WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        winit_windows
            .get_window(primary_window_entity)
            .map(|window| {
                let cloned = window.clone_window();
                let metrics = existing_window_metrics(&cloned);
                (cloned, metrics)
            })
    }) else {
        return;
    };

    runtime.attach_to_window(primary_window_entity, metrics);

    if !runtime.ensure_external_surface(window.clone()) {
        return;
    }

    runtime.paint_frame(time.delta());
    window.request_redraw();
}
