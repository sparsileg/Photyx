<!-- MenuBar.svelte — Application menu bar. Spec §8.2 -->
<script lang="ts">
    import { ui } from '../stores/ui';
    import { notifications } from '../stores/notifications';

    let openMenu = $state<string | null>(null);

    function toggle(name: string) {
        openMenu = openMenu === name ? null : name;
    }

    function close() {
        openMenu = null;
    }

    function action(a: string) {
        close();
        switch (a) {
            case 'theme-dark':   ui.setTheme('dark'); break;
            case 'theme-light':  ui.setTheme('light'); break;
            case 'theme-matrix': ui.setTheme('matrix'); break;
            case 'keywords':     ui.togglePanel('keywords'); break;
            case 'run-macro':    ui.togglePanel('macro-editor'); break;
            case 'macro-library': ui.togglePanel('macro-lib'); break;
            case 'plugin-manager': ui.togglePanel('plugins'); break;
            default: notifications.info(`${a} — not yet implemented`);
        }
    }
</script>

<svelte:window onclick={close} />

<div id="menu-bar">
    {#each [
        { name: 'File', items: [
            { label: 'Open…',        action: 'open-file',     shortcut: 'Ctrl+O' },
            { label: 'Open Recent ▶',action: 'open-recent' },
            { sep: true },
            { label: 'Close',        action: 'close-file' },
            { sep: true },
            { label: 'Exit',         action: 'exit' },
        ]},
        { name: 'Edit', items: [
            { label: 'Keywords',     action: 'keywords' },
            { sep: true },
            { label: 'Preferences',  action: 'preferences' },
        ]},
        { name: 'View', items: [
            { label: 'Zoom: Fit',    action: 'zoom-fit',      shortcut: '0' },
            { label: 'Zoom: 100%',   action: 'zoom-100',      shortcut: '3' },
            { sep: true },
            { label: 'Theme: Dark',  action: 'theme-dark' },
            { label: 'Theme: Light', action: 'theme-light' },
            { label: 'Theme: Matrix',action: 'theme-matrix' },
        ]},
        { name: 'Process', items: [
            { label: 'Run Macro…',   action: 'run-macro' },
            { label: 'Macro Library',action: 'macro-library' },
        ]},
        { name: 'Analyze', items: [
            { label: 'FWHM',         action: 'fwhm' },
            { label: 'Star Count',   action: 'star-count' },
            { label: 'Eccentricity', action: 'eccentricity' },
            { label: 'Median Value', action: 'median-value' },
            { label: 'Contour Plot', action: 'contour' },
        ]},
        { name: 'Tools', items: [
            { label: 'Settings',     action: 'settings' },
            { label: 'Plugin Manager', action: 'plugin-manager' },
            { sep: true },
            { label: 'Log Viewer',   action: 'log-viewer' },
        ]},
        { name: 'Help', items: [
            { label: 'About Photyx', action: 'about' },
            { label: 'Documentation',action: 'documentation' },
            { sep: true },
            { label: 'Check for Updates', action: 'check-updates' },
        ]},
    ] as menu}
        <div
            class="menu-item"
            class:open={openMenu === menu.name}
            onclick={(e) => { e.stopPropagation(); toggle(menu.name); }}
        >
            {menu.name}
            {#if openMenu === menu.name}
            <div class="menu-dropdown">
                {#each menu.items as item}
                    {#if item.sep}
                        <div class="menu-separator"></div>
                    {:else}
                        <div
                            class="menu-dropdown-item"
                            onclick={(e) => { e.stopPropagation(); action(item.action ?? ''); }}
                        >
                            {item.label}
                            {#if item.shortcut}
                                <span class="shortcut">{item.shortcut}</span>
                            {/if}
                        </div>
                    {/if}
                {/each}
            </div>
            {/if}
        </div>
    {/each}
</div>
