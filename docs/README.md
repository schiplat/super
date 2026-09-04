# Project Super — Documentation

Source for the public documentation site:

| | |
| :--- | :--- |
| **Live site** | https://super.docs.sconts.com/ |
| **Source** | `docs/` in this repository |
| **Static site generator** | [Hugo](https://gohugo.io/) (extended) + [Hextra](https://imfing.github.io/hextra/) theme |
| **Docs content language** | English (homepage has zh-cn / ja / es / ru translations) |

## What's in this directory

| Path | Purpose |
| --- | --- |
| `content/docs/` | All documentation pages (Markdown, ordered by numbered sections) |
| `content/go/` | Redirect pages (e.g. `/go/pro/` purchase funnel) |
| `content/legal/` | Legal pages (license, security) |
| `hugo.yaml` | Site configuration (baseURL, params, menu, feedback block) |
| `layouts/` | Template overrides & custom partials (e.g. `_partials/custom/feedback.html`) |
| `assets/` / `static/` | Styles and static assets |
| `themes/hextra/` | Hextra theme (git submodule) |

### Docs sections

| Section | Contents |
| --- | --- |
| `01-getting-started/` | Installation, quick start |
| `02-essentials/` | Core concepts: configuration, scheduling, processes, health |
| `03-orchestration/` | Lifecycle, hooks, system events, readiness-aware reload |
| `04-production-scenarios/` | Migrations, delivery, extensibility, observability, stability |
| `05-advanced-management/` | Licensed plugin features (auth, RBAC, audit, isolation, Dashboard/`ui`, notifications) |
| `06-internals/` | CLI / API / config references, environment variables, changelog |
| `07-editions/` | Edition feature matrix |
| `08-changelog/` | Changelog |
| `09-development/` | Building from source and writing super-core extensions |

## Prerequisites

- Hugo **extended** `0.163.x+` (`brew install hugo`)
- Hextra theme submodule checked out

```sh
git submodule update --init --recursive
```

## Preview locally

From the repository root:

```sh
make docs-serve
# or, inside this directory:
hugo server -D --disableFastRender
```

Open http://localhost:1313/ (homepage translations under `/zh-cn/`, `/ja/`, `/es/`, `/ru/`).

> [!NOTE]
> `hugo.yaml` → `baseURL` is the **production** URL. CI overrides it when publishing.
> Do not open `public/*.html` directly via `file://` — absolute asset paths will 404.

## Build & check

```sh
hugo --quiet                     # build site into public/
hugo --quiet --minify            # production-style build
```

Offline link check (optional, uses [lychee](https://github.com/lycheeverse/lychee)):

```sh
hugo --quiet && lychee --offline --root-dir public public/
```

## Edit the docs

- Add pages as Markdown under `content/docs/<section>/` with `weight:` front matter to control sidebar order.
- Match the existing tone and structure of the section you're editing.
- Internal links should be root-relative paths (e.g. `[config reference](/docs/06-internals/config-reference/)`).
- Use `{{< callout >}}`, `{{< tabs >}}` and other Hextra shortcodes where appropriate.
- When a change touches user-visible behavior, update the relevant page **and** the [changelog](https://github.com/schiplat/super/blob/master/docs/content/docs/08-changelog/_index.md).

### Docs feedback block

The "Was this page helpful?" block at the bottom of every docs page is configured in `hugo.yaml` under `params.ui.feedback` (Docsy-style). Set `enable: false` to remove it, or adjust `text`, `thanks`, `issuePrompt`, `issueRepo`, `sourceURL`. The template lives in `layouts/_partials/custom/feedback.html`.

## Publishing

`super/docs/` is built and deployed to GitHub Pages by [`.github/workflows/deploy-docs.yml`](https://github.com/schiplat/super/blob/master/.github/workflows/deploy-docs.yml) on push to `master`. You do not need to commit `public/`.
