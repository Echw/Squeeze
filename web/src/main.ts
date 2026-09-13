import "./styles.scss";
import { CompressionQueue, type EngineState, type QueueSettings } from "./queue/compression-queue";
import { downloadJob, downloadReport, downloadZip } from "./services/downloads";
import { loadSettings, saveSettings, type AppSettings } from "./settings";
import type { CompressionJob, CompressionMethod, CompressionProfile, OutputFormat, SearchEffort, WorkerCapabilities } from "./types";
import { openComparison } from "./ui/compare-modal";
import { formatBytes, JobView, type JobAction } from "./ui/job-view";

class AppController {
  readonly #settings: AppSettings;
  readonly #queue: CompressionQueue;
  readonly #views = new Map<string, JobView>();
  readonly #dropzone = get<HTMLElement>("dropzone");
  readonly #input = get<HTMLInputElement>("file-input");
  readonly #profile = get<HTMLSelectElement>("profile");
  readonly #outputFormat = get<HTMLSelectElement>("output-format");
  readonly #autoStart = get<HTMLInputElement>("auto-start");
  readonly #method = get<HTMLSelectElement>("method");
  readonly #effort = get<HTMLSelectElement>("effort");
  readonly #expertMode = get<HTMLInputElement>("expert-mode");
  readonly #results = get<HTMLElement>("results");
  readonly #queueElement = get<HTMLElement>("queue");
  readonly #runButton = get<HTMLButtonElement>("run-button");
  readonly #summary = get<HTMLElement>("summary");
  #jobs: readonly CompressionJob[] = [];
  #paused = false;
  #unsubscribe?: () => void;

  constructor() {
    this.#settings = loadSettings();
    this.applySettingsToForm();
    this.#queue = new CompressionQueue({ autoStart: this.#settings.autoStart });
  }

