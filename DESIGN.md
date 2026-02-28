# bevy_xilem Design Document

Date: 2026-02-23

This document describes the **current implementation** after the architecture pivot to
**headless Masonry + Bevy-driven scheduling/input**.

> Note: “headless” here describes the internal retained runtime ownership model,
> not that end-user apps/examples must be non-GUI.

## 1. Purpose

`bevy_xilem` integrates Bevy ECS state management with a retained Masonry UI tree, while using
Xilem Core diff/rebuild semantics for view reconciliation.

The framework now avoids the high-level `xilem::Xilem::new_simple` runner completely.

## 2. Core Runtime Architecture

### 2.1 Event loop ownership is Bevy-first

- Bevy owns scheduling and window/input message flow.
- Masonry is driven as a retained UI runtime resource from Bevy systems.
- `bevy_xilem` does **not** run a separate Xilem/Masonry event loop.
- GUI apps use Bevy's native `App::run()` and `bevy_winit` window lifecycle.

### 2.2 Headless retained runtime resource

`MasonryRuntime` is a Bevy `Resource` that owns:

- Masonry `RenderRoot` (retained widget tree)
- current synthesized root view
- Xilem `ViewCtx` and `ViewState`
- pointer state required for manual event injection
- active Bevy primary-window attachment metrics (logical size + scale factor)

`PostUpdate` applies synthesized root diffs directly with Xilem Core `View::rebuild`.
`PreUpdate` lazily binds runtime viewport/scale to the Bevy-created primary window once
`bevy_winit` exposes it.

**Initialization invariant:**

- `initialize_masonry_runtime_from_primary_window` injects an explicit initial logical resize
  immediately after first attach so Masonry never starts hit-testing from a `(0, 0)` root size.

### 2.3 Explicit Masonry/Vello paint pass (Last)

Because Bevy's renderer plugins are intentionally not required for the retained UI path,
`bevy_xilem` performs an explicit Vello paint/present pass in `Last`:

- `RenderRoot::redraw()` produces the current scene.
- `ExternalWindowSurface` (from `masonry_winit`) owns persistent surface/device state bound
  to the Bevy primary window.
- the pass renders to an intermediate texture, blits to the swapchain surface, and presents.
- the primary window requests another redraw to keep UI animations and visual updates flowing.

This avoids the "window opens but no pixels are drawn" failure mode when only
window/input plugins are active.

## 3. Input and Hit Testing

### 3.1 Input injection bridge (PreUpdate)

`PreUpdate` system consumes Bevy messages (`CursorMoved`, `CursorLeft`, `MouseButtonInput`, `MouseWheel`, `KeyboardInput`, `Ime`, `WindowFocused`, `WindowResized`, `WindowScaleFactorChanged`) and translates them to Masonry events (`PointerEvent`, `TextEvent`, `WindowEvent`), which are injected into `MasonryRuntime.render_root`.

**Pointer bridge invariants:**

- `Window::physical_cursor_position()` from the current `PrimaryWindow` is the source of truth
  for injected Masonry pointer coordinates.
- When physical cursor data is unavailable, logical cursor coordinates can be converted using
  the current window scale factor for compatibility paths.
- `CursorMoved.position` payload is not trusted for hit-test coordinates.
- `MouseButtonInput` / `MouseWheel` are injected only when physical cursor data is available;
  when unavailable (cursor outside), pointer interaction injection is skipped.
- Text input is bridged through both keyboard semantics (`TextEvent::Keyboard` for navigation/editing keys)
  and committed text (`TextEvent::Ime::Commit`) so ECS `UiTextInput` remains editable under
  Bevy-driven input scheduling.
- Window resize injection uses logical `Window::width()` / `Window::height()` from the active
  primary window, ensuring Masonry receives DPI-correct dimensions.
- Click-path ordering is enforced by injecting `PointerMove` before each
  `PointerDown` / `PointerUp` so hot/hovered state is current before activation.

## 4. UI Components and Registration

### 4.1 Component-centric UI component encapsulation (`UiComponentTemplate`)

Built-in logical UI components are organized under `crates/bevy_xilem/src/components/*.rs`. Each UI component module owns its logical component shape, template-part policy, and the trait contract used for registration.

The unifying trait is `UiComponentTemplate`. Trait responsibilities:

- `expand(world, entity)` for one-time logical→template expansion.
- `project(&T, ProjectionCtx) -> UiView` for ECS→Masonry projection.

### 4.2 Streamlined registration API

`AppBevyXilemExt` exposes `.register_ui_component::<T: UiComponentTemplate>()`. One call performs projector registration, `Added<T>` expansion system hookup, and selector type alias registration. Built-in UI components are registered centrally, so user apps only call this for explicit custom usage.

### 4.3 ECS UI component adapter coverage

