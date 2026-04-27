<!-- LogViewer.svelte — Log file viewer modal. Spec §8.17 -->
<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';

    let { onclose } = $props<{ onclose: () => void }>();

    interface LogFile {
        filename:      string;
        path:          string;
        size:          number;
        modified_secs: number;
    }

    interface LogLine {
        timestamp: string;
        level:     string;
        module:    string;
        message:   string;
        raw:       string;
    }

    let files          = $state<LogFile[]>([]);
    let lines          = $state<LogLine[]>([]);
    let selectedFile   = $state<LogFile | null>(null);
    let loading        = $state(false);
    let showFilePicker = $state(true);
    let outputEl       = $state<HTMLDivElement>();

    // Level toggles
    let showError = $state(true);
    let showWarn  = $state(true);
    let showInfo  = $state(true);
    let showDebug = $state(true);

    // Auto-tail state
    let tailInterval: ReturnType<typeof setInterval> | null = null;
    let userScrolledUp = false;
    let lastLineCount  = 0;
    let tailing        = $state(false);

    let filtered = $derived(lines.filter(l => {
        if (l.level === 'ERROR') return showError;
        if (l.level === 'WARN')  return showWarn;
        if (l.level === 'DEBUG') return showDebug;
        if (l.level === 'RAW')   return showDebug;
        return showInfo;
    }));

    function formatSize(bytes: number): string {
        if (bytes < 1024) return `${bytes} B`;
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
        return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    }

    function formatModified(secs: number): string {
        return new Date(secs * 1000).toLocaleString('en-US', {
            year:   '2-digit',
            month:  '2-digit',
            day:    '2-digit',
            hour:   '2-digit',
            minute: '2-digit',
            hour12: false,
        });
    }

    async function loadFileList() {
        try {
            files = await invoke<LogFile[]>('list_log_files');
        } catch (e) {
            console.error('list_log_files error:', e);
        }
    }

    async function selectFile(file: LogFile) {
        selectedFile = file;
        showFilePicker = false;
        loading = true;
        userScrolledUp = false;
        lastLineCount = 0;
        stopTail();
        try {
            lines = await invoke<LogLine[]>('read_log_file', { path: file.path });
            lastLineCount = lines.length;
            startTail();
        } catch (e) {
            console.error('read_log_file error:', e);
        } finally {
            loading = false;
            scrollToBottom();
        }
    }

    function startTail() {
        stopTail();
        tailing = true;
        tailInterval = setInterval(async () => {
            if (!selectedFile) return;
            try {
                const fresh = await invoke<LogLine[]>('read_log_file', { path: selectedFile.path });
                if (fresh.length > lastLineCount) {
                    lines = fresh;
                    lastLineCount = fresh.length;
                    if (!userScrolledUp) scrollToBottom();
                }
            } catch { /* ignore */ }
        }, 2000);
    }

    function stopTail() {
        tailing = false;
        if (tailInterval !== null) {
            clearInterval(tailInterval);
            tailInterval = null;
        }
    }

    function scrollToBottom() {
        setTimeout(() => {
            if (outputEl) outputEl.scrollTop = outputEl.scrollHeight;
        }, 0);
    }

    function onOutputScroll() {
        if (!outputEl) return;
        const atBottom = outputEl.scrollHeight - outputEl.scrollTop - outputEl.clientHeight < 20;
        userScrolledUp = !atBottom;
    }

    function levelClass(level: string): string {
        switch (level) {
            case 'ERROR': return 'lv-error';
            case 'WARN':  return 'lv-warn';
            case 'DEBUG': return 'lv-debug';
            case 'RAW':   return 'lv-debug';
            default:      return 'lv-info';
        }
    }

    onMount(async () => {
        await loadFileList();
    });

    onDestroy(() => {
        stopTail();
    });
</script>

<div class="modal-overlay" onclick={onclose}>
    <div class="modal-box lv-wide" onclick={(e) => e.stopPropagation()}>

        <!-- Header -->
        <div class="modal-header">
            <span class="modal-title">Log Viewer</span>
            {#if !showFilePicker && selectedFile}
                <span class="lv-selected-name">{selectedFile.filename}</span>
                <button class="lv-change-btn" onclick={() => showFilePicker = true}>Change…</button>
            {/if}
            <span class="modal-close" onclick={onclose}>✕</span>
        </div>

        <!-- File picker -->
        {#if showFilePicker}
            <div class="modal-body">
                {#if files.length === 0}
                    <div class="modal-loading">No log files found.</div>
                {:else}
                    <table class="kw-table">
                        <thead>
                            <tr>
                                <th>Filename</th>
                                <th>Modified</th>
                                <th>Size</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each files as f}
                                <tr
                                    class:lv-picker-selected={selectedFile?.path === f.path}
                                    onclick={() => selectFile(f)}
                                    style="cursor: pointer"
                                >
                                    <td class="kw-name">{f.filename}</td>
                                    <td class="kw-value">{formatModified(f.modified_secs)}</td>
                                    <td class="kw-comment">{formatSize(f.size)}</td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                {/if}
            </div>

        <!-- Log contents -->
        {:else}
            <!-- Level toggles -->
            <div class="lv-toggles">
                <label class="lv-check lv-check-error">
                    <input type="checkbox" bind:checked={showError} />
                    <span>ERROR</span>
                </label>
                <label class="lv-check lv-check-warn">
                    <input type="checkbox" bind:checked={showWarn} />
                    <span>WARN</span>
                </label>
                <label class="lv-check lv-check-info">
                    <input type="checkbox" bind:checked={showInfo} />
                    <span>INFO</span>
                </label>
                <label class="lv-check lv-check-debug">
                    <input type="checkbox" bind:checked={showDebug} />
                    <span>DEBUG</span>
                </label>
                <span class="lv-line-count">{filtered.length} lines</span>
            </div>

            <!-- Log output — reuses modal-body for correct scroll behaviour -->
            <div
                class="modal-body lv-output"
                bind:this={outputEl}
                onscroll={onOutputScroll}
            >
                {#if loading}
                    <div class="modal-loading">Loading…</div>
                {:else if filtered.length === 0}
                    <div class="modal-loading">No entries match the active filters.</div>
                {:else}
                    <table class="kw-table">
                        <thead>
                            <tr>
                                <th style="width:220px">Timestamp</th>
                                <th style="width:48px">Level</th>
                                <th>Message</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each filtered as line}
                                <tr class={levelClass(line.level)}>
                                    <td class="lv-ts">{line.timestamp}</td>
                                    <td class="lv-level">{line.level}</td>
                                    <td class="lv-msg">{line.message}</td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                {/if}
            </div>
        {/if}

        <!-- Footer -->
        <div class="modal-footer lv-footer">
            {#if selectedFile && tailing}
                <span class="lv-tail-indicator">⏺ tailing</span>
            {/if}
            {#if selectedFile}
                {filtered.length} lines visible · {selectedFile.filename}
            {:else}
                {files.length} log file{files.length !== 1 ? 's' : ''} available
            {/if}
        </div>

    </div>
</div>
