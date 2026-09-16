import type { CompressionJob, WorkerCapabilities, WorkerRequest, WorkerResponse } from "../types";
import { WORKER_API_VERSION } from "../types";

export type EngineState = "loading" | "ready" | "error";
type Listener = (jobs: readonly CompressionJob[], paused: boolean, engine: EngineState, capabilities?: WorkerCapabilities) => void;

export interface CompressionQueueOptions { workerFactory?: () => Worker; }
export interface AddResult { accepted: number; rejected: File[]; }

/** One worker keeps WebAssembly memory bounded and the UI order predictable. */
export class CompressionQueue {
  #jobs: CompressionJob[] = [];
  #worker: Worker;
  #activeId?: string;
  #paused = false;
  #engine: EngineState = "loading";
  #capabilities?: WorkerCapabilities;
  #workerFactory?: () => Worker;
  #listeners = new Set<Listener>();
  #previewUrls = new Map<string, string>();

  constructor(options: CompressionQueueOptions = {}) {
    this.#workerFactory = options.workerFactory;
    this.#worker = this.#createWorker();
  }

  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener);
    listener(this.#snapshot(), this.#paused, this.#engine, this.#capabilities);
    return () => this.#listeners.delete(listener);
  }

  add(files: Iterable<File>): AddResult {
    const rejected: File[] = [];
    let accepted = 0;
    for (const file of files) {
      if (!isSupported(file)) { rejected.push(file); continue; }
      accepted += 1;
      this.#jobs.push({ id: crypto.randomUUID(), file, attempt: 0, status: "queued" });
    }
    this.#emit();
    void this.#pump();
    return { accepted, rejected };
  }

  start(): void {
    if (!this.#paused) return;
    this.#recoverWorker();
    this.#paused = false;
    this.#emit();
    void this.#pump();
  }

  pause(): void { if (!this.#paused) { this.#paused = true; this.#emit(); } }

  rerun(id: string): void {
    const job = this.#find(id);
    if (!job) return;
    this.#recoverWorker();
    job.status = "queued";
    job.error = undefined;
    job.errorCode = undefined;
    job.recoverable = undefined;
    job.stage = undefined;
    job.isReprocessing = Boolean(job.output);
    job.attempt += 1;
    this.#emit();
    void this.#pump();
  }

  clearCompleted(): void {
    for (const job of this.#jobs) if (["complete", "cancelled", "error"].includes(job.status)) this.#revokePreview(job.id);
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
    if (!url) { url = URL.createObjectURL(job.file); this.#previewUrls.set(job.id, url); }
    return url;
  }

  dispose(): void {
    this.#worker.terminate();
    for (const url of this.#previewUrls.values()) URL.revokeObjectURL(url);
    this.#previewUrls.clear();
  }

  #createWorker(): Worker {
    const worker = this.#workerFactory?.() ?? new Worker(new URL("../worker/optimizer.worker.ts", import.meta.url), { type: "module", name: "squeeze-optimizer" });
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
    if (!job) return;
    this.#activeId = job.id;
    job.status = "processing";
    job.stage = "decoding";
    const attempt = job.attempt;
    this.#emit();
    try {
      const buffer = await job.file.arrayBuffer();
      if (job.status !== "processing" || this.#activeId !== job.id || job.attempt !== attempt) return;
      const request: WorkerRequest = { version: WORKER_API_VERSION, type: "compress", jobId: job.id, attempt, buffer };
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
      this.#engine = message.capabilities.preserve ? "ready" : "error";
      this.#emit();
      if (this.#engine === "ready") void this.#pump();
      return;
    }
    const job = this.#find(message.jobId);
    if (!job || job.attempt !== message.attempt) return;
    if (message.type === "progress") { job.stage = message.stage; this.#emit(); return; }
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
    } else job.status = "cancelled";
    job.stage = undefined;
    if (this.#activeId === job.id) this.#activeId = undefined;
    this.#emit();
    void this.#pump();
  }

  #find(id: string): CompressionJob | undefined { return this.#jobs.find((job) => job.id === id); }
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
  #snapshot(): readonly CompressionJob[] { return this.#jobs.map((job) => ({ ...job, report: job.report ? { ...job.report } : undefined })); }
}

function isSupported(file: File): boolean {
  return file.size > 0 && file.size <= 100 * 1024 * 1024 && ["image/jpeg", "image/png"].includes(file.type);
}
