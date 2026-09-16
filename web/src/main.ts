import "./styles.scss";
import { CompressionQueue, type EngineState } from "./queue/compression-queue";
import { downloadJob, downloadZip } from "./services/downloads";
import { clearRetiredSettings } from "./settings";
import type { CompressionJob } from "./types";
import { openComparison } from "./ui/compare-modal";
import { formatBytes, JobView, type JobAction } from "./ui/job-view";

class AppController {
  readonly #queue = new CompressionQueue();
  readonly #views = new Map<string, JobView>();
  readonly #dropzone = get<HTMLElement>("dropzone");
  readonly #input = get<HTMLInputElement>("file-input");
  readonly #results = get<HTMLElement>("results");
  readonly #queueElement = get<HTMLElement>("queue");
  readonly #clearButton = get<HTMLButtonElement>("clear-button");
  readonly #runButton = get<HTMLButtonElement>("run-button");
  readonly #summary = get<HTMLElement>("summary");
  #jobs: readonly CompressionJob[] = [];
  #paused = false;
  #unsubscribe?: () => void;

  constructor() { clearRetiredSettings(); }

  mount(): void {
    this.#dropzone.addEventListener("click", () => this.#input.click());
    this.#dropzone.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") { event.preventDefault(); this.#input.click(); }
    });
    this.#input.addEventListener("change", () => { void this.addFiles(this.#input.files); this.#input.value = ""; });
    for (const eventName of ["dragenter", "dragover"]) this.#dropzone.addEventListener(eventName, (event) => { event.preventDefault(); this.#dropzone.classList.add("is-dragging"); });
    for (const eventName of ["dragleave", "drop"]) this.#dropzone.addEventListener(eventName, (event) => { event.preventDefault(); this.#dropzone.classList.remove("is-dragging"); });
    this.#dropzone.addEventListener("drop", (event) => void this.addFiles(event.dataTransfer?.files));
    this.#runButton.addEventListener("click", () => this.#paused ? this.#queue.start() : this.#queue.pause());
    this.#clearButton.addEventListener("click", () => this.#queue.clearCompleted());
    get("download-all").addEventListener("click", () => void this.downloadAll());
    this.#unsubscribe = this.#queue.subscribe((jobs, paused, engine) => this.update(jobs, paused, engine));
    window.addEventListener("pagehide", () => this.dispose(), { once: true });
  }

  dispose(): void { this.#unsubscribe?.(); this.#queue.dispose(); }

  private async addFiles(files: FileList | null | undefined): Promise<void> {
    if (!files?.length) return;
    const candidates = Array.from(files);
    const validation = await Promise.all(candidates.map(async (file) => ({ file, format: await detectSupportedFormat(file) })));
    const valid = validation.flatMap(({ file, format }) => format
      ? [new File([file], file.name, { type: `image/${format}`, lastModified: file.lastModified })] : []);
    for (const { file, format } of validation) {
      if (!format) this.showNotice(`Nie dodano „${file.name}”. Plik nie jest prawidłowym obrazem JPEG lub PNG.`, "error");
    }
    const result = this.#queue.add(valid);
    for (const file of result.rejected) this.showNotice(`Nie dodano „${file.name}”. Obsługiwane są pliki JPEG i PNG.`, "error");
  }

  private update(jobs: readonly CompressionJob[], paused: boolean, engine: EngineState): void {
    this.#jobs = jobs;
    this.#paused = paused;
    const hasJobs = jobs.length > 0;
    this.#results.hidden = !hasJobs;
    this.#dropzone.classList.toggle("is-compact", hasJobs);
    get("queue-count").textContent = `(${jobs.length})`;
    const complete = jobs.filter((job) => job.output && job.report).length;
    const active = jobs.filter((job) => job.status === "processing").length;
    const waiting = jobs.filter((job) => job.status === "queued").length;
    this.#clearButton.hidden = !jobs.some((job) => ["complete", "cancelled", "error"].includes(job.status));
    get("queue-progress").textContent = active ? `${complete} gotowe · trwa kompresja` : waiting ? `${complete} gotowe · ${waiting} oczekuje` : `${complete} gotowe`;
    this.#runButton.hidden = waiting === 0 && active === 0;
    this.#runButton.textContent = paused ? `Kompresuj${waiting ? ` (${waiting})` : ""}` : "Wstrzymaj kolejkę";
    this.#runButton.className = paused ? "button button--primary" : "button button--secondary";
    this.updateEngine(engine);
    this.reconcileViews();
    this.updateSummary();
  }

  private updateEngine(engine: EngineState): void {
    const element = get("engine-status");
    const copy = element.querySelector<HTMLElement>("span:last-child")!;
    element.className = `engine-status engine-status--${engine}`;
    copy.textContent = engine === "loading" ? "Uruchamiam silnik" : engine === "error" ? "Błąd silnika" : "Gotowy";
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
      view.update(job);
    }
  }

  private onJobAction(action: JobAction, id: string): void {
    const job = this.#jobs.find((candidate) => candidate.id === id);
    if (!job) return;
    if (action === "cancel") this.#queue.cancel(id);
    else if (action === "remove") this.#queue.remove(id);
    else if (action === "retry") this.#queue.retry(id);
    else if (action === "download") downloadJob(job);
    else if (action === "compare") openComparison(job, document.activeElement as HTMLElement);
  }

  private updateSummary(): void {
    const complete = this.#jobs.filter((job) => job.output && job.report);
    this.#summary.hidden = complete.length === 0;
    if (!complete.length) return;
    const original = complete.reduce((sum, job) => sum + job.file.size, 0);
    const optimized = complete.reduce((sum, job) => sum + job.report!.optimizedSize, 0);
    const delta = original - optimized;
    get("summary-label").textContent = "Łączna oszczędność";
    get("summary-saving").textContent = `−${formatBytes(Math.max(0, delta))}`;
    get("summary-detail").textContent = `${complete.length} ${complete.length === 1 ? "gotowy obraz" : "gotowe obrazy"} · ${original ? Math.max(0, delta / original * 100).toFixed(1) : "0"}%`;
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
  return png ? "png" : jpeg ? "jpeg" : undefined;
}

async function bootstrap(): Promise<void> {
  new AppController().mount();
  if (!("serviceWorker" in navigator)) return;
  if (import.meta.env.PROD) {
    try { await navigator.serviceWorker.register("/sw.js"); }
    catch (error) { console.warn("Nie udało się przygotować trybu offline.", error); }
    return;
  }
  // A production build may have registered a worker on this local origin.
  // Leaving it in place while Vite serves source files can silently run stale
  // WASM or workers, so development always starts with the current assets.
  try { await Promise.all((await navigator.serviceWorker.getRegistrations()).map((registration) => registration.unregister())); }
  catch (error) { console.warn("Nie udało się wyczyścić lokalnego cache deweloperskiego.", error); }
}

void bootstrap();
