import "./styles.scss";
import { CompressionQueue } from "./queue/compression-queue";
import { downloadJob, downloadReport, downloadZip } from "./services/downloads";
import type { CompressionJob, CompressionProfile, ProgressStage } from "./types";
import { openComparison } from "./ui/compare-modal";

const app = document.querySelector<HTMLDivElement>("#app")!;
app.innerHTML = `
  <header class="site-header">
    <a class="brand" href="#main" aria-label="Squeeze — strona główna">
      <span class="brand-mark" aria-hidden="true">${sparkIcon()}</span>
      <span>Squeeze</span>
    </a>
    <div class="privacy-pill">${lockIcon()} <span>100% lokalnie</span></div>
  </header>
  <main id="main">
    <section class="hero" aria-labelledby="hero-title">
      <p class="eyebrow">Mniej bajtów. Ten sam obraz.</p>
      <h1 id="hero-title">Kompresja, która<br /><span>patrzy jak człowiek.</span></h1>
      <p class="hero-copy">Squeeze analizuje każdy obraz, sprawdza jakość piksel po pikselu i wybiera najmniejszy bezpieczny wynik. Bez uploadu.</p>
    </section>

    <section class="workspace" aria-label="Kompresor obrazów">
      <div class="toolbar">
        <div class="toolbar-controls">
          <label class="profile-control">
            <span>Profil jakości</span>
            <select id="profile">
              <option value="maximumQuality">Maksymalna jakość</option>
              <option value="balanced" selected>Zbalansowany</option>
              <option value="maximumCompression">Maksymalna kompresja</option>
              <option value="lossless">Bezstratny</option>
            </select>
          </label>
          <label class="expert-toggle"><input id="expert-mode" type="checkbox" /><span>Expert mode</span></label>
        </div>
        <p id="profile-hint">Najlepszy balans jakości, rozmiaru i czasu.</p>
      </div>

      <div class="dropzone" id="dropzone" role="button" tabindex="0" aria-describedby="drop-hint">
        <input id="file-input" type="file" accept="image/jpeg,image/png,.jpg,.jpeg,.png" multiple hidden />
        <span class="drop-icon" aria-hidden="true">${uploadIcon()}</span>
        <div>
          <h2>Upuść obrazy tutaj</h2>
          <p id="drop-hint">albo <span>wybierz pliki</span> · JPEG i PNG · maks. 24 MP</p>
        </div>
        <span class="drop-badge">Format zostaje bez zmian</span>
      </div>

      <div class="queue-header" id="queue-header" hidden>
        <div>
          <p class="eyebrow">Kolejka</p>
          <h2>Twoje obrazy <span id="queue-count"></span></h2>
        </div>
        <button class="text-button" id="clear-button" type="button">Wyczyść zakończone</button>
      </div>
      <div class="queue" id="queue" aria-live="polite"></div>
      <section class="summary" id="summary" hidden aria-label="Podsumowanie kompresji"></section>
    </section>

    <section class="privacy-note">
      <span aria-hidden="true">${shieldIcon()}</span>
      <div><h2>Twoje obrazy nie opuszczają urządzenia</h2><p>Silnik działa w osobnym wątku przeglądarki. Nazwy i piksele nie są wysyłane do żadnego serwera.</p></div>
    </section>
  </main>
  <footer class="site-footer"><span>Squeeze v0.1</span><span>JPEG · PNG · offline po załadowaniu</span></footer>
`;

const queue = new CompressionQueue();
const dropzone = document.querySelector<HTMLDivElement>("#dropzone")!;
const input = document.querySelector<HTMLInputElement>("#file-input")!;
const profile = document.querySelector<HTMLSelectElement>("#profile")!;
const profileHint = document.querySelector<HTMLParagraphElement>("#profile-hint")!;
const expertModeInput = document.querySelector<HTMLInputElement>("#expert-mode")!;
const queueElement = document.querySelector<HTMLDivElement>("#queue")!;
const queueHeader = document.querySelector<HTMLDivElement>("#queue-header")!;
const queueCount = document.querySelector<HTMLSpanElement>("#queue-count")!;
const summary = document.querySelector<HTMLElement>("#summary")!;
let currentJobs: readonly CompressionJob[] = [];
let expertMode = false;

dropzone.addEventListener("click", () => input.click());
dropzone.addEventListener("keydown", (event) => {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    input.click();
  }
});
input.addEventListener("change", () => {
  addFiles(input.files);
  input.value = "";
});
for (const eventName of ["dragenter", "dragover"]) {
  dropzone.addEventListener(eventName, (event) => {
    event.preventDefault();
    dropzone.classList.add("is-dragging");
  });
}
for (const eventName of ["dragleave", "drop"]) {
  dropzone.addEventListener(eventName, (event) => {
    event.preventDefault();
    dropzone.classList.remove("is-dragging");
  });
}
dropzone.addEventListener("drop", (event) => addFiles(event.dataTransfer?.files));
profile.addEventListener("change", () => {
  profileHint.textContent = profileDescription(profile.value as CompressionProfile);
});
expertModeInput.addEventListener("change", () => {
  expertMode = expertModeInput.checked;
  renderQueue(currentJobs);
});
document.querySelector("#clear-button")!.addEventListener("click", () => queue.clearCompleted());