`bevy_xilem` provides ECS adapters for components producing user actions, such as `ecs_button`, `ecs_checkbox`, `ecs_slider`, `ecs_switch`, and `ecs_text_input`. Non-interactive display/layout widgets are reused directly.

### 4.4 Portal-based `UiScrollView` UI component

Implemented as a logical ECS UI component projected through a Masonry portal view, with explicit scroll state (`scroll_offset`, `content_size`) and optional external scrollbar parts (`PartScrollBarVertical`, `PartScrollThumbVertical`).

- `PreUpdate` reads back portal geometry from Masonry and synchronizes ECS `viewport_size` / `content_size` each frame.
- `scroll_offset` is strictly clamped to physical bounds (`0..max_scroll_x`, `0..max_scroll_y`) after drag/wheel/layout-sync updates.
- Wheel deltas are routed from deepest hit target outward, and are consumed by the first ancestor `UiScrollView` that can actually move, preventing boundary desync in nested scroll views.

## 5. Event Handling

### 5.1 Zero-closure ECS button path

To remove user-facing closure boilerplate:

- `EcsButtonView` implements `xilem_core::View` on top of a custom wrapper widget.
- `EcsButtonWithChildView` provides the same event semantics for composed button content
  (icon + label rows, swatches, custom child layouts).
- On interaction, it emits structural events (`PointerEntered`, `PointerPressed`) and typed ECS actions (`UiEventQueue`).

This keeps built-in interactive controls (`UiButton`, checkbox/radio compositions, menu/dropdown items,
color/date trigger buttons) on a unified ECS event route even in headless-runtime mode.

### 5.2 Typed action queue

`UiEventQueue` is a Bevy `Resource` backed by a queue. Widgets push type-erased actions. Bevy systems drain typed actions via `drain_actions::<T>()` non-destructively for multiple consumers.

## 6. Styling Engine

The runtime supports a data-driven style pipeline with four phases:

1. **Inline style overrides:** `InlineStyle` (preferred consolidated override), or legacy split components `LayoutStyle`, `ColorStyle`, `TextStyle`, `StyleTransition`.
2. **Selector-based stylesheet & cascade:** `StyleSheet` resource mapped from `.ron` files.
3. **Pseudo classes:** `InteractionState { hovered, pressed }` synchronized from interaction events (mutated in-place to avoid archetype churn).
4. **Computed-style cache & incremental invalidation:** Resolves final traits via `StyleDirty` / `ComputedStyle`.

### 6.1 Smooth transitions

`TargetColorStyle` + `CurrentColorStyle` are driven by `bevy_tweening::TweenAnim`, targeting `CurrentColorStyle` for smooth micro-interaction transforms and color transitions without snapping.

### 6.2 Base vs active stylesheet tiers

Runtime styling distinguishes two explicit tiers: `BaseStyleSheet` (embedded Fluent baseline) and `ActiveStyleSheet` (user asset). Active rules cascade over baseline rules by priority.

Baseline Fluent theme includes a global `Type("UiRoot")` preflight rule for app-surface background, and the `UiRoot` projector stretches to full viewport so root background styling consistently covers the entire window.

`BevyXilemPlugin` boots with embedded **Fluent Dark** by default. Built-in Fluent theming is provided as a **single multi-variant bundle** (`fluent_theme.ron`) that contains named variants (`dark`, `light`, `high-contrast`, and future additions). The styling system parses/registers this bundle into `RegisteredStyleVariants`.

Runtime variant selection is state-driven via `ActiveStyleVariant`. Apps/examples set desired variant by name through `set_active_style_variant_by_name(...)`, and `sync_active_style_variant` automatically applies it to `BaseStyleSheet` + runtime `StyleSheet`. Plugin bootstrap sets the theme file's own default variant as active, and the first `Update` pass applies it automatically.

Theme activation no longer exposes `install_*` APIs. The only public path is active-variant state (`set_active_style_variant_by_name` / `set_active_style_variant_to_registered_default`) plus automatic sync (`sync_active_style_variant`).

Variant bundles support top-level shared `rules`/`tokens` plus per-variant overrides. This keeps common selector graphs out of any single variant and lets each variant focus on palette/token deltas.

When an entity has no matched selector rules and no inline style sources, style resolution intentionally uses a transparent text fallback so the UI does not inherit Masonry/Xilem intrinsic default text appearance. In practice, this keeps "no theme selected" surfaces visually empty instead of looking partially themed by engine defaults.

### 6.3 Hit-testing invariants

Layout-affecting styles (padding/border/background) are applied directly to the target UI component widget itself, ensuring Masonry's hit-testing matches the structural box model users see, specifically on bounded overlays/dialogs vs global backgrounds.

## 7. Overlays and Modals

`bevy_xilem` includes a built-in ECS overlay model using floating/portal roots natively stacked through Masonry.

### 7.1 Layering and Positioning

