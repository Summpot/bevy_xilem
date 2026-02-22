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
- `bevy_xilem` does **not** run a separate Xilem/Masonry event loop.
- GUI apps use Bevy's native `App::run()` and `bevy_winit` window lifecycle.

### 2) Headless retained runtime resource

`MasonryRuntime` is a Bevy `Resource` that owns:

- Masonry `RenderRoot` (retained widget tree)
- current synthesized root view
- Xilem `ViewCtx` and `ViewState`
- pointer state required for manual event injection
- active Bevy primary-window attachment metrics (logical size + scale factor)

`PostUpdate` applies synthesized root diffs directly with Xilem Core `View::rebuild`.
`PreUpdate` lazily binds runtime viewport/scale to the Bevy-created primary window once
`bevy_winit` exposes it.

Initialization invariant:

- `initialize_masonry_runtime_from_primary_window` injects an explicit initial logical resize
  immediately after first attach so Masonry never starts hit-testing from a `(0, 0)` root size.

### 3) Input injection bridge (PreUpdate)

`PreUpdate` system consumes Bevy messages:

- `CursorMoved`
- `CursorLeft`
- `MouseButtonInput`
- `MouseWheel`
- `WindowResized`
- `WindowScaleFactorChanged`

and translates them to Masonry events:

- `PointerEvent::{Move,Leave,Down,Up,Scroll}`
- `WindowEvent::{Resize,Rescale}`

which are injected into `MasonryRuntime.render_root`.

Pointer bridge invariants:

- `Window::physical_cursor_position()` from the current `PrimaryWindow` is the source of truth
  for injected Masonry pointer coordinates.
- When physical cursor data is unavailable, logical cursor coordinates can be converted using
  the current window scale factor for compatibility paths.
- `CursorMoved.position` payload is not trusted for hit-test coordinates.
- `MouseButtonInput` / `MouseWheel` are injected only when physical cursor data is available;
  when unavailable (cursor outside), pointer interaction injection is skipped.
- Window resize injection uses logical `Window::width()` / `Window::height()` from the active
  primary window, ensuring Masonry receives DPI-correct dimensions.
- Click-path ordering is enforced by injecting `PointerMove` before each
  `PointerDown` / `PointerUp` so hot/hovered state is current before activation.

### 3.5) Explicit Masonry/Vello paint pass (Last)

Because Bevy's renderer plugins are intentionally not required for the retained UI path,
`bevy_xilem` performs an explicit Vello paint/present pass in `Last`:

- `RenderRoot::redraw()` produces the current scene.
- `ExternalWindowSurface` (from `masonry_winit`) owns persistent surface/device state bound
  to the Bevy primary window.
- the pass renders to an intermediate texture, blits to the swapchain surface, and presents.
- the primary window requests another redraw to keep UI animations and visual updates flowing.

This avoids the "window opens but no pixels are drawn" failure mode when only
window/input plugins are active.

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
  `Selector::{Type, Class, PseudoClass, And, Descendant}` and payload `StyleSetter`
- **Pseudo classes from structural interaction events:**
  `Hovered` / `Pressed` marker components synchronized from interaction events
- **Computed-style cache + incremental invalidation:**
  `StyleDirty` marks entities requiring recomputation; `ComputedStyle` stores
  cached resolved layout/text/color/transition plus
  `font_family: Option<Vec<String>>` and `box_shadow: Option<BoxShadow>`
  for projector reads
- **Smooth transitions:**
  `TargetColorStyle` + `CurrentColorStyle` driven by
  `bevy_tweening::TweenAnim` tween instances targeting
  `CurrentColorStyle`
  (`EaseFunction::QuadraticInOut` by default for interaction transitions)

Style resolution helpers (`resolve_style`, `resolve_style_for_classes`) and application helpers
(`apply_widget_style`, `apply_label_style`, `apply_text_input_style`) are provided for projectors.
Projectors now primarily consume `ComputedStyle` (through `resolve_style`) rather than
re-running a full cascade per frame.

Asset-driven stylesheet details:

- `StyleSheet` is also a Bevy asset type loaded via `StyleSheetRonLoader` from `.ron` files.
- `ActiveStyleSheetAsset { path, handle }` tracks the active runtime stylesheet asset.
- `sync_stylesheet_asset_events` listens to `AssetEvent<StyleSheet>` and applies updated
  asset content into the global `StyleSheet` resource.