queueElement.addEventListener("click", (event) => {
  const button = (event.target as Element).closest<HTMLButtonElement>("button[data-action]");
  if (!button) return;
  const job = currentJobs.find((candidate) => candidate.id === button.dataset.id);
  if (!job) return;
  switch (button.dataset.action) {
    case "cancel": queue.cancel(job.id); break;
    case "remove": queue.remove(job.id); break;
    case "retry": queue.retry(job.id); break;
    case "download": downloadJob(job); break;
    case "report": downloadReport(job); break;
    case "compare": void openComparison(job); break;
  }
});
summary.addEventListener("click", (event) => {
  if ((event.target as Element).closest("[data-download-zip]")) downloadZip(currentJobs);
});

queue.subscribe((jobs) => {
  currentJobs = jobs;
  renderQueue(jobs);
});

function addFiles(files: FileList | null | undefined): void {
  if (!files?.length) return;
  queue.add(files, profile.value as CompressionProfile);
}

function renderQueue(jobs: readonly CompressionJob[]): void {
  queueHeader.hidden = jobs.length === 0;
  queueCount.textContent = `(${jobs.length})`;
  queueElement.innerHTML = jobs.map(renderJob).join("");
  const complete = jobs.filter((job) => job.status === "complete" && job.report);
  summary.hidden = complete.length === 0;
  if (complete.length) {
    const original = complete.reduce((sum, job) => sum + job.file.size, 0);
    const optimized = complete.reduce((sum, job) => sum + (job.output?.byteLength ?? 0), 0);
    const saved = Math.max(0, original - optimized);
    summary.innerHTML = `
      <div><p class="eyebrow">Podsumowanie</p><strong>${formatBytes(saved)}</strong><span>mniej na ${complete.length} ${plural(complete.length)}</span></div>
      <div class="summary-meter" aria-label="Oszczędzono ${original ? Math.round((saved / original) * 100) : 0} procent"><i style="--saved:${original ? (saved / original) * 100 : 0}%"></i></div>
      <button class="button button--primary" type="button" data-download-zip>${downloadIcon()} Pobierz ZIP</button>
    `;
  }
}

function renderJob(job: CompressionJob): string {
  const report = job.report;
  const status = statusText(job);
  const reduction = report && !report.alreadyOptimized ? `<span class="saving">−${report.savedPercent.toFixed(1)}%</span>` : "";
  return `
    <article class="job job--${job.status}">
      <div class="file-icon" aria-hidden="true">${imageIcon()}</div>
      <div class="job-main">
        <div class="job-heading"><h3 title="${escapeHtml(job.file.name)}">${escapeHtml(job.file.name)}</h3>${reduction}</div>
        <div class="job-meta"><span>${formatBytes(job.file.size)}</span><i></i><span>${profileName(job.profile)}</span></div>
        <div class="job-status">${status}</div>
        ${job.error ? `<p class="job-error">${escapeHtml(job.error)}</p>` : ""}
        ${report?.warnings.length ? `<details><summary>Uwagi silnika (${report.warnings.length})</summary><ul>${report.warnings.map((warning) => `<li>${escapeHtml(warning)}</li>`).join("")}</ul></details>` : ""}
        ${expertMode && report ? expertDetails(report) : ""}
      </div>
      <div class="job-result">
        ${report ? `<div><strong>${formatBytes(report.optimizedSize)}</strong><span>${report.alreadyOptimized ? "Już zoptymalizowany" : `Zaoszczędzono ${formatBytes(report.savedBytes)}`}</span></div>` : ""}
        <div class="job-actions">${jobActions(job)}</div>
      </div>
    </article>
  `;
}

function expertDetails(report: NonNullable<CompressionJob["report"]>): string {
  const metric = (value: number | null) => value === null ? "—" : value.toFixed(3);
  const strategy = [
    report.strategy.encoder,
    report.strategy.quality === null ? null : `Q${report.strategy.quality}`,
    report.strategy.chromaSubsampling,
    report.strategy.paletteColors === null ? null : `${report.strategy.paletteColors} kolorów`,
    report.strategy.dithering,
  ].filter(Boolean).join(" · ");
  return `<dl class="expert-details"><div><dt>SSIMULACRA2</dt><dd>${metric(report.metrics.ssimulacra2)}</dd></div><div><dt>Butteraugli</dt><dd>${metric(report.metrics.butteraugli)}</dd></div><div><dt>Strategia</dt><dd>${escapeHtml(strategy)}</dd></div><div><dt>Kandydaci</dt><dd>${report.candidatesTested}</dd></div><div><dt>Czas</dt><dd>${Math.round(report.processingTimeMs)} ms</dd></div><div><dt>Typ</dt><dd>${report.analysis.kind}</dd></div></dl>`;
}

