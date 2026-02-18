use std::{fmt::Debug, sync::Arc};

use bevy_ecs::{
    entity::Entity,
    message::MessageReader,
    prelude::{Added, FromWorld, NonSendMut, Query, ResMut, With, World},
};
use bevy_input::{
    ButtonState,
    mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel},
};
use bevy_window::{
    CursorLeft, CursorMoved, PrimaryWindow, WindowResized, WindowScaleFactorChanged,
};
use masonry::layout::{Dim, UnitPoint};
use masonry::{
    app::{RenderRoot, RenderRootOptions, WindowSizePolicy},
    core::{
        Handled, PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo,
        PointerScrollEvent, PointerState, PointerType, PointerUpdate, ScrollDelta, WidgetId,
        WindowEvent,
    },
    dpi::{PhysicalPosition, PhysicalSize},
    theme::default_property_set,
    widgets::Passthrough,
};
use masonry_winit::app::{ExistingWindowMetrics, existing_window_metrics};
use xilem::style::Style as _;
use xilem_core::{ProxyError, RawProxy, SendMessage, View, ViewId};
use xilem_masonry::{
    ViewCtx,
    view::{label, zstack},
};

use crate::{
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
        }
    }
}

fn focus_fallback_widget(render_root: &RenderRoot) -> Option<WidgetId> {
    render_root
        .get_layer_root(0)
        .downcast::<Passthrough>()
        .map(|root| root.inner().inner_id())
}

impl MasonryRuntime {
    #[must_use]
    pub fn is_attached_to_window(&self, window: Entity) -> bool {
        self.active_window == Some(window)
    }

    pub fn attach_to_window(&mut self, window: Entity, metrics: ExistingWindowMetrics) {
        self.active_window = Some(window);
        self.window_scale_factor = metrics.scale_factor.max(f64::EPSILON);
        self.viewport_width = metrics.logical_size.width.max(1.0);
        self.viewport_height = metrics.logical_size.height.max(1.0);

        let _ = self
            .render_root
            .handle_window_event(WindowEvent::Rescale(self.window_scale_factor));
        let _ = self
            .render_root
            .handle_window_event(WindowEvent::Resize(PhysicalSize::new(
                metrics.physical_size.width.max(1),
                metrics.physical_size.height.max(1),
            )));
    }

    #[must_use]
    pub fn viewport_size(&self) -> (f64, f64) {
        (self.viewport_width.max(1.0), self.viewport_height.max(1.0))
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
                self.render_root
                    .handle_pointer_event(PointerEvent::Down(PointerButtonEvent {
                        pointer: self.pointer_info.clone(),
                        button: Some(button),
                        state: self.pointer_state.clone(),
                    }))
            }
            ButtonState::Released => {
                self.pointer_state.buttons.remove(button);
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

    for event in cursor_moved.read() {
        runtime.handle_cursor_moved(event.window, event.position.x, event.position.y);
    }

    for event in cursor_left.read() {
        runtime.handle_cursor_left(event.window);
    }

    for event in mouse_button_input.read() {
        let suppressed = match event.state {
            ButtonState::Pressed => {
                overlay_routing.take_suppressed_press(event.window, event.button)
            }
            ButtonState::Released => {
                overlay_routing.take_suppressed_release(event.window, event.button)
            }
        };

        if suppressed {
            continue;
        }

        runtime.handle_mouse_button(event.window, event.button, event.state);
    }

    for event in mouse_wheel.read() {
        runtime.handle_mouse_wheel(event.window, event.unit, event.x, event.y);
    }

    for event in window_resized.read() {
        runtime.handle_window_resized(event.window, event.width, event.height);
    }

    for event in window_scale_factor_changed.read() {
        runtime.handle_window_scale_factor_changed(event.window, event.scale_factor);
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
