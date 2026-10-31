import init, { diff_fonts_wasm } from '../pkg/font_diff.js';

let wasmInit = null;

function ensureWasm() {
    if (!wasmInit) {
        wasmInit = init().catch(error => {
            wasmInit = null;
            throw error;
        });
    }
    return wasmInit;
}

function errorMessage(error) {
    return error && typeof error.message === 'string' ? error.message : String(error);
}

function compareWithLogs(id, oldData, newData) {
    const originalLog = console.log;
    try {
        // Rust currently writes progress through web_sys::console. Keep the
        // bridge local to this synchronous call, including on WASM exceptions.
        console.log = (...args) => {
            const message = args.map(String).join(' ');
            self.postMessage({ type: 'log', id, message });
        };
        return diff_fonts_wasm(oldData, newData);
    } finally {
        console.log = originalLog;
    }
}

self.addEventListener('message', async ({ data }) => {
    if (data.type === 'init') {
        try {
            await ensureWasm();
            self.postMessage({ type: 'ready' });
        } catch (error) {
            self.postMessage({ type: 'init-error', message: errorMessage(error) });
        }
        return;
    }

    if (data.type !== 'diff') return;
    const { id, oldData, newData } = data;
    try {
        await ensureWasm();
        const result = compareWithLogs(id, oldData, newData);
        self.postMessage({ type: 'result', id, result });
    } catch (error) {
        self.postMessage({ type: 'error', id, message: errorMessage(error) });
    }
});
