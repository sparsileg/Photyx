<!-- Dropdown.svelte — Custom select element that is CSS friendly -->

<script lang="ts">
  import { onMount, onDestroy, createEventDispatcher } from 'svelte';

  const dispatch = createEventDispatcher();

  export let value: string;
  export let options: { value: string; label: string }[];
  export let className: string = '';
  export let openUp: boolean = false;
  export let width: number | null = null;

  let open = false;
  let triggerEl: HTMLElement;
  let menuEl: HTMLElement;

  function select(val: string) {
    value = val;
    dispatch('change', val);
    close();
  }

  function close() {
    open = false;
    menuEl.style.display = 'none';
    if (menuEl.parentNode === document.body) {
      document.body.removeChild(menuEl);
    }
  }

  function updateMenuPos() {
    if (!triggerEl || !menuEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const viewH = window.innerHeight;
    const menuHeight = options.length * 24 + 8;
    const shouldOpenUp = openUp || rect.bottom > viewH - menuHeight || rect.top > viewH * 0.75;
    menuEl.style.left = `${rect.left}px`;
    console.log('dropdown width prop:', width, 'rect.width:', rect.width);
    menuEl.style.width = `${width !== null ? width : rect.width}px`;
    menuEl.style.top = shouldOpenUp ? `${rect.top - menuHeight}px` : `${rect.bottom}px`;
  }

  function toggle() {
    if (open) {
      close();
    } else {
      open = true;
      menuEl.style.display = 'block';
      document.body.appendChild(menuEl);
      updateMenuPos();
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }

  function onDocumentClick(e: MouseEvent) {
    if (!open) return;
    if (triggerEl.contains(e.target as Node)) return;
    if (menuEl.contains(e.target as Node)) return;
    close();
  }

  onMount(() => {
    menuEl.style.display = 'none';
    document.addEventListener('click', onDocumentClick, false);
  });

  onDestroy(() => {
    document.removeEventListener('click', onDocumentClick, false);
    if (menuEl && menuEl.parentNode === document.body) {
      document.body.removeChild(menuEl);
    }
  });

  $: selectedLabel = options.find(o => o.value === value)?.label ?? value;
</script>

<svelte:window on:keydown={onKeydown} on:scroll={updateMenuPos} on:resize={updateMenuPos} />

<div class="dropdown {className}" class:open bind:this={triggerEl}>
  <button class="dropdown-trigger" onclick={toggle} type="button">
    <span>{selectedLabel}</span>
    <span class="dropdown-arrow">{open ? '▲' : '▼'}</span>
  </button>
</div>

<div bind:this={menuEl} class="dropdown-menu">
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
