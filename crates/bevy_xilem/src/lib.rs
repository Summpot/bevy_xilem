#![forbid(unsafe_code)]

use std::{
    marker::PhantomData,
    sync::{Mutex, mpsc},
};

use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::{hierarchy::Children, prelude::*};

/// Marker component for UI tree roots.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct UiRoot;

/// Stable node identity used by higher-level diff/caching strategies.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiNodeId(pub u64);

/// Example container component.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiFlexColumn;

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

/// Synthesized view IR that can be adapted to a concrete UI backend.
///
/// TODO(xilem-integration): Remove this IR once Xilem is linked end-to-end.
/// At that point, `UiProjector::project` should return `Box<dyn AnyView>`
/// directly and synthesis should bypass `UiViewNode` entirely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiViewNode {
    FlexColumn {
        entity: Entity,
        id: Option<UiNodeId>,
        children: Vec<UiViewNode>,
    },
    Label {
        entity: Entity,
        id: Option<UiNodeId>,
        text: String,
    },
    Button {
        entity: Entity,
        id: Option<UiNodeId>,
        label: String,
    },
    Unhandled {
        entity: Entity,
        id: Option<UiNodeId>,
        children: Vec<UiViewNode>,
    },
    MissingEntity {
        entity: Entity,
    },
    Cycle {
        entity: Entity,
    },
}

/// Projection context passed to projector implementations.
pub struct ProjectionCtx<'a> {
    pub world: &'a World,
    pub entity: Entity,
    pub node_id: Option<UiNodeId>,
    pub children: Vec<UiViewNode>,
    /// Sender clone intended for projector-owned callbacks/closures.
    pub event_sender: mpsc::Sender<UiEvent>,
}

/// Maps ECS entity data into a synthesized IR node.
///
/// TODO(xilem-integration): Change return type to `Box<dyn AnyView>` and
/// remove `UiViewNode` as an intermediate representation.
pub trait UiProjector: Send + Sync + 'static {
    fn project(&self, ctx: ProjectionCtx<'_>) -> Option<UiViewNode>;
}

struct ComponentProjector<C: Component> {
    projector: fn(&C, ProjectionCtx<'_>) -> UiViewNode,
    _marker: PhantomData<C>,
}

impl<C: Component> UiProjector for ComponentProjector<C> {
    fn project(&self, ctx: ProjectionCtx<'_>) -> Option<UiViewNode> {
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
        projector: fn(&C, ProjectionCtx<'_>) -> UiViewNode,
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
        children: Vec<UiViewNode>,
        event_sender: mpsc::Sender<UiEvent>,
    ) -> UiViewNode {
        // Last registered projector wins, so users can override built-ins.
        for projector in self.projectors.iter().rev() {
            let ctx = ProjectionCtx {
                world,
                entity,
                node_id,
                children: children.clone(),
                event_sender: event_sender.clone(),
            };
            if let Some(node) = projector.project(ctx) {
                return node;
            }
        }

        UiViewNode::Unhandled {
            entity,
            id: node_id,
            children,
        }
    }
}

/// Snapshot resource containing synthesized root trees for the current frame.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SynthesizedUiTrees {
    pub roots: Vec<UiViewNode>,
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

impl UiSynthesisStats {
    #[must_use]
    pub fn from_roots(roots: &[UiViewNode]) -> Self {
        let mut stats = Self {
            root_count: roots.len(),
            ..Self::default()
        };

        for root in roots {
            Self::accumulate_node(root, &mut stats);
        }

        stats
    }

    fn accumulate_node(node: &UiViewNode, stats: &mut UiSynthesisStats) {
        stats.node_count += 1;

        match node {
            UiViewNode::FlexColumn { children, .. } | UiViewNode::Unhandled { children, .. } => {
                if matches!(node, UiViewNode::Unhandled { .. }) {
                    stats.unhandled_count += 1;
                }

                for child in children {
                    Self::accumulate_node(child, stats);
                }
            }
            UiViewNode::Label { .. } | UiViewNode::Button { .. } => {}
            UiViewNode::MissingEntity { .. } => {
                stats.missing_entity_count += 1;
            }
            UiViewNode::Cycle { .. } => {
                stats.cycle_count += 1;
            }
        }
    }
}

/// Semantic UI messages that business systems can consume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    Clicked(Entity),
    Custom(String),
}

/// Sender handle that can be cloned into projector closures.
#[derive(Resource, Clone)]
pub struct UiEventSender(pub mpsc::Sender<UiEvent>);

impl UiEventSender {
    #[must_use]
    pub fn new(sender: mpsc::Sender<UiEvent>) -> Self {
        Self(sender)
    }
}

/// Convenience inbox that drains the UI event receiver each frame.
#[derive(Resource)]
pub struct UiEventInbox {
    receiver: Mutex<mpsc::Receiver<UiEvent>>,
    pub events: Vec<UiEvent>,
}

impl UiEventInbox {
    #[must_use]
    pub fn new(receiver: mpsc::Receiver<UiEvent>) -> Self {
        Self {
            receiver: Mutex::new(receiver),
            events: Vec::new(),
        }
    }