- Resource updates reuse the existing invalidation path (`mark_style_dirty`), so transition and
  cascade behavior remains unchanged while enabling hot-reload.
- Default runtime stylesheet path is `assets/themes/default_theme.ron`.

Label text wrapping policy:

- `apply_label_style` applies `LineBreaking::WordWrap` by default.
- This prevents overflow/tofu-like clipping in constrained containers (such as modal body text)
  while keeping font/color sizing controlled by resolved style.

Style surface details:

- `StyleSetter` and `ResolvedStyle` include optional `box_shadow` support.
- Widget application helpers apply resolved border/background/corner/padding and box-shadow
  on the target surface, allowing overlay/dialog/dropdown surfaces to express depth without
  coupling shadows to backdrop layers.

Hit-testing invariant:

- Layout-affecting style properties for controls (notably padding/border/background) are applied
  on the target control widget itself (instead of only through a purely visual outer wrapper).
- This ensures Masonry's layout and pointer hit-testing use the same structural box model as what
  users see on screen.
- Floating overlay surfaces (`UiDialog`, `UiDropdownMenu`) are wrapped in a dedicated
  pointer-opaque wrapper (`OpaqueHitboxWidget` via `opaque_hitbox_for_entity(...)`) at the
  projected panel root.
  - The wrapper is paint-transparent but pointer-solid across its full layout bounds, preventing
    hit-test fallthrough through internal flex/padding gaps.
  - The wrapper carries entity debug metadata (`opaque_hitbox_entity=<bits>`), which is resolved
    on demand by `MasonryRuntime` using ECS `entity.to_bits()` during click arbitration.

### 5.8) Overlay/Portal layer architecture

`bevy_xilem` now includes a built-in ECS overlay model for floating UI:

- `UiOverlayRoot` marker component defines a global portal root.
- `ensure_overlay_root` guarantees one overlay root exists when regular `UiRoot` exists.
- Overlay root is synthesized as an independent root and rendered on top through root stacking.

Centralized layering model:

- `OverlayStack { active_overlays: Vec<Entity> }` is the single z-order source of truth.
  - Order is bottom → top.
  - `active_overlays.last()` is always the top-most interactive overlay.
- `sync_overlay_stack_lifecycle` keeps the stack synchronized with live entities and prunes stale entries.
- Built-in overlay creation paths (`spawn_in_overlay_root`, combo dropdown open) register overlays into the stack.

Universal placement model:

- `OverlayPlacement` defines canonical positions used by all floating surfaces:
  `Center`, `Top`, `Bottom`, `Left`, `Right`, `TopStart`, `TopEnd`,
  `BottomStart`, `BottomEnd`, `LeftStart`, `RightStart`.
- `OverlayState { is_modal, anchor }` is attached to each active overlay.
  - `is_modal: true` for modal surfaces (dialogs).
  - `anchor: Some(entity)` for anchored overlays (dropdowns/tooltips).
- `OverlayConfig { placement, anchor, auto_flip }` remains the placement policy component.
- `OverlayComputedPosition { x, y, width, height, placement, is_positioned }` stores
  runtime-resolved placement after collision checks.
  - New overlays start with `is_positioned: false`.
  - Floating projectors (`UiDialog`, `UiDropdownMenu`) render with fully transparent styles
    while `is_positioned` is false, preserving layout size measurement but preventing the
    initial `(0, 0)` visual flash.
  - `sync_overlay_positions` sets `is_positioned: true` immediately after writing the
    final clamped coordinates.

Built-in floating widgets:

- `UiDialog` (modal panel; optional visual dimming is rendered by `UiOverlayRoot`)
- `UiComboBox` (anchor control)
- `UiDropdownMenu` (floating list in overlay layer)
- `AnchoredTo(Entity)` + `OverlayAnchorRect` for anchor tracking
- `OverlayState` for behavior and dismissal policy.

Entity↔Widget bridge model:

- Synthesis wraps each ECS node in an entity-scoped Masonry wrapper widget.
- `handle_global_overlay_clicks` passes raw `entity.to_bits()` ids to `MasonryRuntime`.
- `MasonryRuntime` resolves `entity bits -> WidgetId` by traversing the active retained tree
  on demand during click events.
