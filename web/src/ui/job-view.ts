import type { CompressionJob, ProgressStage } from "../types";

export type JobAction = "compare" | "download" | "retry" | "cancel" | "remove";

export class JobView {
  readonly element: HTMLElement;
  readonly #name: HTMLElement;
  readonly #change: HTMLElement;
  readonly #original: HTMLElement;
  readonly #output: HTMLElement;
  readonly #statusIcon: HTMLElement;
  readonly #statusCopy: HTMLElement;
  readonly #error: HTMLElement;
  readonly #details: HTMLDetailsElement;
  readonly #detailsBody: HTMLElement;

  constructor(job: CompressionJob, previewUrl: string, onAction: (action: JobAction, id: string) => void) {
    const template = document.querySelector<HTMLTemplateElement>("#job-template")!;
    this.element = template.content.firstElementChild!.cloneNode(true) as HTMLElement;
    this.element.dataset.jobId = job.id;
    const preview = document.createElement("img");
    preview.alt = "";
    preview.src = previewUrl;
    this.element.querySelector(".job__preview")!.append(preview);
    this.#name = this.element.querySelector("h3")!;
    this.#change = this.element.querySelector(".job__change")!;
    this.#original = this.element.querySelector("[data-original]")!;
    this.#output = this.element.querySelector("[data-output]")!;
    this.#statusIcon = this.element.querySelector("[data-status-icon]")!;
    this.#statusCopy = this.element.querySelector("[data-status-copy]")!;
    this.#error = this.element.querySelector(".job__error")!;
    this.#details = this.element.querySelector(".job__details")!;
    this.#detailsBody = this.#details.querySelector("div")!;
    this.element.addEventListener("click", (event) => {
      const button = (event.target as Element).closest<HTMLButtonElement>("button[data-action]");
      if (button) onAction(button.dataset.action as JobAction, job.id);
    });
  }

  update(job: CompressionJob): void {
    this.element.className = `job job--${job.status}`;
    this.#name.textContent = job.file.name;
    this.#name.title = job.file.name;
    this.#original.textContent = formatBytes(job.file.size);
    this.#output.textContent = job.report ? formatBytes(job.report.optimizedSize) : "—";
    const percent = job.report?.savedPercent;
    this.#change.hidden = percent === undefined || job.isReprocessing === true;
    this.#change.textContent = percent === undefined ? "" : percent > 0 ? `−${percent.toFixed(1)}%` : "bez zmiany";
    this.#statusIcon.className = statusIconClass(job);
    this.#statusCopy.textContent = statusMessage(job);
    this.#error.hidden = !job.error;
    this.#error.textContent = job.error ?? "";
    this.element.querySelector<HTMLButtonElement>('[data-action="retry"]')!.textContent = job.status === "complete" ? "Przelicz ponownie" : "Spróbuj ponownie";
    this.updateWarnings(job);
    for (const button of this.element.querySelectorAll<HTMLButtonElement>("button[data-action]")) button.hidden = !visibleAction(button.dataset.action as JobAction, job);
  }

  private updateWarnings(job: CompressionJob): void {
    const warnings = job.report?.warnings ?? [];
    this.#details.hidden = warnings.length === 0;
    const wasOpen = this.#details.open;
    this.#detailsBody.replaceChildren();
    if (warnings.length) this.#detailsBody.textContent = warnings.join(" ");
    this.#details.open = wasOpen;
  }
}

function visibleAction(action: JobAction, job: CompressionJob): boolean {
  if (action === "remove") return job.status !== "processing";
  if (action === "cancel") return job.status === "processing";
  if (action === "retry") return (job.status === "error" && job.recoverable !== false) || job.status === "cancelled" || job.status === "complete";
  return Boolean(job.output && job.report);
}

function statusMessage(job: CompressionJob): string {
  if (job.status === "queued") return job.isReprocessing ? "Czeka na ponowne przeliczenie" : "Czeka w kolejce";
  if (job.status === "processing") return job.isReprocessing ? "Kompresuję ponownie" : stageName(job.stage);
  if (job.status === "error") return job.output ? "Nowa próba nie powiodła się — poprzedni wynik jest dostępny" : "Nie udało się skompresować";
  if (job.status === "cancelled") return job.output ? "Przerwano — poprzedni wynik jest dostępny" : "Anulowano";
  if (job.report?.alreadyOptimized || job.report?.savedPercent === 0) return "Gotowe · Brak oszczędności w tym przebiegu";
  return "Gotowe";
}

function statusIconClass(job: CompressionJob): string {
  return job.status === "processing" ? "spinner" : job.status === "complete" ? "status-dot status-dot--success" : job.status === "error" ? "status-dot status-dot--error" : "status-dot";
}

function stageName(stage?: ProgressStage): string {
  return ({ decoding: "Odczytuję obraz", analyzing: "Analizuję zawartość", compressing: "Kompresuję", finalizing: "Kończę" } as Record<ProgressStage, string>)[stage ?? "decoding"];
}

export function formatBytes(value: number): string {
  if (Math.abs(value) < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB"];
  let size = Math.abs(value) / 1024;
  let unit = units[0]!;
  for (let index = 1; size >= 1024 && index < units.length; index += 1) { size /= 1024; unit = units[index]!; }
  return `${value < 0 ? "−" : ""}${size >= 10 ? size.toFixed(1) : size.toFixed(2)} ${unit}`;
}
