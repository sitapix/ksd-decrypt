import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
const url = pathToFileURL(resolve('dist/index.html')).href;

async function mockNative(page, options = {}) {
  await page.addInitScript(
    ({
      slow = false,
      count,
      cancelOutput = false,
      failOnce = false,
      multipleFolders = false,
      cancelError = false,
      closeError = false,
      listenError = false,
    }) => {
      window.isTauri = true;
      window.nativeCalls = [];
      window.outputPath = cancelOutput ? null : '/Recovered files';
      const callbacks = {},
        events = {};
      let next = 1,
        stopped = false,
        destination = null;
      let files = [
        {
          id: 'one',
          name: '0000000000000.Holiday.jpg.ksd',
          path: '/backup/Holiday.jpg.ksd',
          bytes: 3248092,
          format: 'JPG',
          kind: 'image',
          issue: null,
        },
        {
          id: 'two',
          name: 'Birthday.mp4.ksd',
          path: '/backup/Birthday.mp4.ksd',
          bytes: 5819048821,
          format: 'MP4',
          kind: 'video',
          issue: null,
        },
        {
          id: 'three',
          name: 'different-vault.ksd',
          path: '/backup/different-vault.ksd',
          bytes: 23001,
          format: 'Unknown',
          kind: 'unknown',
          issue: 'Unsupported format.',
        },
      ];
      if (count !== undefined)
        files = Array.from({ length: count }, (_, i) => ({
          ...files[0],
          id: `file-${i}`,
          name: `Photo ${i + 1}.jpg.ksd`,
        }));
      if (multipleFolders) files[1].path = '/other/Birthday.mp4.ksd';
      window.inputFiles = files;
      const emit = (event, payload) => {
        for (const id of events[event] || []) callbacks[id]({ event, payload });
      };
      window.emitNative = emit;
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
        transformCallback(callback) {
          const id = next++;
          callbacks[id] = callback;
          return id;
        },
        unregisterCallback(id) {
          delete callbacks[id];
        },
        invoke: async (command, args) => {
          args = JSON.parse(JSON.stringify(args || {}));
          window.nativeCalls.push({ command, args });
          if (command === 'plugin:event|listen') {
            if (listenError) throw new Error('Could not listen');
            (events[args.event] ||= []).push(args.handler);
            return next++;
          }
          if (command === 'plugin:event|unlisten') {
            events[args.event] = (events[args.event] || []).filter((id) => id !== args.eventId);
            return;
          }
          if (command === 'plugin:window|close') {
            if (closeError) throw new Error('Could not close');
            return;
          }
          if (command === 'choose_inputs' || command === 'add_paths')
            return {
              files: structuredClone(window.inputFiles),
              warnings: [],
              message: window.selectionMessage || null,
            };
          if (command === 'choose_output') {
            if (window.outputPath) destination = window.outputPath;
            return window.outputPath;
          }
          if (command === 'clear_selection') {
            destination = null;
            return;
          }
          if (command === 'open_result') {
            if (args.reveal && window.revealError) throw new Error('Could not show file');
            return;
          }
          if (command === 'cancel_recovery') {
            if (cancelError) throw new Error('Could not stop');
            stopped = true;
            return;
          }
          if (command === 'recover') {
            stopped = false;
            const results = [];
            const channelCallback = callbacks[Number(args.onEvent.replace('__CHANNEL__:', ''))];
            let streamIndex = 0;
            const send = (event, data) =>
              channelCallback({ index: streamIndex++, message: { event, data } });
            window.deliverLateRecoveryEvent = send;
            for (const [index, file] of window.inputFiles
              .filter((f) => args.ids.includes(f.id))
              .entries()) {
              send('progress', {
                id: file.id,
                name: file.name,
                stage: 'recovering',
                completed: index,
                total: args.ids.length,
                processedBytes: index * 3200000,
                totalBytes: 5822296913,
              });
              await new Promise((r) => setTimeout(r, slow ? 1000 : 80));
              if (stopped) break;
              const failed = failOnce && file.id === 'two';
              const folder = destination || file.path.slice(0, file.path.lastIndexOf('/'));
              const outputName =
                window.recoveryOverrides?.[file.id]?.name ||
                file.name.replace(/^\d{13}\./, '').replace(/\.ksd$/i, '');
              const result = {
                ...file,
                name: failed ? file.name : outputName,
                bytes: failed ? 0 : Math.max(0, file.bytes - 3722),
                status: failed ? 'failed' : 'recovered',
                output: failed ? null : folder + '/' + outputName,
                detail: failed ? 'The destination is full.' : 'Media structure checked.',
              };
              results.push(result);
              send('file', result);
            }
            channelCallback({ index: streamIndex, end: true });
            failOnce = false;
            const folders = new Set(
              results
                .filter((r) => r.output)
                .map((r) => r.output.slice(0, r.output.lastIndexOf('/'))),
            );
            return {
              files: results,
              folder: folders.size === 1 ? [...folders][0] : null,
              cancelled: stopped,
              reportWarning: null,
            };
          }
          return null;
        },
      };
    },
    options,
  );
  await page.goto(url);
  // Chrome has no AppKit material beneath its canvas. Supply its neutral base for contrast checks.
  await page.addStyleTag({ content: ':root { background: var(--paper) !important; }' });
}