- No ECS widget-id sync component and no per-frame widget-id synchronization system are used.

Overlay ownership and lifecycle policy:

- `spawn_in_overlay_root(world, bundle)` is the app-facing helper for portal entities.
- `reparent_overlay_entities` runs in `Update` and automatically moves built-in overlay
  entities (`UiDialog`, `UiDropdownMenu`) under `UiOverlayRoot`.
- This removes example/app-level `ensure_overlay_root_entity` plumbing for common modal/dropdown flows.

Modal panel + dimming policy:

- `UiDialog` projector returns only the interactive dialog panel surface (no structural
  full-screen backdrop sibling/wrapper).
- Click-outside dismissal is handled centrally by `handle_global_overlay_clicks` using retained
  Masonry hit paths (reverse hit-testing), not by dialog-local backdrop
  action widgets.
- Optional dimming is rendered independently by `UiOverlayRoot` (class
  `overlay.modal.dimmer`) when modal overlays exist.
- The dimmer is visual-only and is not inserted into `OverlayStack` / `OverlayState`.

Overlay placement policy:

- `sync_overlay_positions` runs in `PostUpdate` and computes final positions for all entities
  with `OverlayState`.
- The system reads dynamic logical width/height and scale factor from `PrimaryWindow`
  (falling back to the first window when absent in tests/headless cases)
  every frame and anchor widget rectangles gathered from Masonry widget geometry.
- Placement sync is ordered after Masonry retained-tree rebuild so anchor/widget geometry is
  up-to-date before collision and auto-flip resolution.
- Collision handling computes visible area and supports automatic flipping when preferred
  placement would overflow (notably bottom → top for near-bottom dropdowns).
- Final clamped coordinates are written to `OverlayComputedPosition`, and overlay projectors
  read these values when rendering transformed surfaces.

Overlay runtime flow:

- Built-in overlay actions (`OverlayUiAction`) are drained by `handle_overlay_actions`.
- Combo open/close spawns/despawns dropdown entities under `UiOverlayRoot`.
- `ensure_overlay_defaults` applies default placement policy for built-ins:
  - `UiDialog` → `{ Center, None, auto_flip: false }`
  - `UiDropdownMenu` (from combo) → `{ BottomStart, Some(combo), auto_flip: true }`

Layered dismissal / blocking flow:

- `handle_global_overlay_clicks` runs in `PreUpdate` before Masonry input injection.
- On left click:
  1. Read top-most overlay from `OverlayStack`.
  2. Resolve top overlay and optional anchor entity widget ids from `entity.to_bits()` via `MasonryRuntime`.
  3. Query retained Masonry hit path via `RenderRoot::get_hit_path(physical_pos)`.
  4. If hit-path contains top overlay widget id: treat as inside overlay → do nothing.
  5. Else if hit-path contains anchor widget id: close overlay and suppress the click
     (prevents anchor default action from re-triggering in the same frame).
  6. Else: close overlay and do **not** suppress click (underlying UI can react immediately).
- This supports nested overlays with deterministic top-most-only dismissal while avoiding
  Bevy-side rectangle hit math.
- Because top overlay ids now resolve to opaque panel roots, clicks in panel padding/gap regions
  are classified as inside-overlay hits and no longer trigger false outside-dismissals.

Pointer routing + click-outside:

- `handle_global_overlay_clicks` is the canonical implementation.
- Outside/inside resolution uses Masonry-native hit paths and on-demand entity-bit lookup.
- `bubble_ui_pointer_events` remains available for ECS pointer-bubbling paths and walks up
  `ChildOf` parent chains until roots or `StopUiPointerPropagation`.

### 5.6) Font Bridge (Bevy assets/fonts → Masonry/Parley)

`bevy_xilem` now includes an internal font bridge resource (`XilemFontBridge`) and
two-stage sync pipeline to register custom font bytes into Masonry's font database
(`RenderRoot::register_fonts`).

- **Option A (dynamic):** `collect_bevy_font_assets` listens to `AssetEvent<Font>` and
  queues bytes from Bevy's `Assets<Font>`.
- **Bridge flush:** `sync_fonts_to_xilem` registers queued bytes into Masonry/Parley.

