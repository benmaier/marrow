#!/bin/bash
#
# Downloads vendored dependencies and updates manifest with checksums.
# Run this script when you need to add or update dependencies.
#
# Usage: ./scripts/update-vendor.sh [--verify]
#   --verify  Only verify checksums, don't download

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
VENDOR_DIR="$PROJECT_DIR/vendor"
MANIFEST="$VENDOR_DIR/manifest.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

verify_only=false
if [[ "$1" == "--verify" ]]; then
    verify_only=true
fi

# Check for required tools
if ! command -v jq &> /dev/null; then
    echo -e "${RED}Error: jq is required but not installed.${NC}"
    echo "Install with: brew install jq"
    exit 1
fi

if [[ ! -f "$MANIFEST" ]]; then
    echo -e "${RED}Error: manifest.json not found at $MANIFEST${NC}"
    exit 1
fi

echo "Reading manifest..."

# Extract all files from manifest
files=$(jq -r '.dependencies | to_entries[] | .value.files | to_entries[] | "\(.key)|\(.value.url)|\(.value.sha256)"' "$MANIFEST")

all_passed=true

while IFS='|' read -r filename url expected_sha; do
    filepath="$VENDOR_DIR/$filename"

    if $verify_only; then
        # Verify mode
        if [[ ! -f "$filepath" ]]; then
            echo -e "${RED}MISSING: $filename${NC}"
            all_passed=false
            continue
        fi

        if [[ "$expected_sha" == "null" || -z "$expected_sha" ]]; then
            echo -e "${YELLOW}NO CHECKSUM: $filename (run without --verify to update)${NC}"
            all_passed=false
            continue
        fi

        actual_sha=$(shasum -a 256 "$filepath" | cut -d' ' -f1)
        if [[ "$actual_sha" == "$expected_sha" ]]; then
            echo -e "${GREEN}OK: $filename${NC}"
        else
            echo -e "${RED}MISMATCH: $filename${NC}"
            echo "  Expected: $expected_sha"
            echo "  Actual:   $actual_sha"
            all_passed=false
        fi
    else
        # Download mode
        echo "Downloading $filename..."
        curl -sL "$url" -o "$filepath"

        # Compute checksum
        actual_sha=$(shasum -a 256 "$filepath" | cut -d' ' -f1)
        echo "  SHA256: $actual_sha"

        # Update manifest with checksum
        # This is a bit hacky but works for our simple structure
        tmp=$(mktemp)
        jq --arg file "$filename" --arg sha "$actual_sha" '
            .dependencies |= with_entries(
                .value.files |= with_entries(
                    if .key == $file then .value.sha256 = $sha else . end
                )
            )
        ' "$MANIFEST" > "$tmp" && mv "$tmp" "$MANIFEST"

        echo -e "${GREEN}OK: $filename${NC}"
    fi
done <<< "$files"

if $verify_only; then
    if $all_passed; then
        echo -e "\n${GREEN}All checksums verified.${NC}"
        exit 0
    else
        echo -e "\n${RED}Some checks failed.${NC}"
        exit 1
    fi
else
    echo -e "\n${GREEN}Vendor files updated. Checksums saved to manifest.json${NC}"
fi
