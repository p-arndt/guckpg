<div align="center">

<img src="assets/guckpg-icon-transparent.png" alt="guckpg icon" width="180" />

# guckpg

**Your PostgreSQL database, in view.**

Browse schemas, write queries, edit data and explore relationships — all from one desktop app.

[![CI](https://github.com/p-arndt/guckpg/actions/workflows/ci.yml/badge.svg)](https://github.com/p-arndt/guckpg/actions/workflows/ci.yml)
[![Release](https://github.com/p-arndt/guckpg/actions/workflows/release.yml/badge.svg)](https://github.com/p-arndt/guckpg/actions/workflows/release.yml)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app/)
[![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white)](https://svelte.dev/)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-4169E1?logo=postgresql&logoColor=white)](https://www.postgresql.org/)

[Install](#get-started) · [Features](#what-you-get) · [Database objects](#database-objects) · [AI assistant](#ai-assistant) · [Development](#development)

</div>

---

## What you get

|                           |                                                                                               |
| ------------------------- | --------------------------------------------------------------------------------------------- |
| **Connections**           | Connect directly or through an SSH tunnel, with optional TLS and read-only connections.       |
| **Object browser**        | Browse schemas and inspect tables, views, routines, triggers and sequences.                   |
| **SQL and query builder** | Write SQL or build SELECT queries visually, with joins, filters, aggregates and sorting.      |
| **Data editing**          | Inspect and edit results in a grid or a row form, and save staged changes.                    |
| **Table designer**        | Edit columns, constraints and indexes, then preview the SQL before applying it.               |
| **ER diagrams**           | Explore table relationships and foreign keys visually.                                        |
| **AI assistant**          | Connect Ollama, OpenAI or an OpenAI-compatible endpoint and choose its database access level. |

## Get started

Download an installer from [GitHub Releases](https://github.com/p-arndt/guckpg/releases) for macOS (Apple Silicon or Intel) or Windows. Releases are not signed or notarized, so your operating system may warn when opening them.

Create a connection with your PostgreSQL host, port, database and credentials. Configure TLS or an SSH tunnel if your server requires one, then connect and explore the schemas in the sidebar. Open a query tab to run SQL or use the visual query builder.

## Database objects

In a query tab, select **Query builder** to construct a SELECT visually. Click or
drag tables onto the canvas; tables that share a foreign key join automatically,
and dragging a column onto a column of another table adds a join. Tick columns to
select them, then set aggregates, aliases, conditions, sorting and a limit in the
side panel while the SQL updates live. **Run** (⌘↵) runs it into the results
below, **Edit SQL** appends it to the editor. Changing databases starts a new
builder.

Use **Row form** above any result grid, the row context menu, or **Shift+Enter**
to open all fields of a row in a large form. Previous/Next navigates the loaded
rows in their displayed order. Applying or navigating stages edits in the grid;
use the grid's **Save** to write them. Computed columns and read-only results
remain read-only. New-row fields distinguish database defaults, NULL and empty
text; JSON formatting preserves the original numeric tokens.

Expand a schema to browse tables, views, materialized views, functions,
procedures, triggers and sequences. Routine signatures distinguish overloads;
trigger labels include their table.

Use **Open definition** on a view or materialized view, or open a routine,
trigger or sequence, to inspect its SQL in a new query tab. Nothing executes
until you run it. Materialized views and sequences use creation scripts rather
than replacement scripts; review the generated notes before editing an existing
object. These definitions are not a full schema backup.

**Refresh materialized view** rebuilds stored results and can block readers
while it runs. This action is unavailable for read-only connections.

### Table designer

Open a table and select **Structure**, or choose **Design table** from its
context menu. **New table…** in the schema menu opens the same designer for a
new table. Edit columns, types, nullability, defaults, identity, comments,
primary/foreign/unique/check constraints, and ordinary indexes with the forms.
Changes stay pending until **Preview SQL** followed by **Apply**. Dropping or
altering existing objects requires confirmation. Switching tabs preserves the
draft; closing a dirty tab asks before discarding it.

Changes run in one transaction, with a two-second lock timeout and a fresh
structure comparison to reject stale drafts. No automatic `CASCADE` is used.
Read-only connections and unsupported structures remain inspectable. Partitioned
or inherited tables, generated-column editing, advanced constraints and complex
indexes require manual SQL. Type changes rely on PostgreSQL's implicit casts;
custom conversions need SQL. Default/check expressions reject SQL comments,
backslashes and dollar quotes.

## AI assistant

Open **AI → Settings**, choose a provider, enter its base URL and model, and save:

| Provider             | Base URL example             | Protocol            |
| -------------------- | ---------------------------- | ------------------- |
| Ollama               | `http://localhost:11434`     | Native `/api/chat`  |
| OpenAI Responses API | `https://api.openai.com/v1`  | `/responses`        |
| OpenAI-compatible    | Your provider's API base URL | `/chat/completions` |

Use a model that supports function calling. API keys and optional Basic Auth credentials are stored in the OS keychain, bound to the provider and endpoint. Chat stays in memory; prompts, tool results and inspected schema are sent to the configured endpoint.

Choose an access level in the AI panel: **Schema only**, **Read data**, **Approve every action**, or **Read & write**. Read data can run SQL automatically in a read-only transaction; database functions may still have external side effects. Approve every action asks before database access and changes. Read & write requires approval for each proposed change. Read-only connections do not permit AI writes. Use database permissions appropriate for the access you grant.

## Development

You need Node.js 24, pnpm 12, Rust 1.90 or newer, and the [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. Docker is optional for the local PostgreSQL and SSH test services.

```sh
pnpm install --frozen-lockfile
pnpm tauri:dev
```

On macOS, `tauri:dev` signs the debug executable if a code-signing identity named `guckpg Local Development Code Signing` exists. Keychain access may still require approval. Without that identity, the development build runs unsigned.

Run the full local checks with:

```sh
just ci
```

This runs frontend type checking, linting, tests and a build, followed by Rust formatting, Clippy and tests. To build desktop bundles locally, run `pnpm tauri build`.

### Sample data

Start the local database with `docker compose up -d postgres`. The SQL scripts in `dev/` target `public`: run `demo.sql`, optionally
`demo-performance.sql` for a million orders and three million line items, then
`demo-erp.sql` for 38 related tables. Feed them to the local container with
`docker compose exec -T postgres psql -U guckpg -d guckpg < dev/demo.sql`.
Each seed has a matching `*-check.sql`; `demo-performance-queries.sql` contains
sample performance queries. `migrate-demo-to-public.sql` moves older demo-schema
data transactionally and rejects name collisions rather than replacing objects.

### AI database tests

A dedicated AI database test is ignored by default. Point `GUCKPG_AI_TEST_PORT` at a **disposable localhost** PostgreSQL instance containing an empty `guckpg_ai_test` database, then run:

```sh
cargo test ai:: -- --ignored
```

## Releasing

Run `just ci` before releasing. Record release notes with `just note`, preview with `just release-dry`, and run `just release patch` (or `minor` / `major`) from a clean `main` branch. Stamp updates the frontend, Cargo and Tauri versions, refreshes `Cargo.lock`, commits and pushes the release tag. An `origin` remote and GitHub authentication must be configured first.

The tag runs CI and builds Windows installers and macOS bundles for Apple Silicon and Intel. GitHub creates a draft release with the stamp notes; review the completed builds before publishing it.
