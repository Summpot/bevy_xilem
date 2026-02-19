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

3. **Rust dependency hygiene**
   - After adding a new Rust dependency (new crate in `Cargo.toml`), check whether `cargo upgrade` is available.
     - If it exists, run `cargo upgrade` to see whether a newer compatible version is available and prefer the newest reasonable versions.
     - If it does **not** exist (e.g., `cargo-edit` not installed), **do not check newer version**; just skip this step and proceed.

4. **Avoid interactive app runs by default**
   - Do **not** run `cargo run` unless user interaction is required to extract runtime logs or reproduce an interactive issue.
   - Prefer `cargo test`, static checks, and targeted diagnostics for routine verification.

5. **Fluent message-id syntax (required)**
   - CRITICAL SYNTAX NOTE FOR FLUENT (`.ftl`) FILES:
     Do NOT use dots (`.`) to namespace Message IDs (e.g., `nav.home.title` is INVALID).
     In Project Fluent, dots are strictly reserved for Attributes.
     You MUST use hyphens (`-`) for namespacing your localized keys (e.g., use `nav-home-title` and `settings-theme-toggle`).

6. **Default autonomous execution (required)**
   - Do **not** ask the user for routine confirmations or step-by-step permission.
   - For straightforward tasks with a clear implementation path, execute directly and report results.
   - Only ask the user when the decision is **architecture-level** and there are **multiple valid options with meaningful trade-offs**.
   - If there is only one reasonable path, proceed without asking.

7. **Fork + submodule workflow (required)**
   - `third_party/bevy` and `third_party/xilem` are treated as fork-backed submodules.
   - Keep `origin` pointed to the user's fork and `upstream` pointed to the official repository.
   - Do fork modifications only on branch `bevy-xilem-dev` (never directly on `main`/default branch).
   - When syncing upstream, rebase/merge `upstream/*` into `bevy-xilem-dev`, then update the submodule commit in this repo.
   - For local validation before pushing fork commits, temporary Cargo `[patch]` or path overrides are allowed; remove temporary overrides once validation is complete unless they are intentionally part of the design.

If a change affects public behavior (config schema, admin endpoints, tunnel protocol), update `DESIGN.md` and the examples/schema together.

## Quick verification checklist

- `cargo test`
- `cargo fmt` (when Rust code changes)
