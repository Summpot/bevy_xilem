# bevy_xilem Styling System

This document explains the CSS-like, ECS-driven styling pipeline used by `bevy_xilem`.

It covers:

- style data model (`Selector`, `StyleSetter`, `StyleRule`, `StyleSheet`, inline style components)
- cascade + pseudo state resolution
- computed style cache + incremental invalidation
- projector integration patterns
- smooth transition animations (Phase 4) powered by `bevy_tweening`
- practical usage patterns and common pitfalls

> Applies to the current `master` branch state (2026-02-16).

---

## 1. Goals

The styling system is designed to be:

1. **Data-driven**: styles are ECS components/resources, not ad-hoc widget chains.
2. **Composable**: class styles (`StyleClass` + `StyleSheet`) plus inline overrides.
3. **Interactive**: pseudo states (`Hovered`, `Pressed`) driven from UI interaction events.
4. **Animated**: smooth color transitions between interaction states via tweening.

---

## 2. Core Types

All style primitives live in `crates/bevy_xilem/src/styling.rs`.

### 2.1 Inline style components

- `LayoutStyle`
  - `padding: Option<f64>`
  - `gap: Option<f64>`
  - `corner_radius: Option<f64>`
  - `border_width: Option<f64>`
- `ColorStyle`
  - base: `bg`, `text`, `border`
  - pseudo overrides: `hover_*`, `pressed_*`
- `TextStyle`
  - `size: Option<f32>`
- `StyleTransition`
  - `duration: f32` (seconds)

### 2.2 Class stylesheet model

- `StyleClass(pub Vec<String>)` (component on entities)
- `Selector::{Type, Class, PseudoClass, And}`
- `StyleSetter { layout, colors, text, transition }`
- `StyleRule { selector, setter }`
- `StyleSheet { rules: Vec<StyleRule> }` (resource)

Convenience APIs still exist for class-only rules:

- `StyleSheet::set_class("name", setter)`
- `StyleSheet::with_class("name", setter)`

### 2.3 Pseudo-state markers

- `Hovered`
- `Pressed`

### 2.4 Cache + invalidation runtime state

- `StyleDirty` (marks entities requiring recomputation)
- `ComputedStyle` (cached resolved style read by projectors)

### 2.5 Transition runtime state

- `TargetColorStyle` (resolved target colors for the current pseudo state)
- `CurrentColorStyle` (animated colors read by projectors)
- `bevy_tweening::TweenAnim` targeting `CurrentColorStyle`

---

## 3. Cascade and Resolution Order

`resolve_style(world, entity)` follows this precedence (low → high):

1. selector-matched rules from `StyleSheet` (`Type`/`Class`/`PseudoClass`/`And`)
2. inline component overrides (`LayoutStyle`, `ColorStyle`, `TextStyle`, `StyleTransition`)
3. compatibility pseudo color overrides (`hover_*`, `pressed_*`) from `ColorStyle`
4. animated override from `CurrentColorStyle` if present

In short: class + inline define intent, pseudo state chooses target, animator provides smooth in-between values.

---

## 4. Plugin Wiring

`BevyXilemPlugin` automatically wires the style stack:

- initializes `StyleSheet`
- `PreUpdate`: `sync_ui_interaction_markers`
- `Update`: `mark_style_dirty -> sync_style_targets -> animate_style_transitions`
- registers `TweeningPlugin` (from crates.io `bevy_tweening`)

So users only need to define styles and apply them from projectors.

---

## 5. Defining Styles

A common pattern is one startup system per screen/example:

- `setup_*_styles` for style declarations
- `setup_*_world` for ECS structure

Example shape:

- `style_sheet.set_class("todo.root", setter)`
- `style_sheet.set_class("todo.add-button", setter)`
- `style_sheet.add_rule(StyleRule::new(Selector::and(...), setter))`

Naming suggestions:

- namespace by feature: `todo.*`, `calc.*`, `chess.*`
- split container/control/text classes:
  - `*.root`
  - `*.button`
  - `*.button.label`

---

## 6. Applying Styles in Projectors

Key helper functions:

