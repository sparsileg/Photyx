// Ambient type augmentation. See Issue 94.
//
// Adds the non-standard `autocorrect` attribute to Svelte's built-in
// textarea typings. WebKit-only (WKWebView on macOS, WebKitGTK on Linux;
// no-op on Chromium/WebView2 on Windows) — used to suppress autocorrect
// on pcode input textareas (Console.svelte, MacroEditor.svelte) where
// browser text assistance would corrupt command syntax.
//
// Imported for its side effect only, from +page.svelte — this project's
// src-svelte/ layout means SvelteKit's generated tsconfig doesn't pick up
// an ambient app.d.ts at the conventional location, so we rely on the
// import graph instead of file-discovery convention.
export {};

declare module 'svelte/elements' {
  export interface HTMLTextareaAttributes {
    autocorrect?: 'on' | 'off';
  }
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
