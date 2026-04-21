import init, { washImage } from "./pkg/image_washer.js";

const fileInput = document.querySelector("#file-input");
const dropzone = document.querySelector("#dropzone");
const washButton = document.querySelector("#wash-button");
const downloadAllButton = document.querySelector("#download-all-button");
const statusPill = document.querySelector("#status-pill");
const queueSummary = document.querySelector("#queue-summary");
const resultList = document.querySelector("#result-list");
const template = document.querySelector("#result-template");

const state = {
  ready: false,
  processing: false,
  files: [],
  results: [],
};

boot();

async function boot() {
  bindEvents();
  registerServiceWorker();

  try {
    await init();
    state.ready = true;
    setStatus("準備完了", "ready");
  } catch (error) {
    console.error(error);
    setStatus("WASM の初期化に失敗", "error");
  }

  syncButtons();
}

function bindEvents() {
  dropzone.addEventListener("click", () => fileInput.click());
  dropzone.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      fileInput.click();
    }
  });

  fileInput.addEventListener("change", () => {
    updateFiles([...fileInput.files]);
  });

  dropzone.addEventListener("dragover", (event) => {
    event.preventDefault();
    dropzone.classList.add("is-dragover");
  });

  dropzone.addEventListener("dragleave", () => {
    dropzone.classList.remove("is-dragover");
  });

  dropzone.addEventListener("drop", (event) => {
    event.preventDefault();
    dropzone.classList.remove("is-dragover");
    updateFiles([...event.dataTransfer.files]);
  });

  washButton.addEventListener("click", processFiles);
  downloadAllButton.addEventListener("click", downloadAll);
  window.addEventListener("beforeunload", clearResultUrls);
}

function updateFiles(files) {
  clearResultUrls();
  state.files = files.filter((file) => file.size > 0);
  state.results = [];
  renderResults();
  syncButtons();

  if (state.files.length === 0) {
    queueSummary.textContent = "まだ画像はありません";
    return;
  }

  queueSummary.textContent = `${state.files.length} 枚を待機中`;
  setStatus("画像を選択済み", state.ready ? "ready" : "");
}

async function processFiles() {
  if (!state.ready || state.processing || state.files.length === 0) {
    return;
  }

  state.processing = true;
  clearResultUrls();
  state.results = state.files.map((file) => ({
    name: file.name,
    sourceSize: file.size,
    status: "processing",
    message: "洗浄中",
    outputName: nextName(file.name),
    outputSize: null,
    url: null,
  }));
  renderResults();
  syncButtons();
  setStatus("洗浄中", "");

  try {
    for (const [index, file] of state.files.entries()) {
      try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        const washed = washImage(bytes, file.name);
        const blob = new Blob([washed], { type: file.type || inferMime(file.name) });

        state.results[index] = {
          ...state.results[index],
          status: "done",
          message: "準備完了",
          outputSize: blob.size,
          blob,
          url: URL.createObjectURL(blob),
        };
      } catch (error) {
        state.results[index] = {
          ...state.results[index],
          status: "failed",
          message: error?.message || String(error),
        };
      }

      renderResults();
    }
  } finally {
    state.processing = false;
    const doneCount = state.results.filter((item) => item.status === "done").length;
    const failedCount = state.results.filter((item) => item.status === "failed").length;
    queueSummary.textContent = `${doneCount} 枚成功 / ${failedCount} 枚失敗`;
    setStatus(
      failedCount > 0 ? "一部失敗あり" : "洗浄完了",
      failedCount > 0 ? "error" : "ready",
    );
    syncButtons();
  }
}

function registerServiceWorker() {
  if (!("serviceWorker" in navigator)) {
    return;
  }

  window.addEventListener("load", () => {
    navigator.serviceWorker.register("./sw.js").catch((error) => {
      console.warn("Service worker registration failed", error);
    });
  });
}

function renderResults() {
  resultList.textContent = "";

  for (const result of state.results) {
    const node = template.content.firstElementChild.cloneNode(true);
    node.querySelector(".filename").textContent = result.outputName;
    node.querySelector(".meta").textContent = buildMetaText(result);

    const stateNode = node.querySelector(".result-state");
    stateNode.textContent = result.message;
    stateNode.classList.toggle("done", result.status === "done");
    stateNode.classList.toggle("failed", result.status === "failed");

    const button = node.querySelector(".download-button");
    button.disabled = result.status !== "done";
    button.addEventListener("click", () => downloadResult(result));

    resultList.appendChild(node);
  }
}

function buildMetaText(result) {
  const parts = [`元: ${formatSize(result.sourceSize)}`];
  if (result.outputSize != null) {
    parts.push(`洗浄後: ${formatSize(result.outputSize)}`);
  }
  return parts.join("  /  ");
}