    pub fn drain(&mut self) {
        self.events.clear();

        let Ok(receiver) = self.receiver.lock() else {
            return;
        };

        while let Ok(event) = receiver.try_recv() {
            self.events.push(event);
        }
    }
}

/// Collect all entities marked with `UiRoot`.
pub fn gather_ui_roots(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, With<UiRoot>>();
    query.iter(world).collect()
}

/// Synthesize IR trees for the provided roots.
pub fn synthesize_roots(
    world: &World,
    registry: &UiProjectorRegistry,
    roots: impl IntoIterator<Item = Entity>,
) -> Vec<UiViewNode> {
    let mut output = Vec::new();
    let mut visiting = Vec::new();
    let fallback_sender = world
        .get_resource::<UiEventSender>()
        .map(|sender| sender.0.clone())
        .unwrap_or_else(|| {
            let (sender, _receiver) = mpsc::channel();
            sender
        });

    for root in roots {
        output.push(synthesize_entity(
            world,
            registry,
            root,
            &mut visiting,
            &fallback_sender,
        ));
    }

    output
}

/// Synthesize IR trees by auto-discovering all `UiRoot` entities.
pub fn synthesize_world(world: &mut World, registry: &UiProjectorRegistry) -> Vec<UiViewNode> {
    let roots = gather_ui_roots(world);
    synthesize_roots(world, registry, roots)
}

fn synthesize_entity(
    world: &World,
    registry: &UiProjectorRegistry,
    entity: Entity,
    visiting: &mut Vec<Entity>,
    event_sender: &mpsc::Sender<UiEvent>,
) -> UiViewNode {
    if world.get_entity(entity).is_err() {
        return UiViewNode::MissingEntity { entity };
    }

    if visiting.contains(&entity) {
        return UiViewNode::Cycle { entity };
    }

    visiting.push(entity);

    let child_entities = world
        .get::<Children>(entity)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();

    let children = child_entities
        .into_iter()
        .map(|child| synthesize_entity(world, registry, child, visiting, event_sender))
        .collect();

    let node_id = world.get::<UiNodeId>(entity).copied();

    let projected = registry.project_node(world, entity, node_id, children, event_sender.clone());

    let popped = visiting.pop();
    debug_assert_eq!(popped, Some(entity));

    projected
}

fn collect_ui_events(mut inbox: ResMut<UiEventInbox>) {
    inbox.drain();
}

fn synthesize_ui_system(world: &mut World) {
    let roots = gather_ui_roots(world);
    let synthesized = world.resource_scope(|world, registry: Mut<UiProjectorRegistry>| {
        synthesize_roots(world, &registry, roots)
    });
    let stats = UiSynthesisStats::from_roots(&synthesized);
    world.resource_mut::<SynthesizedUiTrees>().roots = synthesized;
    *world.resource_mut::<UiSynthesisStats>() = stats;
}

fn project_flex_column(_: &UiFlexColumn, ctx: ProjectionCtx<'_>) -> UiViewNode {
    UiViewNode::FlexColumn {
        entity: ctx.entity,
        id: ctx.node_id,
        children: ctx.children,
    }
}

fn project_label(label: &UiLabel, ctx: ProjectionCtx<'_>) -> UiViewNode {
    UiViewNode::Label {
        entity: ctx.entity,
        id: ctx.node_id,
        text: label.text.clone(),
    }
}

fn project_button(button: &UiButton, ctx: ProjectionCtx<'_>) -> UiViewNode {
    UiViewNode::Button {
        entity: ctx.entity,
        id: ctx.node_id,
        label: button.label.clone(),
    }
}

/// Register built-in projectors for the built-in demo components.
pub fn register_builtin_projectors(registry: &mut UiProjectorRegistry) {
    registry
        .register_component::<UiFlexColumn>(project_flex_column)
        .register_component::<UiLabel>(project_label)
        .register_component::<UiButton>(project_button);
}

/// Plugin that wires synthesis and event collection into Bevy schedules.
#[derive(Default)]
pub struct BevyXilemPlugin;

impl Plugin for BevyXilemPlugin {
    fn build(&self, app: &mut App) {
        let (event_sender, event_receiver) = mpsc::channel();

        app.init_resource::<UiProjectorRegistry>()
            .init_resource::<SynthesizedUiTrees>()
            .init_resource::<UiSynthesisStats>()
            .insert_resource(UiEventSender::new(event_sender))
            .insert_resource(UiEventInbox::new(event_receiver))
            .add_systems(PreUpdate, collect_ui_events)
            .add_systems(PostUpdate, synthesize_ui_system);

        let mut registry = app.world_mut().resource_mut::<UiProjectorRegistry>();
        register_builtin_projectors(&mut registry);
    }
}

pub mod prelude {
    pub use bevy_ecs::hierarchy::{ChildOf, Children};

