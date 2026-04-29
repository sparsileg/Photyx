<!-- HelpModal.svelte — Command help modal. Triggered by: help <command> -->
<script lang="ts">
    import type { HelpEntry } from '../pcodeHelp';

    let { entry, onclose }: { entry: HelpEntry; onclose: () => void } = $props();

    function onKeyDown(e: KeyboardEvent) {
        if (e.key === 'Escape') onclose();
    }
</script>

<svelte:window onkeydown={onKeyDown} />

<div id="help-modal">
    <div class="hm-header">
        <span class="hm-title">{entry.name}</span>
        <button class="hm-close" onclick={onclose}>✕</button>
    </div>

    <div class="hm-body">
        <div class="hm-description">{entry.description}</div>

        <div class="hm-section-label">Syntax</div>
        <div class="hm-syntax">{entry.syntax}</div>

        {#if entry.arguments.length > 0}
            <div class="hm-section-label">Arguments</div>
            <table class="hm-args-table">
                <thead>
                    <tr>
                        <th>Name</th>
                        <th>Type</th>
                        <th>Required</th>
                        <th>Default</th>
                        <th>Description</th>
                    </tr>
                </thead>
                <tbody>
                    {#each entry.arguments as arg}
                        <tr>
                            <td class="hm-arg-name">{arg.name}</td>
                            <td class="hm-arg-type">{arg.type}</td>
                            <td class="hm-arg-req">{arg.required ? 'yes' : 'no'}</td>
                            <td class="hm-arg-default">{arg.default ?? '—'}</td>
                            <td class="hm-arg-desc">{arg.description}</td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        {/if}

        <div class="hm-section-label">Output</div>
        <div class="hm-output">{entry.output}</div>

        <div class="hm-section-label">Example</div>
        <pre class="hm-example">{entry.example}</pre>
    </div>
</div>
