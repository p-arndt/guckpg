# sv

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