    pub use crate::{
        BevyXilemPlugin, SynthesizedUiTrees, UiButton, UiEvent, UiEventInbox, UiEventSender,
        UiFlexColumn, UiLabel, UiNodeId, UiProjector, UiProjectorRegistry, UiRoot,
        UiSynthesisStats, UiViewNode, gather_ui_roots, register_builtin_projectors,
        synthesize_roots, synthesize_world,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::hierarchy::ChildOf;

    #[test]
    fn synthesize_builtin_tree() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        let root = world.spawn((UiRoot, UiNodeId(1), UiFlexColumn)).id();
        let label = world
            .spawn((UiNodeId(2), UiLabel::new("hello"), ChildOf(root)))
            .id();

        let trees = synthesize_roots(&world, &registry, [root]);

        assert_eq!(
            trees,
            vec![UiViewNode::FlexColumn {
                entity: root,
                id: Some(UiNodeId(1)),
                children: vec![UiViewNode::Label {
                    entity: label,
                    id: Some(UiNodeId(2)),
                    text: "hello".to_string(),
                }],
            }]
        );
    }

    #[test]
    fn latest_projector_overrides_previous() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        fn override_label(label: &UiLabel, ctx: ProjectionCtx<'_>) -> UiViewNode {
            UiViewNode::Button {
                entity: ctx.entity,
                id: ctx.node_id,
                label: format!("override:{}", label.text),
            }
        }

        registry.register_component::<UiLabel>(override_label);

        let label = world
            .spawn((UiRoot, UiNodeId(7), UiLabel::new("name")))
            .id();
        let trees = synthesize_roots(&world, &registry, [label]);

        assert_eq!(
            trees,
            vec![UiViewNode::Button {
                entity: label,
                id: Some(UiNodeId(7)),
                label: "override:name".to_string(),
            }]
        );
    }

    #[test]
    fn synthesize_detects_cycles() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        let root = world.spawn((UiRoot, UiNodeId(1), UiFlexColumn)).id();
        let child = world.spawn((UiNodeId(2), UiFlexColumn, ChildOf(root))).id();
        world.entity_mut(root).insert(ChildOf(child));

        let trees = synthesize_roots(&world, &registry, [root]);

        assert_eq!(
            trees,
            vec![UiViewNode::FlexColumn {
                entity: root,
                id: Some(UiNodeId(1)),
                children: vec![UiViewNode::FlexColumn {
                    entity: child,
                    id: Some(UiNodeId(2)),
                    children: vec![UiViewNode::Cycle { entity: root }],
                }],
            }]
        );
    }

    #[test]
    fn plugin_wires_event_collection_and_synthesis() {
        let mut app = App::new();
        app.add_plugins(BevyXilemPlugin);

        let root = app
            .world_mut()
            .spawn((UiRoot, UiNodeId(10), UiFlexColumn))
            .id();
        let label = app
            .world_mut()
            .spawn((UiNodeId(11), UiLabel::new("ok"), ChildOf(root)))
            .id();

        let event_sender = app.world().resource::<UiEventSender>().0.clone();
        event_sender
            .send(UiEvent::Clicked(root))
            .expect("event channel should accept UiEvent");

        app.update();

        let inbox = app.world().resource::<UiEventInbox>();
        assert_eq!(inbox.events, vec![UiEvent::Clicked(root)]);

        let trees = app.world().resource::<SynthesizedUiTrees>();
        assert_eq!(
            trees.roots,
            vec![UiViewNode::FlexColumn {
                entity: root,
                id: Some(UiNodeId(10)),
                children: vec![UiViewNode::Label {
                    entity: label,
                    id: Some(UiNodeId(11)),
                    text: "ok".to_string(),
                }],
            }]
        );

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
    fn synthesis_tracks_missing_entities_in_stats() {
        let mut world = World::new();
        let mut registry = UiProjectorRegistry::default();
        register_builtin_projectors(&mut registry);

        let stale_root = world.spawn_empty().id();
        assert!(world.despawn(stale_root));

        let trees = synthesize_roots(&world, &registry, [stale_root]);
        let stats = UiSynthesisStats::from_roots(&trees);

        assert_eq!(
            trees,
            vec![UiViewNode::MissingEntity { entity: stale_root }]
        );

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
}
