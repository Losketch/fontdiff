import '@material/web/button/filled-button.js';
import '@material/web/progress/linear-progress.js';
import './styles.css';

let init = null;
let diffFontsWasm = null;
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
const errorDiv = document.getElementById('error');
const resultActions = document.getElementById('result-actions');
const downloadBtn = document.getElementById('download-btn');
const newTabBtn = document.getElementById('new-tab-btn');

async function initWasm() {
    try {
        const module = await import('../pkg/font_diff.js');
        init = module.default;
        diffFontsWasm = module.diff_fonts_wasm;
        await init();
        
        loadingInit.style.display = 'none';
        uploadSection.style.display = 'block';
    } catch (err) {
        showError('初始化 WASM 失败: ' + err.message);
        console.error(err);
    }
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
    diffBtn.disabled = !(oldFontData && newFontData);
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

diffBtn.addEventListener('click', async () => {
    loading.style.display = 'block';
    errorDiv.style.display = 'none';
    resultActions.style.display = 'none';

    try {
        currentHtmlResult = diffFontsWasm(oldFontData, newFontData);
        resultActions.style.display = 'flex';
    } catch (err) {
        showError('生成 Diff 失败: ' + err.message);
        console.error(err);
    } finally {
        loading.style.display = 'none';
    }
});

downloadBtn.addEventListener('click', downloadHtml);
newTabBtn.addEventListener('click', openInNewTab);

initWasm();
