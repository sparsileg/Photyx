<!-- MacroEditor.svelte — Spec §8.6 -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { ui } from '../../stores/ui';
    import { notifications } from '../../stores/notifications';

    let macroText = $state('');

    async function runMacro() {
        const lines = macroText.split('\n').filter(l => l.trim() && !l.trim().startsWith('#'));
        if (lines.length === 0) { notifications.warning('Macro editor is empty.'); return; }
        notifications.info('Running macro…');
        for (const line of lines) {
            const firstSpace = line.search(/\s/);
            const command = firstSpace === -1 ? line.trim() : line.slice(0, firstSpace);
            const rest = firstSpace === -1 ? '' : line.slice(firstSpace + 1).trim();
            const args: Record<string, string> = {};
            const argRe = /([A-Za-z_][A-Za-z0-9_]*)=(?:"([^"]*)"|(\S+))/g;
            let match;
            while ((match = argRe.exec(rest)) !== null) {
                args[match[1].toLowerCase()] = match[2] !== undefined ? match[2] : match[3];
            }
            try {
                await invoke('dispatch_command', { request: { command, args } });
            } catch (err) {
                notifications.error(`Error on '${command}': ${err}`);
                return;
            }
        }
        notifications.success('Macro complete.');
    }
</script>

<div class="sliding-panel active" style="height:100%;">
    <div class="panel-header">
        <span>Macro Editor</span>
        <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
    </div>
    <div class="macro-editor-wrap">
        <div class="macro-editor-toolbar">
            <button class="macro-btn run-btn" onclick={runMacro}>▶ Run</button>
            <button class="macro-btn" onclick={() => notifications.info('Save — Phase 5')}>Save…</button>
            <button class="macro-btn" onclick={() => notifications.info('Load — Phase 5')}>Load…</button>
        </div>
        <textarea
            id="macro-textarea"
            bind:value={macroText}
            spellcheck={false}
            placeholder="# pcode macro&#10;SelectDirectory path=&quot;/path/to/images&quot;&#10;ReadAllFITFiles&#10;AutoStretch method=asinh"
        ></textarea>
    </div>
</div>
