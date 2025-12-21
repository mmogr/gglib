#!/bin/zsh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_NAME="GGLib GUI.app"
APP_PATH="$SCRIPT_DIR/$APP_NAME"
APPLICATIONS_DIR="/Applications"

echo "🔧 GGLib GUI Installer"
echo "======================"
echo ""

# Check if app exists
if [[ ! -d "$APP_PATH" ]]; then
    echo "❌ Error: Could not find '$APP_NAME' in the same directory as this script."
    echo "   Expected location: $APP_PATH"
    exit 1
fi

# Remove quarantine attribute
echo "📦 Removing macOS quarantine attribute..."
xattr -cr "$APP_PATH"
echo "✅ Quarantine attribute removed."
echo ""

# Ask about moving to Applications
echo -n "Would you like to move the app to /Applications? (y/n): "
read -r response

if [[ "$response" =~ ^[Yy]$ ]]; then
    DEST_PATH="$APPLICATIONS_DIR/$APP_NAME"
    
    # Check if already exists in Applications
    if [[ -d "$DEST_PATH" ]]; then
        echo -n "⚠️  '$APP_NAME' already exists in /Applications. Overwrite? (y/n): "
        read -r overwrite
        
        if [[ "$overwrite" =~ ^[Yy]$ ]]; then
            echo "🗑️  Removing existing installation..."
            rm -rf "$DEST_PATH"
        else
            echo "❌ Installation cancelled."
            exit 0
        fi
    fi
    
    echo "📁 Moving to /Applications..."
    mv "$APP_PATH" "$DEST_PATH"
    echo "✅ Installed successfully!"
    echo ""
    echo "🚀 You can now launch GGLib GUI from your Applications folder or Spotlight."
else
    echo ""
    echo "✅ Done! You can run the app from: $APP_PATH"
fi
