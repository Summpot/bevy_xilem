# AGENTS.md

This document defines how automated agents (and humans operating like them) should work in this repository.

## Non‑negotiables

1. **Design consistency (required)**
   - For any requested change, **verify it matches `DESIGN.md`**.
   - If it does **not** match, **update `DESIGN.md` in the same change** (or immediately before) so design and implementation remain consistent.
   - Do not implement behavior that contradicts the design without also updating the design.

2. **Keep the project test-first**
   - Add/adjust tests for behavior changes.
   - Ensure `cargo test` passes before finishing.

3. **Prefer minimal, reviewable diffs**
   - Make small, incremental changes.
   - Avoid unrelated refactors/renames.
   - Don’t reformat unrelated code; only apply formatting that naturally results from touching code (`cargo fmt`, Biome).

4. **Rust dependency hygiene**
   - Before adding a new Rust dependency (new crate in `Cargo.toml`), check whether `cargo upgrade` is available.
     - If it exists, run `cargo upgrade` to see whether a newer compatible version is available and prefer the newest reasonable versions.
     - If it does **not** exist (e.g., `cargo-edit` not installed), **do not check newer version**; just skip this step and proceed.

If a change affects public behavior (config schema, admin endpoints, tunnel protocol), update `DESIGN.md` and the examples/schema together.

## Quick verification checklist

- `cargo test`
- `cargo fmt` (when Rust code changes)
