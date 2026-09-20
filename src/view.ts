function required<T extends HTMLElement>(id: string, type: { new (): T }): T {
  const element = document.getElementById(id);
  if (!(element instanceof type)) throw new Error(`Missing interface element: ${id}`);
  return element;
}

const elements = {
  'open-folder': required('open-folder', HTMLButtonElement),
  'more-actions': required('more-actions', HTMLDetailsElement),
  'more-trigger': required('more-trigger', HTMLElement),
  'add-files': required('add-files', HTMLButtonElement),
  'choose-folder': required('choose-folder', HTMLButtonElement),
  'change-destination': required('change-destination', HTMLButtonElement),
  'clear-button': required('clear-button', HTMLButtonElement),
  'help-button': required('help-button', HTMLButtonElement),
  notice: required('notice', HTMLElement),
  'drop-notice': required('drop-notice', HTMLElement),
  workspace: required('workspace', HTMLElement),
  'drop-zone': required('drop-zone', HTMLElement),
  'drop-title': required('drop-title', HTMLElement),
  'choose-files': required('choose-files', HTMLButtonElement),
  'file-area': required('file-area', HTMLElement),
  'search-area': required('search-area', HTMLElement),
  search: required('search', HTMLInputElement),
  'file-list': required('file-list', HTMLElement),
  pagination: required('pagination', HTMLElement),
  'previous-page': required('previous-page', HTMLButtonElement),
  'page-label': required('page-label', HTMLElement),
  'next-page': required('next-page', HTMLButtonElement),
  'progress-area': required('progress-area', HTMLElement),
  'progress-title': required('progress-title', HTMLElement),
  'progress-percent': required('progress-percent', HTMLElement),
  progress: required('progress', HTMLProgressElement),
  'progress-detail': required('progress-detail', HTMLElement),
  'stop-button': required('stop-button', HTMLButtonElement),
  'recovery-actions': required('recovery-actions', HTMLElement),
  'recover-button': required('recover-button', HTMLButtonElement),
  'file-count': required('file-count', HTMLElement),
  completion: required('completion', HTMLElement),
  'completion-title': required('completion-title', HTMLElement),
  'completion-detail': required('completion-detail', HTMLElement),
  'help-dialog': required('help-dialog', HTMLDialogElement),
  'close-dialog': required('close-dialog', HTMLDialogElement),
  'keep-running': required('keep-running', HTMLButtonElement),
  'stop-close': required('stop-close', HTMLButtonElement),
};

export const $ = <K extends keyof typeof elements>(id: K): (typeof elements)[K] => elements[id];
