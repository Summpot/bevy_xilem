# bevy_xilem Design Document

Date: 2026-02-16

This document describes the **current implementation** after the architecture pivot to
**headless Masonry + Bevy-driven scheduling/input**.

## Purpose

`bevy_xilem` integrates Bevy ECS state management with a retained Masonry UI tree, while using
Xilem Core diff/rebuild semantics for view reconciliation.

The framework now avoids the high-level `xilem::Xilem::new_simple` runner completely.

## Core Architectural Decisions

### 1) Event loop ownership is Bevy-first

- Bevy owns scheduling and window/input message flow.
- Masonry is driven as a retained UI runtime resource from Bevy systems.
- No Winit event loop is started by `bevy_xilem` itself.

### 2) Headless retained runtime resource

`MasonryRuntime` is a Bevy `Resource` that owns:

- Masonry `RenderRoot` (retained widget tree)
- current synthesized root view
- Xilem `ViewCtx` and `ViewState`
- pointer state required for manual event injection

`PostUpdate` applies synthesized root diffs directly with Xilem Core `View::rebuild`.

### 3) Input injection bridge (PreUpdate)

`PreUpdate` system consumes Bevy messages:

- `CursorMoved`
- `CursorLeft`
- `MouseButtonInput`
- `MouseWheel`
- `WindowResized`

and translates them to Masonry events:

- `PointerEvent::{Move,Leave,Down,Up,Scroll}`
- `WindowEvent::Resize`

which are injected into `MasonryRuntime.render_root`.

### 4) Zero-closure ECS button path

To remove user-facing closure boilerplate:

- `EcsButtonWidget` implements `masonry::core::Widget`.
- `EcsButtonView` implements `xilem_core::View`.
- `ecs_button(entity, action, label)` builds this view directly.
- On click, widget pushes `UiEvent { entity, action }` into global queue-backed resource.

This enables projector code like:

`Arc::new(ecs_button(ctx.entity, TodoAction::Submit, "Add"))`

with no per-button channel sender/closure wiring by end users.

### 5) Typed action queue

`UiEventQueue` is a Bevy `Resource` backed by `crossbeam_queue::SegQueue<UiEvent>`.

- Widgets push type-erased actions (`Box<dyn Any + Send + Sync>`).
- Bevy systems drain typed actions via `drain_actions::<T>()`.

## ECS data model

Built-in components:

- `UiRoot`
- `UiNodeId(u64)`
- `UiFlexColumn`
- `UiFlexRow`
- `UiLabel { text }`
- `UiButton { label }`

## Projection and synthesis

- `UiProjectorRegistry` holds ordered projector implementations.
- Projector precedence: **last registered wins**.
- `PostUpdate` synthesis pipeline:
  1. gather `UiRoot`
  2. recursive child-first projection
  3. fallback views for cycle/missing/unhandled nodes
  4. store `SynthesizedUiViews`
  5. rebuild retained Masonry root in `MasonryRuntime`

## Plugin wiring

`BevyXilemPlugin` initializes:

- `UiProjectorRegistry`
- `SynthesizedUiViews`
- `UiSynthesisStats`
- `UiEventQueue`
- `MasonryRuntime`

and registers systems:

- `PreUpdate`: `inject_bevy_input_into_masonry`
- `PostUpdate`: `synthesize_ui -> rebuild_masonry_runtime` (chained)

It also registers built-in projectors.

## Built-in button behavior

Built-in `UiButton` projector maps to `ecs_button(...)` with action `BuiltinUiAction::Clicked`.

## Examples

Examples were rewritten to demonstrate this architecture with:

- Bevy-driven updates
- typed action drains from `UiEventQueue`
- simulated Bevy input messages feeding Masonry through the PreUpdate bridge
- no `xilem::Xilem::new_simple` usage

## Non-goals in current repository state

- No direct window creation/event-loop management by `bevy_xilem`
- No custom render-graph integration beyond Masonry retained runtime ownership
