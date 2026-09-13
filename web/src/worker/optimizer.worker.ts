/// <reference lib="webworker" />

import encodeWebp, { init as initWebpEncoder } from "@jsquash/webp/encode.js";
import decodeWebp, { init as initWebpDecoder } from "@jsquash/webp/decode.js";
import decoderWasmUrl from "@jsquash/webp/codec/dec/webp_dec.wasm?url";
import encoderWasmUrl from "@jsquash/webp/codec/enc/webp_enc.wasm?url";
import encoderSimdWasmUrl from "@jsquash/webp/codec/enc/webp_enc_simd.wasm?url";
import type { OptimizationOptions, OptimizationReport, ProgressStage, WorkerRequest, WorkerResponse } from "../types";
import { WORKER_API_VERSION } from "../types";

interface WasmResult { report_json: string; take_bytes(): Uint8Array }
interface OptimizerWasm {
  default(input?: { module_or_path: URL }): Promise<unknown>;
  worker_api_version(): number;
  optimize_image(input: Uint8Array, optionsJson: string, progress: (eventJson: string) => void): WasmResult;
  optimize_image_with_diagnostics?(input: Uint8Array, optionsJson: string, progress: (eventJson: string) => void, diagnostics: (eventJson: string) => void): WasmResult;
}

let modulePromise: Promise<OptimizerWasm> | undefined;
let webpPromise: Promise<void> | undefined;
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
    if (request.options.outputFormat === "webp") {
      const result = await optimizeWebp(request.buffer, request.options, (stage, candidate, total) => {
        if (activeJobId !== request.jobId || activeAttempt !== request.attempt) return;
        post({ version: WORKER_API_VERSION, type: "progress", jobId: request.jobId, attempt: request.attempt, stage, candidate, total });
      });
      complete(request.jobId, request.attempt, result.report, result.buffer);
    } else {
      const wasm = await loadWasm();
      const started = performance.now();
      const active = new Map<string, number>();
      const timings: Record<string, number> = {};
      const progress = (eventJson: string) => {
        const progress = JSON.parse(eventJson) as { stage: ProgressStage; candidate?: number; total?: number; variant?: string };
        if (activeJobId !== request.jobId || activeAttempt !== request.attempt) return;
        post({ version: WORKER_API_VERSION, type: "progress", jobId: request.jobId, attempt: request.attempt, ...progress });
      };
      const diagnostics = (eventJson: string) => {
        const event = JSON.parse(eventJson) as { type: "begin" | "end"; operation: string };
        const now = performance.now();
        if (event.type === "begin") active.set(event.operation, now);
        else {
          const began = active.get(event.operation);
          if (began !== undefined) timings[event.operation] = (timings[event.operation] ?? 0) + now - began;
        }
      };
      const result = request.diagnostics && wasm.optimize_image_with_diagnostics
        ? wasm.optimize_image_with_diagnostics(new Uint8Array(request.buffer), JSON.stringify(request.options), progress, diagnostics)
        : wasm.optimize_image(new Uint8Array(request.buffer), JSON.stringify(request.options), progress);
      const output = result.take_bytes();
      const buffer = output.buffer.slice(output.byteOffset, output.byteOffset + output.byteLength) as ArrayBuffer;
      timings.total = performance.now() - started;
      complete(request.jobId, request.attempt, JSON.parse(result.report_json) as OptimizationReport, buffer, request.diagnostics ? timings : undefined);
    }
  } catch (error) {
    const parsed = parseError(error);
    postError(request.jobId, request.attempt, parsed.code, parsed.message, parsed.recoverable);
  } finally {
    activeJobId = undefined;
    activeAttempt = undefined;
  }
};

async function announceReadiness(): Promise<void> {
  const [preserve, webp] = await Promise.allSettled([loadWasm(), initializeWebp()]);
  post({
    version: WORKER_API_VERSION,
    type: "ready",
    capabilities: { preserve: preserve.status === "fulfilled", webp: webp.status === "fulfilled", maxPixels: 24_000_000 },
  });
}

