<!-- FileBrowser.svelte — Spec §8.6 -->
<script lang="ts">
    import { ui } from '../../stores/ui';
    import { session } from '../../stores/session';
</script>

<div class="sliding-panel active">
    <div class="panel-header">
        <span>File Browser</span>
        <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
    </div>
    <div class="dir-bar">
        <span class="dir-path">{$session.activeDirectory ?? '(no directory selected)'}</span>
        <button class="dir-browse-btn">…</button>
    </div>
    <div class="panel-body">
        <ul class="file-list">
            {#if $session.fileList.length === 0}
                <li class="file-item" style="color:var(--text-secondary);font-size:10px;cursor:default;">
                    Use SelectDirectory in the console to load a directory.
                </li>
            {:else}
                {#each $session.fileList as file}
                    <li class="file-item">
                        <span class="file-name">{file.split('/').pop()}</span>
                    </li>
                {/each}
            {/if}
        </ul>
    </div>
</div>
