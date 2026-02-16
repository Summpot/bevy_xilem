use std::sync::Arc;

use bevy_ecs::{hierarchy::Children, prelude::*};
use xilem_masonry::view::{FlexExt as _, flex_col, label};

use crate::{
    ecs::UiRoot,
    projection::{UiProjectorRegistry, UiView},
};

/// Snapshot containing synthesized root views for the current frame.
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

/// Collect all entities marked with [`UiRoot`].
pub fn gather_ui_roots(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, With<UiRoot>>();
    query.iter(world).collect()
}

/// Synthesize Xilem Masonry views and stats for provided roots.
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

    for root in roots {
        output.push(synthesize_entity(
            world,
            registry,
            root,
            &mut visiting,
            &mut stats,
        ));
    }

    (output, stats)
}

/// Synthesize Xilem Masonry views for provided roots.
pub fn synthesize_roots(
    world: &World,
    registry: &UiProjectorRegistry,
    roots: impl IntoIterator<Item = Entity>,
) -> Vec<UiView> {
    synthesize_roots_with_stats(world, registry, roots).0
}

/// Synthesize by auto-discovering all [`UiRoot`] entities.
pub fn synthesize_world(world: &mut World, registry: &UiProjectorRegistry) -> Vec<UiView> {
    let roots = gather_ui_roots(world);
    synthesize_roots(world, registry, roots)
}

fn synthesize_entity(
    world: &World,
    registry: &UiProjectorRegistry,
    entity: Entity,
    visiting: &mut Vec<Entity>,
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
        .map(|child| synthesize_entity(world, registry, child, visiting, stats))
        .collect::<Vec<_>>();

    let node_id = entity.to_bits();

    let projected = registry.project_node(world, entity, node_id, children.clone());

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

/// Bevy system that synthesizes all roots and updates [`SynthesizedUiViews`] + [`UiSynthesisStats`].
pub fn synthesize_ui(world: &mut World) {
    let roots = gather_ui_roots(world);
    let (synthesized, stats) = world.resource_scope(|world, registry: Mut<UiProjectorRegistry>| {
        synthesize_roots_with_stats(world, &registry, roots)
    });

    world.resource_mut::<SynthesizedUiViews>().roots = synthesized;
    *world.resource_mut::<UiSynthesisStats>() = stats;
}