  mount(): void {
    this.#dropzone.addEventListener("click", () => this.#input.click());
    this.#dropzone.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") { event.preventDefault(); this.#input.click(); }
    });
    this.#input.addEventListener("change", () => { void this.addFiles(this.#input.files); this.#input.value = ""; });
    for (const eventName of ["dragenter", "dragover"]) this.#dropzone.addEventListener(eventName, (event) => { event.preventDefault(); this.#dropzone.classList.add("is-dragging"); });
    for (const eventName of ["dragleave", "drop"]) this.#dropzone.addEventListener(eventName, (event) => { event.preventDefault(); this.#dropzone.classList.remove("is-dragging"); });
    this.#dropzone.addEventListener("drop", (event) => void this.addFiles(event.dataTransfer?.files));
    get<HTMLFormElement>("settings").addEventListener("change", () => this.onSettingsChange());
    this.#runButton.addEventListener("click", () => this.#paused ? this.#queue.start() : this.#queue.pause());
    get("clear-button").addEventListener("click", () => this.#queue.clearCompleted());
    get("download-all").addEventListener("click", () => void this.downloadAll());
    this.#unsubscribe = this.#queue.subscribe((jobs, paused, engine, capabilities) => this.update(jobs, paused, engine, capabilities));
    window.addEventListener("pagehide", () => this.dispose(), { once: true });
  }

  dispose(): void { this.#unsubscribe?.(); this.#queue.dispose(); }

  private async addFiles(files: FileList | null | undefined): Promise<void> {
    if (!files?.length) return;
    const candidates = Array.from(files);
    const validation = await Promise.all(candidates.map(async (file) => ({ file, format: await detectSupportedFormat(file) })));
    const valid = validation.flatMap(({ file, format }) => format
      ? [new File([file], file.name, { type: `image/${format}`, lastModified: file.lastModified })]
      : []);
    for (const { file, format } of validation) {
      if (!format) this.showNotice(`Nie dodano „${file.name}”. Plik nie jest prawidłowym obrazem JPEG lub PNG.`, "error");
    }
    const result = this.#queue.add(valid, this.queueSettings());
    for (const file of result.rejected) this.showNotice(`Nie dodano „${file.name}”. Obsługiwane są pliki JPEG i PNG.`, "error");
  }

  private onSettingsChange(): void {
    this.#settings.profile = this.#profile.value as CompressionProfile;
    this.#settings.outputFormat = this.#outputFormat.value as OutputFormat;
    this.#settings.autoStart = this.#autoStart.checked;
    this.#settings.method = this.#method.value as CompressionMethod;
    this.#settings.searchEffort = this.#effort.value as SearchEffort;
    this.#settings.expertMode = this.#expertMode.checked;
    saveSettings(this.#settings);
    this.#queue.setAutoStart(this.#settings.autoStart);
    const updated = this.#queue.reconfigureQueued(this.queueSettings());
    if (updated) this.showNotice(`Nowe ustawienia zastosowano do ${updated} oczekujących ${updated === 1 ? "obrazu" : "obrazów"}.`, "info");
    this.updateViews();
  }

  private queueSettings(): QueueSettings {
    return { profile: this.#settings.profile, outputFormat: this.#settings.outputFormat, method: this.#settings.method, searchEffort: this.#settings.searchEffort };
  }

  private applySettingsToForm(): void {
    this.#profile.value = this.#settings.profile;
    this.#outputFormat.value = this.#settings.outputFormat;
    this.#autoStart.checked = this.#settings.autoStart;
    this.#method.value = this.#settings.method;
    this.#effort.value = this.#settings.searchEffort;
    this.#expertMode.checked = this.#settings.expertMode;
  }

  private update(jobs: readonly CompressionJob[], paused: boolean, engine: EngineState, capabilities?: WorkerCapabilities): void {
    this.#jobs = jobs;
    this.#paused = paused;
    const hasJobs = jobs.length > 0;
    this.#results.hidden = !hasJobs;
    this.#dropzone.classList.toggle("is-compact", hasJobs);
    get("queue-count").textContent = `(${jobs.length})`;
    const complete = jobs.filter((job) => job.output && job.report).length;
    const active = jobs.filter((job) => job.status === "processing").length;
    const waiting = jobs.filter((job) => job.status === "queued").length;
    get("queue-progress").textContent = active ? `${complete} gotowe · trwa kompresja` : waiting ? `${complete} gotowe · ${waiting} oczekuje` : `${complete} gotowe`;
    this.#runButton.hidden = waiting === 0 && active === 0;
    this.#runButton.textContent = paused ? `Kompresuj${waiting ? ` (${waiting})` : ""}` : "Wstrzymaj kolejkę";
    this.#runButton.className = paused ? "button button--primary" : "button button--secondary";
    this.updateEngine(engine, capabilities);
    this.reconcileViews();
    this.updateSummary();
  }

  private updateEngine(engine: EngineState, capabilities?: WorkerCapabilities): void {
    const element = get("engine-status");
    const copy = element.querySelector<HTMLElement>("span:last-child")!;
    element.className = `engine-status engine-status--${engine}`;
    copy.textContent = engine === "loading" ? "Uruchamiam silnik" : engine === "error" ? "Błąd silnika" : "Gotowy";
    const preserveOption = this.#outputFormat.querySelector<HTMLOptionElement>('option[value="preserve"]')!;
    const webpOption = this.#outputFormat.querySelector<HTMLOptionElement>('option[value="webp"]')!;
    preserveOption.disabled = capabilities?.preserve === false;
    webpOption.disabled = capabilities?.webp === false;
    if (!capabilities) return;
    const selectedIsAvailable = this.#outputFormat.value === "preserve" ? capabilities.preserve : capabilities.webp;
    const fallback = capabilities.preserve ? "preserve" : capabilities.webp ? "webp" : undefined;
    if (!selectedIsAvailable && fallback) {
      this.#outputFormat.value = fallback;
      this.#settings.outputFormat = fallback;
      saveSettings(this.#settings);
      this.#queue.reconfigureQueued(this.queueSettings());
    }
  }

  private reconcileViews(): void {
    const ids = new Set(this.#jobs.map((job) => job.id));
    for (const [id, view] of this.#views) if (!ids.has(id)) { view.element.remove(); this.#views.delete(id); }
    for (const job of this.#jobs) {
      let view = this.#views.get(job.id);
      if (!view) {
        view = new JobView(job, this.#queue.previewUrl(job), (action, id) => this.onJobAction(action, id));
        this.#views.set(job.id, view);
        this.#queueElement.append(view.element);
      }
      view.update(job, this.#settings.expertMode);
    }
  }

  private updateViews(): void { for (const job of this.#jobs) this.#views.get(job.id)?.update(job, this.#settings.expertMode); }

  private onJobAction(action: JobAction, id: string): void {
    const job = this.#jobs.find((candidate) => candidate.id === id);
    if (!job) return;
    if (action === "cancel") this.#queue.cancel(id);
    else if (action === "remove") this.#queue.remove(id);
    else if (action === "retry") job.status === "complete" ? this.#queue.rerun(id, this.queueSettings()) : this.#queue.retry(id);
    else if (action === "download") downloadJob(job);
    else if (action === "report") downloadReport(job);
    else if (action === "compare") openComparison(job, document.activeElement as HTMLElement);
  }

  private updateSummary(): void {
    const complete = this.#jobs.filter((job) => job.output && job.report);
    this.#summary.hidden = complete.length === 0;
    if (!complete.length) return;
    const original = complete.reduce((sum, job) => sum + job.file.size, 0);
    const optimized = complete.reduce((sum, job) => sum + job.report!.optimizedSize, 0);
    const delta = original - optimized;
    get("summary-label").textContent = delta >= 0 ? "Łączna oszczędność" : "Zmiana rozmiaru";
    get("summary-saving").textContent = `${delta >= 0 ? "−" : "+"}${formatBytes(Math.abs(delta))}`;
    get("summary-detail").textContent = `${complete.length} ${complete.length === 1 ? "gotowy obraz" : "gotowe obrazy"} · ${original ? Math.abs(delta / original * 100).toFixed(1) : "0"}%`;
    get("download-all").querySelector("span")!.textContent = complete.length === 1 ? "Pobierz obraz" : `Pobierz wszystkie (${complete.length})`;
  }

  private async downloadAll(): Promise<void> {
    const complete = this.#jobs.filter((job) => job.output);
    if (complete.length === 1) { downloadJob(complete[0]!); return; }
    const button = get<HTMLButtonElement>("download-all");
    button.disabled = true;
    try { await downloadZip(complete); }
    catch (error) { this.showNotice(error instanceof Error ? error.message : String(error), "error"); }
    finally { button.disabled = false; }
  }

  private showNotice(message: string, kind: "info" | "error"): void {
    const template = document.querySelector<HTMLTemplateElement>("#notice-template")!;
    const notice = template.content.firstElementChild!.cloneNode(true) as HTMLElement;
    notice.classList.add(`notice--${kind}`);
    notice.querySelector("span")!.textContent = message;
    notice.querySelector("button")!.addEventListener("click", () => notice.remove());
    get("notices").append(notice);
  }
}

function get<T extends HTMLElement = HTMLElement>(id: string): T { return document.getElementById(id) as T; }

async function detectSupportedFormat(file: File): Promise<"jpeg" | "png" | undefined> {
  if (file.size === 0 || file.size > 100 * 1024 * 1024) return undefined;
  const bytes = new Uint8Array(await file.slice(0, 12).arrayBuffer());
  const png = bytes.length >= 8 && bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47
    && bytes[4] === 0x0d && bytes[5] === 0x0a && bytes[6] === 0x1a && bytes[7] === 0x0a;
  const jpeg = bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff;
  if (png) return "png";
  if (jpeg) return "jpeg";
  return undefined;
}

async function bootstrap(): Promise<void> {
  new AppController().mount();
  if (import.meta.env.PROD && "serviceWorker" in navigator) {
    try {
      await navigator.serviceWorker.register("/sw.js");
    } catch (error) {
      console.warn("Nie udało się przygotować trybu offline.", error);
    }
  }
}

void bootstrap();