async function openMore(page) {
  await page.locator('#more-trigger').click();
}

async function audit(page) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21aa'])
    .analyze();
  expect(results.violations).toEqual([]);
}

test('adding files immediately recovers all supported files without a destination prompt', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await mockNative(page);
  await page.locator('#choose-files').click();
  await expect(page.locator('#file-count')).toHaveText('3 files');
  await expect(page.getByText('5.4 GB', { exact: true })).toBeVisible();
  await expect(page.getByText('Unsupported format.', { exact: true })).toBeVisible();
  await expect(page.locator('#completion-title')).toHaveText('2 files recovered.');
  await expect(page.getByRole('img', { name: 'Recovered', exact: true })).toHaveCount(2);
  await expect(page.getByRole('button', { name: 'Show recovered files' })).toBeVisible();
  await expect(page.locator('#open-folder')).toHaveAttribute('title', 'Show files in /backup');
  await audit(page);
  expect(
    await page.evaluate(() =>
      window.nativeCalls.filter((c) => ['choose_output', 'recover'].includes(c.command)),
    ),
  ).toEqual([
    {
      command: 'recover',
      args: { ids: ['one', 'two'], onEvent: expect.stringMatching(/^__CHANNEL__:/) },
    },
  ]);
  await page.getByRole('button', { name: 'Open Holiday.jpg', exact: true }).click();
  expect(await page.evaluate(() => window.nativeCalls.at(-1))).toEqual({
    command: 'open_result',
    args: { id: 'one' },
  });
  await page.locator('#open-folder').click();
  expect(await page.evaluate(() => window.nativeCalls.at(-1))).toEqual({
    command: 'open_result',
    args: { id: null },
  });
  await openMore(page);
  await page.locator('#clear-button').click();
  await expect(page.locator('#drop-zone')).toBeVisible();
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('2 files recovered.');
  expect(
    await page.evaluate(() => window.nativeCalls.filter((c) => c.command === 'choose_output')),
  ).toHaveLength(0);
  expect(errors).toEqual([]);
});

test('completed rows show the actual output name and size and remain searchable by either name', async ({
  page,
}) => {
  await mockNative(page, { count: 21, slow: true });
  await page.evaluate(() => {
    Object.assign(window.inputFiles[0], {
      name: '0000000000000.Example-photo.jpg.ksd',
      path: '/backup/0000000000000.Example-photo.jpg.ksd',
      bytes: 8192,
    });
    for (const file of window.inputFiles.slice(1)) file.issue = 'Unsupported format.';
    window.recoveryOverrides = { 'file-0': { name: 'Example-photo (2).jpg' } };
  });
  await page.locator('#choose-files').click();
  const row = page.locator('[data-id="file-0"]');
  await expect(row.locator('.file-name')).toHaveText('0000000000000.Example-photo.jpg.ksd');
  await expect(row.locator('.file-size')).toHaveText('8 KB');
  await expect(page.locator('#completion-title')).toHaveText('1 file recovered.');
  await expect(row.locator('.file-name')).toHaveText('Example-photo (2).jpg');
  await expect(row.locator('.file-name')).toHaveAttribute(
    'title',
    'Open Example-photo (2).jpg\nOriginal: 0000000000000.Example-photo.jpg.ksd',
  );
  await expect(row.locator('.file-size')).toHaveText('4 KB');
  await row.getByRole('button', { name: 'Open Example-photo (2).jpg', exact: true }).click();
  expect(await page.evaluate(() => window.nativeCalls.at(-1))).toEqual({
    command: 'open_result',
    args: { id: 'file-0' },
  });
  await row
    .getByRole('button', { name: 'Show Example-photo (2).jpg in folder', exact: true })
    .click();
  expect(await page.evaluate(() => window.nativeCalls.at(-1))).toEqual({
    command: 'open_result',
    args: { id: 'file-0', reveal: true },
  });
  for (const query of ['0000000000000', 'Example-photo (2).JPG']) {
    await page.locator('#search').fill(query);
    await expect(page.locator('.file-row')).toHaveCount(1);
    await expect(row.locator('.file-name')).toHaveText('Example-photo (2).jpg');
  }
});

