# Contributing to Kadrell

Thanks for taking the time to contribute. This guide covers the build setup, the quality bar every change has to clear, and the commit conventions.

## Before you build a feature

Kadrell has a deliberate scope: keep all of a user's parallel Claude Code sessions visible and usable in one window, for people with many simultaneous sessions across projects. It is keyboard-first, needs no second tool beside it, and follows tmux habits rather than inventing its own. If an idea does not fit that, please open an issue and ask before building it. The positioning is spelled out in `CLAUDE.md`.

## Build setup

Kadrell is a Swift 6 / AppKit app for macOS 15 and newer, on Apple Silicon and Intel.

Requirements:

- macOS 15 or newer
- Xcode 16
- [XcodeGen](https://github.com/yonaskolb/XcodeGen): `brew install xcodegen`

The Xcode project is generated from `app/project.yml`; the `.xcodeproj` is gitignored, so generate it first. The only third-party dependency is [SwiftTerm](https://github.com/migueldeicaza/SwiftTerm), which Swift Package Manager resolves automatically.

```bash
cd app
xcodegen generate
xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug \
  -derivedDataPath build -skipPackagePluginValidation -skipMacroValidation build
open build/Build/Products/Debug/Kadrell.app
```

When testing against a running build, always use a fresh throwaway profile so you never touch your own Kadrell data:

```bash
open -n build/Build/Products/Debug/Kadrell.app --args --profile tmp
```

## Run the tests

Run the test suite before opening a pull request:

```bash
cd app
xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug \
  -derivedDataPath build -skipPackagePluginValidation -skipMacroValidation test
```

## Code quality

Every change has to meet these before it is ready:

- **Clean build.** No new compiler warnings. The only accepted exception is the toolchain note `appintentsmetadataprocessor: Metadata extraction skipped`, which is not from project code.
- **Low complexity.** [lizard](https://github.com/terryyin/lizard) must stay clean, cyclomatic complexity 15 or lower:

  ```bash
  cd app && uv tool run lizard Kadrell -w
  ```

  This must print nothing. When a function grows too branchy, split it into small named functions instead of adding more branches, and reuse the existing services and helpers rather than duplicating logic.

## Bilingual user text

Kadrell is German and English, switchable in settings without a restart. Every user-facing string has to exist in both languages:

- New AppKit user text uses `String(localized: "…", bundle: Bundle.app)`. Without `bundle:` it stays in the start language when switching, and `LocalizationTests` will fail.
- SwiftUI dialogs inherit the language over `\.locale`, so `Text("…")` is enough there.
- Add both German and English in the same change to `app/Kadrell/Localization/Localizable.xcstrings`.

## Commits, changelog and version

- User-visible changes go at the top of `CHANGELOG.md` under `## Unreleased` (create the section if it is missing), newest first. One line per change, written for a normal user, prefixed with `Feature:`, `Fix:`, `Änderung:` (change) or `Entfernt:` (removed). Pure internals (refactors, tests, docs) are not listed.
- Bump the version with `python3 tools/bump-version.py <patch|minor|major>`: `patch` for small bug fixes, `minor` for new features or several noticeable changes, `major` for something large or breaking. The script sets the version in `app/project.yml` and `app/Kadrell/Info.plist` and turns `## Unreleased` into the version heading. Commit those files along with your change.
- Do not add a co-author trailer to commits.

## Pull requests

- Base branch is `master`.
- Keep pull requests focused and describe what changed and why. The pull request template walks through the checklist.
- Confirm the build is clean, the tests pass and lizard is clean before marking a pull request ready.
- Maintainers do the merge after review.

## Reporting bugs and requesting features

Use the issue templates. Bug reports need your macOS version, Kadrell version, Claude Code version and clear steps to reproduce. Feature requests should say what problem you are trying to solve and keep the positioning above in mind.
