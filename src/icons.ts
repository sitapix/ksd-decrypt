import {
  Check,
  ChevronLeft,
  ChevronRight,
  Ellipsis,
  FolderOpen,
  Square,
  X,
  createElement,
  createIcons,
} from 'lucide';

const icons = { Check, ChevronLeft, ChevronRight, Ellipsis, FolderOpen, Square, X };
const attributes = { class: 'icon', 'aria-hidden': 'true', focusable: 'false' };

export function initializeIcons() {
  createIcons({ icons, attrs: attributes });
}

export function icon(name: keyof typeof icons) {
  return createElement(icons[name], attributes);
}
