import type { CompressionJob, ProgressStage } from "../types";

export type JobAction = "compare" | "download" | "report" | "retry" | "cancel" | "remove";

export class JobView {
  readonly element: HTMLElement;
  readonly #preview: HTMLImageElement;
  readonly #name: HTMLElement;
  readonly #change: HTMLElement;
  readonly #original: HTMLElement;
  readonly #output: HTMLElement;
  readonly #statusIcon: HTMLElement;
  readonly #statusCopy: HTMLElement;
  readonly #progress: HTMLProgressElement;
  readonly #error: HTMLElement;
  readonly #details: HTMLDetailsElement;
  readonly #detailsBody: HTMLElement;

  constructor(job: CompressionJob, previewUrl: string, onAction: (action: JobAction, id: string) => void) {
    const template = document.querySelector<HTMLTemplateElement>("#job-template")!;
    this.element = template.content.firstElementChild!.cloneNode(true) as HTMLElement;
    this.element.dataset.jobId = job.id;
    const previewHost = this.element.querySelector(".job__preview")!;
    this.#preview = document.createElement("img");
    this.#preview.alt = "";
    previewHost.append(this.#preview);
    this.#name = this.element.querySelector("h3")!;
    this.#change = this.element.querySelector(".job__change")!;
    this.#original = this.element.querySelector("[data-original]")!;
    this.#output = this.element.querySelector("[data-output]")!;
    this.#statusIcon = this.element.querySelector("[data-status-icon]")!;
    this.#statusCopy = this.element.querySelector("[data-status-copy]")!;
    this.#progress = this.element.querySelector("progress")!;
    this.#error = this.element.querySelector(".job__error")!;
    this.#details = this.element.querySelector(".job__details")!;
    this.#detailsBody = this.#details.querySelector("div")!;
    this.#preview.src = previewUrl;
    this.element.addEventListener("click", (event) => {
      const button = (event.target as Element).closest<HTMLButtonElement>("button[data-action]");
      if (button) onAction(button.dataset.action as JobAction, job.id);
    });
  }

  update(job: CompressionJob, expertMode: boolean): void {
    this.element.className = `job job--${job.status}`;
    this.#name.textContent = job.file.name;
    this.#name.title = job.file.name;
    this.#original.textContent = formatBytes(job.file.size);
    this.#output.textContent = job.report ? formatBytes(job.report.optimizedSize) : "—";
    const percent = job.report?.savedPercent;
    this.#change.hidden = percent === undefined || job.isReprocessing === true;
    this.#change.textContent = percent === undefined ? "" : percent > 0 ? `−${percent.toFixed(1)}%` : percent < 0 ? `+${Math.abs(percent).toFixed(1)}%` : "bez zmiany";
    this.#change.classList.toggle("is-growth", (percent ?? 0) < 0);
    this.#statusIcon.className = statusIconClass(job);
    this.#statusCopy.textContent = statusMessage(job);
    const hasProgress = job.status === "processing" && Boolean(job.total);
    this.#progress.hidden = !hasProgress;
    this.#progress.max = job.total ?? 1;
    this.#progress.value = job.candidate ?? 0;
    this.#error.hidden = !job.error;
    this.#error.textContent = job.error ?? "";
    this.element.querySelector<HTMLButtonElement>('[data-action="retry"]')!.textContent = job.status === "complete" ? "Przelicz ponownie" : "Spróbuj ponownie";
    this.updateDetails(job, expertMode);
    for (const button of this.element.querySelectorAll<HTMLButtonElement>("button[data-action]")) {
      button.hidden = !visibleAction(button.dataset.action as JobAction, job);
    }
  }

  private updateDetails(job: CompressionJob, expertMode: boolean): void {
    const warnings = job.report?.warnings ?? [];
    this.#details.hidden = !expertMode && warnings.length === 0;
    const wasOpen = this.#details.open;
    this.#detailsBody.replaceChildren();
    if (warnings.length) appendFact(this.#detailsBody, "Uwagi", warnings.join(" "));
    if (expertMode && job.report) {
      appendFact(this.#detailsBody, "Strategia", strategyName(job));
      appendFact(this.#detailsBody, "Kandydaci", String(job.report.candidatesTested));
      appendFact(this.#detailsBody, "Czas", `${Math.round(job.report.processingTimeMs)} ms`);
      if (job.report.metrics.ssimulacra2 !== null) appendFact(this.#detailsBody, "SSIMULACRA2", job.report.metrics.ssimulacra2.toFixed(2));
      if (job.report.metrics.butteraugli !== null) appendFact(this.#detailsBody, "Butteraugli", job.report.metrics.butteraugli.toFixed(2));
    }
    this.#details.open = wasOpen;
  }
}

function appendFact(parent: HTMLElement, label: string, value: string): void {
  const row = document.createElement("p");
  const strong = document.createElement("strong");
  strong.textContent = `${label}: `;
  row.append(strong, document.createTextNode(value));
  parent.append(row);
}

function visibleAction(action: JobAction, job: CompressionJob): boolean {
  if (action === "remove") return job.status !== "processing";
  if (action === "cancel") return job.status === "queued" || job.status === "processing";
  if (action === "retry") return (job.status === "error" && job.recoverable !== false) || job.status === "cancelled" || job.status === "complete";
  return Boolean(job.output && job.report);
}

function statusMessage(job: CompressionJob): string {
  if (job.status === "queued") return job.isReprocessing ? "Czeka na ponowne przeliczenie" : "Czeka w kolejce";
  if (job.status === "processing") return `${job.isReprocessing ? "Przeliczam" : stageName(job.stage)}${job.candidate && job.total ? ` · wariant ${job.candidate} z ${job.total}` : ""}`;
  if (job.status === "error") return job.output ? "Nowa próba nie powiodła się — poprzedni wynik jest dostępny" : "Nie udało się skompresować";
  if (job.status === "cancelled") return job.output ? "Przerwano — poprzedni wynik jest dostępny" : "Anulowano";
  if ((job.report?.savedPercent ?? 0) < 0) return "Gotowe · WebP jest większy od oryginału";
  if (job.report?.alreadyOptimized || job.report?.savedPercent === 0) return "Gotowe · oryginał był już dobrze zoptymalizowany";
  return "Gotowe";
}

function statusIconClass(job: CompressionJob): string {
  return job.status === "processing" ? "spinner" : job.status === "complete" ? "status-dot status-dot--success" : job.status === "error" ? "status-dot status-dot--error" : "status-dot";
}

function stageName(stage?: ProgressStage): string {
  return ({ decoding: "Odczytuję obraz", analyzing: "Analizuję zawartość", searching: "Szukam mniejszego wariantu", measuring: "Sprawdzam jakość", finalizing: "Kończę" } as Record<ProgressStage, string>)[stage ?? "decoding"];
}

function strategyName(job: CompressionJob): string {
  const strategy = job.report!.strategy;
  return [strategy.encoder, strategy.quality === null ? null : `Q${strategy.quality}`, strategy.paletteColors === null ? null : `${strategy.paletteColors} kolorów`, strategy.dithering].filter(Boolean).join(" · ");
}

export function formatBytes(value: number): string {
  if (Math.abs(value) < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB"];
  let size = Math.abs(value) / 1024;
  let unit = units[0]!;
  for (let index = 1; size >= 1024 && index < units.length; index += 1) { size /= 1024; unit = units[index]!; }
  return `${value < 0 ? "−" : ""}${size >= 10 ? size.toFixed(1) : size.toFixed(2)} ${unit}`;
}
