# Summary

<!-- What does this change do, in one or two sentences? -->

## Motivation

<!-- Why is this change needed? Link any related issue. -->

## Testing

- [ ] Build is clean (no new compiler warnings)
- [ ] Tests pass (`xcodebuild ... test`)
- [ ] lizard is clean (`cd app && uv tool run lizard Kadrell -w` prints nothing)

<!-- Note anything you could not test and why. -->

## Screenshots

<!-- For UI changes, add before/after screenshots. -->

## Checklist

- [ ] `CHANGELOG.md` updated under `## Unreleased` (for user-visible changes)
- [ ] Version bumped with `tools/bump-version.py` if appropriate
- [ ] New user-facing text localized in both German and English (`Localizable.xcstrings`)
- [ ] No new compiler warnings
- [ ] Base branch is `master`
