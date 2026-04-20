<!-- KeywordEditor.svelte — Spec §8.6 -->
<script lang="ts">
    import { ui } from '../../stores/ui';
    import { currentImage } from '../../stores/session';
</script>

<div class="sliding-panel active">
    <div class="panel-header">
        <span>Keyword Editor</span>
        <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
    </div>
    <div class="panel-body">
        <div class="kw-actions">
            <button class="kw-btn">+ Add</button>
            <button class="kw-btn">− Delete</button>
            <button class="kw-btn">⟳ Reload</button>
        </div>
        <table class="keyword-table">
            <thead>
                <tr><th>Keyword</th><th>Value</th><th>Comment</th></tr>
            </thead>
            <tbody>
                {#if $currentImage}
                    {#each Object.values($currentImage.keywords) as kw}
                        <tr>
                            <td class="kw-name">{kw.name}</td>
                            <td class="kw-value">{kw.value}</td>
                            <td class="kw-comment">{kw.comment ?? ''}</td>
                        </tr>
                    {/each}
                {:else}
                    <tr><td class="kw-name" colspan="3" style="color:var(--text-secondary)">No image loaded</td></tr>
                {/if}
            </tbody>
        </table>
    </div>
</div>
