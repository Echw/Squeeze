/// <reference lib="webworker" />

import type { OptimizationReport, ProgressStage, WorkerRequest, WorkerResponse } from "../types";
import { WORKER_API_VERSION } from "../types";
import { isJpeg, optimizeJpegWithJpegli } from "./jpegli";

interface WasmResult { report_json: string; take_bytes(): Uint8Array }
interface OptimizerWasm {
  default(input?: { module_or_path: URL }): Promise<unknown>;
  worker_api_version(): number;
  optimize_image(input: Uint8Array, optionsJson: string, progress: (eventJson: string) => void): WasmResult;
}

const OPTIONS = JSON.stringify({
  limits: {
    maxInputBytes: 100 * 1024 * 1024,
    maxPixels: 24_000_000,
    maxWorkingBytes: 768 * 1024 * 1024,
  },
});

let modulePromise: Promise<OptimizerWasm> | undefined;
let activeJobId: string | undefined;
let activeAttempt: number | undefined;

void announceReadiness();

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  if (request.version !== WORKER_API_VERSION) {
    postError("unknown", -1, "WORKER_API_MISMATCH", "Nieobsługiwana wersja protokołu.", false);
    return;
  }
  if (request.type === "cancel") {
    if (request.jobId === activeJobId) post({ version: WORKER_API_VERSION, type: "cancelled", jobId: request.jobId, attempt: request.attempt });
    return;
  }

  activeJobId = request.jobId;
  activeAttempt = request.attempt;
  try {
    const progress = (eventJson: string) => {
      const { stage } = JSON.parse(eventJson) as { stage: ProgressStage };
      if (activeJobId !== request.jobId || activeAttempt !== request.attempt) return;
      post({ version: WORKER_API_VERSION, type: "progress", jobId: request.jobId, attempt: request.attempt, stage });
    };

    if (isJpeg(new Uint8Array(request.buffer))) {
      try {
        const jpegli = await optimizeJpegWithJpegli(new Uint8Array(request.buffer), (stage) => {
          if (activeJobId !== request.jobId || activeAttempt !== request.attempt) return;
          post({ version: WORKER_API_VERSION, type: "progress", jobId: request.jobId, attempt: request.attempt, stage });
        });
        complete(request.jobId, request.attempt, jpegli.report, jpegli.output);
        return;
      } catch {
        // Malformed metadata and browsers without the required canvas
        // primitives use the established Rust/WASM path.
      }
    }

    const wasm = await loadWasm();
    const result = wasm.optimize_image(new Uint8Array(request.buffer), OPTIONS, progress);
    const output = result.take_bytes();
    const buffer = output.buffer.slice(output.byteOffset, output.byteOffset + output.byteLength) as ArrayBuffer;
    complete(request.jobId, request.attempt, JSON.parse(result.report_json) as OptimizationReport, buffer);
  } catch (error) {
    const parsed = parseError(error);
    postError(request.jobId, request.attempt, parsed.code, parsed.message, parsed.recoverable);
  } finally {
    activeJobId = undefined;
    activeAttempt = undefined;
  }
};

async function announceReadiness(): Promise<void> {
  const result = await Promise.allSettled([loadWasm()]);
  post({
    version: WORKER_API_VERSION,
    type: "ready",
    capabilities: { preserve: result[0]?.status === "fulfilled", maxPixels: 24_000_000 },
  });
}

async function loadWasm(): Promise<OptimizerWasm> {
  modulePromise ??= (async () => {
    const moduleUrl = new URL("/wasm/optimizer_wasm.js", self.location.origin).href;
    const wasm = (await import(/* @vite-ignore */ moduleUrl)) as OptimizerWasm;
    await wasm.default({ module_or_path: new URL("/wasm/optimizer_wasm_bg.wasm", self.location.origin) });
    if (wasm.worker_api_version() !== WORKER_API_VERSION) throw userError("ENGINE_VERSION", "Wersja silnika WASM nie pasuje do aplikacji.", false);
    return wasm;
  })();
  return modulePromise;
}

function complete(jobId: string, attempt: number, result: OptimizationReport, buffer: ArrayBuffer): void {
  post({ version: WORKER_API_VERSION, type: "complete", jobId, attempt, result, buffer }, [buffer]);
}

function userError(code: string, message: string, recoverable = true) { return { code, message, recoverable }; }

function parseError(error: unknown): { code: string; message: string; recoverable: boolean } {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    const value = error as { code: string; message: string; recoverable?: boolean };
    return { code: value.code, message: value.message, recoverable: value.recoverable ?? true };
  }
  const raw = error instanceof Error ? error.message : String(error);
  try { return JSON.parse(raw) as { code: string; message: string; recoverable: boolean }; }
  catch { return { code: "WORKER_FAILURE", message: raw, recoverable: true }; }
}

function post(message: WorkerResponse, transfer: Transferable[] = []): void { self.postMessage(message, { transfer }); }

function postError(jobId: string, attempt: number, code: string, message: string, recoverable: boolean): void {
  post({ version: WORKER_API_VERSION, type: "error", jobId, attempt, code, message, recoverable });
}

export {};
