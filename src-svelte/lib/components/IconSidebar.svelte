<!-- IconSidebar.svelte — Icon bar + sliding panels. Spec §8.5, §8.6 -->
<script lang="ts">
    import { ui } from '../stores/ui';
    import type { PanelId } from '../stores/ui';
    import FileBrowser from './panels/FileBrowser.svelte';
    import KeywordEditor from './panels/KeywordEditor.svelte';
    import MacroEditor from './panels/MacroEditor.svelte';
    import MacroLibrary from './panels/MacroLibrary.svelte';
    import PluginManager from './panels/PluginManager.svelte';

    const icons: { id: PanelId; icon: string; tooltip: string }[] = [
        { id: 'files',    icon: '📁', tooltip: 'File Browser' },
        { id: 'keywords', icon: '🏷',  tooltip: 'Keyword Editor' },
        { id: 'macro-lib',icon: '☰',   tooltip: 'Macro Library' },
        { id: 'plugins',  icon: '⬡',   tooltip: 'Plugin Manager' },
    ];

    function onWindowClick(e: MouseEvent) {
        if ($ui.activePanel === null) return;
        const target = e.target as HTMLElement;
        if (target.closest('#panel-container') || target.closest('#icon-sidebar') || target.closest('.macro-editor-panel')) return;
        ui.closePanel();
    }
</script>

<svelte:window onclick={onWindowClick} />

<div id="icon-sidebar">
    {#each icons as item}
        <div
            class="sidebar-icon"
            class:active={$ui.activePanel === item.id}
            data-tooltip={item.tooltip}
            onclick={() => ui.togglePanel(item.id)}
        >{item.icon}</div>
    {/each}
</div>

{#if $ui.activePanel !== null && $ui.activePanel !== 'macro-editor'}
<div id="panel-container" class="open" class:wide={$ui.activePanel === 'keywords'}>
    {#if $ui.activePanel === 'files'}
        <FileBrowser />
    {:else if $ui.activePanel === 'keywords'}
        <KeywordEditor />
    {:else if $ui.activePanel === 'macro-lib'}
        <MacroLibrary />
    {:else if $ui.activePanel === 'plugins'}
        <PluginManager />
    {/if}
</div>
{/if}
