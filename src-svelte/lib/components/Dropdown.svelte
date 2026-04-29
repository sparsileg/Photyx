<script lang="ts">
    export let value: string;
    export let options: { value: string; label: string }[];
    export let className: string = '';

    let open = false;

    function select(val: string) {
        value = val;
        open = false;
    }

    function toggle() { open = !open; }

    function onKeydown(e: KeyboardEvent) {
        if (e.key === 'Escape') open = false;
    }

    $: selectedLabel = options.find(o => o.value === value)?.label ?? value;
</script>

<svelte:window on:keydown={onKeydown} />

<div class="dropdown {className}" class:open>
    <button class="dropdown-trigger" onclick={toggle} type="button">
        <span>{selectedLabel}</span>
        <span class="dropdown-arrow">{open ? '▲' : '▼'}</span>
    </button>
    {#if open}
        <div class="dropdown-menu">
            {#each options as opt}
                <div
                    class="dropdown-item"
                    class:selected={opt.value === value}
                    onclick={() => select(opt.value)}
                >
                    {opt.label}
                </div>
            {/each}
        </div>
    {/if}
</div>

<style>
    .dropdown {
        position: relative;
        display: inline-block;
    }
    .dropdown-trigger {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 3px 8px;
        background: var(--input-bg, #111);
        color: var(--input-fg, #00ff41);
        border: 1px solid var(--border, #00ff41);
        cursor: pointer;
        font-size: 0.85rem;
        min-width: 140px;
        justify-content: space-between;
    }
    .dropdown-trigger:hover {
        background: var(--input-bg-hover, #1a1a1a);
    }
    .dropdown-arrow {
        font-size: 0.65rem;
        opacity: 0.7;
    }
    .dropdown-menu {
        position: absolute;
        top: 100%;
        left: 0;
        z-index: 999;
        background: var(--input-bg, #111);
        border: 1px solid var(--border, #00ff41);
        min-width: 100%;
        max-height: 260px;
        overflow-y: auto;
    }
    .dropdown-item {
        padding: 4px 8px;
        color: var(--input-fg, #00ff41);
        cursor: pointer;
        font-size: 0.85rem;
    }
    .dropdown-item:hover {
        background: var(--highlight-bg, #003b00);
    }
    .dropdown-item.selected {
        background: var(--highlight-bg, #003b00);
        font-weight: bold;
    }
</style>
