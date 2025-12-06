#!/usr/bin/env python3
"""
Sync version from workspace Cargo.toml to non-workspace files.

Workspace crates (crates/*) inherit version via `version.workspace = true`,
so they don't need syncing. This script handles:
- src-tauri/Cargo.toml (Tauri binary, not a workspace member)
- package.json (npm/frontend)
- src-tauri/tauri.conf.json (Tauri app metadata)
"""

import re
import json
import os


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


def update_cargo_toml(path, version):
    """Update version in a Cargo.toml [package] section"""
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
        
    print(f"Syncing version {version} to non-workspace files...")
    
    # These files are NOT workspace members and need explicit version sync
    update_cargo_toml('src-tauri/Cargo.toml', version)
    update_json('package.json', version)
    update_json('src-tauri/tauri.conf.json', version)
    
    print("Done!")


if __name__ == '__main__':
    main()
