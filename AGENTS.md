# Repository Guidelines

## Project Structure & Module Organization

- `src/`: Rust backend (Axum API + embedded static Web UI + SQLite via `sqlx`).
- `web/`: Frontend (React + Vite + Bun). Build output `web/dist` is embedded into the backend at compile time, so it must exist before building/running the Rust service.
- `tests/`: Rust integration tests (`tests/*.rs`) and HTML fixtures in `tests/fixtures/`.
- `deploy/`: Docker Compose + Caddy reverse-proxy example for same-origin access and injecting the user-id header.
- `docs/plan/`: Scoped plan docs (frozen acceptance criteria per plan). `docs/ui/` contains consolidated design SVGs.

## Build, Test, and Development Commands

Backend (from repo root):

- `cargo run`: Run the service (expects `web/dist` to exist).
- `cargo fmt`: Format Rust.
- `cargo clippy --all-targets --all-features -- -D warnings`: Lint Rust (warnings are errors).
- `cargo test --all-features`: Run backend tests.

Frontend:

- `cd web && bun install`: Install dependencies.
- `cd web && bun run dev`: Local dev server.
- `cd web && bun run build`: Build `web/dist` for embedding.
- Storybook (fixed port `18181`):
  - `cd web && bun run storybook` (dev server)
  - `cd web && bun run storybook:ci` (CI-friendly: no browser auto-open)

Important: Always start Storybook via the `web/package.json` scripts above (from `web/`). Do not run `storybook dev`/`bunx storybook dev`/`npx storybook dev` from the repo root; that will typically fall back to the default port (`6006`) and break the repo's assumptions (e.g. Vitest storybook tests expect `http://localhost:18181`).

Deploy:

- `docker build -t catnap .`: Build container image.
- `cd deploy && docker compose up -d --build`: Run with Caddy reverse-proxy and a persistent SQLite volume.

## Coding Style & Naming Conventions

- Rust: rely on `cargo fmt`; prefer idiomatic Rust + clippy-clean code.
- Web: Biome enforces formatting/linting (2 spaces, 100 cols). Run `cd web && bun run lint` and `bun run typecheck`.
- Naming: Rust uses `snake_case` modules/functions and `PascalCase` types; React components use `PascalCase` and files are typically `*.tsx`.

## Testing Guidelines

- Backend: `cargo test` (integration tests in `tests/`).
- Frontend story-based tests: `cd web && bun run test:storybook` (Vitest + Playwright). First-time setup: `cd web && bunx playwright install chromium`.

## Commit & Pull Request Guidelines

- Commits follow Conventional Commits (examples in history: `fix(ci): ...`, `docs(plan): ...`). Subjects must be English, lower-case, ≤72 chars; keep body lines ≤100 chars (enforced by `commitlint`).
- Hooks: `lefthook.yml` runs `cargo fmt`, `cargo clippy`, `cargo test`, and web linting pre-commit.
- PRs: include a clear description, link the relevant plan/issue if applicable, and attach screenshots for UI changes. For merges to `main`, select exactly one release intent label: `type:docs|type:skip|type:patch|type:minor|type:major` (CI enforces this).
