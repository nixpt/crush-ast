#!/usr/bin/env bash
# scripts/bump-version.sh — bump the crush-ast version on merge to main.
# Installed by squadron/bin/bump-version --install (from squadron; edit there
# and re-run --install to update, not here — this copy will be overwritten).
#
# Bump type inferred from conventional-commit messages since the last v* tag:
#   BREAKING CHANGE / `feat!:` / `fix!:`   → major
#   feat:                                    → minor
#   fix: / perf: / refactor: / revert: / *   → patch  (default)
#
# Version surface auto-detected: [workspace.package] or [package] version in
# root Cargo.toml, else a plain top-level VERSION file. See squadron/bin/bump-version
# --help for the full doc.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DRY_RUN="${BUMPVER_DRY_RUN:-0}"
GIT_USER="${BUMPVER_GIT_USER:-crush-ast-release}"
GIT_EMAIL="${BUMPVER_GIT_EMAIL:-release@crush-ast.local}"

log() { printf 'bump-version: %s\n' "$*" >&2; }

# Auto-detect main vs master unless BUMPVER_MAIN_BRANCH pins one explicitly —
# several fleet repos still default to master (workspace-meta, jokersquad, …).
CUR_BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo HEAD)"
if [ -n "${BUMPVER_MAIN_BRANCH:-}" ]; then
  MAIN_BRANCH="$BUMPVER_MAIN_BRANCH"
  if [ "$CUR_BRANCH" != "$MAIN_BRANCH" ] && [ "${GITHUB_ACTIONS:-}" != "true" ]; then
    log "on '$CUR_BRANCH', not '$MAIN_BRANCH' — skipping (set BUMPVER_MAIN_BRANCH to override)."
    exit 0
  fi
elif [ "$CUR_BRANCH" != "main" ] && [ "$CUR_BRANCH" != "master" ]; then
  log "on '$CUR_BRANCH', not main or master — skipping (set BUMPVER_MAIN_BRANCH to force)."
  exit 0
else
  MAIN_BRANCH="$CUR_BRANCH"
fi

LAST_TAG="$(git describe --tags --match 'v[0-9]*' --abbrev=0 HEAD 2>/dev/null || true)"
if [ -z "$LAST_TAG" ]; then
  log "no prior v* tag found — nothing to bump (initial release). Tag v0.1.0 by hand."
  exit 0
fi

SUBJECTS="$(git log --pretty=format:'%s%n%b' "${LAST_TAG}..HEAD" 2>/dev/null || true)"
bump="patch"
if printf '%s' "$SUBJECTS" | grep -qiE '(^|[[:space:]])BREAKING[[:space:]]CHANGE|^[a-z]+(\([^)]*\))?!:'; then
  bump="major"
elif printf '%s' "$SUBJECTS" | grep -qiE '^feat(\([^)]*\))?:'; then
  bump="minor"
fi

# Package names of this workspace's own members: the root [package] plus each
# [workspace] member dir's [package] name (a "." member is the root itself).
# Scopes the Cargo.lock text rewrite so it never bumps a path dependency that
# coincidentally shares the version — path deps also carry no `source =` line.
workspace_member_names() {
  awk '/^\[package\]/{f=1;next}/^\[/{f=0}f&&/^name[[:space:]]*=/{v=$0;sub(/^[^=]*=[[:space:]]*"?/,"",v);sub(/".*/,"",v);gsub(/[[:space:]]/,"",v);print v;exit}' Cargo.toml
  awk '/^\[workspace\]/{f=1;next}/^\[/{f=0}f&&/^members[[:space:]]*=/{line=$0;sub(/^[^=]*=[[:space:]]*/,"",line);gsub(/[][",]/," ",line);print line}' Cargo.toml \
    | tr ' ' '\n' \
    | while IFS= read -r d; do
        [ -n "$d" ] || continue
        [ "$d" = "." ] && continue
        [ -f "$d/Cargo.toml" ] || continue
        awk '/^\[package\]/{f=1;next}/^\[/{f=0}f&&/^name[[:space:]]*=/{v=$0;sub(/^[^=]*=[[:space:]]*"?/,"",v);sub(/".*/,"",v);gsub(/[[:space:]]/,"",v);print v;exit}' "$d/Cargo.toml"
      done
}

