# Contributing to Terraline

Thank you for showing an interest in contributing to Terraline! All kinds of contributions are valuable to the project. In this guide, we will cover how you can quickly onboard and make your first contribution.

Terraline is a modified fork of [Plane](https://github.com/makeplane/plane). Upstream code remains subject to its original copyright and the AGPL-3.0-only license.

## Submitting an issue

Before submitting a new issue, please search the [issues](https://github.com/ghifarizuhir/plane-for-itsm/issues) tab. Maybe an issue or discussion already exists and might inform you of workarounds. Otherwise, you can give new information.

While we want to fix all the [issues](https://github.com/ghifarizuhir/plane-for-itsm/issues), before fixing a bug we need to be able to reproduce and confirm it. Please provide us with a minimal reproduction scenario using a repository or [Gist](https://gist.github.com/). Having a live, reproducible scenario gives us the information without asking questions back & forth with additional questions like:

- 3rd-party libraries being used and their versions
- a use-case that fails

Without said minimal reproduction, we won't be able to investigate all [issues](https://github.com/ghifarizuhir/plane-for-itsm/issues), and the issue might not be resolved.

You can open a new issue with this [issue form](https://github.com/ghifarizuhir/plane-for-itsm/issues/new).

### Naming conventions for issues

When opening a new issue, please use a clear and concise title that follows this format:

- For bugs: `🐛 Bug: [short description]`
- For features: `🚀 Feature: [short description]`
- For improvements: `🛠️ Improvement: [short description]`
- For documentation: `📘 Docs: [short description]`

**Examples:**

- `🐛 Bug: API token expiry time not saving correctly`
- `📘 Docs: Clarify RAM requirement for local setup`
- `🚀 Feature: Allow custom time selection for token expiration`

This helps us triage and manage issues more efficiently.

## Projects setup and Architecture

### Requirements

- Docker Engine with Docker Compose
- Node.js >= 22.22
- pnpm 11.10 (enabled through Corepack)
- **Memory**: Minimum **12 GB RAM** recommended
  > ⚠️ Running the project on a system with only 8 GB RAM may lead to setup failures or memory crashes (especially during Docker container build/start or dependency install). Use cloud environments like GitHub Codespaces or upgrade local RAM if possible.

Postgres, Valkey/Redis, and MinIO run inside Docker; you do not need local installations.

### Setup the project

The project is a pnpm + Turbo monorepo. The primary API is Rust (`apps/api-rs`); the Django app (`apps/api`) remains as an opt-in fallback.

1. Clone the repo

```bash
git clone https://github.com/ghifarizuhir/plane-for-itsm.git [folder-name]
cd [folder-name]
chmod +x setup.sh
```

2. Run setup.sh

```bash
./setup.sh
```

3. Start the backend containers

```bash
docker compose -f docker-compose-local.yml up -d --build api worker beat-worker
```

4. Start web apps:

```bash
pnpm dev
```

5. Open your browser to http://localhost:3001/god-mode/ and register yourself as instance admin
6. Open up your browser to http://localhost:3000 then log in using the same credentials from the previous step

Other local endpoints: API at `http://localhost:8000`, live collaboration at `http://localhost:3100`. The web app runs at `http://localhost:3000` and admin at `http://localhost:3001`.

That's it! You're all set to begin coding. Detailed commands and service-specific workflows are in [AGENTS.md](AGENTS.md).

## Missing a Feature?

If a feature is missing, you can directly _request_ a new one [here](https://github.com/ghifarizuhir/plane-for-itsm/issues/new?labels=feature). You can also choose "🚀 Feature" when raising a [New Issue](https://github.com/ghifarizuhir/plane-for-itsm/issues/new/choose) on this repository.
If you would like to _implement_ it, an issue with your proposal must be submitted first, to be sure that we can use it. Please consider the guidelines given below.

## Coding guidelines

To ensure consistency throughout the source code, please keep these rules in mind as you are working:

- All features or bug fixes must be tested by one or more specs (unit-tests).
- We lint with [OxLint](https://oxc.rs/docs/guide/usage/linter) using the shared `.oxlintrc.json` and format with [oxfmt](https://oxc.rs/docs/guide/usage/formatter) using `.oxfmtrc.json`.

## Ways to contribute

- Try Terraline and the self-hosting platform and give feedback
- Add new integrations
- Add or update translations
- Help with open [issues](https://github.com/ghifarizuhir/plane-for-itsm/issues) or [create your own](https://github.com/ghifarizuhir/plane-for-itsm/issues/new/choose)
- Share your thoughts and suggestions
- Help create tutorials and blog posts
- Request a feature by submitting a proposal
- Report a bug
- **Improve documentation** — fix incomplete or missing docs in [`docs/`](docs/README.md), bad wording, examples, or explanations.

## Contributing to language support

This guide is designed to help contributors understand how to add or update translations in the application.

### Understanding translation structure

#### File organization

Translations are organized by language in the locales directory. Each language has its own folder containing JSON files for translations. Here's how it looks:

```
packages/i18n/src/locales/
    ├── en/
    │   ├── core.json       # Critical translations
    │   └── translations.json
    ├── fr/
    │   └── translations.json
    └── [language]/
        └── translations.json
```

#### Nested structure

To keep translations organized, we use a nested structure for keys. This makes it easier to manage and locate specific translations. For example:

```json
{
  "issue": {
    "label": "Work item",
    "title": {
      "label": "Work item title"
    }
  }
}
```

### Translation formatting guide

We use [IntlMessageFormat](https://formatjs.github.io/docs/intl-messageformat/) to handle dynamic content, such as variables and pluralization. Here's how to format your translations:

#### Examples

- **Simple variables**

  ```json
  {
    "greeting": "Hello, {name}!"
  }
  ```

- **Pluralization**
  ```json
  {
    "items": "{count, plural, one {Work item} other {Work items}}"
  }
  ```

### Contributing guidelines

#### Updating existing translations

1. Locate the key in `locales/<language>/translations.json`.

2. Update the value while ensuring the key structure remains intact.
3. Preserve any existing ICU formats (e.g., variables, pluralization).

#### Adding new translation keys

1. When introducing a new key, ensure it is added to **all** language files, even if translations are not immediately available. Use English as a placeholder if needed.

2. Keep the nesting structure consistent across all languages.

3. If the new key requires dynamic content (e.g., variables or pluralization), ensure the ICU format is applied uniformly across all languages.

### Adding new languages

Adding a new language involves several steps to ensure it integrates seamlessly with the project. Follow these instructions carefully:

1.  **Update type definitions**
    Add the new language to the TLanguage type in the language definitions file:

```ts
// packages/i18n/src/types/language.ts
export type TLanguage = "en" | "fr" | "your-lang";
```

1.  **Add language configuration**
    Include the new language in the list of supported languages:

```ts
// packages/i18n/src/constants/language.ts
export const SUPPORTED_LANGUAGES: ILanguageOption[] = [
  { label: "English", value: "en" },
  { label: "Your Language", value: "your-lang" },
];
```

2.  **Create translation files**
    1. Create a new folder for your language under locales (e.g., `locales/your-lang/`).

    2. Add a `translations.json` file inside the folder.

    3. Copy the structure from an existing translation file and translate all keys.

3.  **Update import logic**
    Modify the language import logic to include your new language:

```ts
      private importLanguageFile(language: TLanguage): Promise<any> {
      switch (language) {
          case "your-lang":
          return import("../locales/your-lang/translations.json");
          // ...
      }
      }
```

### Quality checklist

Before submitting your contribution, please ensure the following:

- All translation keys exist in every language file.
- Nested structures match across all language files.
- ICU message formats are correctly implemented.
- All languages load without errors in the application.
- Dynamic values and pluralization work as expected.
- There are no missing or untranslated keys.

#### Pro tips

- When in doubt, refer to the English translations for context.
- Verify pluralization works with different numbers.
- Ensure dynamic values (e.g., `{name}`) are correctly interpolated.
- Double-check that nested key access paths are accurate.

Happy translating! 🌍✨

## License

Terraline is licensed under the GNU Affero General Public License v3.0 only (`AGPL-3.0-only`). By contributing, you agree that your contribution is licensed under the same terms. You retain copyright over your contribution. See [LICENSE.txt](LICENSE.txt) and [NOTICE](NOTICE).

New source files authored for Terraline should carry:

```text
Copyright (c) 2026-present Terraline contributors
SPDX-License-Identifier: AGPL-3.0-only
See the LICENSE file for details.
```

When you substantially modify an upstream Plane file, keep the existing upstream
copyright header and add a `Copyright (c) 2026-present Terraline contributors`
line. Do not remove upstream notices. `COPYRIGHT.txt` remains the upstream
header template used by the copyright-check tooling.

## Need help? Questions and suggestions

Questions, suggestions, and thoughts are most welcome. Open an issue in this repository or read [AGENTS.md](AGENTS.md) for the development workflow.
