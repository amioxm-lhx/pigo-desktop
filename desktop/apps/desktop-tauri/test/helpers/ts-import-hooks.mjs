/**
 * Module hooks that let `node --test` import main-process TypeScript directly.
 *
 * Node strips the types itself, but the sources use bundler-style extensionless
 * relative imports (`./plugin-mcp`), which ESM resolution rejects. Retrying the
 * specifier with a `.ts` extension is enough to load them without a build step.
 *
 * Register with:
 *   register(pathToFileURL(join(here, "helpers/ts-import-hooks.mjs")));
 */

/**
 * A stand-in for `electron` under plain Node.
 *
 * Outside the app the package resolves to a path string, so any main-process
 * module that imports it -- even only to reach the browser it drives -- would
 * fail to load and take its own tests down with it. The stub keeps that tree
 * importable; anything that needs a real window has to run inside Electron.
 */
const ELECTRON_STUB =
  "data:text/javascript," +
  encodeURIComponent(
    [
      'export class BrowserWindow { constructor() { throw new Error("BrowserWindow is only available inside Electron"); } }',
      'export const session = { fromPartition: () => ({ cookies: { get: async () => [] }, clearStorageData: async () => {}, setUserAgent: () => {}, getUserAgent: () => "" }) };',
      'export const app = { getPath: () => "", whenReady: async () => {} };',
    ].join("\n"),
  );

export async function resolve(specifier, context, next) {
  if (specifier === "electron" && !process.versions.electron) {
    return { url: ELECTRON_STUB, shortCircuit: true, format: "module" };
  }
  if (specifier.startsWith(".") && !/\.[a-z]+$/i.test(specifier)) {
    try {
      return await next(`${specifier}.ts`, context);
    } catch {
      // Not a TypeScript sibling; fall through to the default resolution.
    }
  }
  return next(specifier, context);
}
