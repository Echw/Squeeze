import type {
  CompressionJob,
  CompressionMethod,
  CompressionProfile,
  OptimizationOptions,
  OutputFormat,
  SearchEffort,
  WorkerRequest,
  WorkerResponse,
  WorkerCapabilities,
} from "../types";
import { WORKER_API_VERSION } from "../types";

export type EngineState = "loading" | "ready" | "error";
type Listener = (jobs: readonly CompressionJob[], paused: boolean, engine: EngineState, capabilities?: WorkerCapabilities) => void;

export interface QueueSettings {
  profile: CompressionProfile;
  searchEffort: SearchEffort;
  method: CompressionMethod;
  outputFormat?: OutputFormat;
}

export interface CompressionQueueOptions {
  autoStart?: boolean;
  workerFactory?: () => Worker;
}

export interface AddResult {
  accepted: number;
  rejected: File[];
}

/** A deliberately single-worker queue: image encoders compete for a lot of memory. */
export class CompressionQueue {
  #jobs: CompressionJob[] = [];
  #worker: Worker;
  #activeId?: string;
  #paused: boolean;
  #autoStart: boolean;
  #manuallyPaused = false;
  #engine: EngineState = "loading";
  #capabilities?: WorkerCapabilities;
  #workerFactory?: () => Worker;
  #listeners = new Set<Listener>();
  #previewUrls = new Map<string, string>();

  constructor(options: CompressionQueueOptions = {}) {
    this.#autoStart = options.autoStart ?? true;
    this.#paused = !this.#autoStart;
    this.#workerFactory = options.workerFactory;
    this.#worker = this.#createWorker();
  }

  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener);
    listener(this.#snapshot(), this.#paused, this.#engine, this.#capabilities);
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
    if (accepted && this.#autoStart && !this.#manuallyPaused) this.#paused = false;
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
    this.#recoverWorker();
    this.#paused = false;
    this.#manuallyPaused = false;
    this.#emit();
    void this.#pump();
  }

  pause(): void {
    if (this.#paused) return;
    this.#paused = true;
    this.#manuallyPaused = true;
    this.#emit();
  }

  rerun(id: string, settings?: Partial<QueueSettings>): void {
    const job = this.#find(id);
    if (!job) return;
    this.#recoverWorker();
    job.profile = settings?.profile ?? job.profile;
    job.searchEffort = settings?.searchEffort ?? job.searchEffort;
    job.method = isPng(job.file) ? settings?.method ?? job.method : "auto";
    job.outputFormat = settings?.outputFormat ?? job.outputFormat;
    job.status = "queued";
    job.error = undefined;
    job.errorCode = undefined;
    job.recoverable = undefined;
    job.stage = undefined;
    job.candidate = undefined;
    job.total = undefined;
    job.isReprocessing = Boolean(job.output);
    job.attempt += 1;
    if (this.#autoStart && !this.#manuallyPaused) this.#paused = false;
    this.#emit();
    void this.#pump();
  }

  setAutoStart(enabled: boolean): void {
    if (enabled === this.#autoStart) return;
    this.#autoStart = enabled;
    if (!enabled) {
      this.#paused = true;
      this.#emit();
      return;
    }
    if (!this.#manuallyPaused && this.#jobs.some((job) => job.status === "queued")) {
      this.#paused = false;
      this.#emit();
      void this.#pump();
    }
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
      this.#engine = "loading";
      this.#capabilities = undefined;
      this.#worker = this.#createWorker();
    }
    this.#emit();
    void this.#pump();
  }

  retry(id: string): void {
    const job = this.#find(id);
    if (!job || !["error", "cancelled"].includes(job.status)) return;
    if (job.status === "error" && job.recoverable === false) return;
    this.#recoverWorker();
    job.status = "queued";
    job.error = undefined;
    job.errorCode = undefined;
    job.recoverable = undefined;
    job.isReprocessing = Boolean(job.output);
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
    const worker = this.#workerFactory?.() ?? new Worker(new URL("../worker/optimizer.worker.ts", import.meta.url), {
      type: "module",
      name: "squeeze-optimizer",
    });
    worker.onmessage = (event: MessageEvent<WorkerResponse>) => this.#onMessage(event.data);
    worker.onerror = (event) => {
      this.#engine = "error";
      const active = this.#activeId ? this.#find(this.#activeId) : undefined;
      if (active) {
        active.status = "error";
        active.error = `Worker uległ awarii: ${event.message || "nieznany błąd"}`;
        active.errorCode = "WORKER_CRASH";
        active.recoverable = true;
      }
      this.#activeId = undefined;
      worker.terminate();
      this.#emit();
    };
    return worker;
  }

  async #pump(): Promise<void> {
    if (this.#paused || this.#activeId || this.#engine !== "ready") return;
    const job = this.#jobs.find((candidate) => candidate.status === "queued");
    if (!job) {
      if (!this.#autoStart && !this.#paused) {
        this.#paused = true;
        this.#emit();
      }
      return;
    }
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
      job.errorCode = "FILE_READ_FAILED";
      job.recoverable = true;
      this.#activeId = undefined;
      this.#emit();
      void this.#pump();
    }
  }

  #onMessage(message: WorkerResponse): void {
    if (message.version !== WORKER_API_VERSION) return;
    if (message.type === "ready") {
      this.#capabilities = message.capabilities;
      this.#engine = message.capabilities.preserve || message.capabilities.webp ? "ready" : "error";
      this.#emit();
      if (this.#engine === "ready") void this.#pump();
      return;
    }
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
      job.isReprocessing = false;
    } else if (message.type === "error") {
      job.status = "error";
      job.error = message.message;
      job.errorCode = message.code;
      job.recoverable = message.recoverable;
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

  #recoverWorker(): void {
    if (this.#engine !== "error") return;
    this.#worker.terminate();
    this.#engine = "loading";
    this.#capabilities = undefined;
    this.#worker = this.#createWorker();
  }

  #revokePreview(id: string): void {
    const url = this.#previewUrls.get(id);
    if (url) URL.revokeObjectURL(url);
    this.#previewUrls.delete(id);
  }

  #emit(): void {
    const snapshot = this.#snapshot();
    for (const listener of this.#listeners) listener(snapshot, this.#paused, this.#engine, this.#capabilities);
  }

  #snapshot(): readonly CompressionJob[] {
    return this.#jobs.map((job) => ({ ...job, report: job.report ? { ...job.report } : undefined }));
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
  return file.size > 0
    && file.size <= 100 * 1024 * 1024
    && ["image/jpeg", "image/png"].includes(file.type);
}

function isPng(file: File): boolean {
  if (file.type === "image/png") return true;
  if (file.type === "image/jpeg") return false;
  return /\.png$/i.test(file.name);
}
