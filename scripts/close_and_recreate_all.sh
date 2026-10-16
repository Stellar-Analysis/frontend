#!/bin/bash

# Resolve the replacement prerequisite before any issue mutation.
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)" || exit 1
RECREATE_SCRIPT="$SCRIPT_DIR/create_all_70_detailed.py"

if [[ ! -f "$RECREATE_SCRIPT" || ! -r "$RECREATE_SCRIPT" ]]; then
    printf 'Cannot close issues: replacement generator is missing or unreadable: %s\n' "$RECREATE_SCRIPT" >&2
    exit 1
fi

# Keep relative commands anchored to the checkout containing this wrapper.
cd -- "$SCRIPT_DIR/.." || exit 1

echo "=========================================="
echo "CLOSING ALL LAZY ISSUES"
echo "=========================================="
echo ""

# Close all existing issues
echo "Fetching all open issues..."
ISSUES=$(gh issue list --state open --limit 100 --json number,title | jq -r '.[] | select(.title | startswith("[Backend]") or startswith("[SDK]") or startswith("[Mobile]")) | .number')

COUNT=$(echo "$ISSUES" | wc -l)
echo "Found $COUNT issues to close"
echo ""

for issue in $ISSUES; do
    echo "Closing issue #$issue..."
    gh issue close $issue --comment "Closing lazy issue - will be replaced with properly detailed version" 2>/dev/null
    sleep 0.5
done

echo ""
echo "✅ Closed all lazy issues"
echo ""
echo "=========================================="
echo "NOW CREATE PROPER DETAILED ISSUES"
echo "=========================================="
echo ""
echo "Run: python3 scripts/create_all_70_detailed.py"
echo ""
