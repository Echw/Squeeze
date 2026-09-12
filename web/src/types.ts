export const WORKER_API_VERSION = 2 as const;

export type CompressionProfile =
  | "maximumQuality"
  | "balanced"
  | "maximumCompression"
  | "lossless";

export type SearchEffort = "auto" | "detailed";

/** Smart selects one compatible path. Search compares compatible PNG paths before choosing a winner. */
export type CompressionMethod = "auto" | "search" | "lossless" | "palette";

/** Output codecs are explicit so a future encoder can never silently change a file format. */
export type OutputFormat = "preserve" | "webp" | "avif";

export type ProgressStage =
  | "decoding"
  | "analyzing"
  | "searching"
  | "measuring"
  | "finalizing";

export interface OptimizationOptions {
  profile: CompressionProfile;
  searchEffort: SearchEffort;
  method: CompressionMethod;
  outputFormat: OutputFormat;
  metadata: "stripPrivate" | "preserveAll";
  limits: {
    maxInputBytes: number;
    maxPixels: number;
    maxWorkingBytes: number;
  };
}

export interface OptimizationReport {
  format: "jpeg" | "png";
  outputFormat: "jpeg" | "png" | "webp" | "avif";
  width: number;
  height: number;
  originalSize: number;
  optimizedSize: number;
  savedBytes: number;
  savedPercent: number;
  metrics: {
    ssimulacra2: number | null;
    butteraugli: number | null;
  };
  strategy: {
    encoder: string;
    quality: number | null;
    chromaSubsampling: string | null;
    progressive: boolean | null;
    paletteColors: number | null;
    dithering: string | null;
    lossless: boolean;
  };
  candidatesTested: number;
  processingTimeMs: number;
  alreadyOptimized: boolean;
  profileSetVersion: number;
  warnings: string[];
  analysis: {
    kind: "photo" | "graphic" | "screenshot" | "mixed";
    entropy: number;
    estimatedColors: number;
    edgeDensity: number;
    noise: number;
    flatAreaRatio: number;
    hasAlpha: boolean;
  };
}

export type WorkerRequest =
  | {
      version: 2;
      type: "compress";
      jobId: string;
      attempt: number;
      buffer: ArrayBuffer;
      options: OptimizationOptions;
    }
  | { version: 2; type: "cancel"; jobId: string; attempt: number };

export type WorkerResponse =
  | {
      version: 2;
      type: "progress";
      jobId: string;
      attempt: number;
      stage: ProgressStage;
      candidate?: number;
      total?: number;
    }
  | {
      version: 2;
      type: "complete";
      jobId: string;
      attempt: number;
      result: OptimizationReport;
      buffer: ArrayBuffer;
    }
  | {
      version: 2;
      type: "error";
      jobId: string;
      attempt: number;
      code: string;
      message: string;
      recoverable: boolean;
    }
  | { version: 2; type: "cancelled"; jobId: string; attempt: number };

export type JobStatus = "queued" | "processing" | "complete" | "error" | "cancelled";

export interface CompressionJob {
  id: string;
  file: File;
  profile: CompressionProfile;
  searchEffort: SearchEffort;
  method: CompressionMethod;
  outputFormat: OutputFormat;
  /** Increments for every run, preventing a late worker response from changing a retry. */
  attempt: number;
  sourceJobId?: string;
  status: JobStatus;
  stage?: ProgressStage;
  candidate?: number;
  total?: number;
  report?: OptimizationReport;
  output?: Uint8Array;
  error?: string;
}
