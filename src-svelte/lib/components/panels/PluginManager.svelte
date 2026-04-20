<!-- PluginManager.svelte — Spec §8.6 -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { ui } from '../../stores/ui';
    import { onMount } from 'svelte';

    let plugins = $state<string[]>([]);

    onMount(async () => {
        try {
            plugins = await invoke<string[]>('list_plugins');
        } catch (e) {
            plugins = [];
        }
    });

    const WASM_PLUGINS = ['computefwhm', 'countstars', 'computeeccentricity', 'medianvalue', 'contourplot'];
</script>

<div class="sliding-panel active">
    <div class="panel-header">
        <span>Plugin Manager</span>
        <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
    </div>
    <div class="panel-body">
        {#each plugins as name}
            <div class="plugin-item">
                <div class="plugin-name">
                    {name}
                    <span class="plugin-type" class:native={!WASM_PLUGINS.includes(name)} class:wasm={WASM_PLUGINS.includes(name)}>
                        {WASM_PLUGINS.includes(name) ? 'WASM' : 'Native'}
                    </span>
                </div>
                <div class="plugin-version">v1.0</div>
            </div>
        {:else}
            <p style="font-size:11px;color:var(--text-secondary);">Loading plugins…</p>
        {/each}
    </div>
</div>