- `resolve_style(world, entity)`
- `resolve_style_for_classes(world, ["class.a", "class.b"])`
- `resolve_style_for_entity_classes(world, entity, ["class.a", "class.b"])`
- `apply_widget_style(view, &style)`
- `apply_label_style(label(...), &style)`
- `apply_text_input_style(text_input(...), &style)`

Recommended projector pattern:

1. resolve root/entity style with `resolve_style`
2. resolve shared class styles with `resolve_style_for_classes`
3. compose the widget tree using style helpers

This keeps structure and style concerns separated.

---

## 7. Interaction and Pseudo States

Interaction events are emitted by ECS-backed controls (notably the custom ECS button widget path):

- `PointerEntered`
- `PointerLeft`
- `PointerPressed`
- `PointerReleased`

`sync_ui_interaction_markers` consumes these events and mutates `Hovered`/`Pressed` components.

That state then affects color target selection during style resolution.

---

## 8. Phase 4: Smooth Tween-Based Transitions

Phase 4 replaces manual per-frame color lerp logic with a tweening animator pipeline.

### 8.1 `bevy_tweening` integration

`bevy_xilem` now uses crates.io `bevy_tweening` (`0.15`) for transitions:

- `TweeningPlugin`
- `EaseMethod` (configured with a QuadraticInOut-equivalent easing function)
- `Tween<T>`
- `TweenAnim`
- `Lens<T>`

### 8.2 Custom lens: `ColorStyleLens`

`ColorStyleLens` implements:

- `Lens<CurrentColorStyle>`

It linearly interpolates each RGBA channel for:

- background color
- text color
- border color

while easing is applied by tween sampling (`QuadraticInOut` by default for interaction transitions).

### 8.3 State-change behavior

When target style changes (for example, due to hover/press changes):

- `mark_style_dirty` marks entities with changed style dependencies.
- `sync_style_targets` recomputes dirty entities, updates `ComputedStyle`, and computes a new `TargetColorStyle`.
- If a transition is configured and target changed:
  - insert/update `TweenAnim` with a new `Tween` targeting `CurrentColorStyle`
  - tween starts from current animated value and ends at new target value
- `TweeningPlugin` advances animations each frame in `AnimationSystem::AnimationUpdate`.
- Projectors read `ComputedStyle` (+ animated `CurrentColorStyle`) via `resolve_style`.

Result: no color snap; smooth CSS-like interpolation.

### 8.4 Duration recommendations

For UI interaction micro-animations:

- common range: `0.10`–`0.18` seconds
- default used in examples: around `0.15` seconds

---

## 9. Practical Example Checklist

To make a control animate on interaction:

1. define base and hover/pressed colors in `ColorStyle`
2. set `transition: Some(StyleTransition { duration: ... })`
3. ensure control path emits interaction events (`Hovered`/`Pressed` updates) so entities become `StyleDirty`
4. apply style with projector helpers

---

## 10. Common Pitfalls

- **Class-only resolution is static**
  - `resolve_style_for_classes(...)` does not bind pseudo state by itself.
  - Use `resolve_style_for_entity_classes(...)` when pseudo-state-dependent classes are needed.
- **Interaction event source matters**
  - if a control path does not emit `UiInteractionEvent`, pseudo-state-based transitions will not trigger.
- **Wrapper styling vs. inner widget styling**
  - some controls may have internal defaults (such as borders) that require styling the interactive path itself.
- **Keep design/docs in sync**
  - if style behavior changes, update both implementation and `DESIGN.md`/docs in one change.

---

## 11. Reference Files

- Styling core: `crates/bevy_xilem/src/styling.rs`
- Plugin wiring: `crates/bevy_xilem/src/plugin.rs`
- ECS button interaction source: `crates/bevy_xilem/src/widgets/ecs_button_widget.rs`
- ECS button view path: `crates/bevy_xilem/src/views/ecs_button_view.rs`
- Architecture doc: `DESIGN.md`

---

If you plan to extend this system (for example `:disabled`, theme variables/tokens, inherited style contexts, or layout tweening), extend `StyleRule` first, then wire `resolve + sync + animation`, and finally update docs/examples together.
