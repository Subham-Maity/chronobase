# Rust Engineering Standard — Core Rules

This is the **base, project-agnostic** engineering standard for any Rust service
codebase. It exists so an AI coding agent can pick up work on a Rust project
with **zero prior context** and immediately write code that looks like it was
written by the rest of the team.

These rules are deliberately generic — they describe *shapes* (folder layout,
error handling, layering, naming) rather than any specific domain. A project
may layer additional, project-specific rules on top of this file, but nothing
here should ever be contradicted without a very good reason.

If you are an agent reading this: **internalize the patterns below and apply
them by default**, even when the user's request doesn't explicitly ask for
"proper structure." A senior engineer doesn't wait to be told to write clean,
typed, testable code — neither should you.

> Applies to binaries, services, and libraries alike. Where a rule is
> service-specific (e.g. HTTP handlers), it's marked as such.

---

## Table of Contents

1. [Core Engineering Philosophy](#core-engineering-philosophy)
2. [Workspace & Crate Layout](#workspace--crate-layout)
3. [Feature Module Layout](#feature-module-layout)
4. [Naming Conventions](#naming-conventions)
5. [Error Handling Pattern](#error-handling-pattern)
6. [Configuration Pattern](#configuration-pattern)
7. [Application State & Dependency Wiring](#application-state--dependency-wiring)
8. [Scoped-Access / Multi-Tenancy Pattern](#scoped-access--multi-tenancy-pattern)
9. [Authentication & Authorization Pattern](#authentication--authorization-pattern)
10. [Data Access / Repository Pattern](#data-access--repository-pattern)
11. [State-Machine / Layered Processing Pattern](#state-machine--layered-processing-pattern)
12. [Multi-Stage Pipeline Pattern](#multi-stage-pipeline-pattern)
13. [Caching & Key Registry Pattern](#caching--key-registry-pattern)
14. [Logging & Observability](#logging--observability)
15. [Auto-Discovery / Registry Pattern](#auto-discovery--registry-pattern)
16. [Testing Pattern](#testing-pattern)
17. [Documentation Convention](#documentation-convention)
18. [Concurrency Rules](#concurrency-rules)
19. [Quick Pre-PR Checklist](#quick-pre-pr-checklist)
20. [Anti-Patterns to Avoid](#anti-patterns-to-avoid)

---

## Core Engineering Philosophy

- **The compiler is the first reviewer.** Lean on the type system instead of
  fighting it with stringly-typed data, `Option<Option<T>>` juggling, or
  `Box<dyn Any>` escape hatches.
- **Make illegal states unrepresentable.** Model states as `enum`s, not as a
  combination of booleans or nullable fields that can drift out of sync.
- **Fail loud at the edges, fail typed everywhere else.** Every fallible
  operation returns `Result<T, E>` with a real error type — never a bare
  `String` or `Box<dyn Error>` as a permanent type.
- **Explicit over implicit.** Dependencies are passed in, not reached for via
  globals. Tenant/scope/user identity is a function parameter, never inferred
  from ambient state.
- **Boring is good.** Prefer well-known, actively maintained crates
  (`tokio`, `serde`, `thiserror`, `tracing`, `sqlx`/`sea-orm`, `validator`)
  over clever one-off solutions.
- **One layer, one concern.** Transport parsing, validation, business logic,
  and persistence are separate layers that never reach past their neighbor.
- **Security decisions are explicit code, not comments.** Where a check
  exists purely to close an exploit, it is enforced in code and explained
  inline — never just assumed.

---

## Workspace & Crate Layout

For any non-trivial service, use a Cargo workspace so domain logic is
decoupled from the binary entrypoint and can be tested/reused independently:

```
Cargo.toml                   # [workspace] members
crates/
├── app/                     # binary crate — composition root only
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs          # tokio runtime bootstrap, wiring, graceful shutdown
│       └── bootstrap/
│           ├── mod.rs
│           ├── config_loader.rs
│           ├── router.rs
│           └── state.rs
├── core/                    # shared domain types, framework-agnostic
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
└── <feature>/                # one crate per bounded context, see below
    ├── Cargo.toml
    └── src/
        └── lib.rs
```

For small-to-medium services, a single binary crate with the module layout
below is acceptable — the *module* boundaries matter more than the *physical
crate* boundaries. Reach for a multi-crate workspace once a module needs to
be reused across binaries, or compile times start to hurt.

`main.rs` is a composition root and nothing else: it loads config, builds
`AppState`, wires the router, and starts the runtime. It contains **no**
business logic and **no** route handler bodies.

---

## Feature Module Layout

Every feature/domain module — whether a folder inside `src/` or its own
crate — follows this internal shape ("vertical slice"):

```
src/<feature>/
├── mod.rs                   # barrel: submodule list + curated public re-exports
├── router.rs                 # route table for this feature only
├── handler/
│   ├── mod.rs
│   ├── create_<x>.rs          # one file per endpoint, or grouped by resource
│   └── get_<x>.rs
├── dto/
│   ├── mod.rs
│   ├── request.rs             # #[derive(Deserialize, Validate)] wire-in types
│   └── response.rs            # #[derive(Serialize)] wire-out types
├── service/
│   ├── mod.rs
│   └── <feature>_service.rs   # trait + impl, all business logic lives here
├── repository/
│   ├── mod.rs
│   └── <feature>_repository.rs # trait + sqlx/diesel/sea-orm impl
├── middleware/                 # feature-scoped extractors/guards, if any
│   └── mod.rs
├── error.rs                    # thiserror enum + IntoResponse impl
├── model.rs                    # domain structs (DB row <-> domain conversion)
└── constant.rs                  # magic strings/numbers, cache-key prefixes, etc.
```

Rules:

- `mod.rs` is a **barrel**: it declares submodules and re-exports only the
  types other features are allowed to consume (`pub use self::dto::response::*;`).
  Everything else stays `pub(crate)` or private.
- A handler function does **one** thing: parse input → call one service
  method → map the result to a response. No branching business logic in a
  handler body.
- Business logic lives only in `service/`. Handlers never call repositories
  directly, and repositories never call services.
- If a feature grows past ~10 files per subfolder, split by sub-resource
  before splitting by technical layer further.

---

## Naming Conventions

| Element | Convention | Example |
|---|---|---|
| Files | `snake_case.rs`, matching the primary item inside | `invoice_repository.rs` |
| Types (`struct`/`enum`/`trait`) | `PascalCase` | `InvoiceService` |
| Functions, variables, modules | `snake_case` | `find_by_org` |
| Traits describing a capability | plain noun/verb, no `I`-prefix | `InvoiceRepository`, not `IInvoiceRepository` |
| Error enums | `<Feature>Error` | `BillingError`, `AuthError` |
| Request DTOs | `<Action><Resource>Request` | `CreateInvoiceRequest` |
| Response DTOs | `<Resource>Response` | `InvoiceResponse` |
| Cache/queue key constants | `SCREAMING_SNAKE_CASE` ending in the delimiter | `SESSION_PREFIX: &str = "session:"` |

Never mix a database row struct with a wire-format struct — even if their
fields currently match, they represent different contracts and will diverge.

---

## Error Handling Pattern

**Rule: no `unwrap()`, `expect()`, or `panic!()` outside of `main.rs`,
build scripts, or `#[cfg(test)]` blocks.** Every fallible operation returns
`Result<T, E>`.

Each feature module defines its own error enum with `thiserror`:

```rust
// src/billing/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BillingError {
    #[error("invoice {0} not found")]
    NotFound(String),

    #[error("insufficient permissions for org {org_id}")]
    Forbidden { org_id: String },

    #[error("validation failed: {0}")]
    Validation(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
```

Errors convert into HTTP responses via a single `IntoResponse` impl per
feature — never ad-hoc `match` blocks scattered across handlers:

```rust
// src/billing/error.rs (continued)
impl IntoResponse for BillingError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            BillingError::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            BillingError::Forbidden { .. } => (StatusCode::FORBIDDEN, self.to_string()),
            BillingError::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            BillingError::Database(_) => {
                // Internal detail is logged, never leaked to the client.
                tracing::error!(error = %self, "unhandled database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
```

A top-level `AppError` only wraps multiple feature errors when a single
handler can genuinely fail in more than one feature's terms — don't create
one giant catch-all error enum for the whole application.

---

## Configuration Pattern

- One typed `Config` struct per application, deserialized once at boot
  (`envy`, `figment`, or the `config` crate) — never scattered
  `std::env::var()` calls across the codebase.
- **Fail fast:** invalid or missing configuration panics in `main.rs` before
  the server starts accepting traffic. This is one of the few places a
  panic is acceptable, since it happens before any request is served.

```rust
// bootstrap/config_loader.rs
#[derive(Debug, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

pub fn load_config() -> Config {
    envy::from_env::<Config>()
        .expect("invalid or missing environment configuration")
}
```

---

## Application State & Dependency Wiring

- A single `AppState` struct, built once in `main.rs`, wrapped in `Arc`,
  and cloned (cheaply — it's just an `Arc` clone) into the router.
- Services are stored behind trait objects (`Arc<dyn TraitName>`) so they can
  be swapped for mocks in tests — never store the concrete struct type where
  the state will be shared across handlers and tested in isolation.

```rust
// bootstrap/state.rs
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: redis::aio::ConnectionManager,
    pub billing_service: Arc<dyn BillingService>,
}
```

Services depend on repository *traits*, not concrete DB clients:

```rust
#[async_trait]
pub trait BillingRepository: Send + Sync {
    async fn find_invoice(&self, id: &str) -> Result<Invoice, BillingError>;
}

pub struct PgBillingRepository {
    pool: PgPool,
}

#[async_trait]
impl BillingRepository for PgBillingRepository {
    async fn find_invoice(&self, id: &str) -> Result<Invoice, BillingError> {
        sqlx::query_as!(Invoice, "SELECT * FROM invoices WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| BillingError::NotFound(id.to_string()))
    }
}
```

---

## Scoped-Access / Multi-Tenancy Pattern

For any service where data is scoped to an organization, account, or tenant,
carry that scope **explicitly** through every layer — handler, service, and
repository signatures all take it as a parameter, never as ambient state.

```rust
// handler
async fn list_items(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,          // extractor: authenticated user
    OrgScope(org_id): OrgScope,        // extractor: Option<String>
    Query(query): Query<ListItemsQuery>,
) -> Result<Json<Vec<ItemResponse>>, ItemError> {
    let items = state.item_service.list(&user.id, org_id, query).await?;
    Ok(Json(items))
}
```

```rust
// service — scope is always an explicit parameter, never inferred from thread state
pub async fn list(
    &self,
    user_id: &str,
    org_id: Option<String>,
    query: ListItemsQuery,
) -> Result<Vec<Item>, ItemError> {
    match org_id {
        Some(org) => self.repo.find_by_org(&org, query).await,
        None => self.repo.find_by_owner(user_id, query).await,
    }
}
```

**Security note — cross-source validation.** Whenever a scope identifier can
arrive from two places at once (e.g. an auth header *and* a URL path
parameter), cross-validate them in middleware and reject on mismatch before
the handler runs. Never trust a single source for tenant identity.

```rust
// middleware/org_guard.rs
//
// A caller could hold a valid token scoped to org A but target a URL path
// scoped to org B. Trusting either source alone would let org A's caller
// read or write org B's data. Both sources must agree.
if let (Some(header_org), Some(path_org)) = (header_org_id, path_org_id) {
    if header_org != path_org {
        return Err(AuthError::Forbidden { reason: "org scope mismatch".into() });
    }
}
```

---

## Authentication & Authorization Pattern

- **Authentication** is a `tower::Layer`/middleware that runs before
  handlers and injects a typed `AuthUser` via request extensions — handlers
  extract it, they never re-parse tokens themselves.
- **Authorization** (permission checks) is a separate layer/extractor from
  authentication — "who are you" and "what can you do" are different
  concerns and should not be conflated into one guard.
- Permissions are `resource:action` string constants defined once in a
  single module, never inlined as magic strings at call sites.

```rust
// auth/constant.rs
pub const PERM_INVOICE_READ: &str = "invoice:read";
pub const PERM_INVOICE_WRITE: &str = "invoice:write";
```

```rust
// middleware/require_permission.rs
pub fn require_permission(
    perm: &'static str,
) -> impl Fn(Request, Next) -> BoxFuture<'static, Result<Response, AuthError>> + Clone {
    // Resolves the caller's permission set and checks membership.
    // See the fail-closed rule below.
}
```

**Rule: authorization fails closed.** If the permission-resolution step
errors, times out, or returns ambiguous data, the request is denied — never
defaulted to allow.

---

## Data Access / Repository Pattern

- Every table/collection has one repository **trait** plus one concrete
  implementation (`sqlx`, `diesel`, or `sea-orm`).
- Repositories return domain structs across their trait boundary — never a
  raw `sqlx::Row` or an ORM-generated row type.
- No business logic in repositories — only queries, mapping, and
  persistence. Anything that decides *what should happen* belongs in the
  service layer.
- Prefer compile-time-checked queries (`sqlx::query_as!` with `DATABASE_URL`
  set at build time) over hand-built SQL strings; when a query must be
  built dynamically, isolate that in a dedicated query-builder helper and
  never string-concatenate user input.

---

## State-Machine / Layered Processing Pattern

For any multi-step, stateful workflow (a conversational flow, a multi-stage
approval, an order pipeline), model the states as an **exhaustive `enum`**
and route through it with a `match` — never with loosely-typed strings or a
chain of `if`s on booleans.

```rust
// model.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Collecting { turn: u8 },
    Confirming,
    BatchCollecting { pending: usize },
}
```

Each state's handling logic lives in its own function/module ("layer"). The
orchestrator only **routes** — it contains no business logic itself:

```rust
// orchestrator.rs
pub async fn handle(&self, session: Session, input: Input) -> Result<Output, FlowError> {
    match session.state {
        SessionState::Idle => self.idle_layer.handle(session, input).await,
        SessionState::Collecting { .. } => self.collecting_layer.handle(session, input).await,
        SessionState::Confirming => self.confirming_layer.handle(session, input).await,
        SessionState::BatchCollecting { .. } => self.batch_layer.handle(session, input).await,
    }
}
```

Rust's exhaustiveness checking is the enforcement mechanism: adding a new
state without a matching arm is a **compile error**, not a runtime surprise.
Never add a catch-all `_ => { ... }` arm to a state-machine match unless it
is a deliberate, documented default — a silent catch-all defeats the whole
point of using an enum here.

---

## Multi-Stage Pipeline Pattern

For any multi-stage async pipeline (an AI/LLM-orchestrated flow, a
plan-then-execute job, a report-generation pipeline), keep each stage as its
own struct with a single responsibility, and stream progress rather than
blocking silently until the whole thing finishes:

- **Plan stage** — produce a validated, typed execution plan. Deserialize
  any external/LLM output into a `#[derive(Deserialize)]` struct
  immediately, then validate it against a schema or a known registry before
  trusting any of it. Treat all externally produced plans as untrusted
  input, the same as any other user-supplied data.
- **Execute stage** — resolve a dependency graph, run independent steps
  concurrently (`tokio::join!` / `futures::future::join_all`), apply a
  resource/time budget, and truncate by priority if the budget is exceeded.
- **Synthesize stage** — consume the accumulated results and produce the
  final output.

Stream progress over Server-Sent Events (`axum::response::sse::{Event, Sse}`)
with one distinct event type per pipeline milestone (e.g. `planning`,
`plan_ready`, `step_start`, `step_complete`, `result`) — never a single
opaque "done" event for an operation the caller had to wait on.

---

## Caching & Key Registry Pattern

Every cache/queue key prefix used by the project is documented once, in one
place, as a table with: prefix, full key format, producer, consumer, and TTL.

```rust
// constant/cache_keys.rs
pub const SESSION_PREFIX: &str = "session:";       // <userId>            — set by SessionService, TTL: session lifetime
pub const RATE_LIMIT_PREFIX: &str = "rate_limit:";  // <scope>:<id>        — set by RateLimiter,   TTL: window-specific
pub const LOCK_PREFIX: &str = "lock:";               // <feature>:<userId> — set by Orchestrator,  TTL: 60s
```

**Security note — key derivation.** When a cache key is derived from a
sensitive value (a raw token, credential, or other secret), hash the *full*
value with SHA-256 — never truncate to a substring or short prefix, since
distinct secrets can share a substring and collide in the cache.

```rust
// Uses the full token hash, not a substring, so that two distinct tokens
// sharing a suffix can never be treated as the same cache entry.
let cache_key = format!("{}{:x}", REVOCATION_PREFIX, Sha256::digest(raw_token.as_bytes()));
```

---

## Logging & Observability

- Use `tracing`, not `println!` or the `log` crate, everywhere.
- Every request gets a span (request id, authenticated user id if present,
  route) attached by middleware, so every log line inside the handler
  inherits that context automatically.
- Log levels: `error!` for unexpected/unrecoverable conditions, `warn!` for
  degraded-but-handled conditions, `info!` for significant business events
  (created, deleted, state transition), `debug!`/`trace!` for diagnostic
  detail. Never `info!` inside a hot loop.
- Never log secrets, tokens, or raw PII — log identifiers (`user_id`), not
  raw sensitive values (`email`, `password`, `token`).

---

## Auto-Discovery / Registry Pattern

When a project needs components to self-register (an API/tool catalogue, a
route index, a plugin list) instead of maintaining a hand-updated list, use
one of:

- An attribute macro that inserts metadata into a static registry at
  compile time (the `inventory` crate), **or**
- A `OnceLock<Mutex<HashMap<...>>>` populated once during application
  bootstrap by iterating a known, explicit set of modules.

Registration metadata (id, description, params) must mirror the actual
component's real behavior exactly. Treat metadata drift as a bug — add a
test or a startup assertion that catches mismatches rather than trusting the
metadata to stay accurate by hand.

---

## Testing Pattern

```
src/<feature>/
└── ...                          # unit tests live beside the code they test
tests/
└── <feature>_integration.rs     # black-box, hits the real router
```

- Unit tests live in the same file as the code under test, in
  `#[cfg(test)] mod tests { use super::*; ... }`, and exercise one
  function's logic in isolation using mocked traits (`mockall`).
- Integration tests live in the crate's top-level `tests/` directory, spin
  up the real `axum::Router` (or equivalent) against a real test database
  (a Postgres container, not a mock), and exercise it over HTTP.
- Every bug fix ships with a regression test in the same change — no
  exceptions.

---

## Documentation Convention

- Every `pub` item in a library crate gets a `///` doc comment describing
  what it does — and, where it's non-obvious, *why*.
- A module-level `//!` doc comment sits at the top of every `mod.rs`,
  describing that module's responsibility in one or two sentences.
- Inline comments explaining a security-relevant check are **mandatory**
  anywhere a check exists purely to prevent an exploit (IDOR, injection,
  timing, privilege escalation). Explain the *attack being prevented*, not
  just what the code does — so a future editor doesn't "simplify" the check
  away without realizing what it protects against.

---

## Concurrency Rules

- Prefer message-passing (`tokio::sync::mpsc`, actor-style tasks) over
  shared mutable state where the two are equally natural — it's easier to
  reason about and to test.
- When shared state is genuinely required, scope the lock as tightly as
  possible: acquire, read/mutate, drop — never hold a `Mutex`/`RwLock` guard
  across an `.await` point.
- Spawn long-running background work with `tokio::spawn` and always retain
  a way to observe its failure (a `JoinHandle` that's checked, or a result
  channel) — never spawn-and-forget silently.
- Any operation with an external dependency (DB, HTTP, cache) gets an
  explicit timeout — unbounded waits on external calls are not acceptable
  in request-serving code paths.

---

## Quick Pre-PR Checklist

- [ ] No `unwrap()` / `expect()` / `panic!()` outside `main.rs` or tests
- [ ] Every fallible function returns `Result<T, E>` with a typed error, not
      `String` or a permanent `Box<dyn Error>`
- [ ] New error variants are handled in the feature's `IntoResponse` impl
- [ ] New handler does parsing + delegation only — no business logic inline
- [ ] New scoped-access endpoint carries `Option<tenant_id>` (or equivalent)
      explicitly through handler → service → repository
- [ ] Any dual-source identifier (header + path, etc.) is cross-validated
      in middleware
- [ ] Authorization failure paths default to deny, not allow
- [ ] New state-machine variant has a match arm everywhere that state is
      consumed (let the compiler surface the rest)
- [ ] New cache/queue key added to the key registry with prefix, format,
      producer, consumer, and TTL
- [ ] Cache keys derived from secrets are hashed in full, never truncated
- [ ] `tracing` spans/logs added for new significant business events; no
      secrets or raw PII logged
- [ ] Unit tests for new service logic; integration test for new endpoint
- [ ] No `Mutex`/`RwLock` guard held across an `.await`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes clean
- [ ] `cargo fmt --check` passes

---

## Anti-Patterns to Avoid

- A single giant `utils.rs` / `helpers.rs` with unrelated functions dumped
  together — split by actual responsibility instead.
- Passing `serde_json::Value` between internal layers instead of typed
  structs — acceptable only at the very edge, when deserializing genuinely
  dynamic external input.
- Business logic living inside `main.rs`, middleware, or repository
  implementations.
- A `Mutex`/`RwLock`-guarded global as a substitute for proper dependency
  injection through `AppState`.
- Catch-all error variants like `Other(String)` used as a dumping ground
  instead of adding a proper typed variant when a new failure mode appears.
- Silently discarding a `Result` (`.ok()`, `let _ = ...`) when the error
  actually matters — every intentionally discarded error gets a
  `// intentionally ignored: <reason>` comment.
- Reaching for `unsafe` to work around a borrow-checker disagreement instead
  of restructuring the ownership — `unsafe` is reserved for genuine FFI/
  performance boundaries, reviewed and commented as such.