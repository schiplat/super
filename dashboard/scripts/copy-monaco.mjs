// Copy monaco-editor's `min/vs` bundle into public/monaco/vs so the Stack
// Editor loads it same-origin instead of from a CDN at runtime. superd's CSP
// restricts scripts to 'self' (and the dashboard must work offline / in
// air-gapped installs), so a CDN fallback can never load. Run before `vite
// build` (see package.json scripts.build); output is gitignored.
import { cpSync, existsSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const src = join(root, 'node_modules', 'monaco-editor', 'min', 'vs');
const dst = join(root, 'public', 'monaco', 'vs');

if (!existsSync(src)) {
  console.error(
    '[copy-monaco] node_modules/monaco-editor/min/vs not found — run `npm install` first.',
  );
  process.exit(1);
}
mkdirSync(dst, { recursive: true });
cpSync(src, dst, { recursive: true });
console.log('[copy-monaco] monaco assets copied to public/monaco/vs');
