# bevy_xilem Design Document

Date: 2026-02-16

This document describes the **current implementation** after the architecture pivot to
**headless Masonry + Bevy-driven scheduling/input**.

> Note: “headless” here describes the internal retained runtime ownership model,
> not that end-user apps/examples must be non-GUI.

## Purpose

`bevy_xilem` integrates Bevy ECS state management with a retained Masonry UI tree, while using
Xilem Core diff/rebuild semantics for view reconciliation.

The framework now avoids the high-level `xilem::Xilem::new_simple` runner completely.

## Core Architectural Decisions

### 1) Event loop ownership is Bevy-first

- Bevy owns scheduling and window/input message flow.
- Masonry is driven as a retained UI runtime resource from Bevy systems.
- `bevy_xilem` also provides a windowed bridge runner for GUI examples/apps,
  while preserving Bevy-driven synthesis updates.

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

## Windowed example runner

`bevy_xilem` provides:

- `run_app(bevy_app, title)`
- `run_app_with_window_options(bevy_app, title, configure_window)`

This bridge runs a GUI window through `Xilem::new` (not `Xilem::new_simple`) and,
on each frame, advances the Bevy app, reads `SynthesizedUiViews`, and renders the
current synthesized root view.

This keeps examples as normal GUI programs while retaining the new Bevy-first
synthesis architecture.

## Built-in button behavior

Built-in `UiButton` projector maps to `ecs_button(...)` with action `BuiltinUiAction::Clicked`.

## Examples

Examples were rewritten to demonstrate this architecture with:

- GUI windows via the bridge runner
- Bevy-driven synthesis updates each frame
- typed action handling via `UiEventQueue` (ECS queue path only)
- no `xilem::Xilem::new_simple` usage

## Non-goals in current repository state

- No custom render-graph integration beyond Masonry retained runtime ownership
