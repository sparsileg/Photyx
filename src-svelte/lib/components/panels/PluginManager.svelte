<!-- PluginManager.svelte — Spec §8.6 -->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { ui } from '../../stores/ui';
  import { onMount } from 'svelte';

  interface PluginInfo {
    name:        string;
    version:     string;
    plugin_type: string;
  }

  let plugins = $state<PluginInfo[]>([]);
  let loading = $state(true);

  onMount(async () => {
    try {
      plugins = await invoke<PluginInfo[]>('list_plugins');
    } catch (e) {
      plugins = [];
    } finally {
      loading = false;
    }
  });
</script>

<div class="sliding-panel active">
  <div class="panel-header">
    <span>Plugin Manager</span>
    <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
  </div>
  <div class="panel-body">
    {#if loading}
      <p style="font-size:11px;color:var(--text-secondary);padding:8px;">Loading plugins…</p>
    {:else if plugins.length === 0}
      <p style="font-size:11px;color:var(--text-secondary);padding:8px;">No plugins found.</p>
    {:else}
      {#each plugins as plugin}
        <div class="plugin-item">
          <div class="plugin-name">
            {plugin.name}
            <span
              class="plugin-type"
              class:native={plugin.plugin_type === 'Native'}
              class:wasm={plugin.plugin_type === 'WASM'}
              >{plugin.plugin_type}</span>
          </div>
          <div class="plugin-version">v{plugin.version}</div>
        </div>
      {/each}
    {/if}
  </div>
  <div class="ml-footer">
    {plugins.length} plugin{plugins.length !== 1 ? 's' : ''}
  </div>
</div>
