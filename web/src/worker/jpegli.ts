import type { OptimizationReport, ProgressStage } from "../types";
import { insertSegments, orientationSegment, readJpegHeader, rgbProfileKind, type JpegHeader } from "./jpeg-header";
import { FALLBACK_QUALITY, extractTile, sampleTiles, searchQuality } from "./jpeg-quality";

interface JpegliExports {
  memory: WebAssembly.Memory;
  _initialize(): void;
  malloc(size: number): number;
  free(pointer: number): void;
  encode(
    input: number,
    width: number,
    height: number,
    colorspace: number,
    chroma: number,
    size: number,
    quality: number,
    progressive: number,
    optimizeHuffman: number,
    adaptiveQuantization: number,
    standardQuantTables: number,
    fancyDownsampling: number,
    dctMethod: number,
  ): number;
  transcode(input: number, inputSize: number, size: number, progressive: number): number;
}

/** Scores JPEG-encoded tiles against their RGBA pixels with SSIMULACRA2. */
export interface TileScorer {
  /** Prepares a tile once and returns its index. */
  add_tile(rgba: Uint8Array, width: number, height: number): number;
  score(index: number, jpeg: Uint8Array): number;
  /** Releases the WebAssembly memory. */
  free(): void;
}

type Chroma = "4:4:4" | "4:2:0" | "gray";

/**
 * A lossy result must be clearly smaller than the lossless rewrite; otherwise
 * the lossless file keeps the source pixels for almost the same size.
 */
const MIN_LOSSY_GAIN = 0.9;
const JCS_GRAYSCALE = 1;
const JCS_RGB = 2;
const CHROMA_CODE: Record<Chroma, number> = { "4:4:4": 0, "4:2:0": 2, gray: 0 };

let jpegliPromise: Promise<JpegliExports> | undefined;

export async function optimizeJpegWithJpegli(
  input: Uint8Array,
  reportProgress: (stage: ProgressStage) => void,
  createScorer: () => TileScorer,
): Promise<{ report: OptimizationReport; output: ArrayBuffer }> {
  const header = readJpegHeader(input);
  if (!header) throw new Error("Invalid JPEG metadata.");
  const gray = header.components === 1;
  const profile = colourProfilePlan(header, gray);

  const started = performance.now();
  reportProgress("decoding");
  const bitmap = await createImageBitmap(new Blob([toArrayBuffer(input)], { type: "image/jpeg" }), {
    imageOrientation: "from-image",
    colorSpaceConversion: profile.convertToSrgb ? "default" : "none",
  });
  try {
    const width = bitmap.width;
    const height = bitmap.height;
    const context = new OffscreenCanvas(width, height).getContext("2d", { willReadFrequently: true });
    if (!context) throw new Error("Canvas 2D is unavailable in this browser.");
    context.drawImage(bitmap, 0, 0);
    const rgba = context.getImageData(0, 0, width, height).data;

    reportProgress("analyzing");
    const analysis = analyzeRgba(rgba, width, height);
    const chroma: Chroma = gray ? "gray" : analysis.kind === "screenshot" || analysis.kind === "graphic" ? "4:4:4" : "4:2:0";
    const jpegli = await loadJpegli();
    const quality = chooseQuality(jpegli, rgba, width, height, chroma, createScorer);

    reportProgress("compressing");
    const encoded = withJpegli(() => encode(jpegli, rgba, width, height, quality, chroma));
    const lossy = profile.keep ? insertSegments(encoded, header.iccSegments) : encoded;
    const lossless = transcodeLosslessly(jpegli, input, header);
    const useLossless = lossless !== undefined && lossy.byteLength > lossless.byteLength * MIN_LOSSY_GAIN;
    const candidate = useLossless ? lossless : lossy;
    await verifyJpeg(candidate, width, height);
    reportProgress("finalizing");

    if (candidate.byteLength >= input.byteLength) {
      return {
        output: toArrayBuffer(input),
        report: passthroughReport(input.byteLength, width, height, analysis, started),
      };
    }

    return {
      output: toArrayBuffer(candidate),
      report: {
        format: "jpeg",
        outputFormat: "jpeg",
        width,
        height,
        originalSize: input.byteLength,
        optimizedSize: candidate.byteLength,
        savedBytes: input.byteLength - candidate.byteLength,
        savedPercent: (input.byteLength - candidate.byteLength) / input.byteLength * 100,
        strategy: {
          encoder: useLossless ? "jpegli lossless" : "jpegli-wasm",
          quality: useLossless ? null : quality,
          chromaSubsampling: useLossless ? null : chroma,
          progressive: true,
          paletteColors: null,
          lossless: useLossless,
        },
        processingTimeMs: performance.now() - started,
        alreadyOptimized: false,
        optimizerVersion: 3,
        warnings: useLossless ? ["Piksele zachowano bez zmian; przepisano tylko kodowanie JPEG."] : [],
        analysis,
      },
    };
  } finally {
    bitmap.close();
  }
}

