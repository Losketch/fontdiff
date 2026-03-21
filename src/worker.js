import init, { diff_fonts_wasm } from '../pkg/font_diff.js';

let wasmReady = false;

async function ensureWasm() {
    if (!wasmReady) {
        await init();
        wasmReady = true;
    }
}

self.onmessage = async function(e) {
    const { type, oldData, newData, id } = e.data;

    if (type === 'diff') {
        try {
            await ensureWasm();

            const originalLog = console.log;
            const logs = [];

            console.log = (...args) => {
                const msg = args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ');
                logs.push(msg);
                self.postMessage({ type: 'log', id, message: msg });
            };

            const result = diff_fonts_wasm(oldData, newData);

            console.log = originalLog;

            self.postMessage({ type: 'result', id, result, logs });
        } catch (err) {
            self.postMessage({ type: 'error', id, message: err.message });
        }
    }
};

self.postMessage({ type: 'ready' });
