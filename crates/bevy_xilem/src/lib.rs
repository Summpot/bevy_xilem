#![forbid(unsafe_code)]

use std::{any::Any, fmt, marker::PhantomData, sync::Arc};

use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::{hierarchy::Children, prelude::*};
use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use xilem::{
    EventLoop, WidgetView, WindowOptions, Xilem,
    core::{Edit, map_state},
    winit::error::EventLoopError,
};
use xilem_masonry::{
    AnyWidgetView, view::FlexExt as _, view::flex_col, view::flex_row, view::label,
    view::text_button,
};

/// Xilem state used by synthesized UI views.
pub type UiXilemState = ();
/// Xilem action type used by synthesized UI views.
pub type UiXilemAction = ();

/// Type-erased Xilem widget view used as projection output.
pub type UiAnyView = AnyWidgetView<UiXilemState, UiXilemAction>;
/// Shared Xilem view handle.
pub type UiView = Arc<UiAnyView>;

/// Marker component for UI tree roots.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct UiRoot;

/// Stable node identity used by higher-level diff/caching strategies.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiNodeId(pub u64);

/// Example container component.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiFlexColumn;

/// Example horizontal container component.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiFlexRow;

/// Example text component.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiLabel {
    pub text: String,
}

impl UiLabel {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Example button component.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiButton {
    pub label: String,
}

impl UiButton {
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

/// Built-in semantic UI events emitted by framework-level widgets/projectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    Clicked(Entity),
    Custom(String),
}

/// Type-erased action envelope used by the framework event channel.
///
/// - [`XilemAction::Ui`] carries framework-level [`UiEvent`] values.
/// - [`XilemAction::Action`] carries app-specific typed actions.
pub enum XilemAction {
    Ui(UiEvent),
    Action(Box<dyn Any + Send + Sync>),
}

impl fmt::Debug for XilemAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ui(event) => f.debug_tuple("Ui").field(event).finish(),
            Self::Action(_) => f.write_str("Action(<type-erased>)"),
        }
    }
}

impl XilemAction {
    #[must_use]
    pub fn ui(event: UiEvent) -> Self {
        Self::Ui(event)
    }

    #[must_use]
    pub fn action<T: Any + Send + Sync>(action: T) -> Self {
        Self::Action(Box::new(action))
    }

    #[must_use]
    pub fn into_ui_event(self) -> Option<UiEvent> {
        match self {
            Self::Ui(event) => Some(event),
            Self::Action(_) => None,
        }
    }

    #[must_use]
    pub fn into_action<T: Any + Send + Sync>(self) -> Option<T> {
        match self {
            Self::Action(action) => action.downcast::<T>().ok().map(|boxed| *boxed),
            Self::Ui(_) => None,
        }
    }
}

/// Sender handle that can be cloned into projector closures.
#[derive(Resource, Clone)]
pub struct UiEventSender(pub Sender<XilemAction>);

impl UiEventSender {
    #[must_use]
    pub fn new(sender: Sender<XilemAction>) -> Self {
        Self(sender)
    }
}

/// Receiver handle for app-level systems to drain framework actions.
#[derive(Resource)]
pub struct UiEventReceiver(pub Receiver<XilemAction>);

impl UiEventReceiver {
    #[must_use]
    pub fn new(receiver: Receiver<XilemAction>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&self) -> Result<XilemAction, TryRecvError> {
        self.0.try_recv()
    }

    #[must_use]
    pub fn drain_all(&self) -> Vec<XilemAction> {
        self.0.try_iter().collect()
    }

    #[must_use]
    pub fn drain_ui_events(&self) -> Vec<UiEvent> {
        self.0
            .try_iter()
            .filter_map(XilemAction::into_ui_event)
            .collect()
    }

    #[must_use]
    pub fn drain_actions<T: Any + Send + Sync>(&self) -> Vec<T> {
        self.0
            .try_iter()
            .filter_map(|action| action.into_action::<T>())
            .collect()
    }
}

/// Projection context passed to projector implementations.
pub struct ProjectionCtx<'a> {
    pub world: &'a World,
    pub entity: Entity,
    pub node_id: Option<UiNodeId>,
    pub children: Vec<UiView>,
    /// Sender clone intended for projector-owned callbacks/closures.
    pub event_sender: Sender<XilemAction>,
}

