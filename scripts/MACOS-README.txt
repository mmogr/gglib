╔═══════════════════════════════════════════════════════════════╗
║               GGLib GUI - macOS Installation                  ║
╚═══════════════════════════════════════════════════════════════╝

IMPORTANT: Before running the app, you must run the installer!

macOS blocks apps downloaded from the internet that aren't 
code-signed. The installer script removes this restriction.

─────────────────────────────────────────────────────────────────
OPTION 1: Double-click (Easiest)
─────────────────────────────────────────────────────────────────

1. Double-click "macos-install.command"
2. If prompted "are you sure?", click Open
3. Follow the prompts in Terminal

─────────────────────────────────────────────────────────────────
OPTION 2: Terminal
─────────────────────────────────────────────────────────────────

1. Open Terminal
2. Navigate to this folder:
   cd /path/to/extracted/folder
3. Run the installer:
   ./macos-install.command

─────────────────────────────────────────────────────────────────
WHAT THE INSTALLER DOES
─────────────────────────────────────────────────────────────────

• Removes the macOS quarantine flag from the app
• Optionally moves the app to /Applications

After installation, you can launch GGLib GUI from:
• /Applications (if you chose to install there)
• This folder (if you kept it here)
• Spotlight search (Cmd+Space, type "GGLib")

─────────────────────────────────────────────────────────────────
MANUAL FIX (if installer doesn't work)
─────────────────────────────────────────────────────────────────

Run this command in Terminal:
xattr -cr "GGLib GUI.app"

═══════════════════════════════════════════════════════════════
