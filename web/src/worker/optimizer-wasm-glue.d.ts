declare module "virtual:optimizer-wasm-glue" {
  export interface WasmResult {
    report_json: string;
    take_bytes(): Uint8Array;
  }

  export default function init(input?: { module_or_path: URL }): Promise<unknown>;
  export function worker_api_version(): number;
  export function optimize_image(input: Uint8Array, optionsJson: string, progress: (eventJson: string) => void): WasmResult;
  export class JpegTileScorer {
    constructor();
    add_tile(rgba: Uint8Array, width: number, height: number): number;
    score(index: number, jpeg: Uint8Array): number;
    free(): void;
  }
}
