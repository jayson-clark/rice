#!/usr/bin/env python3
"""
apply_theme.py

Smart theme application script that:
1. Reads theme.json (new values)
2. Reads theme.snapshot.json (old values)
3. Finds all instances of old values in config files
4. Replaces them with new values
5. Updates the snapshot

Usage:
  python3 scripts/apply_theme.py           # Apply changes
  python3 scripts/apply_theme.py --dry-run # Preview changes
  python3 scripts/apply_theme.py --init    # Create initial snapshot from current configs
"""

import json
import sys
import shutil
from pathlib import Path
from datetime import datetime
from typing import Any, Dict, List, Tuple

ROOT = Path(__file__).resolve().parents[1]
THEME_FILE = ROOT / "theme.json"
SNAPSHOT_FILE = ROOT / "theme.snapshot.json"
BACKUP_DIR = ROOT / ".theme_backups"

# Directories to search for theme values
CONFIG_DIRS = [
    ROOT / "waybar",
    ROOT / "hypr",
    ROOT / "qutebrowser",
    ROOT / "kitty",
    ROOT / "apps",
]

# File patterns to include (broad)
INCLUDE_PATTERNS = [
    "*.css", "*.conf", "*.py", "*.rs", "*.toml", "*.json", "*.html", "*.js",
    "*.jsx", "*.tsx", "*.md", "*.yaml", "*.yml", "*.ini", "*.sh", "*.zsh", "*.bash",
    "*.lua", "*.vim", "*.vimrc", "*.properties", "*.xml", "*.plist", "*.scss", "*.sass",
    "*.less", "*.yew", "*.java", "*.kt", "*.gradle", "*.yaml", "*.yml", "*.ps1",
    "*.rs", "*.c", "*.h", "*.cpp", "*.hpp", "*.cs",
]

def log(msg: str, level: str = "INFO"):
    timestamp = datetime.now().strftime("%H:%M:%S")
    print(f"[{timestamp}] {level}: {msg}")

def flatten_dict(d: Dict, parent_key: str = "", sep: str = ".") -> Dict[str, Any]:
    """Flatten nested dict into dotted keys."""
    items = []
    for k, v in d.items():
        new_key = f"{parent_key}{sep}{k}" if parent_key else k
        if isinstance(v, dict):
            items.extend(flatten_dict(v, new_key, sep=sep).items())
        else:
            items.append((new_key, v))
    return dict(items)

def load_theme() -> Dict:
    """Load and flatten theme.json."""
    with THEME_FILE.open() as f:
        theme = json.load(f)
    return flatten_dict(theme)

def load_snapshot() -> Dict:
    """Load theme snapshot (previous state)."""
    if not SNAPSHOT_FILE.exists():
        return {}
    with SNAPSHOT_FILE.open() as f:
        return json.load(f)

def save_snapshot(theme: Dict):
    """Save current theme as snapshot."""
    with SNAPSHOT_FILE.open("w") as f:
        json.dump(theme, f, indent=2)
    log(f"Saved snapshot to {SNAPSHOT_FILE.name}")

def find_config_files() -> List[Path]:
    """Find all config files to process by searching the whole repo, excluding common dirs."""
    files = []
    # Exclude backups, build artifacts, IDE and OS dirs, caches, virtualenvs, package outputs
    excluded = {
        ".theme_backups",
        "target",
        "build",
        "node_modules",
        "dist",
        ".git",
        "configs_out",
        "pkg",
        ".venv",
        "venv",
        ".cache",
        ".gradle",
        ".idea",
        ".vscode",
        "__pycache__",
        "node_modules",
        ".parcel-cache",
        ".next",
        "out",
        "dist-packages",
        "target",
        "build.rs",
        ".DS_Store",
        "CMakeFiles",
        "cmake-build-debug",
        "vendor",
        "venv",
        ".tox",
        ".mypy_cache",
        "coverage",
        "coverage_html_report",
        ".pytest_cache",
    }

    for pattern in INCLUDE_PATTERNS:
        for f in ROOT.rglob(pattern):
            # skip files in excluded directories
            if any(ex in f.parts for ex in excluded):
                continue
            # skip this script and snapshot/backups
            if f.resolve() == Path(__file__).resolve():
                continue
            files.append(f)

    # Deduplicate
    unique_files = list(dict.fromkeys(files))
    return unique_files

