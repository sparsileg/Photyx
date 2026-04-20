<!-- MacroLibrary.svelte — Spec §8.6 -->
<script lang="ts">
    import { ui } from '../../stores/ui';

    const macros = [
        { name: 'ProcessLightFrames', desc: 'Debayer, stretch, and write FITS light frames' },
        { name: 'BatchKeywordTag',    desc: 'Add OBJECT keyword to all files in directory' },
        { name: 'ConvertFITStoXISF', desc: 'Convert all FITS files to XISF format' },
    ];
</script>

<div class="sliding-panel active">
    <div class="panel-header">
        <span>Macro Library</span>
        <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
    </div>
    <div class="panel-body">
        {#each macros as macro}
            <div class="macro-lib-item">
                <div class="macro-lib-name">{macro.name}</div>
                <div class="macro-lib-desc">{macro.desc}</div>
            </div>
        {/each}
        <p style="font-size:10px;color:var(--text-secondary);padding:8px;margin-top:8px;border-top:1px solid var(--border-color);">
            Macro library directory scanning — Phase 5.
        </p>
    </div>
</div>