function jobActions(job: CompressionJob): string {
  if (job.status === "processing" || job.status === "queued") {
    return `<button class="button button--ghost" type="button" data-action="cancel" data-id="${job.id}">Anuluj</button>`;
  }
  if (job.status === "error" || job.status === "cancelled") {
    return `<button class="button button--secondary" type="button" data-action="retry" data-id="${job.id}">Spróbuj ponownie</button><button class="icon-button" type="button" data-action="remove" data-id="${job.id}" aria-label="Usuń plik">${trashIcon()}</button>`;
  }
  return `<button class="icon-button" type="button" data-action="compare" data-id="${job.id}" aria-label="Porównaj obrazy" title="Porównaj">${compareIcon()}</button><button class="icon-button" type="button" data-action="report" data-id="${job.id}" aria-label="Pobierz raport JSON" title="Raport JSON">${reportIcon()}</button><button class="button button--secondary" type="button" data-action="download" data-id="${job.id}">${downloadIcon()} Pobierz</button><button class="icon-button" type="button" data-action="remove" data-id="${job.id}" aria-label="Usuń plik">${trashIcon()}</button>`;
}

function statusText(job: CompressionJob): string {
  if (job.status === "queued") return `<span class="status-dot"></span> Czeka w kolejce`;
  if (job.status === "processing") {
    const candidate = job.candidate && job.total ? ` · wariant ${job.candidate}/${job.total}` : "";
    return `<span class="spinner" aria-hidden="true"></span> ${stageName(job.stage)}${candidate}`;
  }
  if (job.status === "complete") return `<span class="status-check">✓</span> Gotowe`;
  if (job.status === "cancelled") return `<span class="status-dot"></span> Anulowano`;
  return `<span class="status-error">!</span> Nie udało się`;
}

function stageName(stage?: ProgressStage): string {
  return ({ decoding: "Dekodowanie", analyzing: "Analiza obrazu", searching: "Szukanie najlepszego wariantu", measuring: "Pomiar jakości", finalizing: "Finalizacja" } as Record<ProgressStage, string>)[stage ?? "decoding"];
}

function profileDescription(value: CompressionProfile): string {
  return ({ maximumQuality: "Najwyższa wierność; subtelne oszczędności.", balanced: "Najlepszy balans jakości, rozmiaru i czasu.", maximumCompression: "Mniejszy plik przy nadal kontrolowanej jakości.", lossless: "Bez zmiany wartości pikseli." })[value];
}

function profileName(value: CompressionProfile): string {
  return ({ maximumQuality: "Maks. jakość", balanced: "Zbalansowany", maximumCompression: "Maks. kompresja", lossless: "Bezstratny" })[value];
}

export function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB"];
  let size = value / 1024;
  let unit = units[0]!;
  for (let index = 1; size >= 1024 && index < units.length; index += 1) {
    size /= 1024;
    unit = units[index]!;
  }
  return `${size >= 10 ? size.toFixed(1) : size.toFixed(2)} ${unit}`;
}

function plural(value: number): string { return value === 1 ? "obrazie" : "obrazach"; }
function escapeHtml(value: string): string { return value.replace(/[&<>"']/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#039;" })[char]!); }
function svg(path: string): string { return `<svg aria-hidden="true" viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${path}</svg>`; }
function sparkIcon(): string { return svg('<path d="M12 2 9.8 8.1 4 10.5l5.8 2.2L12 19l2.2-6.3 5.8-2.2-5.8-2.4L12 2Z"/>'); }
function lockIcon(): string { return svg('<rect x="5.5" y="10" width="13" height="10" rx="3"/><path d="M8.5 10V7a3.5 3.5 0 0 1 7 0v3"/>'); }
function shieldIcon(): string { return svg('<path d="M12 3 5 6v5c0 4.6 2.8 7.8 7 10 4.2-2.2 7-5.4 7-10V6l-7-3Z"/><path d="m9 12 2 2 4-4"/>'); }
function uploadIcon(): string { return svg('<path d="M12 16V4m0 0L7.5 8.5M12 4l4.5 4.5"/><path d="M5 15v3a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-3"/>'); }
function imageIcon(): string { return svg('<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="8.5" cy="9" r="1.5"/><path d="m4 17 5-5 3.5 3 2.5-2 5 4"/>'); }
function downloadIcon(): string { return svg('<path d="M12 3v12m0 0 4-4m-4 4-4-4M5 20h14"/>'); }
function trashIcon(): string { return svg('<path d="M4 7h16m-10-3h4m-7 3 .7 13h8.6L17 7M10 11v5m4-5v5"/>'); }
function compareIcon(): string { return svg('<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M12 5v14m-3-9-3 2 3 2m6-4 3 2-3 2"/>'); }
function reportIcon(): string { return svg('<path d="M7 3h7l4 4v14H7z"/><path d="M14 3v5h4M10 12h5m-5 4h5"/>'); }