- App-level synchronous API is exposed through `AppBevyXilemExt`:
  - `SyncAssetSource::{Bytes(&[u8]), FilePath(&str)}`
  - `.register_xilem_font(SyncAssetSource::...)`
- Registration is fail-fast for missing files and flushes into the active
  Masonry runtime font database immediately during app setup.
- Legacy helpers (`register_xilem_font_bytes` / `register_xilem_font_path`) remain as
  thin compatibility wrappers over the new API.
- Styles can provide a per-node font stack (`Vec<String>`), which is mapped to
  Parley `FontStack` fallback order.
- This enables stylesheet-level `font_family` usage for custom CJK fonts without
  requiring projector-level ad-hoc font wiring.

### 5.7) Synchronous i18n registry + explicit locale font stacks

`bevy_xilem` now uses an in-memory Fluent registry without async asset loading.

- `BevyXilemPlugin` initializes:
  - `AppI18n { active_locale, default_font_stack, bundles, font_stacks }`
- App-level synchronous API is exposed through `AppBevyXilemExt`:
  - `SyncTextSource::{String(&str), FilePath(&str)}`
  - `.register_i18n_bundle(locale, SyncTextSource::..., font_stack)`
- Bundle parsing is fail-fast (invalid locale tags, missing files, or invalid FTL all panic
  during setup).
- `LocalizeText { key }` is resolved through `AppI18n::translate(key)` with key fallback.
- Built-in `UiLabel`/`UiButton` projectors explicitly apply
  `AppI18n::get_font_stack()` as the text font stack for translated views.
- `AppI18n::get_font_stack()` returns locale-specific entries from `font_stacks`,
  or falls back to `default_font_stack`.

Locale/font policy is therefore owned by application setup via i18n bundle registration,
while the styling engine remains locale-agnostic data.

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

Additional ECS-native logical controls and typed events:

- `UiCheckbox` / `UiCheckboxChanged`
- `UiSlider` / `UiSliderChanged`
- `UiSwitch` / `UiSwitchChanged`
- `UiTextInput` / `UiTextInputChanged`

Template-part expansion model:

- Built-in controls can be expanded into child entities tagged with marker components
  (`PartDialogTitle`, `PartDialogDismiss`, `PartComboBoxDisplay`,
  `PartCheckboxIndicator`, `PartSliderTrack`, etc.).
- `expand_builtin_control_templates` maintains these parts from logical state.
- Projectors assemble visuals using `ctx.children` and marker lookups.
- Public helper APIs for user-defined template systems:
  `spawn_template_part`, `ensure_template_part`, `find_template_part`.

### 7) Two-level UI componentization policy

Projector organization follows two complementary componentization levels:

- **Micro-componentization (pure Rust view helpers):**
  Reusable, purely visual fragments (for example tag pills, avatar + name rows,
  common action button variants) should be extracted into pure helper functions that
  return `UiView` or `impl View`.
  Projectors should compose these helpers rather than inlining long builder chains.

- **Macro-componentization (ECS entities + `ChildOf`):**
  UI regions with independent lifecycle/state, or repeated/list items (for example
  feed cards, list rows, sidebars, overlays/panels), should be represented as their own
  ECS entities with dedicated registered projectors.
  Parent projectors should primarily lay out `ctx.children` rather than iterating data
  and constructing many heavy subtrees inline.

This policy is applied across examples to keep projector functions small, improve
incremental ECS updates, and make UI hierarchy ownership explicit.

## ECS data model

Built-in components:

- `UiRoot`
- `UiFlexColumn`
- `UiFlexRow`
- `UiLabel { text }`
- `UiButton { label }`
- `LocalizeText { key }`

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

When multiple `UiRoot` entities exist (for example main root + overlay root),
`MasonryRuntime` composes them into a stacked root so overlay content is rendered above
regular UI content.

## Plugin wiring

`BevyXilemPlugin` initializes:

- `UiProjectorRegistry`
- `SynthesizedUiViews`
- `UiSynthesisStats`
- `UiEventQueue`
- `StyleSheet`
- `ActiveStyleSheetAsset`
- `StyleAssetEventCursor`
- `XilemFontBridge`
- `AppI18n`
- `OverlayStack`
- `MasonryRuntime`

