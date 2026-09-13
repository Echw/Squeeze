import { describe, expect, it } from "vitest";
import type { OptimizationReport, WorkerResponse } from "../types";
import { WORKER_API_VERSION } from "../types";
import { CompressionQueue, optionsFor } from "./compression-queue";

class FakeWorker {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  messages: unknown[] = [];
  postMessage(message: unknown): void { this.messages.push(message); }
  terminate(): void {}
  emit(message: WorkerResponse): void { this.onmessage?.({ data: message } as MessageEvent<WorkerResponse>); }
  fail(message = "boom"): void { this.onerror?.({ message } as ErrorEvent); }
}

describe("optionsFor", () => {
  it("passes the default Auto effort, explicit output format and stable engine limits", () => {
    expect(optionsFor("balanced")).toEqual({
      profile: "balanced",
      searchEffort: "auto",
      method: "auto",
      outputFormat: "preserve",
      metadata: "stripPrivate",
      limits: {
        maxInputBytes: 104_857_600,
        maxPixels: 24_000_000,
        maxWorkingBytes: 805_306_368,
      },
    });
  });

  it("keeps the profile threshold separate from Detailed search effort", () => {
    expect(optionsFor("maximumQuality", "detailed", "palette")).toMatchObject({
      profile: "maximumQuality",
      searchEffort: "detailed",
      method: "palette",
      outputFormat: "preserve",
    });
  });

  it("keeps the explicit multi-method search strategy in the worker options", () => {
    expect(optionsFor("balanced", "auto", "search")).toMatchObject({
      method: "search",
      searchEffort: "auto",
    });
  });

  it("sends one explicit palette variant when the user chooses it", () => {
    expect(optionsFor("maximumCompression", "auto", "palette", "preserve", 128, "floydSteinberg")).toMatchObject({
      method: "palette",
      paletteColors: 128,
      paletteDithering: "floydSteinberg",
    });
  });
});

describe("CompressionQueue", () => {
  it("starts added files automatically by default", async () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    announceReady(worker);
    queue.add([imageFile()], settings());
    await tick();
    expect(worker.messages).toHaveLength(1);
    expect(worker.messages[0]).toMatchObject({ type: "compress" });
    queue.dispose();
  });

  it("does not resume a manually paused queue when another file is added", async () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    announceReady(worker);
    queue.pause();
    queue.add([imageFile("one.png")], settings());
    queue.add([imageFile("two.png")], settings());
    await tick();
    expect(worker.messages).toHaveLength(0);
    queue.start();
    await tick();
    expect(worker.messages).toHaveLength(1);
    queue.dispose();
  });

  it("does not resume a manual pause when the unchanged autostart preference is reapplied", async () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    announceReady(worker);
    queue.pause();
    queue.add([imageFile()], settings());
    queue.setAutoStart(true);
    await tick();
    expect(worker.messages).toHaveLength(0);
    queue.dispose();
  });

  it("keeps new work paused after autostart is disabled", async () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    announceReady(worker);
    queue.setAutoStart(false);
    queue.add([imageFile()], settings());
    await tick();
    expect(worker.messages).toHaveLength(0);
    queue.dispose();
  });

  it("applies changed settings only to waiting work", async () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ autoStart: false, workerFactory: () => worker as unknown as Worker });
    announceReady(worker);
    let jobs = [] as readonly import("../types").CompressionJob[];
    queue.subscribe((next) => { jobs = next; });
    queue.add([imageFile("one.png")], settings());
    queue.reconfigureQueued({ ...settings(), profile: "maximumCompression", outputFormat: "webp" });
    expect(jobs[0]).toMatchObject({ profile: "maximumCompression", outputFormat: "webp", status: "queued" });
    queue.dispose();
  });

  it("uses the detected MIME type instead of a misleading extension", () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ autoStart: false, workerFactory: () => worker as unknown as Worker });
    let jobs = [] as readonly import("../types").CompressionJob[];
    queue.subscribe((next) => { jobs = next; });
    queue.add([imageFile("misleading.jpg")], { ...settings(), method: "palette" });
    expect(jobs[0]).toMatchObject({ method: "palette" });
    queue.dispose();
  });

  it("keeps the previous result available during a rerun", async () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    announceReady(worker);
    let jobs = [] as readonly import("../types").CompressionJob[];
    queue.subscribe((next) => { jobs = next; });
    queue.add([imageFile()], settings());
    await tick();
    const job = jobs[0]!;
    worker.emit({ version: WORKER_API_VERSION, type: "complete", jobId: job.id, attempt: job.attempt, result: report(), buffer: new ArrayBuffer(2) });
    queue.rerun(job.id, { profile: "maximumCompression" });
    expect(jobs[0]).toMatchObject({ id: job.id, status: "processing", isReprocessing: true, profile: "maximumCompression" });
    expect(jobs[0]!.output).toBeDefined();
    queue.dispose();
  });

  it("stays in an error state after a worker crash and recovers only on an explicit retry", async () => {
    const workers: FakeWorker[] = [];
    const queue = new CompressionQueue({ workerFactory: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker as unknown as Worker;
    } });
    let jobs = [] as readonly import("../types").CompressionJob[];
    let engine: import("./compression-queue").EngineState = "loading";
    queue.subscribe((next, _paused, nextEngine) => { jobs = next; engine = nextEngine; });
    announceReady(workers[0]!);
    queue.add([imageFile()], settings());
    await tick();
    workers[0]!.fail();
    expect(workers).toHaveLength(1);
    expect(engine).toBe("error");
    expect(jobs[0]).toMatchObject({ status: "error", error: "Worker uległ awarii: boom" });

    queue.retry(jobs[0]!.id);
    expect(workers).toHaveLength(2);
    expect(engine).toBe("loading");
    announceReady(workers[1]!);
    await tick();
    expect(workers[1]!.messages[0]).toMatchObject({ type: "compress", attempt: 1 });
    queue.dispose();
  });

  it("reports the engine as unavailable when no codec initialized", () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    let engine: import("./compression-queue").EngineState = "loading";
    queue.subscribe((_jobs, _paused, nextEngine) => { engine = nextEngine; });
    worker.emit({ version: WORKER_API_VERSION, type: "ready", capabilities: { preserve: false, webp: false, maxPixels: 24_000_000 } });
    expect(engine).toBe("error");
    queue.dispose();
  });
});

function settings() { return { profile: "balanced" as const, searchEffort: "auto" as const, method: "auto" as const, outputFormat: "preserve" as const }; }
function imageFile(name = "image.png"): File { return new File([new Uint8Array([1, 2, 3])], name, { type: "image/png" }); }
function tick(): Promise<void> { return new Promise((resolve) => setTimeout(resolve, 0)); }
function announceReady(worker: FakeWorker): void {
  worker.emit({ version: WORKER_API_VERSION, type: "ready", capabilities: { preserve: true, webp: true, maxPixels: 24_000_000 } });
}
function report(): OptimizationReport {
  return {
    format: "png", outputFormat: "png", width: 1, height: 1, originalSize: 3, optimizedSize: 2, savedBytes: 1, savedPercent: 33.3,
    metrics: { ssimulacra2: 99, butteraugli: 0 },
    strategy: { encoder: "test", quality: null, chromaSubsampling: null, progressive: null, paletteColors: null, dithering: null, qualityGuard: null, lossless: true },
    candidatesTested: 1, processingTimeMs: 1, alreadyOptimized: false, profileSetVersion: 2, warnings: [],
    analysis: { kind: "graphic", entropy: 0, estimatedColors: 1, edgeDensity: 0, noise: 0, flatAreaRatio: 1, hasAlpha: false },
  };
}
