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

- `EcsButtonView` implements `xilem_core::View` on top of a custom `EcsButtonWidget`
  that wraps Masonry button behavior for ECS integration.
- `ecs_button(entity, action, label)` builds this view directly.
- On click, keyboard activate, or accessibility click, it emits typed ECS actions into `UiEventQueue`.
- It also emits structural interaction events (`PointerEntered`, `PointerLeft`,
  `PointerPressed`, `PointerReleased`) used to drive pseudo-class state.

This enables projector code like:

`Arc::new(ecs_button(ctx.entity, TodoAction::Submit, "Add"))`

with no per-button channel sender/closure wiring by end users.

### 4.5) Fluent projector registration on `App`

`bevy_xilem` exposes `AppBevyXilemExt` so users can register projectors directly on Bevy apps:

- `.register_projector::<MyComponent>(project_my_component)`
- `.register_raw_projector(my_projector_impl)`

This removes direct `UiProjectorRegistry` mutation from most app setup code.

### 5) Typed action queue

`UiEventQueue` is a Bevy `Resource` backed by `crossbeam_queue::SegQueue<UiEvent>`.

- Widgets push type-erased actions (`Box<dyn Any + Send + Sync>`).
- Bevy systems drain typed actions via `drain_actions::<T>()`.
- Typed draining is non-destructive: events with other payload types are preserved for
  later consumers.
- `emit_ui_action(entity, action)` provides a public adapter entry-point for callback-heavy
  Xilem controls while still routing through the same ECS queue path.

### 5.5) ECS styling engine (CSS-like cascade)

The runtime now supports a data-driven style pipeline with four phases:

- **Inline style components:**
  `LayoutStyle`, `ColorStyle`, `TextStyle`, `StyleTransition`
- **Selector-based stylesheet + cascading:**
  `StyleSheet { rules: Vec<StyleRule> }` with selector AST:
  `Selector::{Type, Class, PseudoClass, And}` and payload `StyleSetter`
- **Pseudo classes from structural interaction events:**
  `Hovered` / `Pressed` marker components synchronized from interaction events
- **Computed-style cache + incremental invalidation:**
  `StyleDirty` marks entities requiring recomputation; `ComputedStyle` stores
  cached resolved layout/text/color/transition for projector reads
- **Smooth transitions:**
  `TargetColorStyle` + `CurrentColorStyle` driven by
  `bevy_tweening::TweenAnim` tween instances targeting
  `CurrentColorStyle`
  (`EaseFunction::QuadraticInOut` by default for interaction transitions)

Style resolution helpers (`resolve_style`, `resolve_style_for_classes`) and application helpers
(`apply_widget_style`, `apply_label_style`, `apply_text_input_style`) are provided for projectors.
Projectors now primarily consume `ComputedStyle` (through `resolve_style`) rather than
re-running a full cascade per frame.

### 6) ECS control adapter coverage

`bevy_xilem` scanned `xilem_masonry::view::*` controls and currently provides ECS adapters
for controls that naturally produce user actions:

- `ecs_button` / `ecs_button_with_child` / `ecs_text_button`
- `ecs_checkbox`
- `ecs_slider`
- `ecs_switch`
- `ecs_text_input`

Non-interactive display/layout controls (`label`, `flex`, `grid`, `prose`, `progress_bar`,
`sized_box`, etc.) are reused directly since they do not require event adaptation.

## ECS data model

Built-in components:

- `UiRoot`
- `UiFlexColumn`
- `UiFlexRow`
- `UiLabel { text }`
- `UiButton { label }`

Node identity for projection context is derived from ECS entities (`entity.to_bits()`),
so user code no longer needs to allocate/store a dedicated node-id component.

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
- `StyleSheet`
- `MasonryRuntime`

and registers tweening support with:

- `TweeningPlugin` (from crates.io `bevy_tweening` crate)

and registers systems:

- `PreUpdate`: `inject_bevy_input_into_masonry -> sync_ui_interaction_markers`
- `Update`: `mark_style_dirty -> sync_style_targets -> animate_style_transitions`
- `PostUpdate`: `synthesize_ui -> rebuild_masonry_runtime` (chained)

Transition execution details:

- `mark_style_dirty` incrementally marks entities whose style dependencies changed
  (class/inline/pseudo/style resource changes).
- `sync_style_targets` recomputes style only for dirty entities, updates `ComputedStyle`,
  computes target interaction colors, and on target changes inserts/replaces
  a `TweenAnim` with a fresh tween targeting `CurrentColorStyle`.
- Tween advancement is performed by `TweeningPlugin`'s
  `AnimationSystem::AnimationUpdate` system set.
- `resolve_style` reads `ComputedStyle` + `CurrentColorStyle` so projectors render in-between values,
  producing smooth CSS-like transitions instead of color snapping.

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

## Public API export strategy

To minimize dependency friction, `bevy_xilem` re-exports commonly needed Bevy/Xilem crates and
provides a dual control-view naming scheme:

- ECS event-adapted controls are exported with ergonomic names (`button`, `checkbox`, `slider`,
  `switch`, `text_button`, `text_input`, ...).
- Original `xilem_masonry::view` controls are re-exported with `xilem_` prefixes
  (`xilem_button`, `xilem_checkbox`, ...).
- Legacy `ecs_*` exports remain available for compatibility.

## Examples

Examples were rewritten to demonstrate this architecture with:

- GUI windows via the bridge runner
- Bevy-driven synthesis updates each frame
- typed action handling via `UiEventQueue` (ECS queue path only)
- stylesheet-driven styling (class rules + cascade) instead of hardcoded projector styles
- pseudo-class interaction styling and transition-capable style resolution
- virtualized task scrolling in `todo_list` using `xilem_masonry::view::virtual_scroll`
- no `xilem::Xilem::new_simple` usage

## Non-goals in current repository state

- No custom render-graph integration beyond Masonry retained runtime ownership