and registers tweening support with:

- `TweeningPlugin` (from crates.io `bevy_tweening` crate)

It ensures Bevy asset runtime support is available and registers:

- `StyleSheet` asset type
- `StyleSheetRonLoader` asset loader

and registers systems:

- `PreUpdate`: `collect_bevy_font_assets -> sync_fonts_to_xilem -> initialize_masonry_runtime_from_primary_window -> bubble_ui_pointer_events -> handle_global_overlay_clicks -> inject_bevy_input_into_masonry -> sync_ui_interaction_markers`
- `Update`: `expand_builtin_control_templates -> ensure_overlay_root -> reparent_overlay_entities -> ensure_overlay_defaults -> handle_overlay_actions -> ... -> ensure_active_stylesheet_asset_handle -> sync_stylesheet_asset_events -> mark_style_dirty -> sync_style_targets -> animate_style_transitions`
- `PostUpdate`: `synthesize_ui -> rebuild_masonry_runtime -> sync_overlay_positions`
- `Last`: `paint_masonry_ui` (explicit Masonry/Vello render + present pass)

Transition execution details:

- `mark_style_dirty` incrementally marks entities whose style dependencies changed
  (class/inline/pseudo/style resource changes), and when descendant selectors are present,
  it propagates dirtiness through descendant hierarchies so ancestor-driven style rules
  recompute correctly.
- `sync_style_targets` recomputes style only for dirty entities, updates `ComputedStyle`,
  computes target interaction colors, and on target changes inserts/replaces
  a `TweenAnim` with a fresh tween targeting `CurrentColorStyle`.
- Tween advancement is performed by `TweeningPlugin`'s
  `AnimationSystem::AnimationUpdate` system set.
- `resolve_style` reads `ComputedStyle` + `CurrentColorStyle` so projectors render in-between values,
  producing smooth CSS-like transitions instead of color snapping.

It also registers built-in projectors.

## Bevy-native run helpers

`bevy_xilem` provides:

- `run_app(bevy_app, title)`
- `run_app_with_window_options(bevy_app, title, configure_window)`

These helpers configure Bevy's primary `Window` component (title/size/resizable hints)
and ensure native Bevy window-loop ownership is active before calling `App::run()`.

Specifically, if missing, they install the same windowing path used by
`bevy::DefaultPlugins` for desktop apps:

- `AccessibilityPlugin` (provides accessibility resources required by winit startup systems)
- `InputPlugin` (initializes keyboard/mouse message streams consumed by winit bridge systems)
- `WindowPlugin` (configured with primary-window title/size/resizable hints)
- `WinitPlugin` (sets Bevy's winit event-loop runner)

No custom Xilem runner/event loop is started.

## Built-in button behavior

Built-in `UiButton` projector maps to `ecs_button(...)` with action `BuiltinUiAction::Clicked`.

## Public API export strategy

To minimize dependency friction, `bevy_xilem` re-exports commonly needed Bevy/Xilem crates and
provides a dual control-view naming scheme:

- Runtime-adjacent integration crates used by examples/apps (for example `bevy_tasks` task pools
  and `rfd` native dialogs) are also re-exported, so downstream apps can stay version-aligned with
  `bevy_xilem`.

- ECS event-adapted controls are exported with ergonomic names (`button`, `checkbox`, `slider`,
  `switch`, `text_button`, `text_input`, ...).
- Original `xilem_masonry::view` controls are re-exported with `xilem_` prefixes
  (`xilem_button`, `xilem_checkbox`, ...).
- Legacy `ecs_*` exports remain available for compatibility.

## Examples

Examples were rewritten to demonstrate this architecture with:

- GUI windows via Bevy's native `bevy_winit` runner
- Bevy-driven synthesis updates each frame
- typed action handling via `UiEventQueue` (ECS queue path only)
- stylesheet-driven styling (class rules + cascade) instead of hardcoded projector styles
- pseudo-class interaction styling and transition-capable style resolution
- virtualized task scrolling in `todo_list` using `xilem_masonry::view::virtual_scroll`
- no `xilem::Xilem::new_simple` usage
- no `xilem::Xilem::new` event-loop ownership

## Non-goals in current repository state

- No custom render-graph integration beyond Masonry retained runtime ownership
