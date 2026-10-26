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
