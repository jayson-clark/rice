import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

const search = document.getElementById("search");
const list = document.getElementById("list");

if (!search) console.error('search input not found');
if (!list) console.error('list element not found');

let idx = 0;
let isLaunching = false; // prevent double launches
let currentResults = [];
let searchTimeout = null;
let searchId = 0; // Track search requests to ignore stale results
let isInitialized = false;
let keystrokeBuffer = [];

function highlightMatch(text, query) {
  if (!query) return text;
  const regex = new RegExp(`(${query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi');
  return text.replace(regex, '<strong>$1</strong>');
}

function getResultIcon(result) {
  // Return icon style based on result type
  if (result.icon_data) {
    return `background-image: url("${result.icon_data}")`;
  }
  
  // Default icons based on type
  const icons = {
    app: '📱',
    file: '📄',
    directory: '📁',
    content: '🔍'
  };
  
  return ''; // Will show emoji in pseudo-element via CSS
}

function getResultTypeIcon(type) {
  const icons = {
    app: '📱',
    file: '📄',
    directory: '📁',
    content: '🔍'
  };
  return icons[type] || '•';
}

function render(items) {
  const wrap = document.getElementById("wrap");
  const query = search.value.trim();

  // Show/hide results based on search query
  if (items.length > 0 && query) {
    wrap.classList.remove("collapsed");
  } else if (!query) {
    wrap.classList.add("collapsed");
  }

  list.innerHTML = "";
  currentResults = items;
  
  items.slice(0, 100).forEach((it, i) => {
    const li = document.createElement("li");
    li.className = `item item-${it.result_type}` + (i === idx ? " active" : "");
    li.dataset.resultType = it.result_type;
    li.dataset.path = it.path;

    // Create icon element
    const iconEl = document.createElement("div");
    iconEl.className = "icon";
    if (it.icon_data) {
      iconEl.style.backgroundImage = `url("${it.icon_data}")`;
    } else {
      // Use emoji for non-app items
      iconEl.textContent = getResultTypeIcon(it.result_type);
      iconEl.classList.add('emoji-icon');
    }

    const contentWrapper = document.createElement("div");
    contentWrapper.className = "content-wrapper";

    const nameEl = document.createElement("div");
    nameEl.className = "name";
    nameEl.innerHTML = highlightMatch(it.name, query);

    contentWrapper.appendChild(nameEl);

    // Add context/subtitle based on result type
    if (it.result_type === 'content' && it.context) {
      const contextEl = document.createElement("div");
      contextEl.className = "context";
      const linePrefix = it.line_number ? `Line ${it.line_number}: ` : '';
      contextEl.innerHTML = linePrefix + highlightMatch(it.context, query);
      contentWrapper.appendChild(contextEl);
    } else if (it.result_type === 'app' && it.context) {
      const contextEl = document.createElement("div");
      contextEl.className = "context";
      contextEl.textContent = it.context;
      contentWrapper.appendChild(contextEl);
    } else if (it.result_type === 'file' || it.result_type === 'directory') {
      const pathEl = document.createElement("div");
      pathEl.className = "context path";
      // Show shortened path
      const home = it.path.replace(/^\/home\/[^/]+/, '~');
      pathEl.textContent = home;
      contentWrapper.appendChild(pathEl);
    }

    li.appendChild(iconEl);
    li.appendChild(contentWrapper);
    li.onclick = () => launch(it.path, it.result_type);
    list.appendChild(li);
  });
}

async function performSearch(query, currentSearchId) {
  if (!query.trim()) {
    render([]);
    return;
  }
  
  try {
    const results = await invoke("unified_search", { query: query.trim() });
    
    // Only render if this is still the current search (not superseded by a newer one)
    if (currentSearchId === searchId) {
      render(results);
    }
  } catch (e) {
    console.error('unified_search failed', e);
    if (currentSearchId === searchId) {
      render([]);
    }
  }
}

async function launch(path, resultType) {
  if (isLaunching) return; // prevent double launches
  isLaunching = true;

  try {
    await invoke("launch", { cmdline: path, resultType: resultType });
  } catch (e) {
    console.error(e);
  } finally {
    // Use exit_app command to properly close the app
    try {
      await invoke('exit_app');
    } catch (e) {
      console.error('exit_app failed', e);
      // fallback: try window close
      try { const w = getCurrentWindow(); await w.close(); } catch (e2) { console.error('window close failed', e2); }
    }
  }
}

function move(dir) {
  const count = list.children.length;
  if (!count) return;
  idx = (idx + dir + count) % count;
  for (let i = 0; i < count; i++) list.children[i].classList.toggle("active", i === idx);
  list.children[idx].scrollIntoView({ block: "nearest" });
}

async function main() {
  render([]);

  // Capture keystrokes before the input is fully ready
  window.addEventListener('keydown', (e) => {
    if (!isInitialized && e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
      keystrokeBuffer.push(e.key);
    }
  }, true); // Use capture phase to catch early

  search.addEventListener("input", () => {
    idx = 0;
    const query = search.value.trim();
    
    // Increment search ID to invalidate previous searches
    searchId++;
    const currentSearchId = searchId;
    
    // Clear any pending searches
    if (searchTimeout) clearTimeout(searchTimeout);
    
    if (!query) {
      render([]);
      return;
    }
    
    // Longer debounce for more responsive typing
    searchTimeout = setTimeout(() => {
      performSearch(query, currentSearchId);
    }, 250); // 250ms debounce - more time for typing
  });

  // ensure keys work while typing in the input
  search.addEventListener('keydown', async (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === 'Enter') {
      const el = list.querySelector('.item.active');
      if (el && currentResults[idx]) {
        const result = currentResults[idx];
        launch(result.path, result.result_type);
      }
    } else if (e.key === 'Escape') {
      try {
        await invoke('exit_app');
      } catch (err) {
        console.error('escape exit_app failed', err);
        // fallback: try window close
        try { const w = getCurrentWindow(); await w.close(); } catch (e2) { console.error('escape window close failed', e2); }
      }
    }
  });

  window.addEventListener("keydown", async (e) => {
    if (e.key === "Escape") {
      // close the appWindow whether input is focused or not
      try {
        await invoke('exit_app');
      } catch (err) {
        console.error('window escape exit_app failed', err);
        // fallback: try window close
        try { const w = getCurrentWindow(); await w.close(); } catch (e2) { console.error('window escape window close failed', e2); }
      }
    }
    else if (e.key === "Enter") {
      const el = list.querySelector(".item.active");
      if (el && currentResults[idx]) {
        const result = currentResults[idx];
        launch(result.path, result.result_type);
      }
    }
  });
  
  // Close when the window loses focus (user switched apps)
  try {
    const w = getCurrentWindow();
    w.on('tauri://blur', async () => {
      // ignore immediate blur right after startup (devtools focus, etc.)
      if (isLaunching) return;
      try {
        await invoke('exit_app');
      } catch (e) {
        console.error('blur exit_app failed', e);
        // fallback: try window close
        try { await w.close(); } catch (e2) { console.error('blur window close failed', e2); }
      }
    });
  } catch (e) {
    window.addEventListener('blur', async () => {
      if (isLaunching) return;
      try {
        await invoke('exit_app');
      } catch (e) {
        console.error('window blur exit_app failed', e);
        // fallback: try window close
        try { const w = getCurrentWindow(); await w.close(); } catch (e2) { console.error('window blur window close failed', e2); }
      }
    });
  }
  
  // Force focus multiple times to ensure it sticks
  search.focus();
  
  // Ensure window focus at Tauri level
  try {
    await invoke('ensure_focus');
  } catch (e) {
    console.error('ensure_focus failed', e);
  }
  
  // Use requestAnimationFrame to focus after render
  requestAnimationFrame(() => {
    search.focus();
    
    // Apply buffered keystrokes after a short delay
    setTimeout(() => {
      if (keystrokeBuffer.length > 0) {
        search.value = keystrokeBuffer.join('');
        keystrokeBuffer = [];
        // Trigger input event to start search
        search.dispatchEvent(new Event('input'));
      }
      isInitialized = true;
    }, 50); // 50ms should be enough for the window to be ready
  });
}

// start
main();
