import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const cli = 'node_modules/@tauri-apps/cli/tauri.js';
const preview = mkdtempSync(join(tmpdir(), 'ksd-icon-preview-'));
try {
  execFileSync(process.execPath, [cli, 'icon', 'app-icon.svg'], { stdio: 'inherit' });
  execFileSync(
    process.execPath,
    [cli, 'icon', 'app-icon.svg', '--output', preview, '--png', '1024'],
    { stdio: 'inherit' },
  );
  copyFileSync(join(preview, '1024x1024.png'), 'app-icon.png');
} finally {
  rmSync(preview, { recursive: true, force: true });
}