function downloadResult(result) {
  if (!result.url) {
    return;
  }

  const link = document.createElement("a");
  link.href = result.url;
  link.download = result.outputName;
  link.click();
}

function downloadAll() {
  const successful = state.results.filter((result) => result.status === "done" && result.blob);
  if (successful.length === 0) {
    return;
  }

  void downloadZip(successful);
}

function clearResultUrls() {
  for (const result of state.results) {
    if (result.url) {
      URL.revokeObjectURL(result.url);
    }
  }
}

function syncButtons() {
  washButton.disabled = !state.ready || state.processing || state.files.length === 0;
  downloadAllButton.disabled =
    state.processing || state.results.every((item) => item.status !== "done");
}

function setStatus(text, kind) {
  statusPill.textContent = text;
  statusPill.classList.remove("ready", "error");
  if (kind) {
    statusPill.classList.add(kind);
  }
}

function nextName(fileName) {
  const lastDot = fileName.lastIndexOf(".");
  if (lastDot <= 0) {
    return `${fileName}.washed`;
  }
  return `${fileName.slice(0, lastDot)}.washed${fileName.slice(lastDot)}`;
}

function formatSize(size) {
  if (size < 1024) {
    return `${size} B`;
  }
  if (size < 1024 * 1024) {
    return `${(size / 1024).toFixed(1)} KB`;
  }
  return `${(size / (1024 * 1024)).toFixed(2)} MB`;
}

function inferMime(fileName) {
  const extension = fileName.split(".").pop()?.toLowerCase();
  switch (extension) {
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "png":
      return "image/png";
    case "webp":
      return "image/webp";
    case "tif":
    case "tiff":
      return "image/tiff";
    case "bmp":
      return "image/bmp";
    case "gif":
      return "image/gif";
    default:
      return "application/octet-stream";
  }
}

async function downloadZip(results) {
  const entries = [];

  for (const result of results) {
    entries.push({
      name: result.outputName,
      data: new Uint8Array(await result.blob.arrayBuffer()),
    });
  }

  const blob = new Blob([buildZip(entries)], { type: "application/zip" });
  const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
  const link = document.createElement("a");
  const url = URL.createObjectURL(blob);
  link.href = url;
  link.download = `imageWasher-${stamp}.zip`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function buildZip(entries) {
  const fileRecords = [];
  const centralRecords = [];
  let offset = 0;

  for (const entry of entries) {
    const nameBytes = encodeUtf8(entry.name);
    const crc = crc32(entry.data);
    const local = concatUint8Arrays(
      u32(0x04034b50),
      u16(20),
      u16(0x0800),
      u16(0),
      u16(0),
      u16(0),
      u32(crc),
      u32(entry.data.length),
      u32(entry.data.length),
      u16(nameBytes.length),
      u16(0),
      nameBytes,
      entry.data,
    );
    fileRecords.push(local);

    const central = concatUint8Arrays(
      u32(0x02014b50),
      u16(20),
      u16(20),
      u16(0x0800),
      u16(0),
      u16(0),
      u16(0),
      u32(crc),
      u32(entry.data.length),
      u32(entry.data.length),
      u16(nameBytes.length),
      u16(0),
      u16(0),
      u16(0),
      u16(0),
      u32(0),
      u32(offset),
      nameBytes,
    );
    centralRecords.push(central);
    offset += local.length;
  }

  const centralDirectory = concatUint8Arrays(...centralRecords);
  const end = concatUint8Arrays(
    u32(0x06054b50),
    u16(0),
    u16(0),
    u16(entries.length),
    u16(entries.length),
    u32(centralDirectory.length),
    u32(offset),
    u16(0),
  );

  return concatUint8Arrays(...fileRecords, centralDirectory, end);
}

function crc32(bytes) {
  let crc = 0xffffffff;

  for (const byte of bytes) {
    crc ^= byte;
    for (let i = 0; i < 8; i += 1) {
      const mask = -(crc & 1);
      crc = (crc >>> 1) ^ (0xedb88320 & mask);
    }
  }

  return (crc ^ 0xffffffff) >>> 0;
}

function encodeUtf8(text) {
  return new TextEncoder().encode(text);
}

function u16(value) {
  const bytes = new Uint8Array(2);
  new DataView(bytes.buffer).setUint16(0, value, true);
  return bytes;
}

function u32(value) {
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setUint32(0, value, true);
  return bytes;
}

function concatUint8Arrays(...chunks) {
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const merged = new Uint8Array(total);
  let offset = 0;

  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.length;
  }

  return merged;
}