/// Maps ECS entity data into a concrete Xilem view.
pub trait UiProjector: Send + Sync + 'static {
    fn project(&self, ctx: ProjectionCtx<'_>) -> Option<UiView>;
}

struct ComponentProjector<C: Component> {
    projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    _marker: PhantomData<C>,
}

impl<C: Component> UiProjector for ComponentProjector<C> {
    fn project(&self, ctx: ProjectionCtx<'_>) -> Option<UiView> {
        let component = ctx.world.get::<C>(ctx.entity)?;
        Some((self.projector)(component, ctx))
    }
}

/// Registry of projector implementations.
#[derive(Resource, Default)]
pub struct UiProjectorRegistry {
    projectors: Vec<Box<dyn UiProjector>>,
}

impl UiProjectorRegistry {
    /// Registers a raw projector implementation.
    pub fn register_projector<P: UiProjector>(&mut self, projector: P) -> &mut Self {
        self.projectors.push(Box::new(projector));
        self
    }

    /// Registers a projector bound to a specific ECS component type.
    pub fn register_component<C: Component>(
        &mut self,
        projector: fn(&C, ProjectionCtx<'_>) -> UiView,
    ) -> &mut Self {
        self.register_projector(ComponentProjector::<C> {
            projector,
            _marker: PhantomData,
        })
    }

    fn project_node(
        &self,
        world: &World,
        entity: Entity,
        node_id: Option<UiNodeId>,
        children: Vec<UiView>,
        event_sender: Sender<XilemAction>,
    ) -> Option<UiView> {
        // Last registered projector wins, so users can override built-ins.
        for projector in self.projectors.iter().rev() {
            let ctx = ProjectionCtx {
                world,
                entity,
                node_id,
                children: children.clone(),
                event_sender: event_sender.clone(),
            };
            if let Some(view) = projector.project(ctx) {
                return Some(view);
            }
        }

        None
    }
}

/// Snapshot resource containing synthesized root Xilem views for the current frame.
#[derive(Resource, Default)]
pub struct SynthesizedUiViews {
    pub roots: Vec<UiView>,
}

/// Snapshot metrics for the latest synthesis pass.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct UiSynthesisStats {
    pub root_count: usize,
    pub node_count: usize,
    pub cycle_count: usize,
    pub missing_entity_count: usize,
    pub unhandled_count: usize,
}

/// Reusable Bevy-to-Xilem runtime bridge.
///
/// This keeps Bevy app ticking and provides access to synthesized root views,
/// allowing examples (or apps) to use real Xilem with minimal boilerplate.
pub struct BevyXilemRuntime {
    bevy_app: App,
}

impl BevyXilemRuntime {
    #[must_use]
    pub fn new(bevy_app: App) -> Self {
        Self { bevy_app }
    }

    #[must_use]
    pub fn app(&self) -> &App {
        &self.bevy_app
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.bevy_app
    }

    pub fn update(&mut self) {
        self.bevy_app.update();
    }

    #[must_use]
    pub fn first_root(&self) -> Option<UiView> {
        self.bevy_app
            .world()
            .get_resource::<SynthesizedUiViews>()
            .and_then(|views| views.roots.first().cloned())
    }

    #[must_use]
    pub fn first_root_or_label(&self, fallback_text: impl Into<String>) -> UiView {
        self.first_root()
            .unwrap_or_else(|| Arc::new(label(fallback_text.into())))
    }

    #[must_use]
    pub fn update_and_first_root_or_label(&mut self, fallback_text: impl Into<String>) -> UiView {
        self.update();
        self.first_root_or_label(fallback_text)
    }
}

/// Collect all entities marked with `UiRoot`.
pub fn gather_ui_roots(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, With<UiRoot>>();
    query.iter(world).collect()
}

