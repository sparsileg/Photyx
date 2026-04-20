// pcode.js — Photyx pcode interpreter (browser-side stub implementation)
// Implements the pcode language spec: tokenizer → dispatcher → executor → reporter
// Commands marked [STUB] will be wired to Tauri backend in production.

'use strict';

const PcodeInterpreter = (() => {

    // ── Session state ────────────────────────────────────────────────────────
    const state = {
        activeDirectory: null,
        fileList:        [],
        loadedImages:    [],
        currentFrame:    0,
        variables:       {},
        macros:          {},
    };

    // ── Output callback (set by console UI) ─────────────────────────────────
    let outputCallback = null;
    function setOutputCallback(fn) { outputCallback = fn; }

    function emit(text, type = 'output') {
        if (outputCallback) outputCallback(text, type);
    }

    // ── Tokenizer ────────────────────────────────────────────────────────────
    // Parses:  CommandName arg=value arg2="quoted string" flag
    // Returns: { command, args: {name: value, ...}, positional: [...] }
    function tokenize(line) {
        line = line.trim();
        if (!line || line.startsWith('#')) return null;

        // Variable substitution
        line = line.replace(/\$([A-Za-z_][A-Za-z0-9_]*)/g, (_, name) => {
            return state.variables[name] !== undefined ? state.variables[name] : `$${name}`;
        });

        // Split off the command name
        const firstSpace = line.search(/\s/);
        const command    = firstSpace === -1 ? line : line.slice(0, firstSpace);
        const rest       = firstSpace === -1 ? '' : line.slice(firstSpace + 1).trim();

        const args       = {};
        const positional = [];

        // Parse named args:  name=value  or  name="quoted value"
        const argRe = /([A-Za-z_][A-Za-z0-9_]*)=(?:"([^"]*)"|(\S+))/g;
        let match;
        let lastIndex = 0;

        while ((match = argRe.exec(rest)) !== null) {
            args[match[1].toLowerCase()] = match[2] !== undefined ? match[2] : match[3];
            lastIndex = argRe.lastIndex;
        }

        // Anything left that isn't a named arg is positional
        const bare = rest.replace(/([A-Za-z_][A-Za-z0-9_]*)=(?:"[^"]*"|\S+)/g, '').trim();
        if (bare) positional.push(...bare.split(/\s+/).filter(Boolean));

        return { command, args, positional };
    }

    // ── Helpers ──────────────────────────────────────────────────────────────
    function requireArg(args, name, cmd) {
        if (args[name] === undefined) throw new Error(`${cmd}: missing required argument '${name}'`);
        return args[name];
    }

    function coerceBool(v) {
        if (typeof v === 'boolean') return v;
        return v === 'true' || v === '1' || v === 'yes';
    }

    function now() {
        return new Date().toLocaleTimeString('en-US', { hour12: false });
    }

    // ── Command registry ─────────────────────────────────────────────────────
    const commands = {};

    function register(name, fn, aliases = []) {
        commands[name.toLowerCase()] = fn;
        for (const a of aliases) commands[a.toLowerCase()] = fn;
    }

    // ── File Management ──────────────────────────────────────────────────────

    register('selectdirectory', ({ args }) => {
        const path = requireArg(args, 'path', 'SelectDirectory');
        state.activeDirectory = path;
        state.fileList = [];
        state.loadedImages = [];
        emit(`Active directory: ${path}`, 'success');
        emit(`[STUB] File system not yet wired to Tauri backend.`, 'info');
        StatusBar.set(`Directory: ${path}`, 'info');
    });

    register('listfiles', ({ args }) => {
        if (!state.activeDirectory) { emit('No active directory. Use SelectDirectory first.', 'error'); return; }
        const filter = args.filter || null;
        if (state.fileList.length === 0) {
            emit(`[STUB] No files loaded. Directory: ${state.activeDirectory}`, 'info');
        } else {
            const shown = filter
                ? state.fileList.filter(f => f.includes(filter))
                : state.fileList;
            shown.forEach(f => emit(`  ${f}`, 'output'));
            emit(`${shown.length} file(s)`, 'success');
        }
    });

    register('filterbykeyword', ({ args }) => {
        requireArg(args, 'name', 'FilterByKeyword');
        requireArg(args, 'value', 'FilterByKeyword');
        emit(`[STUB] FilterByKeyword: name=${args.name} value=${args.value}`, 'info');
    });

    // ── I/O ─────────────────────────────────────────────────────────────────

    register('readallfitfiles', ({ args }) => {
        if (!state.activeDirectory) { emit('No active directory.', 'error'); return; }
        emit(`[STUB] ReadAllFITFiles from ${state.activeDirectory}`, 'info');
        emit('FITS reader plugin not yet loaded (Phase 1).', 'warning');
    });

    register('readallxisffiles', ({ args }) => {
        emit(`[STUB] ReadAllXISFFiles from ${state.activeDirectory || '(no directory)'}`, 'info');
    });

    register('readalltifffiles', ({ args }) => {
        emit(`[STUB] ReadAllTIFFFiles from ${state.activeDirectory || '(no directory)'}`, 'info');
    });

    register('writeallfitfiles', ({ args }) => {
        const dest = args.destination || state.activeDirectory;
        emit(`[STUB] WriteAllFITFiles → ${dest}`, 'info');
    });

    register('writeallxisffiles', ({ args }) => {
        emit(`[STUB] WriteAllXISFFiles → ${args.destination || '(no dest)'}`, 'info');
    });

    register('writealltifffiles', ({ args }) => {
        emit(`[STUB] WriteAllTIFFFiles → ${args.destination || '(no dest)'}`, 'info');
    });

    register('writepng', ({ args }) => {
        requireArg(args, 'filename', 'WritePNG');
        emit(`[STUB] WritePNG ${args.filename}`, 'info');
    });

    register('writejpeg', ({ args }) => {
        requireArg(args, 'filename', 'WriteJPEG');
        const q = args.quality || '100';
        emit(`[STUB] WriteJPEG ${args.filename} quality=${q}%`, 'info');
    });

    // ── Keyword operations ───────────────────────────────────────────────────

    register('addkeyword', ({ args }) => {
        requireArg(args, 'name', 'AddKeyword');
        requireArg(args, 'value', 'AddKeyword');
        const comment = args.comment ? ` / ${args.comment}` : '';
        emit(`AddKeyword: ${args.name.toUpperCase()} = ${args.value}${comment}`, 'success');
        emit('[STUB] Keyword written to active image buffer.', 'info');
    });

    register('deletekeyword', ({ args }) => {
        requireArg(args, 'name', 'DeleteKeyword');
        emit(`DeleteKeyword: ${args.name.toUpperCase()}`, 'success');
        emit('[STUB] Keyword removed from active image buffer.', 'info');
    });

    register('modifykeyword', ({ args }) => {
        requireArg(args, 'name', 'ModifyKeyword');
        requireArg(args, 'value', 'ModifyKeyword');
        emit(`ModifyKeyword: ${args.name.toUpperCase()} → ${args.value}`, 'success');
    });

    register('copykeyword', ({ args }) => {
        requireArg(args, 'from', 'CopyKeyword');
        requireArg(args, 'to', 'CopyKeyword');
        emit(`CopyKeyword: ${args.from.toUpperCase()} → ${args.to.toUpperCase()}`, 'success');
    });

    register('listkeywords', ({ args }) => {
        emit('[STUB] No image loaded — keyword list unavailable.', 'info');
    });

    register('getkeyword', ({ args }) => {
        requireArg(args, 'name', 'GetKeyword');
        emit(`[STUB] GetKeyword ${args.name.toUpperCase()} → undefined (no image loaded)`, 'info');
    });

    // ── Interrogation ────────────────────────────────────────────────────────

    register('getimageproperty', ({ args }) => {
        requireArg(args, 'property', 'GetImageProperty');
        emit(`[STUB] GetImageProperty ${args.property} → undefined (no image loaded)`, 'info');
    });

    register('getsessionproperty', ({ args }) => {
        const prop = requireArg(args, 'property', 'GetSessionProperty');
        const lprop = prop.toLowerCase();
        const map = {
            activedirectory:  state.activeDirectory || '(none)',
            filecount:        state.fileList.length.toString(),
            currentframe:     state.currentFrame.toString(),
            platform:         'Browser (Tauri target)',
            phototyxversion:  '1.0.0-dev',
        };
        const val = map[lprop] || 'undefined';
        emit(`${prop} = ${val}`, 'output');
        state.variables['Result'] = val;
    });

    register('test', ({ args, positional }) => {
        const expr = args.expression || positional.join(' ');
        emit(`[STUB] Test: ${expr} → (cannot evaluate without loaded image)`, 'info');
        state.variables['Result'] = 'false';
    });

    // ── Processing ───────────────────────────────────────────────────────────

    register('autostretch', ({ args }) => {
        const method = args.method || 'asinh';
        const sc     = args.shadowclip || '0.0';
        const tb     = args.targetbackground || '0.25';
        emit(`AutoStretch: method=${method} shadowClip=${sc} targetBackground=${tb}`, 'output');
        emit('[STUB] Stretch pipeline not yet wired (Phase 2).', 'info');
    });

    register('linearstretch', ({ args }) => {
        emit(`LinearStretch: black=${args.black || '0'} white=${args.white || '65535'}`, 'output');
        emit('[STUB] Stretch pipeline not yet wired (Phase 2).', 'info');
    });

    register('histogramequalization', ({ args }) => {
        emit('[STUB] HistogramEqualization not yet wired (Phase 2).', 'info');
    });

    register('cropimage', ({ args }) => {
        emit(`[STUB] CropImage x=${args.x} y=${args.y} w=${args.width} h=${args.height}`, 'info');
    });

    register('binimage', ({ args }) => {
        const factor = requireArg(args, 'factor', 'BinImage');
        emit(`[STUB] BinImage factor=${factor}`, 'info');
    });

    register('debayerimage', ({ args }) => {
        const method = args.method || 'bilinear';
        emit(`[STUB] DebayerImage method=${method}`, 'info');
    });

    // ── Blink & View ─────────────────────────────────────────────────────────

    register('blinksequence', ({ args }) => {
        const fps = args.fps || '2';
        emit(`[STUB] BlinkSequence fps=${fps}`, 'info');
    });

    register('cacheframes', ({ args }) => {
        emit('[STUB] CacheFrames — pre-decoding buffer pool (Phase 2).', 'info');
    });

    register('setzoom', ({ args }) => {
        const level = requireArg(args, 'level', 'SetZoom');
        emit(`SetZoom: ${level}`, 'output');
        StatusBar.set(`Zoom: ${level}`, 'info');
    });

    // ── Analysis ─────────────────────────────────────────────────────────────

    register('computefwhm', ({ args }) => {
        emit('[STUB] ComputeFWHM — WASM analysis plugin (Phase 7).', 'info');
    });

    register('countstars', ({ args }) => {
        emit('[STUB] CountStars — WASM analysis plugin (Phase 7).', 'info');
    });

    register('computeeccentricity', ({ args }) => {
        emit('[STUB] ComputeEccentricity — WASM analysis plugin (Phase 7).', 'info');
    });

    register('medianvalue', ({ args }) => {
        emit('[STUB] MedianValue — WASM analysis plugin (Phase 7).', 'info');
    });

    register('contourplot', ({ args }) => {
        emit('[STUB] ContourPlot (FWHM) — WASM analysis plugin (Phase 7).', 'info');
    });

    // ── Scripting ────────────────────────────────────────────────────────────

    register('set', ({ args, positional }) => {
        // Syntax: Set varname = value   OR  Set varname=value
        // Positional: ['varname', '=', 'value']  or named arg varname=value
        if (positional.length >= 3 && positional[1] === '=') {
            const name = positional[0];
            const val  = positional.slice(2).join(' ');
            state.variables[name] = val;
            emit(`${name} = ${val}`, 'output');
        } else if (Object.keys(args).length > 0) {
            for (const [k, v] of Object.entries(args)) {
                state.variables[k] = v;
                emit(`${k} = ${v}`, 'output');
            }
        } else {
            emit('Set: usage: Set varname = value', 'error');
        }
    });

    register('print', ({ args, positional }) => {
        const msg = args.message || positional.join(' ');
        emit(msg, 'output');
    });

    register('echo', ({ args, positional }) => {
        const varname = args.varname || positional[0];
        if (!varname) { emit('Echo: specify variable name', 'error'); return; }
        const val = state.variables[varname];
        emit(val !== undefined ? `${varname} = ${val}` : `${varname} is not set`, 'output');
    });

    register('countfiles', ({ args }) => {
        const n = state.fileList.length;
        state.variables['Result'] = String(n);
        emit(`FileCount = ${n}`, 'output');
    });

    register('runmacro', ({ args, positional }) => {
        const filename = args.filename || positional[0];
        emit(`[STUB] RunMacro ${filename} — macro library (Phase 5).`, 'info');
    });

    // ── Built-in help ────────────────────────────────────────────────────────

    register('help', ({ args, positional }) => {
        const topic = positional[0] || args.topic;
        if (topic) {
            emit(`Help for '${topic}' not yet implemented. See spec §7.8.`, 'info');
        } else {
            emit('Photyx pcode v1.0-dev  —  available commands:', 'output');
            emit('  File:     SelectDirectory ListFiles FilterByKeyword', 'output');
            emit('  I/O:      ReadAllFITFiles ReadAllXISFFiles ReadAllTIFFFiles', 'output');
            emit('            WriteAllFITFiles WriteAllXISFFiles WriteAllTIFFFiles WritePNG WriteJPEG', 'output');
            emit('  Keyword:  AddKeyword DeleteKeyword ModifyKeyword CopyKeyword ListKeywords GetKeyword', 'output');
            emit('  Query:    GetImageProperty GetSessionProperty Test', 'output');
            emit('  Process:  AutoStretch LinearStretch HistogramEqualization CropImage BinImage DebayerImage', 'output');
            emit('  View:     BlinkSequence CacheFrames SetZoom', 'output');
            emit('  Analysis: ComputeFWHM CountStars ComputeEccentricity MedianValue ContourPlot', 'output');
            emit('  Script:   Set Print Echo CountFiles RunMacro', 'output');
            emit('  System:   Help Clear Version', 'output');
            emit('Type "Help CommandName" for details.', 'info');
        }
    });

    register('clear', ({ args }) => {
        if (outputCallback) outputCallback(null, 'clear');
    });

    register('version', ({ args }) => {
        emit('Photyx 1.0.0-dev  |  pcode interpreter v1.0  |  Build: browser-stub', 'output');
    });

    // ── Dispatcher ───────────────────────────────────────────────────────────

    function dispatch(parsed) {
        const fn = commands[parsed.command.toLowerCase()];
        if (!fn) {
            throw new Error(`Unknown command: '${parsed.command}'. Type Help for a command list.`);
        }
        fn(parsed);
    }

    // ── Public: execute a single line or multi-line macro string ─────────────

    function executeLine(line) {
        try {
            const parsed = tokenize(line);
            if (!parsed) return;   // blank or comment
            dispatch(parsed);
        } catch (err) {
            emit(err.message, 'error');
        }
    }

    function executeMacro(text) {
        const lines = text.split('\n');
        // TODO Phase 5: implement If/For/ForEach block tracking
        for (const line of lines) {
            executeLine(line);
        }
    }

    // ── Tab completion ───────────────────────────────────────────────────────

    const ALL_COMMANDS = [
        'SelectDirectory','ListFiles','FilterByKeyword',
        'ReadAllFITFiles','ReadAllXISFFiles','ReadAllTIFFFiles',
        'WriteAllFITFiles','WriteAllXISFFiles','WriteAllTIFFFiles','WritePNG','WriteJPEG',
        'AddKeyword','DeleteKeyword','ModifyKeyword','CopyKeyword','ListKeywords','GetKeyword',
        'GetImageProperty','GetSessionProperty','Test',
        'AutoStretch','LinearStretch','HistogramEqualization','CropImage','BinImage','DebayerImage',
        'BlinkSequence','CacheFrames','SetZoom',
        'ComputeFWHM','CountStars','ComputeEccentricity','MedianValue','ContourPlot',
        'Set','Print','Echo','CountFiles','RunMacro',
        'Help','Clear','Version',
    ];

    function complete(partial) {
        if (!partial) return [];
        const lower = partial.toLowerCase();
        return ALL_COMMANDS.filter(c => c.toLowerCase().startsWith(lower));
    }

    return {
        executeLine,
        executeMacro,
        complete,
        setOutputCallback,
        getState: () => ({ ...state }),
        ALL_COMMANDS,
    };

})();
