import re
import json
import os

def get_main_version():
    with open('Cargo.toml', 'r') as f:
        content = f.read()
        match = re.search(r'^version\s*=\s*"([^"]+)"', content, re.MULTILINE)
        if match:
            return match.group(1)
    return None

def update_cargo_toml(path, version):
    with open(path, 'r') as f:
        content = f.read()
    
    # Replace version under [package]
    # We need to be careful not to replace dependency versions
    # Usually [package] is at the top.
    
    # A simple regex that looks for version = "..." inside the file, 
    # assuming the package version is the first one or explicitly under [package]
    # But Cargo.toml can have dependencies with version keys.
    # However, the package version is usually `version = "x.y.z"` at the top level indentation.
    # Dependencies are usually `{ version = ... }` or `dep = "..."`
    
    new_content = re.sub(r'(^version\s*=\s*)"[^"]+"', f'\\1"{version}"', content, count=1, flags=re.MULTILINE)
    
    with open(path, 'w') as f:
        f.write(new_content)
    print(f"Updated {path} to version {version}")

def update_json(path, version):
    with open(path, 'r') as f:
        data = json.load(f)
    
    if 'version' in data:
        data['version'] = version
        
    with open(path, 'w') as f:
        json.dump(data, f, indent=2)
        # Add a newline at the end to be nice
        f.write('\n')
    print(f"Updated {path} to version {version}")

def main():
    root_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.chdir(root_dir)
    
    version = get_main_version()
    if not version:
        print("Could not find version in Cargo.toml")
        exit(1)
        
    print(f"Syncing version {version} from Cargo.toml...")
    
    # Update src-tauri/Cargo.toml
    update_cargo_toml('src-tauri/Cargo.toml', version)
    
    # Update package.json
    update_json('package.json', version)
    
    # Update src-tauri/tauri.conf.json
    update_json('src-tauri/tauri.conf.json', version)
    
    print("Done!")

if __name__ == '__main__':
    main()