/// Synthesize Xilem views and stats for the provided roots.
pub fn synthesize_roots_with_stats(
    world: &World,
    registry: &UiProjectorRegistry,
    roots: impl IntoIterator<Item = Entity>,
) -> (Vec<UiView>, UiSynthesisStats) {
    let roots = roots.into_iter().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(roots.len());
    let mut stats = UiSynthesisStats {
        root_count: roots.len(),
        ..UiSynthesisStats::default()
    };
    let mut visiting = Vec::new();

    let fallback_sender = world
        .get_resource::<UiEventSender>()
        .map(|sender| sender.0.clone())
        .unwrap_or_else(|| {
            let (sender, _receiver) = unbounded::<XilemAction>();
            sender
        });

    for root in roots {
        output.push(synthesize_entity(
            world,
            registry,
            root,
            &mut visiting,
            &fallback_sender,
            &mut stats,
        ));
    }

    (output, stats)
}

/// Synthesize Xilem views for the provided roots.
pub fn synthesize_roots(
    world: &World,
    registry: &UiProjectorRegistry,
    roots: impl IntoIterator<Item = Entity>,
) -> Vec<UiView> {
    synthesize_roots_with_stats(world, registry, roots).0
}

/// Synthesize Xilem views by auto-discovering all `UiRoot` entities.
pub fn synthesize_world(world: &mut World, registry: &UiProjectorRegistry) -> Vec<UiView> {
    let roots = gather_ui_roots(world);
    synthesize_roots(world, registry, roots)
}

fn synthesize_entity(
    world: &World,
    registry: &UiProjectorRegistry,
    entity: Entity,
    visiting: &mut Vec<Entity>,
    event_sender: &Sender<XilemAction>,
    stats: &mut UiSynthesisStats,
) -> UiView {
    if world.get_entity(entity).is_err() {
        stats.node_count += 1;
        stats.missing_entity_count += 1;
        return Arc::new(label(format!("[missing entity {entity:?}]")));
    }

    if visiting.contains(&entity) {
        stats.node_count += 1;
        stats.cycle_count += 1;
        return Arc::new(label(format!("[cycle at {entity:?}]")));
    }

    visiting.push(entity);

    let child_entities = world
        .get::<Children>(entity)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();

    let children = child_entities
        .into_iter()
        .map(|child| synthesize_entity(world, registry, child, visiting, event_sender, stats))
        .collect::<Vec<_>>();

    let node_id = world.get::<UiNodeId>(entity).copied();

    let projected = registry.project_node(
        world,
        entity,
        node_id,
        children.clone(),
        event_sender.clone(),
    );

    let view = if let Some(view) = projected {
        view
    } else {
        stats.unhandled_count += 1;
        let mut seq = Vec::with_capacity(children.len() + 1);
        seq.push(label(format!("[unhandled entity {entity:?}]")).into_any_flex());
        seq.extend(children.into_iter().map(|child| child.into_any_flex()));
        Arc::new(flex_col(seq))
    };

    stats.node_count += 1;

    let popped = visiting.pop();
    debug_assert_eq!(popped, Some(entity));

    view
}

/// Bevy system that synthesizes all roots and updates `SynthesizedUiViews` + `UiSynthesisStats`.
pub fn synthesize_ui(world: &mut World) {
    let roots = gather_ui_roots(world);
    let (synthesized, stats) = world.resource_scope(|world, registry: Mut<UiProjectorRegistry>| {
        synthesize_roots_with_stats(world, &registry, roots)
    });

    world.resource_mut::<SynthesizedUiViews>().roots = synthesized;
    *world.resource_mut::<UiSynthesisStats>() = stats;
}

fn project_flex_column(_: &UiFlexColumn, ctx: ProjectionCtx<'_>) -> UiView {
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_col(children))
}

fn project_flex_row(_: &UiFlexRow, ctx: ProjectionCtx<'_>) -> UiView {
    let children = ctx
        .children
        .into_iter()
        .map(|child| child.into_any_flex())
        .collect::<Vec<_>>();

    Arc::new(flex_row(children))
}

fn project_label(label_component: &UiLabel, _ctx: ProjectionCtx<'_>) -> UiView {
    Arc::new(label(label_component.text.clone()))
}

fn project_button(button_component: &UiButton, ctx: ProjectionCtx<'_>) -> UiView {
    let sender = ctx.event_sender;
    let entity = ctx.entity;
    let button_label = button_component.label.clone();

    Arc::new(text_button(button_label, move |_| {
        let _ = sender.send(XilemAction::ui(UiEvent::Clicked(entity)));
    }))
}

