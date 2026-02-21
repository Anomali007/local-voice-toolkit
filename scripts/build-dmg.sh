#!/bin/bash
# Build script for Blah³ DMG
# Creates an ad-hoc signed DMG for distribution

set -e

echo "==================================="
echo "  Blah³ DMG Build Script"
echo "==================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$( cd "$SCRIPT_DIR/.." && pwd )"

echo -e "${YELLOW}Project directory:${NC} $PROJECT_DIR"
echo ""

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: Rust/Cargo not found. Install from https://rustup.rs${NC}"
    exit 1
fi

if ! command -v pnpm &> /dev/null; then
    echo -e "${RED}Error: pnpm not found. Install with: npm install -g pnpm${NC}"
    exit 1
fi

if ! command -v cargo-tauri &> /dev/null; then
    echo -e "${YELLOW}Installing Tauri CLI...${NC}"
    cargo install tauri-cli --version "^2"
fi

# Check for espeak-ng (required for TTS)
if ! command -v espeak-ng &> /dev/null; then
    echo -e "${YELLOW}Warning: espeak-ng not found. TTS may not work.${NC}"
    echo "Install with: brew install espeak-ng"
fi

echo -e "${GREEN}Prerequisites OK${NC}"
echo ""

# Navigate to project directory
cd "$PROJECT_DIR"

# Install frontend dependencies
echo "Installing frontend dependencies..."
pnpm install

# Determine signing identity
# Use APPLE_SIGNING_IDENTITY env var if set (e.g. "Developer ID Application: Name (TEAMID)")
# Otherwise fall back to ad-hoc signing ("-")
SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:--}"

if [ "$SIGNING_IDENTITY" = "-" ]; then
    echo -e "${YELLOW}Using ad-hoc code signing (no Apple Developer ID)${NC}"
else
    echo -e "${GREEN}Using signing identity:${NC} $SIGNING_IDENTITY"
fi
echo ""

# Step 1: Build the .app bundle only
echo "Building Blah³ app bundle..."
echo "This may take several minutes on first build."
echo ""

cargo tauri build --bundles app 2>&1 | while IFS= read -r line; do
    # Show progress without overwhelming output
    if [[ "$line" == *"Compiling"* ]] || [[ "$line" == *"Finished"* ]] || [[ "$line" == *"Bundling"* ]]; then
        echo "$line"
    fi
done

# Step 2: Find and sign the .app bundle
APP_PATH=$(find "$PROJECT_DIR/src-tauri/target" -path "*/bundle/macos/*.app" -type d 2>/dev/null | head -1)

if [ -z "$APP_PATH" ]; then
    echo -e "${RED}Error: .app bundle not found after build${NC}"
    exit 1
fi

echo ""
echo "Signing app bundle: $APP_PATH"

# Remove any existing signatures
codesign --remove-signature "$APP_PATH" 2>/dev/null || true

# Sign all nested frameworks and binaries, then the app itself
codesign --force --deep --sign "$SIGNING_IDENTITY" \
    --entitlements "$PROJECT_DIR/src-tauri/entitlements.plist" \
    "$APP_PATH"

# Verify the signature
if codesign --verify --deep --strict "$APP_PATH" 2>/dev/null; then
    echo -e "${GREEN}Code signing successful${NC}"
else
    echo -e "${YELLOW}Warning: Code signing verification failed, continuing anyway${NC}"
fi

# Step 3: Build the DMG from the signed .app
echo ""
echo "Packaging DMG..."

cargo tauri build --bundles dmg 2>&1 | while IFS= read -r line; do
    if [[ "$line" == *"Bundling"* ]] || [[ "$line" == *"Finished"* ]]; then
        echo "$line"
    fi
done

# Find the DMG
DMG_PATH=$(find "$PROJECT_DIR/src-tauri/target" -name "*.dmg" -type f 2>/dev/null | head -1)

if [ -z "$DMG_PATH" ]; then
    echo -e "${RED}Error: DMG not found after build${NC}"
    exit 1
fi

# Get DMG info
DMG_NAME=$(basename "$DMG_PATH")
DMG_SIZE=$(du -h "$DMG_PATH" | cut -f1)

echo ""
echo "==================================="
echo -e "${GREEN}  Build Complete!${NC}"
echo "==================================="
echo ""
echo -e "DMG Location: ${YELLOW}$DMG_PATH${NC}"
echo -e "DMG Size: ${YELLOW}$DMG_SIZE${NC}"
if [ "$SIGNING_IDENTITY" = "-" ]; then
    echo -e "Signing: ${YELLOW}Ad-hoc (users will need to right-click → Open on first launch)${NC}"
else
    echo -e "Signing: ${GREEN}$SIGNING_IDENTITY${NC}"
fi
echo ""
echo "To install:"
echo "  1. Open the DMG file"
echo "  2. Drag Blah³ to Applications"
echo "  3. Right-click the app and select 'Open' (first time only)"
echo ""

# Optionally open the build folder
read -p "Open build folder in Finder? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    open "$(dirname "$DMG_PATH")"
fi
