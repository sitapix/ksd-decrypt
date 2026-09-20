import {
  api,
  native,
  subscribeNative,
  type InputFile,
  type RecoveredFile,
  type Progress,
  type RecoveryEvent,
} from './native';
import { $ } from './view';
import { reveal, openDialog, closeDialog, installPressFeedback } from './motion';
import { icon, initializeIcons } from './icons';

interface AppState {
  files: InputFile[];
  results: Map<string, RecoveredFile>;
  destination: string | null;
  page: number;
  phase: 'idle' | 'loading' | 'recovering';
  ready: boolean;
  active: string | null;
  closeAfter: boolean;
}

const state: AppState = {
  files: [],
  results: new Map(),
  destination: null,
  page: 0,
  phase: 'idle',
  ready: false,
  active: null,
  closeAfter: false,
};
const locked = () => !state.ready || state.phase !== 'idle';
const PAGE_SIZE = 50;
const mac = /Mac/.test(navigator.platform);
document.documentElement.classList.toggle('platform-mac', native && mac);
const bytes = (size: number) => {
  if (size < 1024) return `${size} B`;
  const unit = Math.min(4, Math.floor(Math.log(size) / Math.log(1024)));
  return `${(size / 1024 ** unit).toFixed(unit === 1 ? 0 : 1)} ${['B', 'KB', 'MB', 'GB', 'TB'][unit]}`;
};
const fileCount = (count: number) => `${count.toLocaleString()} ${count === 1 ? 'file' : 'files'}`;
const pendingFiles = () =>
  state.files.filter((file) => !file.issue && state.results.get(file.id)?.status !== 'recovered');
function element<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string) {
  const el = document.createElement(tag);
  if (className) el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}
