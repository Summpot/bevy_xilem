use std::{fmt, marker::PhantomData, time::Duration};

use bevy_app::{App, Plugin};
use bevy_ecs::{component::Mutable, entity::Entity, prelude::*};
use bevy_time::Time;

/// Easing function used by [`Tween`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EaseFunction {
    Linear,
    QuadraticInOut,
}

impl EaseFunction {
    #[must_use]
    pub fn sample(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Self::Linear => x,
            Self::QuadraticInOut => {
                if x < 0.5 {
                    2.0 * x * x
                } else {
                    1.0 - ((-2.0 * x + 2.0).powi(2) / 2.0)
                }
            }
        }
    }
}

impl Default for EaseFunction {
    fn default() -> Self {
        Self::Linear
    }
}

/// Interpolation lens for tweening a component.
pub trait Lens<T>: Send + Sync + 'static {
    fn lerp(&mut self, target: &mut T, ratio: f32);
}

trait DynLens<T>: Send + Sync {
    fn lerp_dyn(&mut self, target: &mut T, ratio: f32);
}

impl<T, L> DynLens<T> for L
where
    L: Lens<T>,
{
    fn lerp_dyn(&mut self, target: &mut T, ratio: f32) {
        self.lerp(target, ratio);
    }
}

/// Tween description for one component type.
pub struct Tween<T: Component> {
    pub ease: EaseFunction,
    pub duration: Duration,
    lens: Box<dyn DynLens<T>>,
}

impl<T: Component> Tween<T> {
    #[must_use]
    pub fn new<L>(ease: EaseFunction, duration: Duration, lens: L) -> Self
    where
        L: Lens<T>,
    {
        Self {
            ease,
            duration,
            lens: Box::new(lens),
        }
    }

    fn apply(&mut self, target: &mut T, ratio: f32) {
        self.lens.lerp_dyn(target, self.ease.sample(ratio));
    }
}

impl<T: Component> fmt::Debug for Tween<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tween")
            .field("ease", &self.ease)
            .field("duration", &self.duration)
            .finish_non_exhaustive()
    }
}

/// Runtime animator component that drives a [`Tween`] for a component type.
#[derive(Component)]
pub struct Animator<T: Component> {
    tween: Tween<T>,
    elapsed: Duration,
    _marker: PhantomData<T>,
}

impl<T: Component> Animator<T> {
    #[must_use]
    pub fn new(tween: Tween<T>) -> Self {
        Self {
            tween,
            elapsed: Duration::ZERO,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    #[must_use]
    pub fn duration(&self) -> Duration {
        self.tween.duration
    }

    fn tick(&mut self, target: &mut T, delta: Duration) -> bool {
        self.elapsed = self.elapsed.saturating_add(delta);

        let ratio = if self.tween.duration.is_zero() {
            1.0
        } else {
            (self.elapsed.as_secs_f32() / self.tween.duration.as_secs_f32()).clamp(0.0, 1.0)
        };

        self.tween.apply(target, ratio);
        ratio >= 1.0
    }
}

impl<T: Component> fmt::Debug for Animator<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Animator")
            .field("tween", &self.tween)
            .field("elapsed", &self.elapsed)
            .finish_non_exhaustive()
    }
}

/// Lightweight plugin marker for tweening support.
///
/// The crate keeps stepping explicit via [`step_animators`] so integrators can
/// place animation updates exactly where they need in schedule ordering.
#[derive(Default)]
pub struct TweeningPlugin;

impl Plugin for TweeningPlugin {
    fn build(&self, _app: &mut App) {}
}

/// Advance all [`Animator<T>`] components for one frame using Bevy `Time`.
///
/// Completed animators are removed automatically.
pub fn step_animators<T: Component<Mutability = Mutable>>(world: &mut World) {
    let delta = world.resource::<Time>().delta();

    let mut finished_entities = Vec::<Entity>::new();
    let mut query = world.query::<(Entity, &mut Animator<T>, &mut T)>();

    for (entity, mut animator, mut target) in query.iter_mut(world) {
        if animator.tick(&mut *target, delta) {
            finished_entities.push(entity);
        }
    }

    for entity in finished_entities {
        if world.get_entity(entity).is_ok() {
            world.entity_mut(entity).remove::<Animator<T>>();
        }
    }
}
