<!-- MenuBar.svelte — Application menu bar. Spec §8.2 -->
<script lang="ts">
    import { ui } from '../stores/ui';
    import { notifications } from '../stores/notifications';
    import { selectDirectory, closeSession } from '../commands';
    import { getCurrentWindow } from '@tauri-apps/api/window';
    import { invoke } from '@tauri-apps/api/core';
    import { consolePipe } from '../stores/consoleHistory';

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
            case 'analyze-frames':   runAnalyzeFrames(); break;
            case 'analysis-results': ui.showView('analysisResults'); break;
            case 'analysis-graph':   ui.showView('analysisGraph'); break;
            case 'close-session':     closeSession(); break;
            case 'contour-plot':      notifications.info('Contour Plot — not yet implemented'); break;
            case 'exit':              getCurrentWindow().close(); break;
            case 'keywords':          ui.togglePanel('keywords'); break;
            case 'macro-library':     ui.togglePanel('macro-lib'); break;
            case 'plugin-manager':    ui.togglePanel('plugins'); break;
            case 'run-macro':         ui.togglePanel('macro-editor'); break;
            case 'select-directory':  selectDirectory(); break;
            case 'theme-dark':        ui.setTheme('dark'); break;
            case 'theme-light':       ui.setTheme('light'); break;
            case 'theme-matrix':      ui.setTheme('matrix'); break;
            default: notifications.info(`${a} — not yet implemented`);
        }
    }

    async function runAnalyzeFrames() {
        notifications.running('AnalyzeFrames running…');
        try {
            const response = await invoke<{
                success: boolean;
                output: string | null;
                error: string | null;
            }>('dispatch_command', {
                request: { command: 'AnalyzeFrames', args: {} }
            });
            if (response.success) {
                const msg = response.output ?? 'AnalyzeFrames complete';
                consolePipe.set({ id: Date.now(), text: msg, type: 'success' });
                notifications.success('AnalyzeFrames complete');
            } else {
                const err = response.error ?? 'AnalyzeFrames failed';
                consolePipe.set({ id: Date.now(), text: err, type: 'error' });
                notifications.error(err);
            }
        } catch (err) {
            const msg = `AnalyzeFrames error: ${err}`;
            consolePipe.set({ id: Date.now(), text: msg, type: 'error' });
            notifications.error(msg);
        }
    }

    async function runAutoStretch() {
        try {
            const response = await invoke<{
                success: boolean;
                output: string | null;
                error: string | null;
            }>('dispatch_command', {
                request: { command: 'AutoStretch', args: {} }
            });
            if (response.success) {
                const msg = response.output ?? 'AutoStretch applied';
                consolePipe.set({ id: Date.now(), text: msg, type: 'success' });
                notifications.success(msg);
                ui.requestFrameRefresh();
            } else {
                const err = response.error ?? 'AutoStretch failed';
                consolePipe.set({ id: Date.now(), text: err, type: 'error' });
                notifications.error(err);
            }
        } catch (err) {
            const msg = `AutoStretch error: ${err}`;
            consolePipe.set({ id: Date.now(), text: msg, type: 'error' });
            notifications.error(msg);
        }
    }
</script>

<svelte:window onclick={close} />

<div id="menu-bar">
    {#each [
        { name: 'File', items: [
            { label: 'Select Directory…',    action: 'select-directory',     shortcut: 'Ctrl+O' },
            { label: 'Close Session',        action: 'close-session' },
            { sep: true },
            { label: 'Exit',         action: 'exit' },
        ]},
        { name: 'Edit', items: [
            { label: 'Preferences',  action: 'preferences' },
        ]},
        { name: 'View', items: [
            { label: 'Theme: Dark',  action: 'theme-dark' },
            { label: 'Theme: Light', action: 'theme-light' },
            { label: 'Theme: Matrix',action: 'theme-matrix' },
        ]},
        { name: 'Analyze', items: [
            { label: 'Analyze Frames',   action: 'analyze-frames' },
            { label: 'Analysis Results', action: 'analysis-results' },
            { label: 'Analysis Graph',   action: 'analysis-graph' },
            { label: 'Contour Plot',     action: 'contour-plot' },
        ]},
        { name: 'Tools', items: [
            { label: 'Settings',     action: 'settings' },
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