test('row folder buttons reveal individual recovered files by mouse and keyboard', async ({
  page,
}) => {
  await mockNative(page, { multipleFolders: true });
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('2 files recovered.');
  await expect(page.locator('.file-reveal')).toHaveCount(2);
  await expect(page.locator('[data-id="three"] .file-reveal')).toHaveCount(0);
  const first = page.locator('[data-id="one"]');
  const second = page.locator('[data-id="two"]');
  await expect(first.locator('.file-reveal')).toHaveAttribute('title', 'Show in folder');
  await second.getByRole('button', { name: /Show .* in folder/ }).click();
  expect(await page.evaluate(() => window.nativeCalls.at(-1))).toEqual({
    command: 'open_result',
    args: { id: 'two', reveal: true },
  });
  await first.locator('.file-name').focus();
  await page.keyboard.press('Tab');
  await expect(first.locator('.file-reveal')).toBeFocused();
  await page.keyboard.press('Enter');
  expect(await page.evaluate(() => window.nativeCalls.at(-1))).toEqual({
    command: 'open_result',
    args: { id: 'one', reveal: true },
  });
  await page.evaluate(() => {
    window.revealError = true;
  });
  await first.locator('.file-reveal').click();
  await expect(page.locator('#notice')).toContainText('Could not show file');
  await expect(page.locator('.file-status.recovered')).toHaveCount(2);
});

test('cancelling optional output selection keeps automatic sibling recovery', async ({ page }) => {
  await mockNative(page, { count: 1, cancelOutput: true });
  await openMore(page);
  await page.locator('#change-destination').click();
  expect(await page.evaluate(() => window.nativeCalls.some((c) => c.command === 'recover'))).toBe(
    false,
  );
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('1 file recovered.');
  await expect(page.locator('#search-area')).toBeHidden();
  await expect(page.getByRole('checkbox')).toHaveCount(0);
  await expect(page.locator('#recovery-actions')).toBeHidden();
  await expect(page.locator('#open-folder')).toHaveAttribute('title', 'Show files in /backup');
  expect(
    await page.evaluate(() => window.nativeCalls.filter((c) => c.command === 'choose_output')),
  ).toHaveLength(1);
});

test('an optional output folder is used until starting over', async ({ page }) => {
  await mockNative(page, { count: 1 });
  await openMore(page);
  await page.locator('#change-destination').click();
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('1 file recovered.');
  await expect(page.locator('#open-folder')).toHaveAttribute(
    'title',
    'Show files in /Recovered files',
  );
  await openMore(page);
  await page.locator('#clear-button').click();
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('1 file recovered.');
  await expect(page.locator('#open-folder')).toHaveAttribute('title', 'Show files in /backup');
  expect(
    await page.evaluate(() => window.nativeCalls.filter((c) => c.command === 'choose_output')),
  ).toHaveLength(1);
});

test('choosing another output folder retries failed writes without rerunning successes', async ({
  page,
}) => {
  await mockNative(page, { failOnce: true, cancelOutput: true });
  await page.locator('#choose-files').click();
  await expect(page.locator('#recover-button')).toHaveText('Retry 1 file');
  await openMore(page);
  await page.locator('#change-destination').click();
  await expect(page.locator('#recover-button')).toHaveText('Retry 1 file');
  expect(
    await page.evaluate(() => window.nativeCalls.filter((c) => c.command === 'recover')),
  ).toHaveLength(1);
  await page.evaluate(() => {
    window.outputPath = '/Recovered files';
  });
  await openMore(page);
  await page.locator('#change-destination').click();
  await expect(page.locator('.file-status.recovered')).toHaveCount(2);
  await expect(page.locator('#open-folder')).toHaveAttribute(
    'title',
    'Show files in /Recovered files',
  );
  expect(
    await page.evaluate(() =>
      window.nativeCalls.filter((c) => c.command === 'recover').map((c) => c.args.ids),
    ),
  ).toEqual([['one', 'two'], ['two']]);
});

