#!/bin/sh
# graphify: rebuild the code knowledge graph (graphify-out/) in the background
# after a merge or history rewrite lands new commits on disk.
#
# `graphify hook install` (see app/README.md) sets up post-commit and
# post-checkout hooks natively - those cover the common cases and already do
# the right thing (worktree guard, detached rebuild, its own per-repo lock).
# It does NOT install a post-merge hook, and a `git merge`/`git pull` fires
# post-merge, not post-commit. This script fills that gap. Wire it up by
# appending a call to it from .git/hooks/post-merge (and, if you also want a
# rebuild after `git commit --amend`/`git rebase`, .git/hooks/post-rewrite -
# skipped here since this repo's workflow avoids amend/rebase, see CLAUDE.md).
#
# Install (idempotent, never overwrites an existing hook):
#   for h in post-merge; do
#     hook=".git/hooks/$h"
#     [ -f "$hook" ] || printf '#!/bin/sh\n' > "$hook"
#     grep -q 'tools/graphify-hook.sh' "$hook" || \
#       printf '\n"$(git rev-parse --show-toplevel)"/tools/graphify-hook.sh "$@" &\n' >> "$hook"
#     chmod +x "$hook"
#   done
#
# Never blocks the git command: graphify runs detached, in the background,
# logging to graphify-out/graphify-hook.log. If graphify isn't installed, or
# anything here fails, this script exits 0 silently - it must never fail the
# git operation that triggered it.

# Skip in a linked worktree (.claude/worktrees/*): the canonical graphify-out/
# belongs to the primary checkout only, and many agents work in worktrees in
# parallel here - none of them should trigger a graph rebuild.
_GFY_GITDIR=$(cd "$(git rev-parse --git-dir 2>/dev/null)" 2>/dev/null && pwd)
_GFY_COMMONDIR=$(cd "$(git rev-parse --git-common-dir 2>/dev/null)" 2>/dev/null && pwd)
if [ -z "$_GFY_COMMONDIR" ] || [ "$_GFY_GITDIR" != "$_GFY_COMMONDIR" ]; then
    exit 0
fi

REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$REPO_ROOT" 2>/dev/null || exit 0

# Locate the graphify launcher: prefer the interpreter the skill/CLI already
# pinned for this repo (survives uv-tool reinstalls), else PATH, else the
# common uv-tool bin dir (not always on PATH in a git GUI/hook context).
GRAPHIFY_BIN=$(command -v graphify 2>/dev/null)
if [ -z "$GRAPHIFY_BIN" ] && [ -x "$HOME/.local/bin/graphify" ]; then
    GRAPHIFY_BIN="$HOME/.local/bin/graphify"
fi
[ -z "$GRAPHIFY_BIN" ] && exit 0

OUT_DIR="$REPO_ROOT/graphify-out"
LOG="$OUT_DIR/graphify-hook.log"
LOCK="$OUT_DIR/.graphify-hook.lock"
mkdir -p "$OUT_DIR" 2>/dev/null || exit 0

# Only one background rebuild queued at a time. graphify's own per-repo flock
# (inside `graphify update`) already serializes the actual rebuild work and
# would just make a second launch wait - this extra directory lock avoids
# piling up N waiting background processes when post-merge fires repeatedly
# in a short span (e.g. a scripted sequence of merges).
# ponytail: a lock dir left behind by a killed/crashed run (kill -9, power
# loss) would otherwise wedge every future hook run forever - a plain mkdir
# lock has no owner check. Treat a lock older than 30 min as stale and clear
# it; upgrade to a pid-checked lock if that ever proves too coarse.
if [ -d "$LOCK" ]; then
    _stale=$(find "$LOCK" -maxdepth 0 -mmin +30 2>/dev/null)
    [ -n "$_stale" ] && rmdir "$LOCK" 2>/dev/null
fi
mkdir "$LOCK" 2>/dev/null || exit 0

nohup sh -c '
    "$1" update "$2" >>"$3" 2>&1
    rmdir "$4" 2>/dev/null
' sh "$GRAPHIFY_BIN" "$REPO_ROOT" "$LOG" "$LOCK" >/dev/null 2>&1 &

exit 0