read_version() {
  if grep -q '^\[workspace\.package\]' Cargo.toml 2>/dev/null; then
    awk '/^\[workspace\.package\]/{f=1;next}/^\[/{f=0}f&&/^[[:space:]]*version[[:space:]]*=/{v=$0;sub(/^[^=]*=[[:space:]]*"?/,"",v);sub(/".*/,"",v);gsub(/[[:space:]]/,"",v);print v;exit}' Cargo.toml
  elif [ -f Cargo.toml ] && grep -q '^\[package\]' Cargo.toml 2>/dev/null; then
    awk '/^\[package\]/{f=1;next}/^\[/{f=0}f&&/^[[:space:]]*version[[:space:]]*=/{v=$0;sub(/^[^=]*=[[:space:]]*"?/,"",v);sub(/".*/,"",v);gsub(/[[:space:]]/,"",v);print v;exit}' Cargo.toml
  elif [ -f VERSION ]; then
    tr -d '[:space:]' < VERSION
  fi
}
write_version() {
  local nextver="$1"
  # NOTE: the awk -v variable is deliberately NOT named "next" — gawk treats
  # `next` as a reserved builtin (the loop-control statement used right
  # below), and `awk -v next=...` fails fatally ("cannot use gawk builtin
  # `next` as variable name") rather than just shadowing it. That failure
  # then went undetected upstream because `awk ... > file.tmp && mv file.tmp
  # file` still "succeeds" on a fatal-but-nonzero-exit awk with empty
  # output — `mv` doesn't check what it's moving, so Cargo.toml would have
  # been silently truncated to empty. Caught by actually running this
  # end-to-end (not just bash -n) during the s408 generalization pass.
  if [ -f Cargo.toml ] && grep -qE '^\[(workspace\.package|package)\]' Cargo.toml 2>/dev/null; then
    # Bump EVERY literal version= line under [workspace.package] AND/OR
    # [package] in one pass — a manifest that is both a workspace and its own
    # package (e.g. zorro: members = [".", "xtask"]) has two independent
    # literal version surfaces; bumping only the first-matched section
    # silently leaves the other stale (zorro's real d40a732 finding).
    awk -v nextver="$nextver" '/^\[workspace\.package\]/{ws=1;pk=0;print;next}/^\[package\]/{pk=1;ws=0;print;next}/^\[/{ws=0;pk=0;print;next}(ws||pk)&&/^[[:space:]]*version[[:space:]]*=/{print "version = \"" nextver "\"";next}{print}' Cargo.toml > Cargo.toml.tmp
    [ -s Cargo.toml.tmp ] || { log "write_version: awk produced empty output, refusing to overwrite Cargo.toml"; rm -f Cargo.toml.tmp; exit 1; }
    mv Cargo.toml.tmp Cargo.toml
  elif [ -f VERSION ]; then
    printf '%s\n' "$nextver" > VERSION
  fi
}

CUR="$(read_version)"
if [ -z "$CUR" ]; then
  log "no version surface found (no [workspace.package]/[package] version in Cargo.toml, no VERSION file)"
  exit 1
fi

IFS='.' read -r MA MI PA <<<"$CUR"
case "$bump" in
  major) MA=$((MA+1)); MI=0;  PA=0  ;;
  minor) MI=$((MI+1)); PA=0  ;;
  patch) PA=$((PA+1)) ;;
esac
NEXT="${MA}.${MI}.${PA}"

if [ "$NEXT" = "$CUR" ]; then
  log "next == current ($CUR); nothing to bump."
  exit 0
fi

log "last tag=$LAST_TAG  current=$CUR  bump=$bump  next=$NEXT"

if [ "$DRY_RUN" = "1" ]; then
  log "DRY_RUN=1 — would commit + tag v${NEXT}."
  exit 0
fi

write_version "$NEXT"

if [ -f Cargo.toml ] && command -v cargo >/dev/null 2>&1; then
  log "refreshing Cargo.lock (cargo update --workspace)…"
  # update -w rewrites only the workspace members' own lock entries — metadata,
  # no compilation. A full `cargo check` needed the whole dep tree to BUILD on
  # the CI runner and failed fast there (swallowed by ||), shipping stale locks.
  cargo update --workspace >/dev/null 2>&1 || true
  # Path-dep repos (e.g. zorro's optional ../zpu) make EVERY cargo command fail
  # on a bare CI runner (siblings not checked out) — so the lock may still
  # carry $CUR after the `cargo update` above. Fall back to a text rewrite
  # that bumps ONLY this workspace's own members, keyed by name: path deps also
  # have no `source =` line and can coincidentally share $CUR (flownet has
  # cezanne/contextgc at 0.1.1 next to flownet at 0.1.1), so a source-less-only
  # match would over-bump them. The old guard `! grep "^version = \"$NEXT\""`
  # also false-positived on unrelated registry deps already at $NEXT
  # (foldhash/matchers at 0.2.0), skipping the rewrite and shipping a stale
  # lock (flownet v0.2.0 shipped Cargo.lock still pinned at 0.1.1).
  if [ -f Cargo.lock ]; then
    members="$(workspace_member_names)"
    if [ -n "$members" ]; then
      awk -v cur="$CUR" -v nxt="$NEXT" -v members="$members" '
        BEGIN { n=split(members, m, "\n"); for (i=1;i<=n;i++) if (m[i]!="") want[m[i]]=1 }
        /^\[\[package\]\]/ { for (i=0;i<nb;i++) print buf[i]; nb=0; inpkg=1; member=0 }
        inpkg { buf[nb++]=$0
                if ($0 ~ /^name = /) { nm=$0; sub(/^name = "/,"",nm); sub(/".*/,"",nm); member=(nm in want) }
                if ($0=="") { for (i=0;i<nb;i++) { l=buf[i]
                    if (member && l=="version = \"" cur "\"") l="version = \"" nxt "\""
                    print l } ; nb=0; inpkg=0; member=0 }
                next }
        { print }
        END { for (i=0;i<nb;i++) { l=buf[i]
                if (member && l=="version = \"" cur "\"") l="version = \"" nxt "\""
                print l } }
      ' Cargo.lock > Cargo.lock.tmp && mv Cargo.lock.tmp Cargo.lock
      log "Cargo.lock: text rewrite $CUR → $NEXT for workspace members (no-op if cargo update already bumped them)"
    fi
  fi