export function isJpeg(input: Uint8Array): boolean {
  return input.byteLength >= 3 && input[0] === 0xff && input[1] === 0xd8 && input[2] === 0xff;
}

/**
 * How the lossy path treats the embedded profile. The quality search compares
 * raw pixel values, so they must be sRGB or close to it:
 * - sRGB is what browsers assume, so its profile only costs bytes;
 * - Display P3 (phone photos) is kept with its wide gamut;
 * - wider RGB spaces such as Adobe RGB or ProPhoto, CMYK, and profiles that do
 *   not match the output channels are converted to sRGB by the browser.
 */
function colourProfilePlan(header: JpegHeader, gray: boolean): { convertToSrgb: boolean; keep: boolean } {
  if (!header.iccSegments.length) return { convertToSrgb: false, keep: false };
  if (gray) {
    const grayProfile = header.iccColorSpace === "GRAY";
    return { convertToSrgb: !grayProfile, keep: grayProfile };
  }
  const kind = header.iccColorSpace === "RGB " ? rgbProfileKind(header.iccSegments) : undefined;
  if (kind === "srgb") return { convertToSrgb: false, keep: false };
  if (kind === "display-p3") return { convertToSrgb: false, keep: true };
  return { convertToSrgb: true, keep: false };
}

function chooseQuality(
  jpegli: JpegliExports,
  rgba: Uint8ClampedArray,
  width: number,
  height: number,
  chroma: Chroma,
  createScorer: () => TileScorer,
): number {
  const sample = sampleTiles(rgba, width, height);
  if (!sample.tiles.length) return FALLBACK_QUALITY;
  const scorer = createScorer();
  try {
    const tiles = sample.tiles.map((tile) => {
      const pixels = extractTile(rgba, width, tile);
      return { tile, pixels, index: scorer.add_tile(pixels, tile.size, tile.size) };
    });
    return searchQuality((quality) => Math.min(...tiles.map(({ tile, pixels, index }) => {
      const jpeg = withJpegli(() => encode(jpegli, pixels, tile.size, tile.size, quality, chroma));
      return scorer.score(index, jpeg);
    })), sample.target);
  } finally {
    scorer.free();
  }
}

/**
 * Rewrites Huffman coding and scans without touching the coefficients. The
 * pixels stay stored as in the source, so its orientation tag is kept.
 */
function transcodeLosslessly(jpegli: JpegliExports, input: Uint8Array, header: JpegHeader): Uint8Array | undefined {
  try {
    const transcoded = withJpegli(() => transcode(jpegli, input));
    const orientation = header.orientation === 1 ? [] : [orientationSegment(header.orientation)];
    return insertSegments(transcoded, [...orientation, ...header.iccSegments]);
  } catch {
    // Arithmetic coding, 12-bit and lossless JPEG are outside Jpegli's
    // decoder; those files only take the lossy path.
    return undefined;
  }
}

async function loadJpegli(): Promise<JpegliExports> {
  jpegliPromise ??= (async () => {
    const module = await WebAssembly.compileStreaming(fetch(new URL("/wasm/jpegli.wasm", self.location.origin)));
    const wasi = {
      clock_time_get: () => 0,
      fd_close: () => 0,
      fd_seek: () => 0,
      fd_write: () => 0,
      proc_exit: (code: number) => { throw new Error(`Jpegli stopped with WASI code ${code}.`); },
    };
    const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: wasi });
    const exports = instance.exports as unknown as JpegliExports;
    exports._initialize();
    return exports;
  })();
  return jpegliPromise;
}

/**
 * A Jpegli error aborts inside WebAssembly without unwinding its stack or
 * heap, so the instance is discarded and the next call loads a fresh one.
 */
function withJpegli<T>(operation: () => T): T {
  try {
    return operation();
  } catch (error) {
    jpegliPromise = undefined;
    throw error;
  }
}

function encode(jpegli: JpegliExports, rgba: Uint8Array | Uint8ClampedArray, width: number, height: number, quality: number, chroma: Chroma): Uint8Array {
  const pixels = chroma === "gray" ? grayChannel(rgba, width * height) : rgba;
  return callWithOutput(jpegli, pixels, (inputPointer, sizePointer) => jpegli.encode(
    inputPointer, width, height,
    chroma === "gray" ? JCS_GRAYSCALE : JCS_RGB, // the RGB path accepts RGBA components
    CHROMA_CODE[chroma],
    sizePointer, quality,
    2, // progressive
    1, // optimized Huffman coding
    1, // adaptive quantization
    0, // Jpegli quantization tables
    1, // high-quality downsampling
    0,
  ));
}

function transcode(jpegli: JpegliExports, input: Uint8Array): Uint8Array {
  return callWithOutput(jpegli, input, (inputPointer, sizePointer) =>
    jpegli.transcode(inputPointer, input.byteLength, sizePointer, 2));
}

