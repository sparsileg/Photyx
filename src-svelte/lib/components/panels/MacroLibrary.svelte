<!-- MacroLibrary.svelte — Spec §8.6 -->
<script lang="ts">
    import { onMount } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { ui } from '../../stores/ui';
    import { quickLaunch } from '../../stores/quickLaunch';
    import { session } from '../../stores/session';
    import { notifications } from '../../stores/notifications';
    import { consolePipe } from '../../stores/consoleHistory';

    interface MacroEntry {
        name:    string;
        filename: string;
        path:    string;
        lines:   number;
        tooltip: string;
    }

    let macros        = $state<MacroEntry[]>([]);
    let loading       = $state(true);
    let pinned        = $state<Set<string>>(new Set());
    let confirmDelete = $state<string | null>(null);
    let pinnedWarning = $state<string | null>(null);
    let running       = $state<string | null>(null);

    async function loadMacros() {
        loading = true;
        try {
            macros = await invoke<MacroEntry[]>('list_macros');
        } catch (e) {
            notifications.error(`Macro Library: ${e}`);
        } finally {
            loading = false;
        }
    }

    function editMacro(macro: MacroEntry) {
        ui.openMacroEditor({ path: macro.path, name: macro.name });
    }

    async function newMacro() {
        const name = window.prompt('New macro name:')?.trim();
        if (!name) return;
        try {
            const dir  = await invoke<string>('get_macros_dir');
            const safe = name.replace(/[^a-zA-Z0-9_\- ]/g, '').trim() || 'Untitled';
            const path = `${dir}/${safe}.phs`;
            ui.openMacroEditor({ path, name: safe });
        } catch (e) {
            notifications.error(`Cannot resolve Macros directory: ${e}`);
        }
    }

    async function renameMacro(macro: MacroEntry) {
        const newName = window.prompt('Rename macro:', macro.name)?.trim();
        if (!newName || newName === macro.name) return;
        try {
            await invoke<string>('rename_macro', {
                oldPath: macro.path,
                newName,
            });
            notifications.success(`Renamed to: ${newName}`);
            await loadMacros();
        } catch (e) {
            notifications.error(`Rename failed: ${e}`);
        }
    }

    function pinMacro(macro: MacroEntry) {
        quickLaunch.pin({
            name:   macro.name,
            script: `RunMacro path="${macro.path}"`,
            icon:   '📜',
        });
        notifications.success(`Pinned: ${macro.name}`);
    }

    function requestDelete(macro: MacroEntry) {
        if (pinned.has(macro.path)) {
            pinnedWarning = macro.path;
            confirmDelete = null;
        } else {
            confirmDelete = macro.path;
            pinnedWarning = null;
        }
    }

    async function confirmDeleteMacro(path: string) {
        try {
            await invoke('delete_macro', { path });
            confirmDelete = null;
            notifications.success('Macro deleted.');
            await loadMacros();
        } catch (e) {
            notifications.error(`Delete failed: ${e}`);
        }
    }

    function cancelDelete() {
        confirmDelete = null;
        pinnedWarning = null;
    }

    async function runMacro(macro: MacroEntry) {
        if (running === macro.path) return;
        running = macro.path;
        notifications.running(`Running: ${macro.name}…`);
        try {
            const response = await invoke<{
                results: Array<{ line_number: number; command: string; success: boolean; message: string | null }>;
                session_changed: boolean;
                display_changed: boolean;
            }>('run_script', { script: `RunMacro path="${macro.path}"` });

            let anyError = false;
            for (const r of response.results) {
                if (!r.success) {
                    notifications.error(`${r.command}: ${r.message ?? 'error'}`);
                    anyError = true;
                } else if (r.message) {
                    r.message.split('\n').forEach(line => {
                        if (line) consolePipe.set({ id: Date.now(), text: line, type: 'success' });
                    });
                }
            }
            if (!anyError) notifications.success(`${macro.name} complete.`);

            if (response.session_changed) {
                const s = await invoke<{ activeDirectory: string; fileList: string[]; currentFrame: number }>('get_session');
                session.setDirectory(s.activeDirectory ?? '');
                session.setFileList(s.fileList);
            }
            if (response.display_changed) {
                ui.requestFrameRefresh();
            }
        } catch (e) {
            notifications.error(`Run failed: ${e}`);
        } finally {
            running = null;
        }
    }

    function formatLines(lines: number): string {
        return `${lines} line${lines !== 1 ? 's' : ''}`;
    }

    // Keep pinned state in sync with Quick Launch store
    $effect(() => {
        const ql = $quickLaunch;
        const pinnedPaths = new Set<string>();
        for (const entry of ql) {
            const match = entry.script.match(/RunMacro path="([^"]+)"/);
            if (match) {
                pinnedPaths.add(match[1].replace(/\\/g, '/'));
            }
        }
        pinned = new Set(
            macros
                .filter(m => pinnedPaths.has(m.path.replace(/\\/g, '/')))
                .map(m => m.path)
        );
    });

    onMount(loadMacros);
