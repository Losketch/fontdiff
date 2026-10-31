import '@material/web/button/filled-button.js';
import '@material/web/progress/linear-progress.js';
import './styles.css';
import DiffClient from './diff-client.js';

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

const client = new DiffClient();
const fonts = { old: null, new: null };
const fileReads = { old: 0, new: 0 };
let ready = false;
let running = false;
let revision = 0;
let currentHtmlResult = null;
let errorTimer = null;

function updateButtonState() {
    diffBtn.disabled = !ready || running || !fonts.old || !fonts.new;
}

function hideError() {
    clearTimeout(errorTimer);
    errorDiv.style.display = 'none';
}

function showError(message) {
    hideError();
    errorDiv.textContent = message;
    errorDiv.style.display = 'block';
    errorTimer = setTimeout(() => { errorDiv.style.display = 'none'; }, 5000);
}

function invalidateResult() {
    revision++;
    currentHtmlResult = null;
    resultActions.style.display = 'none';
    progressDiv.textContent = '';
    progressDiv.style.display = 'none';
    hideError();
}

async function selectFont(side, input, label) {
    const file = input.files?.[0];
    if (!file) return;
    const readId = ++fileReads[side];
    invalidateResult();
    fonts[side] = null;
    label.textContent = file.name;
    updateButtonState();
    try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        if (readId !== fileReads[side]) return;
        fonts[side] = bytes;
    } catch (error) {
        if (readId !== fileReads[side]) return;
        label.textContent = 'File not selected';
        showError(`读取${side === 'old' ? '旧' : '新'}字体文件失败: ${error.message}`);
        console.error(error);
    } finally {
        updateButtonState();
    }
}

async function generateDiff() {
    if (!ready || running || !fonts.old || !fonts.new) return;
    running = true;
    invalidateResult();
    // invalidateResult changes the revision; capture the version of this run.
    const taskRevision = revision;
    progressDiv.style.display = 'block';
    loading.style.display = 'block';
    updateButtonState();

    try {
        const html = await client.compare(fonts.old, fonts.new, message => {
            if (taskRevision !== revision) return;
            progressDiv.textContent += message + '\n';
            progressDiv.scrollTop = progressDiv.scrollHeight;
        });
        if (taskRevision === revision) {
            currentHtmlResult = html;
            resultActions.style.display = 'flex';
        }
    } catch (error) {
        if (taskRevision === revision) showError('生成 Diff 失败: ' + error.message);
    } finally {
        running = false;
        loading.style.display = 'none';
        updateButtonState();
    }
}

function reportUrl() {
    return URL.createObjectURL(new Blob([currentHtmlResult], { type: 'text/html' }));
}

function downloadHtml() {
    if (!currentHtmlResult) return;
    const url = reportUrl();
    const link = document.createElement('a');
    link.href = url;
    link.download = 'font-diff.html';
    document.body.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

function openInNewTab() {
    if (!currentHtmlResult) return;
    const url = reportUrl();
    const tab = window.open(url, '_blank');
    if (!tab) showError('浏览器阻止了新窗口，请允许弹出窗口后重试。');
    setTimeout(() => URL.revokeObjectURL(url), tab ? 60_000 : 0);
}

oldFontBtn.addEventListener('click', () => oldFontInput.click());
newFontBtn.addEventListener('click', () => newFontInput.click());
oldFontInput.addEventListener('change', () => selectFont('old', oldFontInput, oldFontName));
newFontInput.addEventListener('change', () => selectFont('new', newFontInput, newFontName));
diffBtn.addEventListener('click', generateDiff);
downloadBtn.addEventListener('click', downloadHtml);
newTabBtn.addEventListener('click', openInNewTab);
window.addEventListener('beforeunload', () => client.terminate());

client.onFatal = error => {
    if (!ready) return; // The initialization handler reports its own error.
    ready = false;
    updateButtonState();
    showError('Worker 不可用: ' + error.message);
};

client.ready.then(() => {
    ready = true;
    loadingInit.style.display = 'none';
    uploadSection.style.display = 'block';
    updateButtonState();
}).catch(error => {
    loadingInit.textContent = 'WASM 初始化失败';
    showError('初始化失败: ' + error.message);
});