- **Centralized Layering Model:** `OverlayStack` maintains top-most order. `sync_overlay_stack_lifecycle` keeps it pruned.
- **Universal Placement Model:** `OverlayPlacement` handles Center/Top/Bottom alignments. `sync_overlay_positions` calculates clamping and auto-flipping against screen edges.
- **Built-in Floating Widgets:** `UiDialog` (modal), `UiComboBox` (anchor), `UiDropdownMenu` (floating list), `UiTooltip` (hover-anchor), `UiToast` (default bottom-end placement, configurable placement/width/close-button).
- **FOUC prevention invariant:** overlay projectors must render with fully transparent resolved styles while `OverlayComputedPosition.is_positioned == false`, then become visible once synchronized placement is available.
- **Generic temporary lifecycle:** `AutoDismiss { timer }` supports timer-driven teardown for temporary overlays (e.g. toasts).

### 7.2 Layered Dismissal and Blocking Flow

`handle_global_overlay_clicks` dynamically evaluates pointer location against the top-most overlay using `RenderRoot::get_hit_path(physical_pos)` and all widget IDs bound to that overlay entity, with an `OverlayComputedPosition` rectangle fallback. This avoids false outside-click dismissal when interacting with deeply nested portal/menu content (e.g., combo-box options).

Clicks outside the opaque overlay root cause dismissals without disrupting interactive siblings. Optional `UiOverlayRoot` dimly rendering full-view backgrounds without structurally wrapping modal UI boundaries.

When clicking an overlay anchor to close an anchored overlay, pointer suppression is press-only
for the consumed click. This avoids stale suppressed-release state that can otherwise leave
trigger buttons in a sticky pressed visual/input state.

## 8. Iconography

Built-in directional indicators and radio markers are provided through a dedicated
`bevy_xilem::icons` module backed by `lucide-icons` icon data/font assets (instead of
hand-drawn canvas paths). The plugin registers bundled Lucide font bytes at startup and icon
text styling uses the upstream Lucide family name (`"lucide"`) so rendering remains stable
across locales and system font configurations.

## 9. Assets and Internationalization

### 8.1 Font Bridge

`XilemFontBridge` manages moving Bevy `Asset<Font>` to Masonry's system. Registers font bytes from `collect_bevy_font_assets` directly to `MasonryRuntime` using `sync_fonts_to_xilem` in the asset queue lifecycle.

### 8.2 Synchronous i18n Registry

Centralized in `AppI18n`. Synchronous setup through `.register_i18n_bundle()`. Uses declarative font stacks applied based on locale priorities.

## 10. ECS Data Model & Synthesis Pipeline

### 9.1 Data Model

Core built-ins: `UiRoot`, `UiFlexColumn`, `UiFlexRow`, `UiLabel`, `UiButton`, `LocalizeText`. Node identities for projection context use `entity.to_bits()`.

### 9.2 Synthesis Pipeline

Driven via `UiProjectorRegistry`.
`PostUpdate` executes:

1. Gather `UiRoot`.
2. Recursive projection (`project()`).
3. Store `SynthesizedUiViews`.
4. Rebuild retained Masonry root in `MasonryRuntime`.

## 11. Developer Ergonomics

### 10.1 Two-level UI componentization policy

- **Micro-componentization:** Reusable fragments returned as pure Rust View helpers (`UiView` or `impl View`).
- **Macro-componentization:** UI regions mapped purely to ECS via `register_ui_component::<T>()`.

### 10.2 Bevy-native run helpers

`run_app()` avoids raw setup tasks, bootstrapping native `bevy_winit` safely to Bevy systems for seamless desktop lifecycle apps.

## 12. Activation / Deep-link Runtime

The workspace now includes a dedicated crate: `bevy_xilem-activation`.

### 12.1 Responsibilities

- **Single-instance gate** (`single-instance`): only one primary process keeps the UI/runtime.
- **Activation IPC bridge** (`interprocess` local socket): secondary launches forward URI payloads to the primary process, then exit.
- **Custom URI protocol registration** (`sysuri`): app-managed registration for OS-level protocol handling.

### 12.2 Pixiv callback flow

`example_pixiv_client` registers `pixiv:` on startup through `bevy_xilem-activation` and uses activation IPC to route callback URIs:

1. User starts browser OAuth from running app.
2. Browser callback opens `pixiv://account/login?code=...&via=login`.
3. OS launches app entrypoint; secondary instance forwards URI to primary and exits.
4. Primary instance auto-extracts `code` and triggers token exchange (`authorization_code`) using current PKCE verifier.

This removes the manual copy/paste requirement in normal desktop callback flow while preserving manual fallback input behavior.

## 13. Examples and Non-goals

- **Examples:** Highlighted in crates evaluating architectures (`chess_game`, `ui_showcase`, `todo_list`).
- **Non-goals:** Custom render-graph bridging out of scope; sticks to Masonry retained runtime ownership implicitly.
