/// <reference lib="webworker" />

import type { OptimizationReport, ProgressStage, WorkerRequest, WorkerResponse } from "../types";
import { WORKER_API_VERSION } from "../types";

interface WasmResult {
  report_json: string;
  take_bytes(): Uint8Array;
}

interface OptimizerWasm {
  default(input?: { module_or_path: URL }): Promise<unknown>;
  worker_api_version(): number;
  optimize_image(
    input: Uint8Array,
    optionsJson: string,
    progress: (eventJson: string) => void,
  ): WasmResult;
}

let modulePromise: Promise<OptimizerWasm> | undefined;
let activeJobId: string | undefined;
let activeAttempt: number | undefined;

// Start the static WASM fetch with the initial application load. Adding an image
// never triggers a request containing image data or its name.
void loadWasm().catch(() => undefined);

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  if (request.version !== WORKER_API_VERSION) {
    postError("unknown", -1, "WORKER_API_MISMATCH", "Nieobsługiwana wersja protokołu.", false);
    return;
  }
  if (request.type === "cancel") {
    if (request.jobId === activeJobId) {
      post({ version: WORKER_API_VERSION, type: "cancelled", jobId: request.jobId, attempt: request.attempt });
    }
    return;
  }
  activeJobId = request.jobId;
  activeAttempt = request.attempt;
  try {
    const wasm = await loadWasm();
    const result = wasm.optimize_image(
      new Uint8Array(request.buffer),
      JSON.stringify(request.options),
      (eventJson) => {
        const progress = JSON.parse(eventJson) as {
          stage: ProgressStage;
          candidate?: number;
          total?: number;
        };
        if (activeJobId !== request.jobId || activeAttempt !== request.attempt) return;
        post({ version: WORKER_API_VERSION, type: "progress", jobId: request.jobId, attempt: request.attempt, ...progress });
      },
    );
    const output = result.take_bytes();
    const transferable = output.buffer.slice(
      output.byteOffset,
      output.byteOffset + output.byteLength,
    ) as ArrayBuffer;
    post(
      {
        version: WORKER_API_VERSION,
        type: "complete",
        jobId: request.jobId,
        attempt: request.attempt,
        result: JSON.parse(result.report_json) as OptimizationReport,
        buffer: transferable,
      },
      [transferable],
    );
  } catch (error) {
    const parsed = parseError(error);
    postError(request.jobId, request.attempt, parsed.code, parsed.message, parsed.recoverable);
  } finally {
    activeJobId = undefined;
    activeAttempt = undefined;
  }
};

async function loadWasm(): Promise<OptimizerWasm> {
  modulePromise ??= (async () => {
    try {
      const moduleUrl = new URL("/wasm/optimizer_wasm.js", self.location.origin).href;
      const wasm = (await import(/* @vite-ignore */ moduleUrl)) as OptimizerWasm;
      await wasm.default({
        module_or_path: new URL("/wasm/optimizer_wasm_bg.wasm", self.location.origin),
      });
      if (wasm.worker_api_version() !== WORKER_API_VERSION) {
        throw new Error("Wersja modułu WASM nie pasuje do Workera.");
      }
      return wasm;
    } catch (error) {
      throw {
        code: "ENGINE_NOT_BUILT",
        message:
          "Nie znaleziono silnika WASM. Uruchom `npm run build:wasm`, a potem odśwież aplikację.",
        recoverable: false,
        cause: error,
      };
    }
  })();
  return modulePromise;
}

function parseError(error: unknown): { code: string; message: string; recoverable: boolean } {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    const value = error as { code: string; message: string; recoverable?: boolean };
    return { code: value.code, message: value.message, recoverable: value.recoverable ?? true };
  }
  const raw = error instanceof Error ? error.message : String(error);
  try {
    return JSON.parse(raw) as { code: string; message: string; recoverable: boolean };
  } catch {
    return { code: "WORKER_FAILURE", message: raw, recoverable: true };
  }
}

function post(message: WorkerResponse, transfer: Transferable[] = []): void {
  self.postMessage(message, { transfer });
}

function postError(
  jobId: string,
  attempt: number,
  code: string,
  message: string,
  recoverable: boolean,
): void {
  post({ version: WORKER_API_VERSION, type: "error", jobId, attempt, code, message, recoverable });
}

export {};
