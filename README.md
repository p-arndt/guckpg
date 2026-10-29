<div align="center">

<img src="assets/guckpg-icon-transparent.png" alt="guckpg icon" width="180" />

# guckpg

**Your PostgreSQL database, in view.**

_guck_ is German for “look” — _pg_ is short for PostgreSQL.

Browse schemas, write queries, edit data and explore relationships — all from one desktop app.

[![CI](https://github.com/p-arndt/guckpg/actions/workflows/ci.yml/badge.svg)](https://github.com/p-arndt/guckpg/actions/workflows/ci.yml)
[![Release](https://github.com/p-arndt/guckpg/actions/workflows/release.yml/badge.svg)](https://github.com/p-arndt/guckpg/actions/workflows/release.yml)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app/)
[![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white)](https://svelte.dev/)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-4169E1?logo=postgresql&logoColor=white)](https://www.postgresql.org/)

[Install](#install) · [Features](#features) · [AI assistant](#ai-assistant) · [Development](#development)

</div>

---

## Features

|                           |                                                                                               |
| ------------------------- | --------------------------------------------------------------------------------------------- |
| **Connections**           | Connect directly or through an SSH tunnel, with optional TLS and read-only connections.       |
| **Object browser**        | Browse schemas and inspect tables, views, routines, triggers and sequences.                   |
| **SQL and query builder** | Write SQL or build SELECT queries visually, with joins, filters, aggregates and sorting.      |
| **Data editing**          | Inspect and edit results in a grid or a row form, and save staged changes.                    |
| **Table designer**        | Edit columns, constraints and indexes, then preview the SQL before applying it.               |
| **ER diagrams**           | Explore table relationships and foreign keys visually.                                        |
| **AI assistant**          | Connect Ollama, OpenAI or an OpenAI-compatible endpoint and choose its database access level. |

The query builder can save a visual query as a PostgreSQL view after reviewing the SQL. Supported existing views can also be opened in the builder, including common joins, filters, aggregates and HAVING conditions; unsupported SQL opens in the
editor.

## Install

Download for **macOS** (Apple Silicon / Intel) or **Windows** from [GitHub Releases](https://github.com/p-arndt/guckpg/releases).

Use **Check for updates** in the app to install a newer release. Save your work first: installing an update restarts the app. Versions released before the updater was added must be replaced manually once.

> [!NOTE]
> Windows builds are unsigned. macOS builds use an ad-hoc signature and are not notarized, so your operating system may warn when opening them.

## Database tools

Open a table to import or export CSV (UTF-8, up to 16 MiB). Import maps the header to column names and inserts rows. Export includes the entire table or view.

Use the table filter to search a column across all rows, and the row form to choose values for single-column foreign keys in the same schema. The picker shows up to 100 values; manual entry remains available. The SQL editor shows catalog matches on hover.

**Generate test data** previews an INSERT for 1–1,000 rows before applying it. It generates common types for required columns without defaults; unsupported required types, constraints, existing values, or triggers may prevent insertion.

The connection menu offers schema comparison, session monitoring, and backup/restore. Schema comparison reports relation kinds and column properties; it does not compare constraints, indexes, or object definitions. Backups use local `pg_dump` and `pg_restore`, require direct connectivity, and use custom-format archives. Restore replaces matching objects and requires confirmation; SSH connections are not supported.

In a query tab, **Explain** displays the estimated PostgreSQL plan for the selection or current statement without running the query. Session visibility depends on PostgreSQL permissions.

## AI assistant

Configure a model with function calling under **AI → Settings**:

| Provider          | Base URL                    | API                 |
| ----------------- | --------------------------- | ------------------- |
| Ollama            | `http://localhost:11434`    | `/api/chat`         |
| OpenAI            | `https://api.openai.com/v1` | `/responses`        |
| OpenAI-compatible | Your provider's base URL    | `/chat/completions` |

Choose **Schema only**, **Read data**, **Approve every action**, or **Read & write** in the AI panel. Writes require approval; read-only connections block them. Read data can run SQL automatically in a read-only transaction, but database functions may have external side effects.

Credentials are stored in the OS keychain. Prompts, inspected schema and tool results are sent to the configured provider.

## Development

Requires Node.js 24, pnpm 12, Rust 1.90+ and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
pnpm install --frozen-lockfile
pnpm tauri:dev                  # run the desktop app
just ci                         # checks, lint, tests and build
pnpm tauri build                # build desktop bundles
```

For a local PostgreSQL database with sample data:

```sh
docker compose up -d postgres
docker compose exec -T postgres psql -U guckpg -d guckpg < dev/demo.sql
```

Optional datasets and validation scripts are in [`dev/`](dev/). Database tests skip themselves without a server; `just test-it` starts the compose services and runs all of them.

## Releasing

```sh
just ci
just note
just release-dry
just release patch              # or minor / major
```

Run from a clean `main` branch with an `origin` remote and GitHub authentication. Stamp commits and pushes the release tag. CI and installer builds run in parallel; once both pass, the workflow publishes the release with the installers attached. Record user-facing changes with `just note fixed "…"`; the next release puts them into `CHANGELOG.md` and the release notes.

Release builds require the Tauri updater signing key and password in the GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Keep a secure backup of both and never commit the private key. Installed versions trust the public key in the Tauri configuration, so replacing or losing the private key prevents them from installing future updates. Local builds do not need the signing key.
