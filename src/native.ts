import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { DragDropEvent } from '@tauri-apps/api/webview';

export interface InputFile {
  id: string;
  name: string;
  path: string;
  bytes: number;
  format: string;
  kind: 'image' | 'video' | 'unknown';
  issue: string | null;
}

export interface RecoveredFile {
  id: string;
  name: string;
  source: string;
  output: string | null;
  bytes: number;
  kind: InputFile['kind'];
  format: string;
  status: 'recovered' | 'failed';
  detail: string;
  source_sha256: string | null;
  recovered_sha256: string | null;
}

export interface Progress {
  id: string;
  stage: 'recovering' | 'checking';
  completed: number;
  total: number;
  processedBytes: number;
  totalBytes: number;
}

export type RecoveryEvent =
  { event: 'progress'; data: Progress } | { event: 'file'; data: RecoveredFile };

interface Selection {
  files: InputFile[];
  warnings: string[];
  message: string | null;
}
interface BatchResult {
  files: RecoveredFile[];
  folder: string | null;
  cancelled: boolean;
  reportWarning: string | null;
}

export const native = isTauri();
export const api = {
  chooseInputs: (folder: boolean) => invoke<Selection>('choose_inputs', { folder }),
  addPaths: (paths: string[]) => invoke<Selection>('add_paths', { paths }),
  chooseOutput: () => invoke<string | null>('choose_output'),
  clear: () => invoke<void>('clear_selection'),
  cancel: () => invoke<void>('cancel_recovery'),
  open: (id: string | null) => invoke<void>('open_result', { id }),
  reveal: (id: string) => invoke<void>('open_result', { id, reveal: true }),
  close: () => getCurrentWebviewWindow().close(),
  async recover(ids: string[], onEvent: (event: RecoveryEvent) => void): Promise<BatchResult> {
    const channel = new Channel<RecoveryEvent>(onEvent);
    try {
      return await invoke<BatchResult>('recover', { ids, onEvent: channel });
    } finally {
      // The response is authoritative; ignore any queued messages after it settles.
      channel.onmessage = () => {};
    }
  },
};

export async function subscribeNative(
  onClose: () => void,
  onDrop: (event: DragDropEvent) => void,
): Promise<UnlistenFn> {
  const listeners: UnlistenFn[] = [];
  const dispose = () => {
    for (const unlisten of listeners.splice(0)) unlisten();
  };
  try {
    listeners.push(await listen('recovery-close-requested', onClose));
    listeners.push(
      await getCurrentWebviewWindow().onDragDropEvent(({ payload }) => onDrop(payload)),
    );
    return dispose;
  } catch (error) {
    dispose();
    throw error;
  }
}
