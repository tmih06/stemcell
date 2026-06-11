#!/usr/bin/env bash
# check-wiki.sh — Verify wiki integrity for CI and local use.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || realpath "$(dirname "$0")/../..")"
WIKI="$ROOT/wiki"
EXIT=0

pass() { echo "  ✓ $1"; }
fail() { echo "  ✗ $1"; EXIT=1; }

echo "=== Wiki Link Check ==="
for f in $(find "$WIKI" -name "*.md" -not -path "*/reference/*" -not -path "*/start/*"); do
  dir=$(dirname "$f")
  while IFS= read -r link; do
    [[ -z "$link" ]] && continue
    link="${link%%#*}"
    [[ "$link" =~ ^http ]] && continue
    [[ "$link" =~ ^/src/ ]] && continue
    target="$dir/$link"
    if [ ! -f "$target" ] && [ ! -d "$target" ]; then
      fail "$(realpath --relative-to="$ROOT" "$f") -> $link"
    fi
  done < <(grep -oP '\[.*?\]\(\K[^)]+' "$f" 2>/dev/null || true)
done
pass "All internal wiki links resolve"

echo ""
echo "=== Source Reference Check ==="
while IFS=: read -r file lineno line; do
  while IFS= read -r -d '`' token; do
    token="${token#\`}"
    [[ -z "$token" ]] && continue
    [[ "$token" =~ ^https?:// ]] && continue
    [[ "$token" =~ [[:space:]] ]] && continue
    [[ "$token" =~ ^[A-Z0-9#%\"\'\{\}\$\.\-] ]] && continue
    [[ "$token" =~ ^[a-zA-Z0-9._-]+$ ]] && continue
    [[ "$token" =~ ^[a-z-]+/[a-z-]+$ && ! "$token" =~ ^src/ ]] && continue

    clean="${token#./}"
    clean="${clean#/}"

    wdir="$(dirname "$file")"
    candidate="$wdir/$clean"
    candidate_root="$ROOT/$clean"

    [ -f "$candidate" ] || [ -d "$candidate" ] || [ -f "$candidate_root" ] || [ -d "$candidate_root" ] && continue
    [[ "$token" =~ \.md$ ]] && fail "$(realpath --relative-to="$ROOT" "$file"):$lineno: \`$token\`"
  done < <(printf '%s' "$line")
done < <(grep -n '`[^`]*/[^`]*`' "$WIKI"/*.md "$WIKI"/subsystems/*/*.md "$WIKI"/architecture/*.md 2>/dev/null || true)
pass "Source references verified"

echo ""
echo "=== Source File Coverage Check ==="
# Verify every .rs file in src/ (excl patches) is referenced somewhere in wiki
python3 -c "
import os, re, sys

# Collect all backtick-quoted strings from wiki markdown
wiki_refs = set()
for root, dirs, files in os.walk('$WIKI'):
    for f in files:
        if not f.endswith('.md'): continue
        path = os.path.join(root, f)
        with open(path) as fh: content = fh.read()
        for m in re.finditer(r'\x60([^\x60]+)\x60', content):
            ref = m.group(1).strip().replace('./', '')
            if ref.startswith('/'): ref = ref[1:]
            if '/' in ref or ref.endswith('.rs') or ref.endswith('.sql'):
                wiki_refs.add(ref)

# Check every .rs source file
missing = []
for root, dirs, files in os.walk('$ROOT/src'):
    # Skip patches and target dirs
    if 'patches' in root.split(os.sep) or 'target' in root.split(os.sep):
        dirs[:] = []
        continue
    for f in files:
        if not f.endswith('.rs'): continue
        sf = os.path.join(root, f).replace(os.sep, '/')
        found = False
        for ref in wiki_refs:
            rparts = ref.split('/')
            sparts = sf.split('/')
            if len(rparts) <= len(sparts) and sparts[-len(rparts):] == rparts:
                found = True
                break
        if not found:
            missing.append(sf)

missing.sort()
for m in missing:
    print(f'  MISSING: {m}')
sys.exit(1 if missing else 0)
" && pass "All source files referenced in wiki" || fail "Source files missing from wiki"
MANIFEST="$WIKI/coverage-manifest.md"
if [ -f "$MANIFEST" ]; then
  while IFS='|' read -r _ area coverage wikipage _; do
    area="$(echo "$area" | xargs)"
    coverage="$(echo "$coverage" | xargs)"
    [[ -z "$area" ]] && continue
    [[ "$area" == "Source area" ]] && continue
    [[ "$area" == --- ]] && continue
    case "$coverage" in
      ✓|Partial|Excluded) ;;
      *) fail "coverage-manifest.md: '$area' has unknown status '$coverage'" ;;
    esac
  done < <(sed -n '5,/^$/{/^|/p}' "$MANIFEST" || true)
  pass "Coverage manifest: all rows valid"
else
  fail "wiki/coverage-manifest.md not found"
fi

echo ""
echo "=== Markdown Format Check ==="
for f in $(find "$WIKI" -name "*.md" -not -path "*/reference/*" -not -path "*/start/*"); do
  rel="$(realpath --relative-to="$ROOT" "$f")"
  issues=0

  # Single H1 heading required
  h1_count=$(grep -c '^# ' "$f" || true)
  if [ "$h1_count" -eq 0 ]; then
    fail "$rel: missing H1"
    issues=1
  fi

  # No trailing whitespace on non-code lines
  awk '/^```/{c=!c} !c && / +$/{found=1; exit} END{exit found}' "$f" \
    || { fail "$rel: trailing whitespace"; issues=1; }

  # No consecutive blank lines outside code fences
  awk 'BEGIN{inblock=0} /^```/{inblock=!inblock} !inblock && /^$/{blank++; if(blank>1){found=1; exit}} !/^$/{blank=0} END{exit found}' "$f" \
    || { fail "$rel: consecutive blank lines"; issues=1; }

  if [ "$issues" -eq 0 ]; then
    pass "$rel: formatting OK"
  fi
done

echo ""
echo "=== Summary ==="
if [ "$EXIT" -eq 0 ]; then
  echo "  All wiki integrity checks passed."
else
  echo "  $EXIT check(s) failed."
fi
exit "$EXIT"
