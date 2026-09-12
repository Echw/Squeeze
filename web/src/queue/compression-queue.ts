import type {
  CompressionJob,
  CompressionProfile,
  OptimizationOptions,
  WorkerRequest,
  WorkerResponse,
} from "../types";

type Listener = (jobs: readonly CompressionJob[]) => void;

export class CompressionQueue {
  #jobs: CompressionJob[] = [];
  #worker = this.#createWorker();
  #activeId?: string;
  #listeners = new Set<Listener>();

  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener);
    listener(this.#jobs);
    return () => this.#listeners.delete(listener);
  }

  add(files: Iterable<File>, profile: CompressionProfile): void {
    for (const file of files) {
      if (!isSupported(file)) continue;
      this.#jobs.push({
        id: crypto.randomUUID(),
        file,
        profile,
        status: "queued",
      });
    }
    this.#emit();
    void this.#pump();
  }

  clearCompleted(): void {
    this.#jobs = this.#jobs.filter((job) => !["complete", "cancelled", "error"].includes(job.status));
    this.#emit();
  }

  remove(id: string): void {
    if (this.#activeId === id) this.cancel(id);
    this.#jobs = this.#jobs.filter((job) => job.id !== id);
    this.#emit();
  }

  cancel(id: string): void {
    const job = this.#find(id);
    if (!job || !["queued", "processing"].includes(job.status)) return;
    job.status = "cancelled";
    job.stage = undefined;
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
    this.#emit();
    void this.#pump();
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
    if (this.#activeId) return;
    const job = this.#jobs.find((candidate) => candidate.status === "queued");
    if (!job) return;
    this.#activeId = job.id;
    job.status = "processing";
    job.stage = "decoding";
    this.#emit();
    try {
      const buffer = await job.file.arrayBuffer();
      if (job.status !== "processing" || this.#activeId !== job.id) return;
      const request: WorkerRequest = {
        version: 1,
        type: "compress",
        jobId: job.id,
        buffer,
        options: optionsFor(job.profile),
      };
      this.#worker.postMessage(request, [buffer]);
    } catch (error) {
      job.status = "error";
      job.error = error instanceof Error ? error.message : String(error);
      this.#activeId = undefined;
      this.#emit();
      void this.#pump();
    }
  }

  #onMessage(message: WorkerResponse): void {
    if (message.version !== 1) return;
    const job = this.#find(message.jobId);
    if (!job) return;
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

  #emit(): void {
    for (const listener of this.#listeners) listener(this.#jobs);
  }
}

export function optionsFor(profile: CompressionProfile): OptimizationOptions {
  return {
    profile,
    outputFormat: "preserve",
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
