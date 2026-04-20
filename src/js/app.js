// app.js — Photyx application init, wiring, panel manager, status bar, menu

'use strict';

// ── Status Bar ───────────────────────────────────────────────────────────────

const StatusBar = (() => {
    let barEl, iconEl, textEl;
    let clearTimer = null;
    const notifications = [];

    function init() {
        barEl  = document.getElementById('status-bar');
        iconEl = document.getElementById('status-icon');
        textEl = document.getElementById('status-text');

        barEl.addEventListener('click', () => NotifHistory.toggle());
    }

    const TYPE_META = {
        idle:    { icon: '◈', cls: '' },
        info:    { icon: '◉', cls: 'status-info' },
        success: { icon: '✓', cls: 'status-success' },
        warning: { icon: '⚠', cls: 'status-warning' },
        error:   { icon: '✕', cls: 'status-error' },
    };

    function set(msg, type = 'info', autoClear = 8000) {
        const meta = TYPE_META[type] || TYPE_META.info;
        barEl.className = meta.cls ? `${meta.cls}` : '';
        barEl.style.cssText = '';   // remove inline
        if (iconEl) iconEl.textContent = meta.icon;
        if (textEl) textEl.textContent = msg;

        // Record
        notifications.unshift({ msg, type, time: new Date() });
        if (notifications.length > 200) notifications.length = 200;
        NotifHistory.refresh();

        if (clearTimer) clearTimeout(clearTimer);
        if (autoClear) {
            clearTimer = setTimeout(() => {
                barEl.className = '';
                if (textEl) textEl.textContent = 'Ready';
                if (iconEl) iconEl.textContent = TYPE_META.idle.icon;
            }, autoClear);
        }
    }

    function idle(msg = 'Ready') {
        barEl.className = '';
        if (iconEl) iconEl.textContent = TYPE_META.idle.icon;
        if (textEl) textEl.textContent = msg;
    }

    return { init, set, idle, notifications };
})();

// ── Notification History ─────────────────────────────────────────────────────

const NotifHistory = (() => {
    let el;

    function init() {
        el = document.getElementById('notif-history');
        document.getElementById('notif-close')?.addEventListener('click', close);
    }

    function toggle() {
        el.classList.toggle('open');
        if (el.classList.contains('open')) refresh();
    }

    function close() {
        el.classList.remove('open');
    }

    function refresh() {
        const list = el.querySelector('.notif-list');
        if (!list) return;
        list.innerHTML = '';
        for (const n of StatusBar.notifications) {
            const div  = document.createElement('div');
            div.className = `notif-item ${n.type}`;
            const t = n.time.toLocaleTimeString('en-US', { hour12: false });
            div.innerHTML = `<span class="notif-time">${t}</span><span class="notif-msg">${n.msg}</span>`;
            list.appendChild(div);
        }
    }

    return { init, toggle, close, refresh };
})();

// ── Panel Manager ────────────────────────────────────────────────────────────

const PanelManager = (() => {
    let containerEl;
    let current = null;   // currently open panel id

    function init() {
        containerEl = document.getElementById('panel-container');

        // Sidebar icon clicks
        document.querySelectorAll('.sidebar-icon[data-panel]').forEach(icon => {
            icon.addEventListener('click', () => {
                const pid = icon.dataset.panel;
                toggle(pid);
            });
        });

        // Panel close buttons
        document.querySelectorAll('.panel-close').forEach(btn => {
            btn.addEventListener('click', closeAll);
        });
    }

    function open(panelId) {
        // Deactivate all panels
        document.querySelectorAll('.sliding-panel').forEach(p => p.classList.remove('active'));
        document.querySelectorAll('.sidebar-icon').forEach(i => i.classList.remove('active'));

        const panel = document.getElementById(panelId);
        if (!panel) return;
        panel.classList.add('active');

        const icon = document.querySelector(`.sidebar-icon[data-panel="${panelId}"]`);
        if (icon) icon.classList.add('active');

        containerEl.classList.add('open');
        current = panelId;
    }

    function closeAll() {
        document.querySelectorAll('.sliding-panel').forEach(p => p.classList.remove('active'));
        document.querySelectorAll('.sidebar-icon').forEach(i => i.classList.remove('active'));
        containerEl.classList.remove('open');
        current = null;
    }

    function toggle(panelId) {
        if (current === panelId) {
            closeAll();
        } else {
            open(panelId);
        }
    }

    return { init, open, close: closeAll, toggle };
})();

// ── Menu Bar ─────────────────────────────────────────────────────────────────

