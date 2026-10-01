# web/ — player and source editor (Phase 2)

Vite + TypeScript. The engine runs as WASM in a Web Worker and paints to an `OffscreenCanvas` through WebGPU (`vello`), falling back to `vello_cpu` → `ImageBitmap`. No backend: OPFS + File System Access for storage, single-file HTML export for sharing, BYOK assistant in the worker. See SPEC §9.2 and PLAN §Phase 2. Nothing here until gate 1 is logged.