/// Register built-in projectors for the built-in demo components.
pub fn register_builtin_projectors(registry: &mut UiProjectorRegistry) {
    registry
        .register_component::<UiFlexColumn>(project_flex_column)
        .register_component::<UiFlexRow>(project_flex_row)
        .register_component::<UiLabel>(project_label)
        .register_component::<UiButton>(project_button);
}

/// Plugin that wires event channel creation and synthesis into Bevy schedules.
#[derive(Default)]
pub struct BevyXilemPlugin;

impl Plugin for BevyXilemPlugin {
    fn build(&self, app: &mut App) {
        let (event_sender, event_receiver) = unbounded::<XilemAction>();

        app.init_resource::<UiProjectorRegistry>()
            .init_resource::<SynthesizedUiViews>()
            .init_resource::<UiSynthesisStats>()
            .insert_resource(UiEventSender::new(event_sender))
            .insert_resource(UiEventReceiver::new(event_receiver))
            .add_systems(PostUpdate, synthesize_ui);

        let mut registry = app.world_mut().resource_mut::<UiProjectorRegistry>();
        register_builtin_projectors(&mut registry);
    }
}

fn synthesized_root_app_logic(
    runtime: &mut BevyXilemRuntime,
) -> impl WidgetView<Edit<BevyXilemRuntime>> + use<> {
    runtime.update();

    let root_view = runtime.first_root_or_label("No synthesized bevy_xilem root");

    map_state(root_view, |_runtime: &mut BevyXilemRuntime, _| ())
}

