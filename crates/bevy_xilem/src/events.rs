use std::{
    any::Any,
    fmt,
    sync::{Arc, OnceLock, PoisonError, RwLock},
};

use bevy_ecs::{entity::Entity, prelude::Resource};
use crossbeam_queue::SegQueue;

/// Type-erased UI action emitted by Masonry widgets.
pub struct UiEvent {
    pub entity: Entity,
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
    #[must_use]
    pub fn new(entity: Entity, action: Box<dyn Any + Send + Sync>) -> Self {
        Self { entity, action }
    }

    #[must_use]
    pub fn typed<T: Any + Send + Sync>(entity: Entity, action: T) -> Self {
        Self {
            entity,
            action: Box::new(action),
        }
    }

    #[must_use]
    pub fn into_action<T: Any + Send + Sync>(self) -> Option<TypedUiEvent<T>> {
        match self.action.downcast::<T>() {
            Ok(action) => Some(TypedUiEvent {
                entity: self.entity,
                action: *action,
            }),
            Err(_) => None,
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
    #[must_use]
    pub fn shared_queue(&self) -> Arc<SegQueue<UiEvent>> {
        self.queue.clone()
    }

    pub fn push(&self, event: UiEvent) {
        self.queue.push(event);
    }

    pub fn push_typed<T: Any + Send + Sync>(&self, entity: Entity, action: T) {
        self.push(UiEvent::typed(entity, action));
    }

    #[must_use]
    pub fn drain_all(&self) -> Vec<UiEvent> {
        let mut drained = Vec::new();
        while let Some(event) = self.queue.pop() {
            drained.push(event);
        }
        drained
    }

    /// Drain queue entries and keep only typed actions.
    ///
    /// Note: entries with other action types are discarded.
    #[must_use]
    pub fn drain_actions<T: Any + Send + Sync>(&self) -> Vec<TypedUiEvent<T>> {
        let mut drained = Vec::new();
        while let Some(event) = self.queue.pop() {
            if let Some(event) = event.into_action::<T>() {
                drained.push(event);
            }
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
pub fn emit_ui_action<T: Any + Send + Sync>(entity: Entity, action: T) {
    push_global_ui_event(UiEvent::typed(entity, action));
}
