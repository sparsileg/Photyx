<!-- QuickLaunch.svelte — Spec §8.4 -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { notifications } from '../stores/notifications';

    const macros = [
        { label: 'Auto-STF',    cmd: 'AutoStretch', args: { method: 'asinh' } },
        { label: 'Blink Start', cmd: 'BlinkSequence', args: { fps: '2' } },
        { label: 'List Files',  cmd: 'ListFiles', args: {} },
        { label: 'List KW',     cmd: 'ListKeywords', args: {} },
        { label: 'FWHM',        cmd: 'ComputeFWHM', args: {} },
        { label: 'Star Count',  cmd: 'CountStars', args: {} },
    ];

    async function run(cmd: string, args: Record<string, string>) {
        try {
            const response = await invoke<{ success: boolean; output: string | null; error: string | null }>(
                'dispatch_command', { request: { command: cmd, args } }
            );
            if (response.success && response.output) notifications.success(response.output);
            else if (!response.success) notifications.error(response.error ?? 'Error');
        } catch (err) {
            notifications.error(`${cmd}: ${err}`);
        }
    }
</script>

<div id="quick-launch">
    <div id="ql-buttons">
        {#each macros as macro}
            <button class="ql-btn" onclick={() => run(macro.cmd, macro.args)}>{macro.label}</button>
        {/each}
    </div>
</div>
