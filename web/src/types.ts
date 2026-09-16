export const WORKER_API_VERSION = 5 as const;

export type ProgressStage = "decoding" | "analyzing" | "compressing" | "finalizing";

export interface OptimizationReport {
  format: "jpeg" | "png";
  outputFormat: "jpeg" | "png";
  width: number;
  height: number;
  originalSize: number;
  optimizedSize: number;
  savedBytes: number;
  savedPercent: number;
  strategy: {
    encoder: string;
    quality: number | null;
    chromaSubsampling: string | null;
    progressive: boolean | null;
    paletteColors: number | null;
    lossless: boolean;
  };
  processingTimeMs: number;
  alreadyOptimized: boolean;
  optimizerVersion: number;
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
  | { version: 5; type: "compress"; jobId: string; attempt: number; buffer: ArrayBuffer }
  | { version: 5; type: "cancel"; jobId: string; attempt: number };

export interface WorkerCapabilities {
  preserve: boolean;
  maxPixels: number;
}

export type WorkerResponse =
  | { version: 5; type: "ready"; capabilities: WorkerCapabilities }
  | { version: 5; type: "progress"; jobId: string; attempt: number; stage: ProgressStage }
  | { version: 5; type: "complete"; jobId: string; attempt: number; result: OptimizationReport; buffer: ArrayBuffer }
  | { version: 5; type: "error"; jobId: string; attempt: number; code: string; message: string; recoverable: boolean }
  | { version: 5; type: "cancelled"; jobId: string; attempt: number };

export type JobStatus = "queued" | "processing" | "complete" | "error" | "cancelled";

export interface CompressionJob {
  id: string;
  file: File;
  attempt: number;
  status: JobStatus;
  stage?: ProgressStage;
  report?: OptimizationReport;
  output?: Uint8Array;
  error?: string;
  errorCode?: string;
  recoverable?: boolean;
}
