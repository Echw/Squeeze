export const WORKER_API_VERSION = 1 as const;

export type CompressionProfile =
  | "maximumQuality"
  | "balanced"
  | "maximumCompression"
  | "lossless";

export type ProgressStage =
  | "decoding"
  | "analyzing"
  | "searching"
  | "measuring"
  | "finalizing";

export interface OptimizationOptions {
  profile: CompressionProfile;
  outputFormat: "preserve";
  metadata: "stripPrivate" | "preserveAll";
  limits: {
    maxInputBytes: number;
    maxPixels: number;
    maxWorkingBytes: number;
  };
}

export interface OptimizationReport {
  format: "jpeg" | "png";
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
      version: 1;
      type: "compress";
      jobId: string;
      buffer: ArrayBuffer;
      options: OptimizationOptions;
    }
  | { version: 1; type: "cancel"; jobId: string };

export type WorkerResponse =
  | {
      version: 1;
      type: "progress";
      jobId: string;
      stage: ProgressStage;
      candidate?: number;
      total?: number;
    }
  | {
      version: 1;
      type: "complete";
      jobId: string;
      result: OptimizationReport;
      buffer: ArrayBuffer;
    }
  | {
      version: 1;
      type: "error";
      jobId: string;
      code: string;
      message: string;
      recoverable: boolean;
    }
  | { version: 1; type: "cancelled"; jobId: string };

export type JobStatus = "queued" | "processing" | "complete" | "error" | "cancelled";

export interface CompressionJob {
  id: string;
  file: File;
  profile: CompressionProfile;
  status: JobStatus;
  stage?: ProgressStage;
  candidate?: number;
  total?: number;
  report?: OptimizationReport;
  output?: Uint8Array;
  error?: string;
}