fi

# Mechanical CHANGELOG.md entry (commit subjects since last tag) — only if the
# repo already has one; never creates one. Ported from the direct-bump path
# (the template originally lacked it: v0.19.1/v0.19.2 on zorro shipped without
# changelog lines — s408 fix).
if [ -f CHANGELOG.md ]; then
  ENTRY_FILE="$(mktemp)"
  {
    printf '## [%s] - %s\n\n' "$NEXT" "$(date -u +%Y-%m-%d)"
    git log --pretty=format:'- %s' "${LAST_TAG}..HEAD" 2>/dev/null | grep -vE '^- chore\(release\):' || true
    printf '\n\n'
  } > "$ENTRY_FILE"
  if grep -qiE '^##[[:space:]]*\[unreleased\]' CHANGELOG.md; then
    awk -v entryfile="$ENTRY_FILE" '
      BEGIN { while ((getline line < entryfile) > 0) entry = entry line "\n" }
      { print }
      tolower($0) ~ /^##[[:space:]]*\[unreleased\]/ && !done { print ""; printf "%s", entry; done=1 }
    ' CHANGELOG.md > CHANGELOG.md.tmp
  else
    awk -v entryfile="$ENTRY_FILE" '
      BEGIN { while ((getline line < entryfile) > 0) entry = entry line "\n" }
      !done && /^##[[:space:]]/ { printf "%s", entry; done=1 }
      { print }
      END { if (!done) printf "%s", entry }
    ' CHANGELOG.md > CHANGELOG.md.tmp
  fi
  [ -s CHANGELOG.md.tmp ] && mv CHANGELOG.md.tmp CHANGELOG.md || rm -f CHANGELOG.md.tmp
  rm -f "$ENTRY_FILE"
  log "appended mechanical CHANGELOG.md entry for v${NEXT}"
fi

git config user.name  "$GIT_USER"
git config user.email "$GIT_EMAIL"
# One `git add` per path, not one call with all of them — `git add a b c`
# fails ATOMICALLY (stages nothing at all, not even the paths that DO
# exist) if any single pathspec doesn't match, so a missing Cargo.lock
# (e.g. cargo check failed above, or this repo has no Cargo.toml at all
# and only uses VERSION) would otherwise silently skip staging Cargo.toml
# too, then `git commit` fails with "nothing to commit" and `set -e`
# kills the script with no tag, no push, and no visible error under the
# `>/dev/null` on the commit line. Reproduced + fixed s408.
# Skip a path that IS gitignored (e.g. a library crate that deliberately
# doesn't track Cargo.lock) instead of letting `git add` hard-fail on it —
# `git add` refuses an ignored file without `-f` and exits nonzero, which
# under `set -e` kills the whole release with no tag, no push (reproduced on
# openko-network/fabric, which gitignores Cargo.lock; fixed here + the
# direct-bump path below).
for f in Cargo.toml Cargo.lock VERSION CHANGELOG.md; do
  [ -f "$f" ] && ! git check-ignore -q "$f" && git add "$f"
done
git commit -m "chore(release): v${NEXT} [skip ci]" >/dev/null
git tag "v${NEXT}"
log "tagged v${NEXT}"

if git remote get-url origin >/dev/null 2>&1; then
  git push origin "HEAD:${MAIN_BRANCH}" "v${NEXT}"
  log "pushed ${MAIN_BRANCH} + v${NEXT} to origin"
  # A pushed tag is not a GitHub Release — the Releases page / `gh release
  # list` only shows actual Release objects. Pre-1.0, stay tag-only on
  # purpose (captain: "cut a release once it hits 1.0, don't need release
  # for every tag, but tag is important" — matches checkstand's own state:
  # 13 tags, zero Releases, by design). Once a repo crosses 1.0, every bump
  # gets a real Release. Non-fatal: a missing `gh` or an already-existing
  # release (re-run, race) must not fail the bump — the tag+push already
  # succeeded and is the source of truth either way.
  if [ "$MA" -ge 1 ] 2>/dev/null; then
    if command -v gh >/dev/null 2>&1; then
      gh release create "v${NEXT}" --title "v${NEXT}" --generate-notes --target "${MAIN_BRANCH}" \
        && log "created GitHub Release v${NEXT}" \
        || log "gh release create failed (non-fatal) — tag+push already succeeded"
    else
      log "gh CLI not available — tag pushed, no GitHub Release object created"
    fi
  else
    log "v${NEXT} is pre-1.0 — tag only, no GitHub Release (cut one by hand once this repo hits 1.0)"
  fi
else
  log "no 'origin' remote — commit+tag are local only."
fi
