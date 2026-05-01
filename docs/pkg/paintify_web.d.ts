/* tslint:disable */
/* eslint-disable */

/**
 * Global initialization — call once from JS before using paintify.
 */
export function init(): void;

export function paintify_default_js(input_data: Uint8Array): Uint8Array;

/**
 * The main entry point. Takes raw image bytes (PNG, JPEG, WebP), applies
 * the Paintify pipeline, and returns a PNG byte array.
 *
 * # Arguments
 * - `input_data`: Raw bytes of the input image (supports PNG, JPEG, WebP).
 * - `pixel_size`: How chunky the pixels get. 4 = default. Higher = chunkier.
 * - `extended_palette`: If `true`, uses 28-color extended palette instead of classic 16.
 *
 * # Returns
 * Raw bytes of a PNG image, or an empty Vec on error.
 */
export function paintify_js(input_data: Uint8Array, pixel_size: number, extended_palette: boolean, kuwahara_radius: number, edge_overlay: boolean): Uint8Array;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly paintify_default_js: (a: number, b: number) => [number, number];
    readonly paintify_js: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly init: () => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