const MenuBar = (() => {
    function init() {
        document.querySelectorAll('.menu-item').forEach(item => {
            item.addEventListener('click', (e) => {
                e.stopPropagation();
                const wasOpen = item.classList.contains('open');
                // Close all
                document.querySelectorAll('.menu-item').forEach(m => m.classList.remove('open'));
                if (!wasOpen) item.classList.add('open');
            });
        });

        // Close on outside click
        document.addEventListener('click', () => {
            document.querySelectorAll('.menu-item').forEach(m => m.classList.remove('open'));
        });

        // Wire menu actions
        document.querySelectorAll('.menu-dropdown-item[data-action]').forEach(el => {
            el.addEventListener('click', (e) => {
                e.stopPropagation();
                document.querySelectorAll('.menu-item').forEach(m => m.classList.remove('open'));
                handleMenuAction(el.dataset.action);
            });
        });
    }

    function handleMenuAction(action) {
        switch (action) {
            case 'open-file':
                StatusBar.set('[STUB] File open dialog — Tauri fs plugin (Phase 1)', 'info');
                break;
            case 'exit':
                StatusBar.set('Exit: use window close button (Tauri shell).', 'info');
                break;
            case 'keywords':
                PanelManager.open('panel-keywords');
                break;
            case 'preferences':
                StatusBar.set('Preferences — Phase 9', 'info');
                break;
            case 'theme-dark':
                setTheme('dark');
                break;
            case 'theme-light':
                setTheme('light');
                break;
            case 'theme-matrix':
                setTheme('matrix');
                break;
            case 'run-macro':
                PanelManager.open('panel-macro-editor');
                break;
            case 'macro-library':
                PanelManager.open('panel-macro-lib');
                break;
            case 'fwhm':
            case 'star-count':
            case 'eccentricity':
            case 'median-value':
            case 'contour':
                StatusBar.set(`Analysis plugin: ${action} — Phase 7 (WASM)`, 'info');
                break;
            case 'plugin-manager':
                PanelManager.open('panel-plugins');
                break;
            case 'log-viewer':
                StatusBar.set('Log Viewer — Phase 9', 'info');
                break;
            case 'about':
                StatusBar.set('Photyx v1.0.0-dev  |  High-performance astrophotography platform', 'info');
                break;
            default:
                StatusBar.set(`Menu: ${action} (not yet implemented)`, 'warning');
        }
    }

    return { init };
})();

// ── Theme Manager ────────────────────────────────────────────────────────────

let activeThemeLink = null;

function setTheme(name) {
    if (activeThemeLink) activeThemeLink.remove();
    const link = document.createElement('link');
    link.rel  = 'stylesheet';
    link.href = `css/themes/${name}.css`;
    link.id   = 'theme-stylesheet';
    document.head.appendChild(link);
    activeThemeLink = link;
    localStorage.setItem('photyx-theme', name);
    StatusBar.set(`Theme: ${name}`, 'success');
}

// ── Quick Launch ─────────────────────────────────────────────────────────────

const QuickLaunch = (() => {
    // Seed with placeholder macros matching spec
    const defaultMacros = [
        { label: 'Auto-STF',    cmd: 'AutoStretch method=asinh' },
        { label: 'Blink Start', cmd: 'BlinkSequence fps=2' },
        { label: 'List Files',  cmd: 'ListFiles' },
        { label: 'List KW',     cmd: 'ListKeywords' },
        { label: 'FWHM',        cmd: 'ComputeFWHM' },
        { label: 'Star Count',  cmd: 'CountStars' },
    ];

    let visible = true;   // matches initial display:flex in CSS

    function init() {
        const container = document.getElementById('ql-buttons');
        if (!container) return;

        defaultMacros.forEach(macro => {
            const btn = document.createElement('button');
            btn.className = 'ql-btn';
            btn.textContent = macro.label;
            btn.title = macro.cmd;
            btn.addEventListener('click', () => {
                PcodeInterpreter.executeLine(macro.cmd);
                ConsoleUI.appendLine(macro.cmd, 'input-echo');
            });
            container.appendChild(btn);
        });

        // toggle deferred — vertical collapse requires layout work
        // document.getElementById('ql-toggle')?.addEventListener('click', toggle);
    }

    function toggle() {
        visible = !visible;
        const buttons = document.getElementById('ql-buttons');
        const btn = document.getElementById('ql-toggle');
        if (buttons) buttons.style.display = visible ? 'flex' : 'none';
        if (btn) btn.textContent = visible ? '▲' : '▼';
    }

    return { init };
})();

// ── Toolbar wiring ───────────────────────────────────────────────────────────