def get_changes(old_theme: Dict, new_theme: Dict) -> List[Tuple[str, Any, Any]]:
    """Find what changed between old and new theme."""
    changes = []
    for key, new_value in new_theme.items():
        old_value = old_theme.get(key)
        if old_value != new_value and old_value is not None:
            changes.append((key, old_value, new_value))
    return changes

def backup_file(filepath: Path):
    """Create backup of file before modifying."""
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    backup_path = BACKUP_DIR / timestamp / filepath.relative_to(ROOT)
    backup_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(filepath, backup_path)

def apply_replacements(filepath: Path, changes: List[Tuple[str, Any, Any]], dry_run: bool = False) -> int:
    """Apply theme changes to a file. Returns number of replacements made."""
    try:
        content = filepath.read_text()
        original_content = content
        replacements = 0
        
        for key, old_val, new_val in changes:
            # Convert values to strings for replacement
            old_str = str(old_val)
            new_str = str(new_val)
            
            # Skip if old value not in file
            if old_str not in content:
                continue
            
            # Count occurrences
            count = content.count(old_str)
            if count > 0:
                if not dry_run:
                    content = content.replace(old_str, new_str)
                replacements += count
                log(f"  {filepath.relative_to(ROOT)}: {old_str} → {new_str} ({count}x)", "CHANGE")
        
        # Write changes
        if replacements > 0 and not dry_run:
            backup_file(filepath)
            filepath.write_text(content)
        
        return replacements
    except Exception as e:
        log(f"Error processing {filepath}: {e}", "ERROR")
        return 0

def init_snapshot():
    """Initialize snapshot from current theme.json."""
    log("Initializing snapshot from current theme.json...")
    theme = load_theme()
    save_snapshot(theme)
    log("✓ Snapshot initialized. You can now edit theme.json and run apply_theme.py")

def apply_theme(dry_run: bool = False):
    """Main theme application logic."""
    if not THEME_FILE.exists():
        log("theme.json not found!", "ERROR")
        sys.exit(1)
    
    if not SNAPSHOT_FILE.exists():
        log("No snapshot found. Creating initial snapshot...", "WARN")
        init_snapshot()
        log("No changes to apply (snapshot just created).")
        return
    
    # Load themes
    new_theme = load_theme()
    old_theme = load_snapshot()
    
    # Find changes
    changes = get_changes(old_theme, new_theme)
    
    if not changes:
        log("✓ No theme changes detected.")
        return
    
    log(f"Found {len(changes)} theme value changes:")
    for key, old_val, new_val in changes:
        log(f"  {key}: {old_val} → {new_val}", "CHANGE")
    
    # Find files to process
    files = find_config_files()
    log(f"\nScanning {len(files)} config files...")
    
    if dry_run:
        log("\n=== DRY RUN MODE ===", "WARN")
    
    # Apply changes
    total_replacements = 0
    for filepath in files:
        count = apply_replacements(filepath, changes, dry_run)
        total_replacements += count
    
    # Summary
    log(f"\n{'Would replace' if dry_run else 'Replaced'} {total_replacements} occurrence(s) across {len(files)} files")
    
    if not dry_run and total_replacements > 0:
        save_snapshot(new_theme)
        log(f"✓ Theme applied successfully!")
        log(f"  Backups saved to: {BACKUP_DIR}")
    elif dry_run and total_replacements > 0:
        log("\nRe-run without --dry-run to apply changes.")

def main():
    args = sys.argv[1:]
    
    if "--init" in args:
        init_snapshot()
    elif "--dry-run" in args:
        apply_theme(dry_run=True)
    elif "--help" in args or "-h" in args:
        print(__doc__)
    else:
        apply_theme(dry_run=False)

if __name__ == "__main__":
    main()
