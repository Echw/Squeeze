import type { OptimizationReport, ProgressStage } from "../types";

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
}

interface JpegliInstance {
  exports: JpegliExports;
}

let jpegliPromise: Promise<JpegliInstance> | undefined;

export async function optimizeJpegWithJpegli(
  input: Uint8Array,
  reportProgress: (stage: ProgressStage) => void,
): Promise<{ report: OptimizationReport; output: ArrayBuffer }> {
  const iccSegments = extractIccSegments(input);
  if (!iccSegments) throw new Error("Invalid JPEG metadata.");

  const started = performance.now();
  reportProgress("decoding");
  const bitmap = await createImageBitmap(new Blob([toArrayBuffer(input)], { type: "image/jpeg" }), {
    imageOrientation: "from-image",
    colorSpaceConversion: "none",
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
    const quality = selectJpegliQuality(analysis);

    reportProgress("compressing");
    const candidate = injectIccSegments(encode(await loadJpegli(), rgba, width, height, quality), iccSegments);
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
          encoder: "jpegli-wasm",
          quality,
          chromaSubsampling: "4:2:0",
          progressive: true,
          paletteColors: null,
          lossless: false,
        },
        processingTimeMs: performance.now() - started,
        alreadyOptimized: false,
        optimizerVersion: 2,
        warnings: [],
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
 * One measured quality choice. All candidates keep 4:2:0: across the public
 * photo corpus, 4:2:2 and 4:4:4 increased bytes and did not improve the
 * perceptual result enough to justify their cost.
 */
export function selectJpegliQuality(analysis: OptimizationReport["analysis"]): number {
  const { edgeDensity, flatAreaRatio, noise } = analysis;

  // Sparse, almost-flat photographs show banding first, so preserve more data.
  if (flatAreaRatio >= 0.9 && noise < 0.02) return 83;
  // Fine, low-noise detail benefits from a small quality lift.
  if (edgeDensity >= 0.045 && noise < 0.32) return 74;
  // Smooth areas intersected by distinct edges include buildings and text.
  if (flatAreaRatio >= 0.62 && edgeDensity >= 0.02 && noise < 0.12) return 73;
  return 68;
}

async function loadJpegli(): Promise<JpegliInstance> {
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
    return { exports };
  })();
  return jpegliPromise;
}

function encode(instance: JpegliInstance, rgba: Uint8ClampedArray, width: number, height: number, quality: number): Uint8Array {
  const { exports } = instance;
  const inputPointer = exports.malloc(rgba.byteLength);
  const sizePointer = exports.malloc(4);
  try {
    new Uint8Array(exports.memory.buffer).set(rgba, inputPointer);
    const outputPointer = exports.encode(
      inputPointer, width, height,
      2, // JCS_RGB; the wrapper accepts RGBA components.
      2, // 4:2:0
      sizePointer, quality,
      2, // progressive
      1, // optimized Huffman coding
      1, // adaptive quantization
      0, // Jpegli quantization tables
      1, // high-quality downsampling
      0,
    );
    const size = new DataView(exports.memory.buffer).getUint32(sizePointer, true);
    if (!outputPointer || !size) throw new Error("Jpegli produced an empty JPEG.");
    const output = new Uint8Array(exports.memory.buffer).slice(outputPointer, outputPointer + size);
    exports.free(outputPointer);
    return output;
  } finally {
    exports.free(sizePointer);
    exports.free(inputPointer);
  }
}

async function verifyJpeg(bytes: Uint8Array, width: number, height: number): Promise<void> {
  const decoded = await createImageBitmap(new Blob([toArrayBuffer(bytes)], { type: "image/jpeg" }));
  try {
    if (decoded.width !== width || decoded.height !== height) throw new Error("Jpegli changed image dimensions.");
  } finally {
    decoded.close();
  }
}

function extractIccSegments(input: Uint8Array): Uint8Array[] | undefined {
  const segments: Uint8Array[] = [];
  for (let offset = 2; offset + 4 <= input.byteLength && input[offset] === 0xff;) {
    const marker = input[offset + 1];
    if (marker === undefined || marker === 0xda || marker === 0xd9) return segments;
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) { offset += 2; continue; }
    const length = ((input[offset + 2] ?? 0) << 8) | (input[offset + 3] ?? 0);
    if (length < 2 || offset + 2 + length > input.byteLength) return undefined;
    if (marker === 0xe2 && new TextDecoder().decode(input.slice(offset + 4, offset + 16)) === "ICC_PROFILE\0") {
      segments.push(input.slice(offset, offset + 2 + length));
    }
    offset += 2 + length;
  }
  return undefined;
}

function injectIccSegments(jpeg: Uint8Array, iccSegments: readonly Uint8Array[]): Uint8Array {
  if (!iccSegments.length) return jpeg;
  const metadataSize = iccSegments.reduce((size, segment) => size + segment.byteLength, 0);
  const output = new Uint8Array(jpeg.byteLength + metadataSize);
  output.set(jpeg.slice(0, 2));
  let offset = 2;
  for (const segment of iccSegments) { output.set(segment, offset); offset += segment.byteLength; }
  output.set(jpeg.slice(2), offset);
  return output;
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
    processingTimeMs: performance.now() - started, alreadyOptimized: true, optimizerVersion: 2,
    warnings: ["Brak oszczędności w tym przebiegu."], analysis,
  };
}
