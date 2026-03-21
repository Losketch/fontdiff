import '@material/web/button/filled-button.js';
import '@material/web/progress/linear-progress.js';
import './styles.css';

let worker = null;
let workerReady = false;
let oldFontData = null;
let newFontData = null;
let currentHtmlResult = null;

const loadingInit = document.getElementById('loading-init');
const uploadSection = document.getElementById('upload-section');
const oldFontBtn = document.getElementById('old-font-btn');
const newFontBtn = document.getElementById('new-font-btn');
const oldFontInput = document.getElementById('old-font-input');
const newFontInput = document.getElementById('new-font-input');
const oldFontName = document.getElementById('old-font-name');
const newFontName = document.getElementById('new-font-name');
const diffBtn = document.getElementById('diff-btn');
const loading = document.getElementById('loading');
const progressDiv = document.getElementById('progress');
const errorDiv = document.getElementById('error');
const resultActions = document.getElementById('result-actions');
const downloadBtn = document.getElementById('download-btn');
const newTabBtn = document.getElementById('new-tab-btn');

function initWorker() {
    worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });

    worker.onmessage = function(e) {
        const { type, message, result, logs } = e.data;

        if (type === 'ready') {
            workerReady = true;
            loadingInit.style.display = 'none';
            uploadSection.style.display = 'block';
            return;
        }

        if (type === 'log') {
            progressDiv.textContent += message + '\n';
            progressDiv.scrollTop = progressDiv.scrollHeight;
            return;
        }

        if (type === 'result') {
            loading.style.display = 'none';
            diffBtn.disabled = false;
            currentHtmlResult = result;
            resultActions.style.display = 'flex';
            return;
        }

        if (type === 'error') {
            loading.style.display = 'none';
            diffBtn.disabled = false;
            showError('生成 Diff 失败: ' + message);
            return;
        }
    };

    worker.onerror = function(err) {
        loading.style.display = 'none';
        diffBtn.disabled = false;
        showError('Worker error: ' + err.message);
    };
}

function readFileAsArrayBuffer(file) {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = (e) => resolve(new Uint8Array(e.target.result));
        reader.onerror = reject;
        reader.readAsArrayBuffer(file);
    });
}

function updateButtonState() {
    diffBtn.disabled = !(oldFontData && newFontData && workerReady);
}

function showError(message) {
    errorDiv.textContent = message;
    errorDiv.style.display = 'block';
    setTimeout(() => {
        errorDiv.style.display = 'none';
    }, 5000);
}

function downloadHtml() {
    if (!currentHtmlResult) return;

    const blob = new Blob([currentHtmlResult], { type: 'text/html' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'font-diff.html';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

function openInNewTab() {
    if (!currentHtmlResult) return;

    const blob = new Blob([currentHtmlResult], { type: 'text/html' });
    const url = URL.createObjectURL(blob);
    window.open(url, '_blank');
}

oldFontBtn.addEventListener('click', () => oldFontInput.click());
newFontBtn.addEventListener('click', () => newFontInput.click());

oldFontInput.addEventListener('change', async (e) => {
    const file = e.target.files[0];
    if (file) {
        oldFontName.textContent = file.name;
        try {
            oldFontData = await readFileAsArrayBuffer(file);
            updateButtonState();
        } catch (err) {
            showError('读取旧字体文件失败');
            console.error(err);
        }
    }
});

newFontInput.addEventListener('change', async (e) => {
    const file = e.target.files[0];
    if (file) {
        newFontName.textContent = file.name;
        try {
            newFontData = await readFileAsArrayBuffer(file);
            updateButtonState();
        } catch (err) {
            showError('读取新字体文件失败');
            console.error(err);
        }
    }
});

diffBtn.addEventListener('click', () => {
    if (!oldFontData || !newFontData) return;

    progressDiv.textContent = '';
    progressDiv.style.display = 'block';
    loading.style.display = 'block';
    diffBtn.disabled = true;
    errorDiv.style.display = 'none';
    resultActions.style.display = 'none';

    worker.postMessage({
        type: 'diff',
        id: Date.now(),
        oldData: oldFontData,
        newData: newFontData
    });
});

downloadBtn.addEventListener('click', downloadHtml);
newTabBtn.addEventListener('click', openInNewTab);

initWorker();
