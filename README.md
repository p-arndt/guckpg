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

## Install

Download for **macOS** (Apple Silicon / Intel) or **Windows** from [GitHub Releases](https://github.com/p-arndt/guckpg/releases).

> [!NOTE]
> Builds are unsigned and macOS builds are not notarized. Your operating system may warn when opening them.

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

Optional datasets and validation scripts are in [`dev/`](dev/). The AI database test is ignored by default; run `cargo test ai:: -- --ignored` with `GUCKPG_AI_TEST_PORT` pointing to a disposable localhost PostgreSQL instance containing an empty `guckpg_ai_test` database.

## Releasing

```sh
just ci
just note
just release-dry
just release patch              # or minor / major
```

Run from a clean `main` branch with an `origin` remote and GitHub authentication. Stamp commits and pushes the release tag; CI builds the installers and creates a draft release for review.