function initToolbar() {
    // Zoom buttons
    const ZOOM_KEYS = { 'zoom-fit': 'fit', 'zoom-25': '25', 'zoom-50': '50', 'zoom-100': '100', 'zoom-200': '200' };
    for (const [id, level] of Object.entries(ZOOM_KEYS)) {
        document.getElementById(id)?.addEventListener('click', () => {
            PcodeInterpreter.executeLine(`SetZoom level=${level}`);
            // Update active state
            document.querySelectorAll('.toolbar-btn[id^="zoom-"]').forEach(b => b.classList.remove('active'));
            document.getElementById(id)?.classList.add('active');
        });
    }

    // Stretch selector
    document.getElementById('stretch-select')?.addEventListener('change', (e) => {
        const val = e.target.value;
        if (val === 'auto') PcodeInterpreter.executeLine('AutoStretch method=asinh');
        if (val === 'linear') PcodeInterpreter.executeLine('LinearStretch black=0 white=65535');
        if (val === 'histeq') PcodeInterpreter.executeLine('HistogramEqualization');
    });
}

// ── Info Panel tabs ──────────────────────────────────────────────────────────

function initInfoPanelTabs() {
    document.querySelectorAll('.info-tab').forEach(tab => {
        tab.addEventListener('click', () => {
            document.querySelectorAll('.info-tab').forEach(t => t.classList.remove('active'));
            document.querySelectorAll('.info-panel-body').forEach(b => b.classList.remove('active'));
            tab.classList.add('active');
            const target = document.getElementById(tab.dataset.target);
            if (target) target.classList.add('active');
        });
    });
}

// ── Keyboard shortcuts ───────────────────────────────────────────────────────

function initKeyboardShortcuts() {
    document.addEventListener('keydown', (e) => {
        // Don't intercept if typing in an input
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

        switch (e.key) {
            case ' ':
                e.preventDefault();
                PcodeInterpreter.executeLine('BlinkSequence fps=2');
                break;
            case 'j': case 'J':
                StatusBar.set('Previous frame (blink) — Phase 2', 'info');
                break;
            case 'k': case 'K':
                StatusBar.set('Next frame (blink) — Phase 2', 'info');
                break;
            case '0':
                PcodeInterpreter.executeLine('SetZoom level=fit'); break;
            case '1':
                PcodeInterpreter.executeLine('SetZoom level=25'); break;
            case '2':
                PcodeInterpreter.executeLine('SetZoom level=50'); break;
            case '3':
                PcodeInterpreter.executeLine('SetZoom level=100'); break;
            case '4':
                PcodeInterpreter.executeLine('SetZoom level=200'); break;
        }
    });
}

// ── Macro editor panel ───────────────────────────────────────────────────────

function initMacroEditor() {
    document.getElementById('macro-run-btn')?.addEventListener('click', () => {
        const text = document.getElementById('macro-textarea')?.value || '';
        if (!text.trim()) { StatusBar.set('Macro editor is empty.', 'warning'); return; }
        StatusBar.set('Running macro…', 'info');
        PcodeInterpreter.executeMacro(text);
        StatusBar.set('Macro complete.', 'success');
    });

    document.getElementById('macro-save-btn')?.addEventListener('click', () => {
        StatusBar.set('[STUB] Save macro — Tauri file dialog (Phase 5)', 'info');
    });

    document.getElementById('macro-load-btn')?.addEventListener('click', () => {
        StatusBar.set('[STUB] Load macro — Tauri file dialog (Phase 5)', 'info');
    });

    document.getElementById('macro-copy-from-console')?.addEventListener('click', () => {
        // Mirrors console "Copy to Editor" — copies console input lines
        ConsoleUI.focus();
        StatusBar.set('Use "Copy to Editor" button in the console.', 'info');
    });
}

// ── Application boot ─────────────────────────────────────────────────────────

function boot() {
    // Subsystems — StatusBar must init before setTheme() which calls StatusBar.set()
    StatusBar.init();
    NotifHistory.init();

    // Theme
    const savedTheme = localStorage.getItem('photyx-theme') || 'matrix';
    setTheme(savedTheme);
    PanelManager.init();
    MenuBar.init();
    QuickLaunch.init();
    ConsoleUI.init();
    initToolbar();
    initInfoPanelTabs();
    initKeyboardShortcuts();
    initMacroEditor();

    // Starfield
    const canvas = document.getElementById('viewer-canvas');
    if (canvas) {
        Starfield.init(canvas);
        Starfield.start();
    }

    // Focus console input
    setTimeout(() => ConsoleUI.focus(), 200);

    // Initial status
    StatusBar.idle('Photyx 1.0.0-dev  —  Ready');
}

document.addEventListener('DOMContentLoaded', boot);
