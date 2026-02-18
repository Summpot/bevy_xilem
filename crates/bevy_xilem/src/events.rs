use std::{
    any::Any,
    fmt,
    sync::{Arc, OnceLock, PoisonError, RwLock},
};

use bevy_ecs::{entity::Entity, prelude::Component, prelude::Resource};
use bevy_input::mouse::MouseButton;
use crossbeam_queue::SegQueue;

/// Pointer phase used by high-level UI pointer events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiPointerPhase {
    Pressed,
    Released,
}

/// Hit-tested UI pointer event before ECS bubbling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiPointerHitEvent {
    pub target: Entity,
    pub position: (f64, f64),
    pub button: MouseButton,
    pub phase: UiPointerPhase,
}

/// Bubbling UI pointer event emitted for each ancestor in the hierarchy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiPointerEvent {
    pub target: Entity,
    pub current_target: Entity,
    pub position: (f64, f64),
    pub button: MouseButton,
    pub phase: UiPointerPhase,
    pub consumed: bool,
}

/// Marker that stops bubbling at the tagged entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StopUiPointerPropagation;

/// Type-erased UI action emitted by Masonry widgets.
pub struct UiEvent {
    /// Source ECS entity for this action.
    pub entity: Entity,
    /// Type-erased action payload.
    pub action: Box<dyn Any + Send + Sync>,
}

impl fmt::Debug for UiEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiEvent")
            .field("entity", &self.entity)
            .field("action", &"<type-erased>")
            .finish()
    }
}

impl UiEvent {
    /// Create a new type-erased UI event.
    #[must_use]
    pub fn new(entity: Entity, action: Box<dyn Any + Send + Sync>) -> Self {
        Self { entity, action }
    }

    /// Create a typed UI event and erase it into [`UiEvent`].
    #[must_use]
    pub fn typed<T: Any + Send + Sync>(entity: Entity, action: T) -> Self {
        Self {
            entity,
            action: Box::new(action),
        }
    }

    /// Attempt to recover a typed event payload.
    #[must_use]
    pub fn into_action<T: Any + Send + Sync>(self) -> Option<TypedUiEvent<T>> {
        self.try_into_action::<T>().ok()
    }

    /// Attempt to recover a typed event payload, returning the original event on mismatch.
    #[must_use]
    pub fn try_into_action<T: Any + Send + Sync>(self) -> Result<TypedUiEvent<T>, Self> {
        match self.action.downcast::<T>() {
            Ok(action) => Ok(TypedUiEvent {
                entity: self.entity,
                action: *action,
            }),
            Err(action) => Err(Self {
                entity: self.entity,
                action,
            }),
        }
    }
}

/// Typed UI event produced from a type-erased [`UiEvent`] queue entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedUiEvent<T> {
    pub entity: Entity,
    pub action: T,
}

/// Lock-free queue shared between Bevy systems and Masonry widgets.
///
/// # Example
///
/// ```
/// use bevy_xilem::{UiEventQueue, bevy_ecs::world::World};
///
/// let mut world = World::new();
/// let entity = world.spawn_empty().id();
///
/// let mut queue = UiEventQueue::default();
/// queue.push_typed(entity, 7_u32);
///
/// let drained = queue.drain_actions::<u32>();
/// assert_eq!(drained.len(), 1);
/// assert_eq!(drained[0].entity, entity);
/// assert_eq!(drained[0].action, 7);
/// ```
#[derive(Resource, Clone, Debug)]
pub struct UiEventQueue {
    queue: Arc<SegQueue<UiEvent>>,
}

impl Default for UiEventQueue {
    fn default() -> Self {
        Self {
            queue: Arc::new(SegQueue::new()),
        }
    }
}

impl UiEventQueue {
    /// Get a shared queue handle for cross-runtime wiring.
    #[must_use]
    pub fn shared_queue(&self) -> Arc<SegQueue<UiEvent>> {
        self.queue.clone()
    }

    /// Push a pre-built type-erased event.
    pub fn push(&self, event: UiEvent) {
        self.queue.push(event);
    }

    /// Push a typed action payload for an entity.
    pub fn push_typed<T: Any + Send + Sync>(&self, entity: Entity, action: T) {
        self.push(UiEvent::typed(entity, action));
    }

    /// Drain every queued event, regardless of payload type.
    #[must_use]
    pub fn drain_all(&mut self) -> Vec<UiEvent> {
        let mut drained = Vec::new();
        while let Some(event) = self.queue.pop() {
            drained.push(event);
        }
        drained
    }

    /// Drain queue entries and keep only typed actions.
    ///
    /// Entries with other action types are preserved in the queue.
    #[must_use]
    pub fn drain_actions<T: Any + Send + Sync>(&mut self) -> Vec<TypedUiEvent<T>> {
        let mut drained = Vec::new();
        let mut unmatched = Vec::new();
        while let Some(event) = self.queue.pop() {
            match event.try_into_action::<T>() {
                Ok(typed) => drained.push(typed),
                Err(event) => unmatched.push(event),
            }
        }

        for event in unmatched {
            self.queue.push(event);
        }

        drained
    }
}

static GLOBAL_UI_EVENT_QUEUE: OnceLock<RwLock<Option<Arc<SegQueue<UiEvent>>>>> = OnceLock::new();

fn global_ui_event_queue_slot() -> &'static RwLock<Option<Arc<SegQueue<UiEvent>>>> {
    GLOBAL_UI_EVENT_QUEUE.get_or_init(|| RwLock::new(None))
}

pub(crate) fn install_global_ui_event_queue(queue: Arc<SegQueue<UiEvent>>) {
    let mut slot = global_ui_event_queue_slot()
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    *slot = Some(queue);
}

pub(crate) fn push_global_ui_event(event: UiEvent) {
    let queue = {
        let slot = global_ui_event_queue_slot()
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        slot.as_ref().cloned()
    };

    if let Some(queue) = queue {
        queue.push(event);
    }
}

/// Emit a typed UI action into the global ECS-backed UI queue.
///
/// This is intended for callback-based widget APIs in examples/apps that still
/// want to route all interactions through [`UiEventQueue`].
///
/// # Example
///
/// ```
/// use bevy_xilem::{emit_ui_action, UiEventQueue, bevy_ecs::world::World};
///
/// let mut world = World::new();
/// let entity = world.spawn_empty().id();
/// let mut queue = UiEventQueue::default();
///
/// // In real app wiring this global queue is installed by `MasonryRuntime`.
/// // Here we directly push through queue APIs in tests/docs.
/// emit_ui_action(entity, "ignored without installed global queue".to_string());
/// queue.push_typed(entity, "clicked".to_string());
/// let actions = queue.drain_actions::<String>();
/// assert_eq!(actions[0].action, "clicked");
/// ```
pub fn emit_ui_action<T: Any + Send + Sync>(entity: Entity, action: T) {
    push_global_ui_event(UiEvent::typed(entity, action));
}