async function optimizeWebp(
  input: ArrayBuffer,
  options: OptimizationOptions,
  progress: (stage: ProgressStage, candidate?: number, total?: number) => void,
): Promise<{ report: OptimizationReport; buffer: ArrayBuffer }> {
  const started = performance.now();
  if (input.byteLength > options.limits.maxInputBytes) throw userError("RESOURCE_LIMIT", "Plik przekracza limit 100 MB.");
  await initializeWebp();
  progress("decoding");
  if (isPng(input) && pngBitDepth(input) === 16) {
    throw userError("UNSUPPORTED_COLOR_DEPTH", "Konwersja PNG 16-bit do WebP nie jest jeszcze obsługiwana. Wybierz „Zachowaj format”.");
  }
  const source = await decodeBrowserImage(input, options);
  progress("analyzing");
  const analysis = analyzePixels(source);
  const qualities = options.profile === "maximumQuality" ? [92, 88, 84] : options.profile === "maximumCompression" ? [82, 74, 66, 58] : [88, 82, 76, 70];
  const maxMeanError = options.profile === "maximumQuality" ? 2.4 : options.profile === "maximumCompression" ? 7.5 : 4.6;
  let winner: { buffer: ArrayBuffer; quality: number; error: number } | undefined;
  for (let index = 0; index < qualities.length; index += 1) {
    const quality = qualities[index]!;
    progress("searching", index + 1, qualities.length);
    const buffer = await encodeWebp(source, { quality, method: 6, pass: 6, alpha_quality: 100, exact: 1, use_sharp_yuv: 1 });
    progress("measuring", index + 1, qualities.length);
    const decoded = await decodeWebp(buffer.slice(0));
    if (decoded.width !== source.width || decoded.height !== source.height) continue;
    const error = compositeMeanError(source.data, decoded.data);
    if (error <= maxMeanError && (!winner || buffer.byteLength < winner.buffer.byteLength)) winner = { buffer, quality, error };
  }
  if (!winner) throw userError("QUALITY_THRESHOLD", "WebP nie przeszedł wybranego progu jakości. Wybierz łagodniejszy profil.");
  progress("finalizing");
  const originalSize = input.byteLength;
  const optimizedSize = winner.buffer.byteLength;
  const delta = originalSize - optimizedSize;
  return {
    buffer: winner.buffer,
    report: {
      format: isPng(input) ? "png" : "jpeg",
      outputFormat: "webp",
      width: source.width,
      height: source.height,
      originalSize,
      optimizedSize,
      savedBytes: delta,
      savedPercent: originalSize ? (delta / originalSize) * 100 : 0,
      metrics: { ssimulacra2: null, butteraugli: null },
      strategy: { encoder: "libwebp", quality: winner.quality, chromaSubsampling: null, progressive: null, paletteColors: null, dithering: null, qualityGuard: null, lossless: false },
      candidatesTested: qualities.length,
      processingTimeMs: performance.now() - started,
      alreadyOptimized: false,
      profileSetVersion: 3,
      warnings: [
        ...(winner.error > maxMeanError * 0.8 ? ["Wynik jest blisko progu jakości wybranego profilu."] : []),
        ...(isPng(input) && hasPngColorProfile(input) ? ["Osadzony profil koloru został przeliczony przez przeglądarkę do przestrzeni wyniku WebP."] : []),
      ],
      analysis,
    },
  };
}

function initializeWebp(): Promise<void> {
  webpPromise ??= Promise.all([
    initWebpEncoder({ locateFile: (path: string) => path.includes("simd") ? encoderSimdWasmUrl : encoderWasmUrl }),
    initWebpDecoder({ locateFile: () => decoderWasmUrl }),
  ]).then(() => undefined);
  return webpPromise;
}

