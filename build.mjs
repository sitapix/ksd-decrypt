import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
const root = new URL('./', import.meta.url);
const read = (name) => readFile(new URL(name, root), 'utf8');
const [template, css, lucideLicense] = await Promise.all(
  ['src/index.html', 'src/style.css', 'node_modules/lucide/LICENSE'].map(read),
);
const bundle = await build({
  entryPoints: [fileURLToPath(new URL('src/app.ts', root))],
  bundle: true,
  write: false,
  platform: 'browser',
  format: 'iife',
  target: ['es2022'],
  minify: true,
  legalComments: 'inline',
  banner: { js: '/*! Lucide icons: see lucide-LICENSE.txt for ISC and MIT notices. */' },
});
const js = bundle.outputFiles[0].text;
await mkdir(new URL('dist/', root), { recursive: true });
await Promise.all([
  writeFile(new URL('dist/index.html', root), template),
  writeFile(new URL('dist/style.css', root), css),
  writeFile(new URL('dist/app.js', root), js),
  writeFile(new URL('dist/lucide-LICENSE.txt', root), lucideLicense),
]);
console.log('Built the offline Tauri interface in dist/.');