/// Run a Bevy app inside a real Xilem window.
///
/// This helper encapsulates runtime and runner boilerplate:
/// - wraps the app in `BevyXilemRuntime`
/// - ticks Bevy each frame
/// - reads the synthesized root view
/// - launches Xilem's event loop
pub fn run_app(
    bevy_app: App,
    window_options: WindowOptions<BevyXilemRuntime>,
) -> Result<(), EventLoopError> {
    let runtime = BevyXilemRuntime::new(bevy_app);

    let app = Xilem::new_simple(runtime, synthesized_root_app_logic, window_options);

    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

pub mod prelude {
    pub use bevy_ecs::hierarchy::{ChildOf, Children};

    pub use crate::{
        BevyXilemPlugin, BevyXilemRuntime, ProjectionCtx, SynthesizedUiViews, UiAnyView, UiButton,
        UiEvent, UiEventReceiver, UiEventSender, UiFlexColumn, UiFlexRow, UiLabel, UiNodeId,
        UiProjector, UiProjectorRegistry, UiRoot, UiSynthesisStats, UiView, XilemAction,
        gather_ui_roots, register_builtin_projectors, run_app, synthesize_roots,
        synthesize_roots_with_stats, synthesize_ui, synthesize_world,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::hierarchy::ChildOf;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestAction(i32);

    #[test]
    fn synthesize_builtin_tree_stats() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        let root = world.spawn((UiRoot, UiNodeId(1), UiFlexColumn)).id();
        world.spawn((UiNodeId(2), UiLabel::new("hello"), ChildOf(root)));

        let (roots, stats) = synthesize_roots_with_stats(&world, &registry, [root]);

        assert_eq!(roots.len(), 1);
        assert_eq!(
            stats,
            UiSynthesisStats {
                root_count: 1,
                node_count: 2,
                cycle_count: 0,
                missing_entity_count: 0,
                unhandled_count: 0,
            }
        );
    }

    #[test]
    fn latest_projector_overrides_previous() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        fn override_label(_: &UiLabel, _: ProjectionCtx<'_>) -> UiView {
            Arc::new(text_button("override", |_| ()))
        }

        registry.register_component::<UiLabel>(override_label);

        let label_entity = world
            .spawn((UiRoot, UiNodeId(7), UiLabel::new("name")))
            .id();
        let (roots, stats) = synthesize_roots_with_stats(&world, &registry, [label_entity]);

        assert_eq!(roots.len(), 1);
        assert!(!roots[0].as_any().is::<xilem_masonry::view::Label>());
        assert_eq!(stats.unhandled_count, 0);
    }

    #[test]
    fn synthesize_detects_cycles() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        let root = world.spawn((UiRoot, UiNodeId(1), UiFlexColumn)).id();
        let child = world.spawn((UiNodeId(2), UiFlexColumn, ChildOf(root))).id();
        world.entity_mut(root).insert(ChildOf(child));

        let (_roots, stats) = synthesize_roots_with_stats(&world, &registry, [root]);

        assert_eq!(stats.root_count, 1);
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.cycle_count, 1);
        assert_eq!(stats.missing_entity_count, 0);
        assert_eq!(stats.unhandled_count, 0);
    }

    #[test]
    fn plugin_wires_event_channel_and_synthesis() {
        let mut app = App::new();
        app.add_plugins(BevyXilemPlugin);

        let root = app
            .world_mut()
            .spawn((UiRoot, UiNodeId(10), UiFlexColumn))
            .id();
        app.world_mut()
            .spawn((UiNodeId(11), UiLabel::new("ok"), ChildOf(root)));

        let event_sender = app.world().resource::<UiEventSender>().0.clone();
        event_sender
            .send(XilemAction::ui(UiEvent::Clicked(root)))
            .expect("event channel should accept XilemAction");

        app.update();

        let receiver = app.world().resource::<UiEventReceiver>();
        assert_eq!(receiver.drain_ui_events(), vec![UiEvent::Clicked(root)]);

        let views = app.world().resource::<SynthesizedUiViews>();
        assert_eq!(views.roots.len(), 1);

        let stats = app.world().resource::<UiSynthesisStats>();
        assert_eq!(
            *stats,
            UiSynthesisStats {
                root_count: 1,
                node_count: 2,
                cycle_count: 0,
                missing_entity_count: 0,
                unhandled_count: 0,
            }
        );
    }

    #[test]
    fn receiver_drains_typed_actions() {
        let mut app = App::new();
        app.add_plugins(BevyXilemPlugin);

        let event_sender = app.world().resource::<UiEventSender>().0.clone();
        event_sender
            .send(XilemAction::action(TestAction(7)))
            .expect("event channel should accept typed action");
        event_sender
            .send(XilemAction::ui(UiEvent::Custom("ignore".into())))
            .expect("event channel should accept ui action");

        let receiver = app.world().resource::<UiEventReceiver>();
        let actions = receiver.drain_actions::<TestAction>();
        assert_eq!(actions, vec![TestAction(7)]);
    }

    #[test]
    fn synthesis_tracks_missing_entities_in_stats() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        let stale_root = world.spawn_empty().id();
        assert!(world.despawn(stale_root));

        let (roots, stats) = synthesize_roots_with_stats(&world, &registry, [stale_root]);

        assert_eq!(roots.len(), 1);
        assert_eq!(
            stats,
            UiSynthesisStats {
                root_count: 1,
                node_count: 1,
                cycle_count: 0,
                missing_entity_count: 1,
                unhandled_count: 0,
            }
        );
    }

    #[test]
    fn synthesize_builtin_row_tree_stats() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        let root = world.spawn((UiRoot, UiNodeId(1), UiFlexRow)).id();
        world.spawn((UiNodeId(2), UiLabel::new("left"), ChildOf(root)));
        world.spawn((UiNodeId(3), UiLabel::new("right"), ChildOf(root)));

        let (roots, stats) = synthesize_roots_with_stats(&world, &registry, [root]);

        assert_eq!(roots.len(), 1);
        assert_eq!(
            stats,
            UiSynthesisStats {
                root_count: 1,
                node_count: 3,
                cycle_count: 0,
                missing_entity_count: 0,
                unhandled_count: 0,
            }
        );
    }

    #[test]
    fn runtime_bridge_updates_and_exposes_root() {
        let mut app = App::new();
        app.add_plugins(BevyXilemPlugin);

        let root = app
            .world_mut()
            .spawn((UiRoot, UiNodeId(1), UiFlexColumn))
            .id();
        app.world_mut()
            .spawn((UiNodeId(2), UiLabel::new("runtime"), ChildOf(root)));

        let mut runtime = BevyXilemRuntime::new(app);
        let root_view = runtime.update_and_first_root_or_label("fallback");

        assert!(!root_view.as_any().is::<xilem_masonry::view::Label>());
        assert!(runtime.first_root().is_some());
    }
}
