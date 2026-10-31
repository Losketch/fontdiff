// Owns the Worker protocol; the page only deals with promises and progress text.
export default class DiffClient {
    constructor() {
        this.worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
        this.pending = new Map();
        this.nextId = 0;
        this.closed = false;
        this.ready = new Promise((resolve, reject) => {
            this.resolveReady = resolve;
            this.rejectReady = reject;
        });

        this.worker.addEventListener('message', ({ data: message }) => {
            if (message.type === 'ready') {
                this.resolveReady();
                return;
            }
            if (message.type === 'init-error') {
                this.fail(new Error(message.message || 'WASM initialization failed'));
                return;
            }

            const request = this.pending.get(message.id);
            if (!request) return;
            if (message.type === 'log') {
                request.onLog?.(message.message);
            } else if (message.type === 'result' || message.type === 'error') {
                this.pending.delete(message.id);
                if (message.type === 'result') request.resolve(message.result);
                else request.reject(new Error(message.message || 'Font comparison failed'));
            }
        });
        this.worker.addEventListener('error', error => {
            this.fail(new Error(error.message || 'Worker failed'));
        });
        this.worker.addEventListener('messageerror', () => {
            this.fail(new Error('Invalid Worker message'));
        });
        this.worker.postMessage({ type: 'init' });
    }

    fail(error) {
        if (this.closed) return;
        this.closed = true;
        this.rejectReady(error);
        for (const request of this.pending.values()) request.reject(error);
        this.pending.clear();
        this.worker.terminate();
        this.onFatal?.(error);
    }

    async compare(oldData, newData, onLog) {
        if (this.closed) throw new Error('Worker is unavailable');
        await this.ready;
        if (this.closed) throw new Error('Worker is unavailable');
        const id = ++this.nextId;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject, onLog });
            try {
                // No transfer list: both buffers must stay usable for another comparison.
                this.worker.postMessage({ type: 'diff', id, oldData, newData });
            } catch (error) {
                this.pending.delete(id);
                reject(error);
            }
        });
    }

    terminate() {
        this.fail(new Error('Worker has been terminated'));
    }
}