function callWithOutput(jpegli: JpegliExports, input: Uint8Array | Uint8ClampedArray, call: (inputPointer: number, sizePointer: number) => number): Uint8Array {
  const inputPointer = jpegli.malloc(input.byteLength);
  const sizePointer = jpegli.malloc(4);
  try {
    new Uint8Array(jpegli.memory.buffer).set(input, inputPointer);
    const outputPointer = call(inputPointer, sizePointer);
    const size = new DataView(jpegli.memory.buffer).getUint32(sizePointer, true);
    if (!outputPointer || !size) throw new Error("Jpegli produced an empty JPEG.");
    const output = new Uint8Array(jpegli.memory.buffer).slice(outputPointer, outputPointer + size);
    jpegli.free(outputPointer);
    return output;
  } finally {
    jpegli.free(sizePointer);
    jpegli.free(inputPointer);
  }
}

/** Greyscale JPEGs decode to equal RGB channels; the red one is kept. */
function grayChannel(rgba: Uint8Array | Uint8ClampedArray, pixels: number): Uint8Array {
  const gray = new Uint8Array(pixels);
  for (let index = 0; index < pixels; index++) gray[index] = rgba[index * 4] ?? 0;
  return gray;
}

/** The decoded size must match, turned as the source is displayed. */
async function verifyJpeg(bytes: Uint8Array, width: number, height: number): Promise<void> {
  const decoded = await createImageBitmap(new Blob([toArrayBuffer(bytes)], { type: "image/jpeg" }), { imageOrientation: "from-image" });
  try {
    if (decoded.width !== width || decoded.height !== height) {
      throw new Error("Jpegli changed image dimensions.");
    }
  } finally {
    decoded.close();
  }
}

function analyzeRgba(pixels: Uint8ClampedArray, width: number, height: number): OptimizationReport["analysis"] {
  const stride = Math.max(1, Math.floor(width * height / 250_000));
  const histogram = new Uint32Array(256);
  const colors = new Set<number>();
  let sampled = 0, edges = 0, noisy = 0, flat = 0;
  for (let pixelIndex = 0; pixelIndex < width * height; pixelIndex += stride) {
    const offset = pixelIndex * 4;
    const r = pixels[offset] ?? 0, g = pixels[offset + 1] ?? 0, b = pixels[offset + 2] ?? 0;
    const luma = (r * 54 + g * 183 + b * 19) >> 8;
    histogram[luma] = (histogram[luma] ?? 0) + 1;
    if (colors.size < 65_536) colors.add((r << 16) | (g << 8) | b);
    sampled++;
    const x = pixelIndex % width, y = Math.floor(pixelIndex / width);
    if (x > 0 && y > 0 && y < height) {
      const left = offset - 4, up = offset - width * 4;
      const difference = Math.max(channelDifference(pixels, offset, left), channelDifference(pixels, offset, up));
      if (difference > 42) edges++;
      else if (difference >= 12) noisy++;
      else if (difference < 5) flat++;
    }
  }
  let entropy = 0;
  for (const count of histogram) if (count) { const probability = count / sampled; entropy -= probability * Math.log2(probability); }
  const divisor = Math.max(1, sampled);
  const edgeDensity = edges / divisor, noise = noisy / divisor, flatAreaRatio = flat / divisor;
  const estimatedColors = Math.min(colors.size * stride, 16_777_216);
  return {
    kind: flatAreaRatio > 0.72 && edgeDensity > 0.04 ? "screenshot" : estimatedColors < 2_000 && flatAreaRatio > 0.55 ? "graphic" : entropy > 6.8 && noise > 0.18 ? "photo" : "mixed",
    entropy, estimatedColors, edgeDensity, noise, flatAreaRatio, hasAlpha: false,
  };
}

function channelDifference(pixels: Uint8ClampedArray, first: number, second: number): number {
  return Math.floor((Math.abs((pixels[first] ?? 0) - (pixels[second] ?? 0)) + Math.abs((pixels[first + 1] ?? 0) - (pixels[second + 1] ?? 0)) + Math.abs((pixels[first + 2] ?? 0) - (pixels[second + 2] ?? 0))) / 3);
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  return Uint8Array.from(bytes).buffer as ArrayBuffer;
}

function passthroughReport(size: number, width: number, height: number, analysis: OptimizationReport["analysis"], started: number): OptimizationReport {
  return {
    format: "jpeg", outputFormat: "jpeg", width, height, originalSize: size, optimizedSize: size,
    savedBytes: 0, savedPercent: 0,
    strategy: { encoder: "already-optimized", quality: null, chromaSubsampling: null, progressive: null, paletteColors: null, lossless: true },
    processingTimeMs: performance.now() - started, alreadyOptimized: true, optimizerVersion: 3,
    warnings: ["Brak oszczędności w tym przebiegu."], analysis,
  };
}