async function decodeBrowserImage(input: ArrayBuffer, options: OptimizationOptions): Promise<ImageData> {
  const bitmap = await createImageBitmap(new Blob([input], { type: isPng(input) ? "image/png" : "image/jpeg" }), { imageOrientation: "from-image" });
  try {
    const pixels = bitmap.width * bitmap.height;
    if (pixels > options.limits.maxPixels) throw userError("RESOURCE_LIMIT", "Obraz przekracza limit 24 MP.");
    if (pixels * 12 > options.limits.maxWorkingBytes) throw userError("RESOURCE_LIMIT", "Obraz wymaga zbyt dużo pamięci roboczej.");
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) throw userError("DECODE_FAILED", "Przeglądarka nie udostępniła dekodera obrazu.");
    context.drawImage(bitmap, 0, 0);
    return context.getImageData(0, 0, bitmap.width, bitmap.height);
  } finally { bitmap.close(); }
}

function compositeMeanError(source: Uint8ClampedArray, candidate: Uint8ClampedArray): number {
  let total = 0;
  for (let index = 0; index < source.length; index += 4) {
    const sourceAlpha = source[index + 3]! / 255;
    const candidateAlpha = candidate[index + 3]! / 255;
    for (const background of [0, 255]) for (let channel = 0; channel < 3; channel += 1) {
      const a = source[index + channel]! * sourceAlpha + background * (1 - sourceAlpha);
      const b = candidate[index + channel]! * candidateAlpha + background * (1 - candidateAlpha);
      total += Math.abs(a - b);
    }
  }
  return total / Math.max(1, (source.length / 4) * 6);
}

function analyzePixels(image: ImageData): OptimizationReport["analysis"] {
  let alpha = false, changes = 0, samples = 0;
  const colors = new Set<number>();
  const step = Math.max(4, Math.floor(image.data.length / 16000 / 4) * 4);
  for (let index = 0; index < image.data.length; index += step) {
    const r = image.data[index]!, g = image.data[index + 1]!, b = image.data[index + 2]!, a = image.data[index + 3]!;
    colors.add((r >> 3) << 10 | (g >> 3) << 5 | (b >> 3));
    alpha ||= a < 255;
    if (index >= step) changes += Math.abs(r - image.data[index - step]!) + Math.abs(g - image.data[index - step + 1]!) + Math.abs(b - image.data[index - step + 2]!);
    samples += 1;
  }
  const edgeDensity = Math.min(1, changes / Math.max(1, samples - 1) / 255 / 3);
  const kind = colors.size < 256 ? "graphic" : edgeDensity > 0.24 ? "photo" : "mixed";
  return { kind, entropy: edgeDensity, estimatedColors: colors.size, edgeDensity, noise: 0, flatAreaRatio: Math.max(0, 1 - edgeDensity), hasAlpha: alpha };
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

function complete(jobId: string, attempt: number, result: OptimizationReport, buffer: ArrayBuffer, timings?: Record<string, number>): void {
  post({ version: WORKER_API_VERSION, type: "complete", jobId, attempt, result, buffer, timings }, [buffer]);
}
function isPng(input: ArrayBuffer): boolean {
  const bytes = new Uint8Array(input, 0, Math.min(8, input.byteLength));
  return bytes.length === 8 && bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47;
}
function pngBitDepth(input: ArrayBuffer): number | undefined { return isPng(input) ? new Uint8Array(input)[24] : undefined; }
function hasPngColorProfile(input: ArrayBuffer): boolean {
  if (!isPng(input)) return false;
  const bytes = new Uint8Array(input);
  const names = ["iCCP", "sRGB", "cHRM", "gAMA"];
  let cursor = 8;
  while (cursor + 12 <= bytes.length) {
    const length = ((bytes[cursor]! << 24) | (bytes[cursor + 1]! << 16) | (bytes[cursor + 2]! << 8) | bytes[cursor + 3]!) >>> 0;
    const name = String.fromCharCode(...bytes.slice(cursor + 4, cursor + 8));
    if (names.includes(name)) return true;
    cursor += 12 + length;
  }
  return false;
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
