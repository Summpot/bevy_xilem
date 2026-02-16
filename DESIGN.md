# bevy_xilem Design Document

Date: 2026-02-16

This document describes the **actual repository status** and implementation details.

## Purpose

`bevy_xilem` integrates Bevy ECS state management with a UI projection layer designed for Xilem-oriented rendering pipelines.

The repository currently implements:

- ECS-based UI state components
- A projector registry for component-to-view mapping
- Recursive synthesis from ECS entities directly to type-erased Xilem views
- Bevy plugin wiring for event collection and synthesis execution
- Synthesis runtime metrics

## Workspace and Dependency Policy

The project uses a **virtual workspace**.

- Root `Cargo.toml` contains `[workspace]`, `[workspace.package]`, and `[workspace.dependencies]`.
- Code lives in workspace members (currently `crates/bevy_xilem`).
- Member crates use workspace dependencies (`workspace = true`).

## Implemented Data Model (ECS)

UI entities are modeled with explicit components:

- `UiRoot`: marks a root UI entity.
- `UiNodeId(u64)`: stable node identifier.
- Native Bevy hierarchy components from `bevy_ecs::hierarchy`:
  - `Children`: parent-owned child list used during synthesis traversal.
  - `ChildOf` (the parent-link relationship component; equivalent role to a `Parent` link).
- Built-in view components:
  - `UiFlexColumn`
  - `UiLabel { text: String }`
  - `UiButton { label: String }`

## Implemented View Projection (Real Xilem)

Synthesis produces real Xilem views directly:

- `UiProjector` returns `UiView` (`Arc<AnyWidgetView<(), ()>>`)
- `SynthesizedUiViews` stores `Vec<UiView>` each update cycle
- Built-in component projectors map to Xilem view constructors:
  - `UiFlexColumn` -> `xilem_masonry::view::flex_col`
  - `UiLabel` -> `xilem_masonry::view::label`
  - `UiButton` -> `xilem_masonry::view::text_button`

Fallback handling for unhandled / missing / cycle cases is also represented as concrete Xilem views (labels/flex containers), not enum IR nodes.

## Projector Registry

`UiProjectorRegistry` stores `UiProjector` implementations.

- Dynamic registration is supported.
- Component-specific registration uses `register_component::<C>(...)`.
- Precedence rule: **last registered projector wins**.

Built-in projectors are registered for `UiFlexColumn`, `UiLabel`, and `UiButton`.

## Synthesis Execution

Synthesis runs in `PostUpdate` through `synthesize_ui_system`.

Behavior:

1. Gather all `UiRoot` entities.
2. Recursively synthesize child nodes first.
3. Apply projector dispatch for the current entity.
4. Emit fallback Xilem views for unhandled/missing/cycle cases.
5. Write resulting root views to `SynthesizedUiViews`.

## Event Collection

`UiEvent` intake uses an MPSC channel.

- `UiEventSender(Sender<UiEvent>)` is stored as a resource and is cloneable.
- Projector contexts receive a sender clone so projector-owned closures can emit `UiEvent` values without `World` access.
- `UiEventInbox` owns the receiver end and drains it each `PreUpdate`.

## Runtime Metrics

`UiSynthesisStats` is updated during synthesis traversal and includes:

- `root_count`
- `node_count`
- `cycle_count`
- `missing_entity_count`
- `unhandled_count`

This makes synthesis behavior observable without external instrumentation.

## Plugin Wiring

`BevyXilemPlugin` currently performs the following:

- Initializes resources:
  - `UiProjectorRegistry`
  - `SynthesizedUiViews`
  - `UiSynthesisStats`
  - `UiEventSender`
  - `UiEventInbox`
- Registers systems:
  - `PreUpdate`: `collect_ui_events`
  - `PostUpdate`: `synthesize_ui_system`
- Registers built-in projectors.

## Verified Behavior

The crate contains tests that verify:

- Successful synthesis for built-in components
- Projector override behavior (last registration takes precedence)
- Cycle detection behavior
- Plugin integration (events + synthesis + metrics)
- Missing-entity handling and corresponding metrics

## Current Gaps (Not Implemented in This Repository)

The following are not implemented in the current codebase:

- Plugin-level Masonry runtime driving and widget diff application (windowed runtime wiring currently lives in examples)
- Render backend integration (e.g., Vello/RenderGraph path)
- Input routing from window/input backends into UI actions

This document intentionally separates implemented behavior from unimplemented behavior.