test('results from multiple source folders can be revealed together', async ({ page }) => {
  await mockNative(page, { multipleFolders: true });
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('2 files recovered.');
  await expect(page.locator('#open-folder')).toHaveAttribute('title', 'Show recovered files');
  await page.locator('#open-folder').click();
  expect(await page.evaluate(() => window.nativeCalls.at(-1))).toEqual({
    command: 'open_result',
    args: { id: null },
  });
});

test('stop recovery retains unfinished files for another attempt', async ({ page }) => {
  await mockNative(page, { slow: true });
  await page.locator('#choose-files').click();
  await expect(page.locator('#progress-area')).toBeVisible();
  await page.locator('#stop-button').click();
  await expect(page.locator('#completion-title')).toContainText('Stopped.');
  await expect(page.locator('#recover-button')).toHaveText('Continue');
  await expect(page.locator('#recover-button')).toBeEnabled();
});

test('retry only recovers failures and leaves completed files alone', async ({ page }) => {
  await mockNative(page, { failOnce: true });
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('1 file recovered.');
  await expect(page.getByText('The destination is full.', { exact: true })).toBeVisible();
  await expect(page.locator('#recover-button')).toHaveText('Retry 1 file');
  await page.locator('#recover-button').click();
  await expect(page.locator('.file-status.recovered')).toHaveCount(2);
  expect(
    await page.evaluate(
      () => window.nativeCalls.filter((c) => c.command === 'recover').at(-1).args.ids,
    ),
  ).toEqual(['two']);
});

test('initial, selected, and help screens are keyboard accessible', async ({ page }) => {
  await mockNative(page);
  await audit(page);
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('2 files recovered.');
  await audit(page);
  await page.locator('#more-trigger').focus();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Tab');
  await expect(page.locator('#add-files')).toBeFocused();
  await page.locator('#help-button').click();
  await expect(page.locator('#help-dialog')).toBeVisible();
  await audit(page);
  await page.keyboard.press('Escape');
  await expect(page.locator('#help-dialog')).toBeHidden();
  await expect(page.locator('#more-trigger')).toBeFocused();
});

test('compact and narrow windows keep actions inside the viewport', async ({ page }) => {
  await mockNative(page, { failOnce: true });
  for (const viewport of [
    { width: 600, height: 600 },
    { width: 480, height: 360 },
    { width: 390, height: 844 },
  ]) {
    await page.setViewportSize(viewport);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  }
  await page.locator('#choose-files').click();
  await expect(page.locator('#recover-button')).toHaveText('Retry 1 file');
  for (const viewport of [
    { width: 600, height: 600 },
    { width: 480, height: 360 },
    { width: 390, height: 844 },
  ]) {
    await page.setViewportSize(viewport);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
    await expect(page.locator('#recover-button')).toBeInViewport();
    await expect(page.locator('.file-reveal')).toBeInViewport();
  }
});

test('native drag feedback clears on leave and accepts dropped files', async ({ page }) => {
  await mockNative(page);
  await page.evaluate(() =>
    window.emitNative('tauri://drag-enter', {
      paths: ['/backup/Holiday.jpg.ksd'],
      position: { x: 100, y: 100 },
    }),
  );
  await expect(page.locator('#drop-title')).toHaveText('Release to add files');
  await page.evaluate(() => window.emitNative('tauri://drag-leave', {}));
  await expect(page.locator('#drop-title')).toHaveText('Drop .ksd files or a folder');
  await page.evaluate(() =>
    window.emitNative('tauri://drag-drop', {
      paths: ['/backup/Holiday.jpg.ksd'],
      position: { x: 100, y: 100 },
    }),
  );
  await expect(page.locator('#file-count')).toHaveText('3 files');
  await expect(page.locator('#completion-title')).toHaveText('2 files recovered.');
  await openMore(page);
  await page.locator('#choose-folder').click();
  expect(
    await page.evaluate(
      () => window.nativeCalls.filter((c) => c.command === 'choose_inputs').at(-1).args,
    ),
  ).toEqual({ folder: true });
});

test('dialogs can be dismissed during opening and return keyboard focus', async ({ page }) => {
  await mockNative(page);
  await openMore(page);
  await page.locator('#help-button').click();
  await page.keyboard.press('Escape');
  await expect(page.locator('#help-dialog')).toBeHidden();
  await expect(page.locator('#more-trigger')).toBeFocused();
  await openMore(page);
  await page.locator('#help-button').click();
  await expect(page.locator('#help-dialog')).toBeVisible();
  await page.getByRole('button', { name: 'Close help', exact: true }).click();
  await expect(page.locator('#help-dialog')).toBeHidden();
  await expect(page.locator('#more-trigger')).toBeFocused();
});