</script>

<div class="sliding-panel active">
    <div class="panel-header">
        <span>Macro Library</span>
        <div class="panel-header-actions">
            <button class="panel-action-btn" onclick={newMacro} title="Create a new macro">New</button>
            <button class="panel-action-btn" onclick={loadMacros} title="Refresh">↻</button>
            <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
        </div>
    </div>
    <div class="panel-body">
        {#if loading}
            <div class="ml-empty">Loading…</div>
        {:else if macros.length === 0}
            <div class="ml-empty">
                No macros found.<br/>
                Click New to create a macro.
            </div>
        {:else}
            {#each macros as macro}
                <div class="ml-item" title={macro.tooltip || undefined}>
                    <div class="ml-item-top">
                        <span class="ml-name">{macro.name}</span>
                        <div class="ml-item-actions">
                            <button
                                class="ml-action-btn"
                                onclick={() => editMacro(macro)}
                                title="Edit macro"
                            >✎ Edit</button>
                            <button
                                class="ml-action-btn"
                                onclick={() => renameMacro(macro)}
                                title="Rename macro"
                            >Rename</button>
                            <button
                                class="ml-action-btn ml-delete-btn"
                                onclick={() => requestDelete(macro)}
                                title="Delete macro"
                            >🗑</button>
                        </div>
                    </div>
                    <div class="ml-item-bottom">
                        <span class="ml-size">{formatLines(macro.lines)}</span>
                        <div class="ml-item-actions">
                            <button
                                class="ml-action-btn"
                                class:ml-pin-active={pinned.has(macro.path)}
                                onclick={() => pinMacro(macro)}
                                title="Pin to Quick Launch"
                            >📌 {pinned.has(macro.path) ? 'Pinned' : 'Pin'}</button>
                            <button
                                class="ml-action-btn ml-run-btn"
                                onclick={() => runMacro(macro)}
                                disabled={running === macro.path}
                                title="Run macro"
                            >{running === macro.path ? '◌ Running…' : '▶ Run'}</button>
                        </div>
                    </div>
                    {#if confirmDelete === macro.path}
                        <div class="ml-confirm-bar" onclick={(e) => e.stopPropagation()}>
                            <span>Delete {macro.name}? This cannot be undone.</span>
                            <button class="ml-confirm-yes" onclick={() => confirmDeleteMacro(macro.path)}>Delete</button>
                            <button class="ml-confirm-no" onclick={cancelDelete}>Cancel</button>
                        </div>
                    {/if}
                    {#if pinnedWarning === macro.path}
                        <div class="ml-confirm-bar ml-pinned-warning" onclick={(e) => e.stopPropagation()}>
                            <span>Remove from Quick Launch first.</span>
                            <button class="ml-confirm-no" onclick={cancelDelete}>Close</button>
                        </div>
                    {/if}
                </div>
            {/each}
        {/if}
    </div>
    <div class="ml-footer">
        {macros.length} macro{macros.length !== 1 ? 's' : ''} · Macros/
    </div>
</div>
