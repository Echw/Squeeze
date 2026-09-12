import type {
  CompressionJob,
  CompressionMethod,
  CompressionProfile,
  OptimizationOptions,
  OutputFormat,
  SearchEffort,
  WorkerRequest,
  WorkerResponse,
} from "../types";
import { WORKER_API_VERSION } from "../types";

type Listener = (jobs: readonly CompressionJob[], paused: boolean) => void;

export interface QueueSettings {
  profile: CompressionProfile;
  searchEffort: SearchEffort;
  method: CompressionMethod;
  outputFormat?: OutputFormat;
}

export interface AddResult {
  accepted: number;
  rejected: File[];
}

/** A deliberately single-worker queue: image encoders compete for a lot of memory. */
export class CompressionQueue {
  #jobs: CompressionJob[] = [];
  #worker = this.#createWorker();
  #activeId?: string;
  // Files are collected first. Starting work is an explicit action in the UI.
  #paused = true;
  #listeners = new Set<Listener>();
  #previewUrls = new Map<string, string>();

  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener);
    listener(this.#jobs, this.#paused);
    return () => this.#listeners.delete(listener);
  }

  add(files: Iterable<File>, settings: QueueSettings, sourceJobId?: string): AddResult {
    const rejected: File[] = [];
    let accepted = 0;
    for (const file of files) {
      if (!isSupported(file)) {
        rejected.push(file);
        continue;
      }
      accepted += 1;
      this.#jobs.push(this.#newJob(file, settings, sourceJobId));
    }
    this.#emit();
    void this.#pump();
    return { accepted, rejected };
  }

  /** Applies batch settings only to work that has not started yet. */
  reconfigureQueued(settings: QueueSettings): number {
    let updated = 0;
    for (const job of this.#jobs) {
      if (job.status !== "queued") continue;
      job.profile = settings.profile;
      job.searchEffort = settings.searchEffort;
      job.method = isPng(job.file) ? settings.method : "auto";
      job.outputFormat = settings.outputFormat ?? "preserve";
      updated += 1;
    }
    if (updated) this.#emit();
    return updated;
  }

  start(): void {
    if (!this.#paused) return;
    this.#paused = false;
    this.#emit();
    void this.#pump();
  }

  pause(): void {
    if (this.#paused) return;
    this.#paused = true;
    this.#emit();
  }

  rerun(id: string, searchEffort: SearchEffort, method = this.#find(id)?.method ?? "auto"): void {
    const job = this.#find(id);
    if (!job) return;
    this.add([job.file], { profile: job.profile, searchEffort, method, outputFormat: job.outputFormat }, job.id);
  }

  togglePaused(): void {
    if (this.#paused) this.start();
    else this.pause();
  }

  clearCompleted(): void {
    const removable = this.#jobs.filter((job) => ["complete", "cancelled", "error"].includes(job.status));
    for (const job of removable) this.#revokePreview(job.id);
    this.#jobs = this.#jobs.filter((job) => !["complete", "cancelled", "error"].includes(job.status));
    this.#emit();
  }

  remove(id: string): void {
    if (this.#activeId === id) this.cancel(id);
    this.#jobs = this.#jobs.filter((job) => job.id !== id);
    this.#revokePreview(id);
    this.#emit();
  }

  cancel(id: string): void {
    const job = this.#find(id);
    if (!job || !["queued", "processing"].includes(job.status)) return;
    job.status = "cancelled";
    job.stage = undefined;
    job.attempt += 1;
    if (this.#activeId === id) {
      this.#worker.terminate();
      this.#activeId = undefined;
      this.#worker = this.#createWorker();
    }
    this.#emit();
    void this.#pump();
  }

  retry(id: string): void {
    const job = this.#find(id);
    if (!job || !["error", "cancelled"].includes(job.status)) return;
    job.status = "queued";
    job.error = undefined;
    job.report = undefined;
    job.output = undefined;
    job.attempt += 1;
    this.#emit();
    void this.#pump();
  }

  previewUrl(job: CompressionJob): string {
    let url = this.#previewUrls.get(job.id);
    if (!url) {
      url = URL.createObjectURL(job.file);
      this.#previewUrls.set(job.id, url);
    }
    return url;
  }

  dispose(): void {
    this.#worker.terminate();
    for (const url of this.#previewUrls.values()) URL.revokeObjectURL(url);
    this.#previewUrls.clear();
  }

  #newJob(file: File, settings: QueueSettings, sourceJobId?: string): CompressionJob {
    return {
      id: crypto.randomUUID(),
      file,
      profile: settings.profile,
      searchEffort: settings.searchEffort,
      method: isPng(file) ? settings.method : "auto",
      outputFormat: settings.outputFormat ?? "preserve",
      attempt: 0,
      sourceJobId,
      status: "queued",
    };
  }

  #createWorker(): Worker {
    const worker = new Worker(new URL("../worker/optimizer.worker.ts", import.meta.url), {
      type: "module",
      name: "squeeze-optimizer",
    });
    worker.onmessage = (event: MessageEvent<WorkerResponse>) => this.#onMessage(event.data);
    worker.onerror = (event) => {
      const active = this.#activeId ? this.#find(this.#activeId) : undefined;
      if (active) {
        active.status = "error";
        active.error = `Worker uległ awarii: ${event.message || "nieznany błąd"}`;
      }
      this.#activeId = undefined;
      worker.terminate();
      this.#worker = this.#createWorker();
      this.#emit();
      void this.#pump();
    };
    return worker;
  }

  async #pump(): Promise<void> {
    if (this.#paused || this.#activeId) return;
    const job = this.#jobs.find((candidate) => candidate.status === "queued");
    if (!job) return;
    this.#activeId = job.id;
    job.status = "processing";
    job.stage = "decoding";
    const attempt = job.attempt;
    this.#emit();
    try {
      const buffer = await job.file.arrayBuffer();
      if (job.status !== "processing" || this.#activeId !== job.id || job.attempt !== attempt) return;
      const request: WorkerRequest = {
        version: WORKER_API_VERSION,
        type: "compress",
        jobId: job.id,
        attempt,
        buffer,
        options: optionsFor(job.profile, job.searchEffort, job.method, job.outputFormat),
      };
      this.#worker.postMessage(request, [buffer]);
    } catch (error) {
      if (job.attempt !== attempt) return;
      job.status = "error";
      job.error = error instanceof Error ? error.message : String(error);
      this.#activeId = undefined;
      this.#emit();
      void this.#pump();
    }
  }

  #onMessage(message: WorkerResponse): void {
    if (message.version !== WORKER_API_VERSION) return;
    const job = this.#find(message.jobId);
    if (!job || job.attempt !== message.attempt) return;
    if (message.type === "progress") {
      job.stage = message.stage;
      job.candidate = message.candidate;
      job.total = message.total;
      this.#emit();
      return;
    }
    if (message.type === "complete") {
      job.status = "complete";
      job.report = message.result;
      job.output = new Uint8Array(message.buffer);
    } else if (message.type === "error") {
      job.status = "error";
      job.error = message.message;
    } else {
      job.status = "cancelled";
    }
    job.stage = undefined;
    if (this.#activeId === job.id) this.#activeId = undefined;
    this.#emit();
    void this.#pump();
  }

  #find(id: string): CompressionJob | undefined {
    return this.#jobs.find((job) => job.id === id);
  }

  #revokePreview(id: string): void {
    const url = this.#previewUrls.get(id);
    if (url) URL.revokeObjectURL(url);
    this.#previewUrls.delete(id);
  }

  #emit(): void {
    for (const listener of this.#listeners) listener(this.#jobs, this.#paused);
  }
}

export function optionsFor(
  profile: CompressionProfile,
  searchEffort: SearchEffort = "auto",
  method: CompressionMethod = "auto",
  outputFormat: OutputFormat = "preserve",
): OptimizationOptions {
  return {
    profile,
    searchEffort,
    method,
    outputFormat,
    metadata: "stripPrivate",
    limits: {
      maxInputBytes: 100 * 1024 * 1024,
      maxPixels: 24_000_000,
      maxWorkingBytes: 768 * 1024 * 1024,
    },
  };
}

function isSupported(file: File): boolean {
  return /\.(?:jpe?g|png)$/i.test(file.name) && (!file.type || ["image/jpeg", "image/png"].includes(file.type));
}

function isPng(file: File): boolean {
  return file.type === "image/png" || /\.png$/i.test(file.name);
}