test('reduced motion keeps dialogs immediate and dark mode remains accessible', async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: 'dark' });
  await mockNative(page);
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('2 files recovered.');
  await audit(page);
  await openMore(page);
  await page.locator('#help-button').click();
  await expect(page.locator('#help-dialog')).toHaveCSS('transform', 'none');
  await audit(page);
  await page.keyboard.press('Escape');
  await expect(page.locator('#help-dialog')).toBeHidden();
  await expect(page.locator('#more-trigger')).toBeFocused();
});

test('large batches expose filename search and paging without changing what gets recovered', async ({
  page,
}) => {
  await mockNative(page, { count: 55, slow: true });
  await page.locator('#choose-files').click();
  await page.locator('#stop-button').click();
  await expect(page.locator('#completion-title')).toContainText('Stopped.');
  await expect(page.locator('.file-row')).toHaveCount(50);
  await page.locator('#next-page').click();
  await expect(page.locator('.file-row')).toHaveCount(5);
  await page.locator('#search').fill('Photo 55.');
  await expect(page.locator('.file-row')).toHaveCount(1);
  await expect(page.locator('#recover-button')).toHaveText('Continue');
  await page.locator('#search').fill('missing');
  await expect(page.getByText('No matching files.', { exact: true })).toBeVisible();
  await page.locator('#recover-button').click();
  await expect(page.locator('#progress-area')).toBeVisible();
  expect(
    await page.evaluate(
      () => window.nativeCalls.filter((c) => c.command === 'recover').at(-1).args.ids,
    ),
  ).toHaveLength(55);
  await page.locator('#stop-button').click();
  await expect(page.locator('#completion-title')).toContainText('Stopped.');
});

test('later drops decrypt immediately without prompts and duplicates do nothing', async ({
  page,
}) => {
  await mockNative(page, { count: 1 });
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('1 file recovered.');
  await page.evaluate(() => {
    window.inputFiles.push({ ...window.inputFiles[0], id: 'added', name: 'New photo.jpg.ksd' });
    window.emitNative('tauri://drag-drop', {
      paths: ['/backup/New photo.jpg.ksd'],
      position: { x: 100, y: 100 },
    });
  });
  await expect(page.locator('.file-status.recovered')).toHaveCount(2);
  expect(
    await page.evaluate(() => window.nativeCalls.filter((c) => c.command === 'choose_output')),
  ).toHaveLength(0);
  expect(
    await page.evaluate(() =>
      window.nativeCalls.filter((c) => c.command === 'recover').map((c) => c.args.ids),
    ),
  ).toEqual([['file-0'], ['added']]);
  await openMore(page);
  await page.locator('#add-files').click();
  await expect(page.locator('#workspace')).toHaveAttribute('aria-busy', 'false');
  expect(
    await page.evaluate(() => window.nativeCalls.filter((c) => c.command === 'recover')),
  ).toHaveLength(2);
});

test('unrelated input shows feedback without moving the centered chooser', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await mockNative(page, { count: 0 });
  for (const viewport of [
    { width: 600, height: 600 },
    { width: 480, height: 360 },
  ]) {
    await page.setViewportSize(viewport);
    const title = await page.locator('#drop-title').boundingBox();
    const button = await page.locator('#choose-files').boundingBox();
    await page.evaluate(() => {
      window.selectionMessage = 'No .ksd files found.';
    });
    await page.locator('#choose-files').click();
    await expect(page.locator('#drop-zone')).toBeVisible();
    await expect(page.locator('#file-area')).toBeHidden();
    await expect(page.locator('#notice')).toBeHidden();
    await expect(page.locator('#drop-notice')).toHaveText('No .ksd files found.');
    await expect(page.locator('#drop-notice')).toHaveAttribute('data-tone', 'quiet');
    await expect(page.locator('#drop-notice')).toBeInViewport();
    expect(await page.locator('#drop-title').boundingBox()).toEqual(title);
    expect(await page.locator('#choose-files').boundingBox()).toEqual(button);
    const feedback = await page.locator('#drop-notice').boundingBox();
    expect(feedback.y).toBeGreaterThan(button.y + button.height);
    await page.evaluate(() => {
      window.selectionMessage = null;
    });
    await page.locator('#choose-files').click();
    await expect(page.locator('#drop-notice')).toBeHidden();
    expect(await page.locator('#choose-files').boundingBox()).toEqual(button);
  }
  expect(await page.evaluate(() => window.nativeCalls.some((c) => c.command === 'recover'))).toBe(
    false,
  );
  await page.evaluate(() => {
    window.selectionMessage = 'No .ksd files found.';
  });
  await page.locator('#choose-files').click();
  await audit(page);
});

