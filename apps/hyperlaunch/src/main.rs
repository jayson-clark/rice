#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use regex::Regex;
use serde::Serialize;
use std::{fs, path::PathBuf, process::Command};
use walkdir::WalkDir;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ignore::WalkBuilder;
use std::io::{BufRead, BufReader};
use tauri::Manager;

#[derive(Serialize, Clone, Debug)]
struct SearchResult {
    result_type: String, // "app", "file", "directory", "content"
    name: String,
    path: String, // full path for files/dirs, exec command for apps
    icon_data: Option<String>,
    context: Option<String>, // for content matches: line with matched text
    line_number: Option<usize>, // for content matches
    score: f64, // for sorting
}

#[derive(Serialize, Clone)]
struct AppEntry {
    name: String,
    exec: String,
    icon: Option<String>,
    icon_data: Option<String>, // base64 encoded icon data
    source: String, // "desktop" or "path"
}

fn desktop_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/share/applications"),
        dirs::data_dir()
            .unwrap_or(PathBuf::from("~/.local/share"))
            .join("applications"),
    ]
}

fn resolve_icon_path(icon_name: &str) -> Option<String> {
    if icon_name.starts_with('/') && std::path::Path::new(icon_name).exists() {
        return Some(icon_name.to_string());
    }
    
    // Common icon directories and sizes to check
    let icon_dirs = [
        "/usr/share/icons/hicolor/48x48/apps",
        "/usr/share/icons/hicolor/32x32/apps", 
        "/usr/share/icons/hicolor/64x64/apps",
        "/usr/share/pixmaps",
        "/usr/share/icons/Adwaita/48x48/apps",
    ];
    
    let extensions = ["png", "svg", "xpm"];
    
    for dir in &icon_dirs {
        for ext in &extensions {
            let path = format!("{}/{}.{}", dir, icon_name, ext);
            if std::path::Path::new(&path).exists() {
                return Some(path);
            }
        }
    }
    
    None
}

fn icon_to_data_url(path: &str) -> Option<String> {
    if let Ok(data) = fs::read(path) {
        let mime_type = if path.ends_with(".png") {
            "image/png"
        } else if path.ends_with(".svg") {
            "image/svg+xml"
        } else if path.ends_with(".xpm") {
            "image/x-xpixmap"
        } else {
            "image/png" // default
        };
        
        let encoded = BASE64.encode(&data);
        Some(format!("data:{};base64,{}", mime_type, encoded))
    } else {
        None
    }
}

fn parse_desktop_file(p: &PathBuf) -> Option<AppEntry> {
    let content = fs::read_to_string(p).ok()?;
    if !content.contains("[Desktop Entry]") { return None; }
    // very light parse
    let mut name = None::<String>;
    let mut exec = None::<String>;
    let mut icon_path = None::<String>;
    let mut nodisplay = false;

    for line in content.lines() {
        let l = line.trim();
        if l.starts_with("NoDisplay=") && l.ends_with("true") { nodisplay = true; }
        if l.starts_with("Name=") && name.is_none() { name = Some(l[5..].to_string()); }
        if l.starts_with("Exec=") && exec.is_none() { exec = Some(l[5..].to_string()); }
        if l.starts_with("Icon=") && icon_path.is_none() { 
            let icon_name = l[5..].to_string();
            icon_path = resolve_icon_path(&icon_name);
        }
    }
    if nodisplay { return None; }
    let name = name?;
    let mut exec = exec?;

    // strip desktop placeholders like %U %F etc.
    let re = Regex::new(r"%[fFuUdDnNickvm]").unwrap();
    exec = re.replace_all(&exec, "").to_string();
    exec = exec.trim().to_string();

    let icon_data = icon_path.as_ref().and_then(|p| icon_to_data_url(p));

    Some(AppEntry {
        name,
        exec,
        icon: icon_path,
        icon_data,
        source: "desktop".into(),
    })
}

fn collect_desktop_entries() -> Vec<AppEntry> {
    let mut out = Vec::new();
    for d in desktop_dirs() {
        if !d.exists() { continue; }
        for entry in WalkDir::new(d).min_depth(1).max_depth(2) {
            if let Ok(e) = entry {
                let p = e.path().to_path_buf();
                if p.extension().and_then(|s| s.to_str()) == Some("desktop") {
                    if let Some(app) = parse_desktop_file(&p) {
                        out.push(app);
                    }
                }
            }
        }
    }
    out
}

