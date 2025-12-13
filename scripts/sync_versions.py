#!/usr/bin/env python3
"""
Sync version from workspace Cargo.toml to package files that don't use workspace inheritance.

The workspace Cargo.toml [workspace.package] version is the source of truth.
Workspace crates now use `version.workspace = true`, so this script only syncs to:
- package.json (npm/frontend)
- src-tauri/tauri.conf.json (Tauri app metadata)

Note: src-tauri/Cargo.toml now uses workspace inheritance too, so it's skipped.
"""

import re
import json
import os
import glob


def get_workspace_version():
    """Get version from [workspace.package] section in root Cargo.toml"""
    with open('Cargo.toml', 'r') as f:
        content = f.read()
    
    # Look for version after [workspace.package] section
    match = re.search(
        r'\[workspace\.package\].*?^version\s*=\s*"([^"]+)"',
        content,
        re.MULTILINE | re.DOTALL
    )
    if match:
        return match.group(1)
    
    return None


def check_cargo_toml_needs_sync(path):
    """Check if a Cargo.toml still has a hardcoded version (not using workspace)"""
    with open(path, 'r') as f:
        content = f.read()
    
    # Check if it uses workspace inheritance
    if re.search(r'^version\.workspace\s*=\s*true', content, re.MULTILINE):
        return False
    
    # Check if it has a hardcoded version
    return bool(re.search(r'^version\s*=\s*"[^"]+"', content, re.MULTILINE))


def update_cargo_toml(path, version):
    """Update version in a Cargo.toml [package] section (only if not using workspace)"""
    if not check_cargo_toml_needs_sync(path):
        print(f"  ⊘ {path} (uses workspace.version)")
        return
    
    with open(path, 'r') as f:
        content = f.read()
    
    new_content = re.sub(
        r'(^version\s*=\s*)"[^"]+"',
        f'\\1"{version}"',
        content,
        count=1,
        flags=re.MULTILINE
    )
    
    with open(path, 'w') as f:
        f.write(new_content)
    print(f"  ✓ {path}")


def update_json(path, version):
    """Update version in a JSON file"""
    with open(path, 'r') as f:
        data = json.load(f)
    
    if 'version' in data:
        data['version'] = version
        
    with open(path, 'w') as f:
        json.dump(data, f, indent=2)
        f.write('\n')
    print(f"  ✓ {path}")


def main():
    root_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.chdir(root_dir)
    
    version = get_workspace_version()
    if not version:
        print("❌ Could not find version in [workspace.package]")
        exit(1)
        
    print(f"Syncing version {version} to package files...")
    
    # Check workspace crates (should all use workspace inheritance now)
    crate_tomls = glob.glob('crates/*/Cargo.toml')
    print("\nWorkspace crates:")
    for toml in sorted(crate_tomls):
        update_cargo_toml(toml, version)
    
    # Check src-tauri (excluded from workspace, needs manual sync)
    print("\nTauri crate (excluded from workspace):")
    update_cargo_toml('src-tauri/Cargo.toml', version)
    
    # Sync JSON files (these don't support workspace inheritance)
    print("\nJSON files:")
    update_json('package.json', version)
    update_json('src-tauri/tauri.conf.json', version)
    
    print(f"\n✓ All files synced to version {version}")
    
    print("Done!")


if __name__ == '__main__':
    main()