test('unsupported input never asks for an output folder or starts recovery', async ({ page }) => {
  await mockNative(page);
  await page.evaluate(() => {
    window.inputFiles = window.inputFiles.filter((file) => file.issue);
  });
  await page.locator('#choose-files').click();
  await expect(page.getByText('Unsupported format.', { exact: true })).toBeVisible();
  await expect(page.locator('#recovery-actions')).toBeHidden();
  await expect(page.locator('.file-issue.unsupported')).toHaveText('Unsupported format.');
  expect(
    await page.evaluate(() =>
      window.nativeCalls.filter((c) => ['choose_output', 'recover'].includes(c.command)),
    ),
  ).toEqual([]);
});

test('failed cancellation keeps the app open and allows recovery to finish', async ({ page }) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await mockNative(page, { slow: true, cancelError: true });
  await page.locator('#choose-files').click();
  await page.evaluate(() => window.emitNative('recovery-close-requested'));
  await expect(page.locator('#close-dialog')).toBeVisible();
  await page.locator('#stop-close').click();
  await expect(page.locator('#notice')).toContainText('Could not stop');
  await expect(page.locator('#stop-close')).toBeEnabled();
  await expect(page.locator('#completion-title')).toHaveText('2 files recovered.');
  await expect(page.locator('#close-dialog')).toBeHidden();
  expect(
    await page.evaluate(() =>
      window.nativeCalls.some((call) => call.command === 'plugin:window|close'),
    ),
  ).toBe(false);
  expect(errors).toEqual([]);
});

test('stop and close waits for recovery to finish cleaning up', async ({ page }) => {
  await mockNative(page, { slow: true });
  await page.locator('#choose-files').click();
  await page.evaluate(() => window.emitNative('recovery-close-requested'));
  await page.locator('#stop-close').click();
  await expect(page.locator('#completion-title')).toContainText('Stopped.');
  await expect
    .poll(() => page.evaluate(() => window.nativeCalls.at(-1)?.command))
    .toBe('plugin:window|close');
});

test('native close failures are shown without an unhandled rejection', async ({ page }) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await mockNative(page, { slow: true, closeError: true });
  await page.locator('#choose-files').click();
  await page.evaluate(() => window.emitNative('recovery-close-requested'));
  await page.locator('#stop-close').click();
  await expect(page.locator('#notice')).toContainText('Could not close');
  await expect(page.locator('#recover-button')).toBeEnabled();
  expect(errors).toEqual([]);
});

test('late channel messages cannot change a settled recovery', async ({ page }) => {
  await mockNative(page, { count: 1 });
  await page.locator('#choose-files').click();
  await expect(page.locator('#completion-title')).toHaveText('1 file recovered.');
  await page.evaluate(() =>
    window.deliverLateRecoveryEvent('file', {
      ...window.inputFiles[0],
      status: 'failed',
      output: null,
      detail: 'Stale result',
    }),
  );
  await expect(page.locator('.file-status.recovered')).toHaveCount(1);
  await expect(page.getByText('Stale result')).toHaveCount(0);
});

test('initialization failure keeps native actions disabled', async ({ page }) => {
  await mockNative(page, { listenError: true });
  await expect(page.locator('#drop-notice')).toContainText('could not initialize');
  await expect(page.locator('#choose-files')).toBeDisabled();
  expect(
    await page.evaluate(() => window.nativeCalls.some((call) => call.command === 'recover')),
  ).toBe(false);
});

test('the main chooser offers files and folders through one input command', async ({ page }) => {
  await mockNative(page, { count: 1 });
  await page.getByRole('button', { name: 'Choose files or folders…', exact: true }).click();
  await expect(page.locator('#completion-title')).toHaveText('1 file recovered.');
  expect(
    await page.evaluate(() => window.nativeCalls.find((call) => call.command === 'choose_inputs')),
  ).toEqual({ command: 'choose_inputs', args: { folder: false } });
  await openMore(page);
  await expect(
    page.getByRole('button', { name: 'Add files or folders', exact: true }),
  ).toBeVisible();
});
