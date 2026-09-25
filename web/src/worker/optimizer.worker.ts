/// <reference lib="webworker" />

import type { OptimizationReport, ProgressStage, WorkerRequest, WorkerResponse } from "../types";
import { WORKER_API_VERSION } from "../types";
import { isJpeg, optimizeJpegWithJpegli } from "./jpegli";
import * as optimizerWasm from "virtual:optimizer-wasm-glue";
import initOxiPng, { optimise as optimiseOxiPng } from "@jsquash/oxipng/codec/pkg/squoosh_oxipng.js";
import oxiPngWasmUrl from "@jsquash/oxipng/codec/pkg/squoosh_oxipng_bg.wasm?url";

const OPTIONS = JSON.stringify({
  limits: {
    maxInputBytes: 100 * 1024 * 1024,
    maxPixels: 24_000_000,
    maxWorkingBytes: 768 * 1024 * 1024,
  },
});
const MAX_LOSSLESS_ALPHA_FINALIZER_PIXELS = 1_000_000;

let modulePromise: Promise<typeof optimizerWasm> | undefined;
let pngFinalizerPromise: Promise<unknown> | undefined;
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
    const finalized = await finalizePng(result.take_bytes(), JSON.parse(result.report_json) as OptimizationReport);
    const buffer = finalized.output.buffer.slice(finalized.output.byteOffset, finalized.output.byteOffset + finalized.output.byteLength) as ArrayBuffer;
    complete(request.jobId, request.attempt, finalized.report, buffer);
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

async function loadWasm(): Promise<typeof optimizerWasm> {
  modulePromise ??= (async () => {
    await optimizerWasm.default({ module_or_path: new URL("/wasm/optimizer_wasm_bg.wasm", self.location.origin) });
    if (optimizerWasm.worker_api_version() !== WORKER_API_VERSION) throw userError("ENGINE_VERSION", "Wersja silnika WASM nie pasuje do aplikacji.", false);
    return optimizerWasm;
  })();
  return modulePromise;
}

async function finalizePng(output: Uint8Array, report: OptimizationReport): Promise<{ output: Uint8Array; report: OptimizationReport }> {
  // OxiPNG is lossless. It always follows an already-selected indexed PNG,
  // and also handles small transparent graphics that safely stay RGBA because
  // they have more than 256 exact colors. This is one fixed finalization step,
  // never a second quality candidate.
  if (!shouldFinalizePng(report)) return { output, report };
  try {
    const startedAt = performance.now();
    // The package facade enables its threaded build in a Worker. That build
    // starts nested Workers and cannot resolve Vite's emitted assets here, so
    // initialise its single-threaded codec explicitly. Compression work still
    // stays off the UI thread and uses the same fixed, lossless OxiPNG level.
    await (pngFinalizerPromise ??= initOxiPng(oxiPngWasmUrl));
    const finalized = optimiseOxiPng(output, 2, false, false);
    if (finalized.byteLength >= output.byteLength) return { output, report };
    report.optimizedSize = finalized.byteLength;
    report.savedBytes = report.originalSize - finalized.byteLength;
    report.savedPercent = report.savedBytes / report.originalSize * 100;
    report.processingTimeMs += performance.now() - startedAt;
    report.strategy.encoder = report.strategy.encoder === "already-optimized" ? "OxiPNG" : `${report.strategy.encoder} + OxiPNG`;
    report.alreadyOptimized = false;
    report.warnings = report.warnings.filter((warning) => warning !== "Brak oszczędności w tym przebiegu.");
    return { output: finalized, report };
  } catch {
    // The primary Rust/WASM result is complete and valid without this optional
    // lossless finalizer, so a loading failure never turns into a user error.
    return { output, report };
  }
}

function shouldFinalizePng(report: OptimizationReport): boolean {
  if (report.format !== "png") return false;
  if (report.strategy.paletteColors !== null) return true;
  const pixels = report.width * report.height;
  return report.analysis.hasAlpha
    && report.analysis.kind === "graphic"
    && pixels <= MAX_LOSSLESS_ALPHA_FINALIZER_PIXELS;
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