function fileStatus(recovered: boolean) {
  const status = element('span', `file-status ${recovered ? 'recovered' : 'recovering'}`);
  status.setAttribute('role', 'img');
  status.setAttribute('aria-label', recovered ? 'Recovered' : 'Recovering');
  status.title = recovered ? 'Recovered' : 'Recovering';
  if (recovered) status.append(icon('Check'));
  return status;
}
function notice(message: string, tone = 'info') {
  const target = state.files.length ? $('notice') : $('drop-notice');
  for (const area of [$('notice'), $('drop-notice')]) {
    area.textContent = area === target ? message : '';
    area.dataset.tone = tone;
    area.hidden = area !== target || !message;
  }
}
function closeMenu(restoreFocus = false) {
  $('more-actions').open = false;
  if (restoreFocus) $('more-trigger').focus();
}
function menuAction(action: () => void | Promise<void>) {
  return () => {
    closeMenu(true);
    return action();
  };
}
function updateControls() {
  const hasFiles = state.files.length > 0,
    disabled = locked();
  const enteringFiles = hasFiles && $('file-area').hidden;
  const pending = pendingFiles();
  document.body.classList.toggle('has-files', hasFiles);
  $('workspace').setAttribute('aria-busy', String(state.phase === 'loading'));
  for (const id of [
    'choose-files',
    'add-files',
    'choose-folder',
    'change-destination',
    'clear-button',
  ] as const)
    $(id).disabled = disabled || !native;
  $('clear-button').hidden = !hasFiles;
  $('change-destination').title = state.destination || 'Save copies beside originals';
  $('drop-zone').hidden = hasFiles;
  $('file-area').hidden = !hasFiles;
  if (enteringFiles) reveal($('file-area'));
  $('file-count').textContent = fileCount(state.files.length);
  $('file-count').hidden = !hasFiles;
  $('search-area').hidden = state.files.length <= 20 && !$('search').value;
  $('progress-area').hidden = state.phase !== 'recovering';
  $('recovery-actions').hidden = !pending.length || state.phase === 'recovering';
  $('recover-button').disabled = disabled || !pending.length || !native;
  const retrying =
    pending.length > 0 && pending.every((file) => state.results.get(file.id)?.status === 'failed');
  $('recover-button').textContent = retrying ? `Retry ${fileCount(pending.length)}` : 'Continue';
  $('recover-button').title = state.destination
    ? `Save recovered copies to ${state.destination}`
    : 'Save copies beside originals';
  $('recover-button').classList.toggle('button-secondary', !$('completion').hidden);
  $('recover-button').classList.toggle('button-primary', $('completion').hidden === true);
}
function render() {
  updateControls();
  const query = $('search').value.toLocaleLowerCase();
  const visible = state.files.filter((file) => {
    const result = state.results.get(file.id);
    return (
      file.name.toLocaleLowerCase().includes(query) ||
      (result?.output != null && result.name.toLocaleLowerCase().includes(query))
    );
  });
  const pages = Math.ceil(visible.length / PAGE_SIZE);
  state.page = Math.max(0, Math.min(state.page, pages - 1));
  const list = $('file-list');
  list.replaceChildren();
  if (!visible.length) list.append(element('li', 'empty-list', 'No matching files.'));
  for (const file of visible.slice(state.page * PAGE_SIZE, (state.page + 1) * PAGE_SIZE)) {
    const result = state.results.get(file.id);
    const row = element('li', 'file-row');
    row.dataset.id = file.id;
    const info = element('div', 'file-info');
    const recovered = result?.output != null;
    const displayName = recovered ? result.name : file.name;
    const name = element(recovered ? 'button' : 'span', 'file-name', displayName);
    name.title = recovered ? `Open ${displayName}\nOriginal: ${file.name}` : file.path;
    if (recovered) {
      name.setAttribute('aria-label', `Open ${displayName}`);
      name.onclick = () => api.open(file.id).catch((error) => notice(String(error), 'error'));
    }
    info.append(name, element('span', 'file-size', bytes(recovered ? result.bytes : file.bytes)));
    const issue = file.issue || (result?.status === 'failed' ? result.detail : null);
    if (issue) info.append(element('p', `file-issue${file.issue ? ' unsupported' : ''}`, issue));
    const actions = element('div', 'file-actions');
    if (state.active === file.id && result?.status !== 'recovered')
      actions.append(fileStatus(false));
    else if (result?.status === 'recovered') actions.append(fileStatus(true));
    if (recovered) {
      const show = element('button', 'button button-quiet button-icon file-reveal');
      show.type = 'button';
      show.title = 'Show in folder';
      show.setAttribute('aria-label', `Show ${result.name} in folder`);
      show.append(icon('FolderOpen'));
      show.onclick = () => api.reveal(file.id).catch((error) => notice(String(error), 'error'));
      actions.append(show);
    }
    row.append(info, actions);
    list.append(row);
  }
  $('pagination').hidden = pages <= 1;
  $('page-label').textContent = `${state.page + 1} / ${pages}`;
  $('page-label').setAttribute('aria-label', `Page ${state.page + 1} of ${pages}`);
  $('previous-page').disabled = state.page === 0;
  $('next-page').disabled = state.page >= pages - 1;
}
async function selectFiles(folder: boolean, paths?: string[]) {
  if (locked()) return;
  let addedSupportedFiles = false;
  state.phase = 'loading';
  render();
  if (paths) notice('Checking files…', 'progress');
  try {
    const response = paths ? await api.addPaths(paths) : await api.chooseInputs(folder);
    const previous = new Set(state.files.map((file) => file.id));
    state.files = response.files;
    addedSupportedFiles = state.files.some((file) => !previous.has(file.id) && !file.issue);
    if (state.files.some((file) => !previous.has(file.id))) {
      $('completion').hidden = true;
      $('open-folder').hidden = true;
    }
    state.page = 0;
    if (response.warnings.length) notice(response.warnings.join(' '), 'warning');
    else notice(response.message || '', 'quiet');
  } catch (error) {
    notice(String(error), 'error');
  } finally {
    state.phase = 'idle';
    render();
  }
  if (addedSupportedFiles)
    await recover(
      pendingFiles()
        .filter((file) => !state.results.has(file.id))
        .map((file) => file.id),
    );
}
async function recover(ids = pendingFiles().map((file) => file.id)) {
  if (locked() || !ids.length) return;
  state.phase = 'recovering';
  try {
    state.active = null;
    for (const id of ids) state.results.delete(id);
    $('completion').hidden = true;
    $('open-folder').hidden = true;
    $('progress').value = 0;
    $('progress-percent').textContent = '0%';
    $('progress-title').textContent = 'Decrypting…';
    $('progress-detail').textContent = '';
    setStopping(false);
    notice('');
    render();
    const batch = await api.recover(ids, onRecoveryEvent);
    for (const result of batch.files) state.results.set(result.id, result);
    const recovered = batch.files.filter((result) => result.status === 'recovered').length;
    const failures =
      batch.files.length - recovered + state.files.filter((file) => file.issue).length;
    $('completion').hidden = false;
    $('completion-title').textContent =
      `${batch.cancelled ? 'Stopped. ' : ''}${fileCount(recovered)} recovered.`;
    $('completion-detail').textContent = [
      batch.cancelled ? 'Stopped' : '',
      failures ? `${failures} failed` : '',
    ]
      .filter(Boolean)
      .join(' · ');
    $('completion-detail').hidden = !$('completion-detail').textContent;
    $('open-folder').title = batch.folder
      ? `Show files in ${batch.folder}`
      : 'Show recovered files';
    $('open-folder').hidden = recovered === 0;
    reveal($('completion'));
    notice(batch.reportWarning || '', 'warning');
  } catch (error) {
    notice(String(error), 'error');
  } finally {
    state.phase = 'idle';
    state.active = null;
    render();
    if (!document.querySelector('dialog[open]')) {
      const next =
        !$('completion').hidden && !$('open-folder').hidden
          ? $('open-folder')
          : !$('recovery-actions').hidden
            ? $('recover-button')
            : $('more-trigger');
      next.focus({ preventScroll: true });
    }
    if (state.closeAfter) await closeWindow();
    else closeDialog($('close-dialog'));
  }
}
function onRecoveryEvent(event: RecoveryEvent) {
  if (state.phase !== 'recovering') return;
  if (event.event === 'progress') onProgress(event.data);
  else {
    state.results.set(event.data.id, event.data);
    render();
  }
}
function setStopping(stopping: boolean) {
  const label = stopping ? 'Stopping' : 'Stop recovery';
  $('stop-button').disabled = stopping;
  $('stop-button').setAttribute('aria-label', label);
  $('stop-button').title = label;
  $('stop-close').disabled = stopping;
}
async function closeWindow() {
  state.closeAfter = false;
  try {
    await api.close();
  } catch (error) {
    notice(String(error), 'error');
  }
}
async function stopRecovery(closeAfter = false) {
  if (state.phase !== 'recovering') {
    if (closeAfter) await closeWindow();
    return;
  }
  setStopping(true);
  try {
    await api.cancel();
    if (closeAfter) {
      // Recovery may finish while the cancellation command is in flight.
      if (state.phase !== 'recovering') await closeWindow();
      else state.closeAfter = true;
      closeDialog($('close-dialog'));
    }
  } catch (error) {
    state.closeAfter = false;
    notice(String(error), 'error');
    setStopping(false);
  }
}
function onProgress(payload: Progress) {
  const previous = state.active;
  state.active = payload.id;
  const ratio = payload.totalBytes
    ? Math.min(100, (payload.processedBytes / payload.totalBytes) * 100)
    : 0;
  $('progress').value = ratio;
  $('progress-percent').textContent = `${Math.floor(ratio)}%`;
  const title = `${payload.stage === 'checking' ? 'Checking' : 'Decrypting'}${payload.total > 1 ? ` ${payload.completed + 1} / ${payload.total}` : '…'}`;
  if ($('progress-title').textContent !== title) $('progress-title').textContent = title;
  $('progress-detail').textContent =
    `${bytes(payload.processedBytes)} / ${bytes(payload.totalBytes)}`;
  if (previous !== state.active) render();
}
async function clearFiles() {
  if (locked()) return;
  state.phase = 'loading';
  updateControls();
  try {
    await api.clear();
    state.files = [];
    state.results.clear();
    state.destination = null;
    state.active = null;
    state.page = 0;
    $('completion').hidden = true;
    $('open-folder').hidden = true;
    $('search').value = '';
    notice('');
  } catch (error) {
    notice(String(error), 'error');
  } finally {
    state.phase = 'idle';
    render();
  }
}
async function changeDestination() {
  if (locked()) return;
  let changed = false;
  state.phase = 'loading';
  updateControls();
  try {
    const destination = await api.chooseOutput();
    if (destination) {
      state.destination = destination;
      changed = true;
    }
  } catch (error) {
    notice(String(error), 'error');
  } finally {
    state.phase = 'idle';
    updateControls();
  }
  if (changed && pendingFiles().length) await recover();
}
for (const dialog of document.querySelectorAll('dialog')) {
  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    closeDialog(dialog);
  });
  dialog.addEventListener('click', (event) => {
    if (event.target !== dialog) return;
    const box = dialog.getBoundingClientRect();
    if (
      event.clientX < box.left ||
      event.clientX > box.right ||
      event.clientY < box.top ||
      event.clientY > box.bottom
    )
      closeDialog(dialog);
  });
}
for (const button of document.querySelectorAll<HTMLButtonElement>('[data-close]')) {
  button.onclick = () => {
    const dialog = document.getElementById(button.dataset.close || '');
    if (dialog instanceof HTMLDialogElement) closeDialog(dialog);
  };
}
document.addEventListener('pointerdown', (event) => {
  if (event.target instanceof Node && !$('more-actions').contains(event.target)) closeMenu();
});
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape' && $('more-actions').open) {
    event.preventDefault();
    closeMenu(true);
  }
});
$('more-actions').addEventListener('focusout', (event) => {
  if (event.relatedTarget instanceof Node && !$('more-actions').contains(event.relatedTarget))
    closeMenu();
});
$('help-button').onclick = menuAction(() => openDialog($('help-dialog')));
$('choose-files').onclick = () => selectFiles(false);
$('add-files').onclick = menuAction(() => selectFiles(false));
$('choose-folder').onclick = menuAction(() => selectFiles(true));
$('change-destination').onclick = menuAction(changeDestination);
$('clear-button').onclick = menuAction(clearFiles);
$('recover-button').onclick = () => recover();
$('open-folder').onclick = () => api.open(null).catch((error) => notice(String(error), 'error'));
$('stop-button').onclick = () => stopRecovery();
$('search').oninput = () => {
  state.page = 0;
  render();
};
$('previous-page').onclick = () => {
  state.page--;
  render();
};
$('next-page').onclick = () => {
  state.page++;
  render();
};
$('keep-running').onclick = () => closeDialog($('close-dialog'));
$('stop-close').onclick = () => stopRecovery(true);
let dropActive = false;
function setDropActive(active: boolean) {
  if (active === dropActive) return;
  dropActive = active;
  $('workspace').classList.toggle('dragover', active);
  $('drop-title').textContent = active ? 'Release to add files' : 'Drop .ksd files or a folder';
}
initializeIcons();
installPressFeedback();
async function init() {
  render();
  if (!native) {
    notice('Interface preview. Open KSD Decrypt to recover files.');
    return;
  }
  const dispose = await subscribeNative(
    () => {
      if (state.phase === 'recovering' && !$('close-dialog').open) openDialog($('close-dialog'));
    },
    (payload) => {
      setDropActive(!locked() && (payload.type === 'over' || payload.type === 'enter'));
      if (payload.type === 'drop' && !locked()) void selectFiles(false, payload.paths);
    },
  );
  window.addEventListener('pagehide', dispose, { once: true });
  state.ready = true;
  render();
}
init().catch((error) => notice(`KSD Decrypt could not initialize: ${error}`, 'error'));