// Kept for potential future use
#[allow(dead_code)]
fn collect_path_bins() -> Vec<AppEntry> {
    let mut out = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if let Ok(rd) = fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_file() {
                        if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                            // heuristic: ignore manpages etc.
                            if name.len() > 1 && !name.contains('.') {
                                out.push(AppEntry{
                                    name: name.to_string(),
                                    exec: name.to_string(),
                                    icon: None,
                                    icon_data: None,
                                    source: "path".into(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    // dedup by name
    out.sort_by(|a,b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a,b| a.name.eq_ignore_ascii_case(&b.name));
    out
}

fn fuzzy_score(hay: &str, needle: &str) -> f64 {
    let hay = hay.to_lowercase();
    let needle = needle.to_lowercase();
    
    // Exact match bonus
    if hay == needle {
        return 1000.0;
    }
    
    // Prefix match bonus
    if hay.starts_with(&needle) {
        return 500.0;
    }
    
    // Word boundary match bonus
    if hay.split(|c: char| !c.is_alphanumeric()).any(|word| word.starts_with(&needle)) {
        return 250.0;
    }
    
    // Fuzzy subsequence matching with contiguous bonus
    let mut i = 0;
    let mut j = 0;
    let mut hits = 0;
    let mut cont = 0;
    let mut best_cont = 0;
    let hay_chars: Vec<char> = hay.chars().collect();
    let needle_chars: Vec<char> = needle.chars().collect();
    
    while i < hay_chars.len() && j < needle_chars.len() {
        if hay_chars[i] == needle_chars[j] {
            hits += 1;
            cont += 1;
            best_cont = best_cont.max(cont);
            j += 1;
        } else {
            cont = 0;
        }
        i += 1;
    }
    
    if j == needle_chars.len() {
        (hits as f64) + (best_cont as f64 * 1.5)
    } else {
        -1.0
    }
}

fn search_files_by_name(query: &str, max_results: usize) -> Vec<SearchResult> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let mut results = Vec::new();
    let mut count = 0;
    
    let walker = WalkBuilder::new(&home)
        .hidden(false)
        .git_ignore(true)
        .max_depth(Some(4)) // Reduced from 6 to 4 for speed
        .filter_entry(|e| {
            // Exclude common heavy directories
            if let Some(name) = e.file_name().to_str() {
                !matches!(name, 
                    "node_modules" | ".cargo" | "target" | "build" | "dist" | 
                    ".npm" | ".cache" | "__pycache__" | ".venv" | "venv" |
                    ".git" | ".gradle" | ".m2" | ".ivy2" | "pkg" |
                    "vendor" | "deps" | "Pods" | ".tox" | ".pytest_cache"
                )
            } else {
                true
            }
        })
        .build();
    
    for entry in walker.filter_map(|e| e.ok()) {
        count += 1;
        // Limit iterations for speed
        if count > 5000 {
            break;
        }
        
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let score = fuzzy_score(name, query);
            if score >= 0.0 {
                let result_type = if path.is_dir() { "directory" } else { "file" };
                results.push(SearchResult {
                    result_type: result_type.to_string(),
                    name: name.to_string(),
                    path: path.to_string_lossy().to_string(),
                    icon_data: None,
                    context: None,
                    line_number: None,
                    score,
                });
                
                // Early exit if we have enough good results
                if results.len() >= max_results * 2 {
                    break;
                }
            }
        }
    }
    
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    results.truncate(max_results);
    results
}

fn search_file_contents(query: &str, max_results: usize) -> Vec<SearchResult> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();
    let mut files_checked = 0;
    
    let walker = WalkBuilder::new(&home)
        .hidden(false)
        .git_ignore(true)
        .max_depth(Some(3)) // Reduced from 5 to 3 for speed
        .filter_entry(|e| {
            // Exclude common heavy directories
            if let Some(name) = e.file_name().to_str() {
                !matches!(name, 
                    "node_modules" | ".cargo" | "target" | "build" | "dist" | 
                    ".npm" | ".cache" | "__pycache__" | ".venv" | "venv" |
                    ".git" | ".gradle" | ".m2" | ".ivy2" | "pkg" |
                    "vendor" | "deps" | "Pods" | ".tox" | ".pytest_cache"
                )
            } else {
                true
            }
        })
        .build();
    
    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        
        // Only search text files
        if !path.is_file() {
            continue;
        }
        
        files_checked += 1;
        // Limit number of files to check for speed
        if files_checked > 2000 {
            break;
        }
        
        // Skip large files
        if let Ok(meta) = path.metadata() {
            if meta.len() > 500_000 { // 500KB limit
                continue;
            }
        }
        
        // Try to read as text
        if let Ok(file) = fs::File::open(path) {
            let reader = BufReader::new(file);
            
            for (line_num, line_result) in reader.lines().enumerate().take(500) {
                if let Ok(line) = line_result {
                    if line.to_lowercase().contains(&query_lower) {
                        // Create context with the matched line
                        let context = if line.len() > 100 {
                            format!("{}...", &line[..100])
                        } else {
                            line.clone()
                        };
                        
                        results.push(SearchResult {
                            result_type: "content".to_string(),
                            name: path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            path: path.to_string_lossy().to_string(),
                            icon_data: None,
                            context: Some(context),
                            line_number: Some(line_num + 1),
                            score: 10.0,
                        });
                        
                        break; // Only show first match per file
                    }
                }
            }
            
            if results.len() >= max_results {
                break;
            }
        }
    }
    
    results.truncate(max_results);
    results
}

#[tauri::command]
fn unified_search(query: String) -> Vec<SearchResult> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    
    let mut all_results = Vec::new();
    
    // 1. Search apps (in memory, very fast)
    let apps = collect_desktop_entries();
    for app in apps {
        let name_score = fuzzy_score(&app.name, &query);
        let exec_score = fuzzy_score(&app.exec, &query);
        let score = name_score.max(exec_score);
        
        if score >= 0.0 {
            all_results.push(SearchResult {
                result_type: "app".to_string(),
                name: app.name.clone(),
                path: app.exec.clone(),
                icon_data: app.icon_data.clone(),
                context: Some(app.exec.clone()),
                line_number: None,
                score: score * 100.0, // Massive boost for apps (was 10.0)
            });
        }
    }
    
    // 2. Search files by name (reduced from 30 to 20)
    let file_results = search_files_by_name(&query, 20);
    for mut result in file_results {
        // Heavily penalize config directories
        let penalty = if result.path.contains("/.config/") || 
                         result.path.contains("/.local/") ||
                         result.path.contains("/.cache/") {
            0.1 // 10x penalty for config dirs
        } else {
            1.0
        };
        
        result.score *= 5.0 * penalty; // Boost file name matches but apply penalty
        all_results.push(result);
    }
    
    // 3. Search file contents (only if query is 4+ chars, reduced from 3)
    if query.len() >= 4 {
        let content_results = search_file_contents(&query, 15); // Reduced from 20 to 15
        all_results.extend(content_results);
    }
    
    // Sort by score
    all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    
    // Limit total results (reduced from 100 to 50)
    all_results.truncate(50);
    
    all_results
}

#[tauri::command]
fn list_apps() -> Vec<AppEntry> {
    // Only return desktop entries, no PATH binaries
    collect_desktop_entries()
}

#[tauri::command]
fn launch(cmdline: String, result_type: String) -> Result<(), String> {
    match result_type.as_str() {
        "app" => {
            // Launch app via sh -c
            Command::new("sh")
                .arg("-c")
                .arg(cmdline)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "file" | "content" => {
            // Open file with xdg-open
            Command::new("xdg-open")
                .arg(&cmdline)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "directory" => {
            // Open directory in file manager
            Command::new("xdg-open")
                .arg(&cmdline)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        _ => {
            return Err(format!("Unknown result type: {}", result_type));
        }
    }
    Ok(())
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn ensure_focus(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_focus();
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|_app| {
            // Devtools disabled - was causing focus issues on startup
            // #[cfg(debug_assertions)]
            // if let Some(w) = _app.get_webview_window("main") {
            //     let _ = w.open_devtools();
            // }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_apps, launch, exit_app, unified_search, ensure_focus])
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
