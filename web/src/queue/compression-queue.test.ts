import { describe, expect, it, vi } from "vitest";
import type { CompressionJob, OptimizationReport, WorkerRequest, WorkerResponse } from "../types";
import { WORKER_API_VERSION } from "../types";
import { CompressionQueue } from "./compression-queue";

class FakeWorker {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  messages: unknown[] = [];
  postMessage(message: unknown): void { this.messages.push(message); }
  terminate(): void {}
  emit(message: WorkerResponse): void { this.onmessage?.({ data: message } as MessageEvent<WorkerResponse>); }
  fail(message = "boom"): void { this.onerror?.({ message } as ErrorEvent); }
}

describe("CompressionQueue", () => {
  it("sends exactly one fixed compression request for an added image", async () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    announceReady(worker);
    queue.add([imageFile()]);
    await tick();
    expect(worker.messages).toHaveLength(1);
    expect(worker.messages[0]).toMatchObject({ version: WORKER_API_VERSION, type: "compress" });
    expect(worker.messages[0] as WorkerRequest).not.toHaveProperty("options");
    queue.dispose();
  });

  it("keeps work paused until the user resumes it", async () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    announceReady(worker);
    queue.pause();
    queue.add([imageFile("one.png"), imageFile("two.png")]);
    await tick();
    expect(worker.messages).toHaveLength(0);
    queue.start();
    await tick();
    expect(worker.messages).toHaveLength(1);
    queue.dispose();
  });

  it("recovers a crashed worker only after a retry", async () => {
    const workers: FakeWorker[] = [];
    const queue = new CompressionQueue({ workerFactory: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker as unknown as Worker;
    } });
    let jobs: readonly CompressionJob[] = [];
    let engine: import("./compression-queue").EngineState = "loading";
    queue.subscribe((next, _paused, nextEngine) => { jobs = next; engine = nextEngine; });
    announceReady(workers[0]!);
    queue.add([imageFile()]);
    await tick();
    workers[0]!.fail();
    expect(engine).toBe("error");
    expect(jobs[0]).toMatchObject({ status: "error", error: "Worker uległ awarii: boom" });
    queue.retry(jobs[0]!.id);
    expect(workers).toHaveLength(2);
    announceReady(workers[0]!);
    expect(engine).toBe("loading");
    announceReady(workers[1]!);
    await tick();
    expect(workers[1]!.messages[0]).toMatchObject({ type: "compress", attempt: 1 });
    queue.dispose();
  });

  it("lets the user retry when the WASM engine is unavailable", () => {
    const worker = new FakeWorker();
    const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
    let jobs: readonly CompressionJob[] = [];
    let engine: import("./compression-queue").EngineState = "loading";
    queue.subscribe((next, _paused, nextEngine) => { jobs = next; engine = nextEngine; });
    queue.add([imageFile()]);
    worker.emit({ version: WORKER_API_VERSION, type: "ready", capabilities: { preserve: false, maxPixels: 24_000_000 } });
    expect(engine).toBe("error");
    expect(jobs[0]).toMatchObject({ status: "error", errorCode: "ENGINE_UNAVAILABLE", recoverable: true });
    queue.add([imageFile("another.png")]);
    expect(jobs[1]).toMatchObject({ status: "error", errorCode: "ENGINE_UNAVAILABLE", recoverable: true });
    queue.dispose();
  });

  it("reports a silent worker startup failure instead of waiting forever", () => {
    vi.useFakeTimers();
    try {
      const worker = new FakeWorker();
      const queue = new CompressionQueue({ workerFactory: () => worker as unknown as Worker });
      let jobs: readonly CompressionJob[] = [];
      let engine: import("./compression-queue").EngineState = "loading";
      queue.subscribe((next, _paused, nextEngine) => { jobs = next; engine = nextEngine; });
      queue.add([imageFile()]);
      vi.advanceTimersByTime(30_000);
      expect(engine).toBe("error");
      expect(jobs[0]).toMatchObject({ status: "error", errorCode: "WORKER_START_TIMEOUT", recoverable: true });
      queue.dispose();
    } finally {
      vi.useRealTimers();
    }
  });
});

function imageFile(name = "image.png"): File { return new File([new Uint8Array([1, 2, 3])], name, { type: "image/png" }); }
function tick(): Promise<void> { return new Promise((resolve) => setTimeout(resolve, 0)); }
function announceReady(worker: FakeWorker): void {
  worker.emit({ version: WORKER_API_VERSION, type: "ready", capabilities: { preserve: true, maxPixels: 24_000_000 } });
}
function report(): OptimizationReport {
  return {
    format: "png", outputFormat: "png", width: 1, height: 1, originalSize: 3, optimizedSize: 2, savedBytes: 1, savedPercent: 33.3,
    strategy: { encoder: "test", quality: null, chromaSubsampling: null, progressive: null, paletteColors: null, lossless: true },
    processingTimeMs: 1, alreadyOptimized: false, optimizerVersion: 4, warnings: [],
    analysis: { kind: "graphic", entropy: 0, estimatedColors: 1, edgeDensity: 0, noise: 0, flatAreaRatio: 1, hasAlpha: false },
  };
}
