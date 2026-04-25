<!-- +page.svelte — Photyx main application shell. Spec §8.1 -->
<script lang="ts">
    import { onMount } from 'svelte';
    import { ui } from '../lib/stores/ui';

    import AnalysisGraph from '../lib/components/AnalysisGraph.svelte';
    import Console from '../lib/components/Console.svelte';
    import IconSidebar from '../lib/components/IconSidebar.svelte';
    import InfoPanel from '../lib/components/InfoPanel.svelte';
    import KeywordModal from '../lib/components/KeywordModal.svelte';
    import MacroEditor from '../lib/components/panels/MacroEditor.svelte';
    import MenuBar from '../lib/components/MenuBar.svelte';
    import QuickLaunch from '../lib/components/QuickLaunch.svelte';
    import StatusBar from '../lib/components/StatusBar.svelte';
    import Toolbar from '../lib/components/Toolbar.svelte';
    import Viewer from '../lib/components/Viewer.svelte';

    // Load theme stylesheet dynamically
    let themeLink: HTMLLinkElement | null = null;

    $effect(() => {
        const theme = $ui.theme;
        if (themeLink) themeLink.remove();
        themeLink = document.createElement('link');
        themeLink.rel = 'stylesheet';
        themeLink.href = `/themes/${theme}.css`;
        document.head.appendChild(themeLink);
    });

    // Blink filename overlay
    let blinkFilename = $state('');
    function onBlinkFrame(filename: string) {
        blinkFilename = filename;
    }

    // Mouse pixel tracking — prop callback, never touches the store
    let mousePixel = $state<{ x: number; y: number } | null>(null);
    function onMousePixel(px: { x: number; y: number } | null) {
        mousePixel = px;
    }

    // Keyboard shortcuts per spec §8.13
    function onKeyDown(e: KeyboardEvent) {
        if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
        switch (e.key) {
            case '0': ui.setZoom('fit');  break;
            case '1': ui.setZoom('25');   break;
            case '2': ui.setZoom('50');   break;
            case '3': ui.setZoom('100');  break;
            case '4': ui.setZoom('200');  break;
        }
    }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if $ui.keywordModalOpen}
    <KeywordModal onclose={() => ui.closeKeywordModal()} />
{/if}

<div id="app">
    <MenuBar />
    <Toolbar />
    <QuickLaunch />
    <div id="content-area">
        <IconSidebar />
        {#if $ui.activePanel === 'macro-editor'}
            <MacroEditor />
        {/if}

            <div id="viewer-region">
            {#if $ui.showAnalysisGraph}
                <AnalysisGraph />
            {:else}
                <Viewer onMousePixel={onMousePixel} />
            {/if}
            {#if $ui.blinkTabActive && blinkFilename}
                <div id="blink-filename-overlay">{blinkFilename}</div>
            {/if}
            <div id="bottom-panel" class:console-expanded={$ui.consoleExpanded}>
                <Console />
                <InfoPanel onBlinkFrame={onBlinkFrame} mousePixel={mousePixel} />
            </div>
        </div>
    </div>

    <StatusBar />
</div>
