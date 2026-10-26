# guckpg

A desktop PostgreSQL client built with Tauri and Svelte.

## Releases

Run `just ci` before releasing. Record release notes with `just note`, preview
with `just release-dry`, and run `just release patch` (or `minor` / `major`)
from a clean `main` branch. Stamp updates the frontend, Cargo and Tauri versions,
refreshes Cargo.lock, commits and pushes the release tag. An `origin` remote and
GitHub authentication must be configured first.

The tag runs CI and builds Windows installers and macOS bundles for Apple Silicon
and Intel on macOS 26. GitHub creates a draft release with the stamp notes; review
the completed builds before publishing it. The builds are not code signed or
notarized, so operating systems may warn when opening them.

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

### Local sample data

The SQL scripts in `dev/` target `public`: run `demo.sql`, optionally
`demo-performance.sql` for a million orders and three million line items, then
`demo-erp.sql` for 38 related tables. Feed them to the local container with
`docker compose exec -T postgres psql -U guckpg -d guckpg < dev/demo.sql`.
Each seed has a matching `*-check.sql`; `demo-performance-queries.sql` contains
sample performance queries. `migrate-demo-to-public.sql` moves older demo-schema
data transactionally and rejects name collisions rather than replacing objects.

## AI assistant

Open **AI → Settings**, choose a provider, enter its base URL and model, and save:

| Provider             | Base URL example             | Protocol            |
| -------------------- | ---------------------------- | ------------------- |
| Ollama               | `http://localhost:11434`     | Native `/api/chat`  |
| OpenAI Responses API | `https://api.openai.com/v1`  | `/responses`        |
| OpenAI-compatible    | Your provider's API base URL | `/chat/completions` |

Use a model that supports function calling. API keys are stored in the OS keychain,
and **Authentication → Basic Auth** supports endpoints behind an authenticated
proxy, including Ollama. Enter username and password once; both are stored in the
keychain separately from bearer tokens. Leave both fields blank to keep saved
credentials, or use the deletion checkbox. Choose **None** to send no authorization
header. Authentication credentials are
bound to the provider and endpoint; chat remains in memory. Prompts and inspected
schema are sent to the configured endpoint. Enable **Allow read-only queries and
row samples** to let the assistant filter, join, group, sort, and aggregate data
with COUNT/SUM/AVG/MIN/MAX. It submits a structured query plan: the backend builds
the SELECT, binds filter parameters, quotes identifiers and checks table access.
Executed queries and their parameters appear in the tool details.

Reads use isolated read-only transactions, a 5-second statement timeout, bounded
results (100 rows, 32 columns, 64 KiB output), and a small allowlist of built-in
types, operators and aggregates. Views, RLS tables, system schemas, custom types,
custom access methods and expression/partial indexes are rejected to avoid
indirect code execution. Inherited rows are excluded with `ONLY`. Sample reads
use the same relation/type checks. These restrictions cannot protect against a
compromised database server; use a dedicated SELECT-only DB user for an additional
permission boundary. Read-only queries can still scan large tables until timeout.

Switching databases, changing settings, or clearing chat revokes data access.
Suggested free-form SQL opens in a new editor tab for manual review; the assistant
cannot execute arbitrary SQL, call arbitrary functions or modify data.

The Rust adapters handle protocol-specific tool calls; OpenAI uses stateless
Responses requests with reasoning items preserved during each tool loop. Requests
are cancellable and bounded by time, response size, and tool-call count. This first
version displays complete replies rather than streaming tokens.

Run frontend checks with `pnpm check`, `pnpm test`, `pnpm lint`, and `pnpm build`;
Rust checks with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
and `cargo test`. A dedicated AI database test is ignored by default: point
`GUCKPG_AI_TEST_PORT` at a **disposable localhost** Postgres containing an empty
`guckpg_ai_test` database and run
`cargo test ai:: -- --ignored`.

## Desktop development

Run `pnpm tauri:dev` to start the desktop app. On macOS, if a code-signing
identity named `guckpg Local Development Code Signing` exists, this command signs
the debug executable before launch. Existing keychain items may still require
access approval.
Without that identity, the development build runs without this signing step.

## Svelte development

Everything you need to build a Svelte project, powered by [`sv`](https://github.com/sveltejs/cli).

## Creating a project

If you're seeing this, you've probably already done this step. Congrats!

```sh
# create a new project
npx sv create my-app
```

To recreate this project with the same configuration:

```sh
# recreate this project
pnpm dlx sv@1.1.1 create --template minimal --types ts --add prettier eslint vitest="usages:unit" tailwindcss="plugins:none" sveltekit-adapter="adapter:static" experimental="features:none" --install pnpm .
```

## Adding features

Add features to your project with `sv add`:

```sh
npx sv add
```

For example, to add Tailwind CSS:

```sh
npx sv add tailwindcss
```

## Developing

Once you've created a project and installed dependencies with `npm install` (or `pnpm install` or `yarn`), start a development server:

```sh
npm run dev

# or start the server and open the app in a new browser tab
npm run dev -- --open
```

## Building

To create a production version of your app:

```sh
npm run build
```

You can preview the production build with `npm run preview`.
