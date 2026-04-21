<!-- KeywordModal.svelte — Displays all FITS keywords in a modal dialog -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { onMount } from 'svelte';

    let { onclose } = $props<{ onclose: () => void }>();

    interface Keyword {
        name: string;
        value: string;
        comment: string | null;
    }

    let keywords = $state<Keyword[]>([]);
    let loading = $state(true);
    let searchQuery = $state('');

    onMount(async () => {
        try {
            const map = await invoke<Record<string, Keyword>>('get_keywords');
            keywords = Object.values(map).sort((a, b) => a.name.localeCompare(b.name));
        } catch (e) {
            console.error('get_keywords error:', e);
        } finally {
            loading = false;
        }
    });

    let filtered = $derived(
        searchQuery.trim() === ''
            ? keywords
            : keywords.filter(kw =>
                kw.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
                kw.value.toLowerCase().includes(searchQuery.toLowerCase()) ||
                (kw.comment ?? '').toLowerCase().includes(searchQuery.toLowerCase())
            )
    );
</script>

<div class="modal-overlay" onclick={onclose}>
    <div class="modal-box" onclick={(e) => e.stopPropagation()}>
        <div class="modal-header">
            <span class="modal-title">FITS Keywords</span>
            <input
                class="modal-search"
                type="text"
                placeholder="Search…"
                bind:value={searchQuery}
                autocomplete="off"
                spellcheck={false}
            />
            <span class="modal-close" onclick={onclose}>✕</span>
        </div>
        <div class="modal-body">
            {#if loading}
                <div class="modal-loading">Loading…</div>
            {:else if filtered.length === 0}
                <div class="modal-loading">No keywords found.</div>
            {:else}
                <table class="kw-table">
                    <thead>
                        <tr>
                            <th>Name</th>
                            <th>Value</th>
                            <th>Comment</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each filtered as kw}
                            <tr>
                                <td class="kw-name">{kw.name}</td>
                                <td class="kw-value">{kw.value}</td>
                                <td class="kw-comment">{kw.comment ?? ''}</td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            {/if}
        </div>
        <div class="modal-footer">
            {filtered.length} keyword{filtered.length !== 1 ? 's' : ''}
        </div>
    </div>
</div>



<!-- ---------------------------------------------------------------------- -->
