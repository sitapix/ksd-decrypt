const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
interface Spring {
  to(next: number, options?: { immediate?: boolean }): void;
  step(time: number): void;
  finish(): void;
}
const running = new Set<Spring>();
let frame = 0;

function tick(time: number) {
  frame = 0;
  for (const spring of running) spring.step(time);
  if (running.size) frame = requestAnimationFrame(tick);
}

// An exact critically damped spring. Retargeting keeps both position and velocity.
export function spring(
  initial: number,
  paint: (value: number) => void,
  { response = 0.3, rest = (_value: number) => {} } = {},
): Spring {
  let value = initial,
    target = initial,
    velocity = 0,
    previous = 0;
  const frequency = (2 * Math.PI) / response;
  const finish = () => {
    value = target;
    velocity = 0;
    previous = 0;
    running.delete(controller);
    paint(value);
    rest(target);
  };
  const controller: Spring = {
    to(next, { immediate = false } = {}) {
      target = next;
      if (
        immediate ||
        reducedMotion.matches ||
        (Math.abs(value - target) < 0.001 && Math.abs(velocity) < 0.01)
      ) {
        finish();
        return;
      }
      if (!running.has(controller)) previous = performance.now();
      running.add(controller);
      if (!frame) frame = requestAnimationFrame(tick);
    },
    step(time) {
      const dt = Math.min((time - previous) / 1000, 0.064);
      previous = time;
      const displacement = value - target;
      const offset = velocity + frequency * displacement;
      const decay = Math.exp(-frequency * dt);
      value = target + (displacement + offset * dt) * decay;
      velocity = (velocity - frequency * offset * dt) * decay;
      paint(value);
      if (Math.abs(value - target) < 0.001 && Math.abs(velocity) < 0.01) finish();
    },
    finish,
  };
  paint(value);
  return controller;
}

reducedMotion.addEventListener('change', () => {
  if (reducedMotion.matches) for (const controller of [...running]) controller.finish();
});

document.addEventListener('visibilitychange', () => {
  if (document.hidden) for (const controller of [...running]) controller.finish();
});

const reveals = new WeakMap<HTMLElement, Spring>();
export function reveal(element: HTMLElement) {
  if (reducedMotion.matches) return;
  let controller = reveals.get(element);
  if (!controller) {
    controller = spring(0, (value) => element.style.setProperty('--reveal', String(value)));
    reveals.set(element, controller);
  } else {
    controller.to(0, { immediate: true });
  }
  controller.to(1);
}

const dialogs = new WeakMap<HTMLElement, Spring>();
function dialogSpring(dialog: HTMLDialogElement) {
  if (!dialogs.has(dialog)) {
    dialogs.set(
      dialog,
      spring(0, (value) => dialog.style.setProperty('--presence', String(value)), {
        response: 0.24,
        rest: (value) => {
          if (value === 0 && dialog.open) dialog.close();
        },
      }),
    );
  }
  return dialogs.get(dialog)!;
}

export function openDialog(dialog: HTMLDialogElement) {
  const trigger = document.activeElement?.getBoundingClientRect();
  const controller = dialogSpring(dialog);
  if (!dialog.open) {
    dialog.showModal();
    if (trigger) {
      const bounds = dialog.getBoundingClientRect();
      const x = Math.max(
        0,
        Math.min(100, ((trigger.x + trigger.width / 2 - bounds.x) / bounds.width) * 100),
      );
      const y = Math.max(
        0,
        Math.min(100, ((trigger.y + trigger.height / 2 - bounds.y) / bounds.height) * 100),
      );
      dialog.style.transformOrigin = `${x}% ${y}%`;
    }
  }
  controller.to(1);
}

export function closeDialog(dialog: HTMLDialogElement) {
  if (dialog.open) dialogSpring(dialog).to(0);
}

export function installPressFeedback() {
  const presses = new WeakMap<HTMLElement, Spring>();
  let pressed: HTMLButtonElement | null = null;
  const scale = (element: HTMLButtonElement | null, value: number) => {
    if (!element) return;
    if (!presses.has(element))
      presses.set(
        element,
        spring(1, (next) => element.style.setProperty('--press', String(next)), { response: 0.18 }),
      );
    presses.get(element)!.to(reducedMotion.matches ? 1 : value);
  };
  const release = () => {
    scale(pressed, 1);
    pressed = null;
  };
  document.addEventListener('pointerdown', (event) => {
    if (event.button !== 0 || !event.isPrimary) return;
    release();
    pressed =
      event.target instanceof Element
        ? event.target.closest<HTMLButtonElement>('button:not(:disabled)')
        : null;
    scale(pressed, 0.975);
  });
  document.addEventListener('pointermove', (event) => {
    if (!pressed) return;
    const bounds = pressed.getBoundingClientRect();
    const inside =
      event.clientX >= bounds.left - 8 &&
      event.clientX <= bounds.right + 8 &&
      event.clientY >= bounds.top - 8 &&
      event.clientY <= bounds.bottom + 8;
    scale(pressed, inside ? 0.975 : 1);
  });
  document.addEventListener('pointerup', release);
  document.addEventListener('pointercancel', release);
  window.addEventListener('blur', release);
  document.addEventListener('keydown', (event) => {
    if (event.repeat || ![' ', 'Enter'].includes(event.key)) return;
    pressed =
      event.target instanceof Element
        ? event.target.closest<HTMLButtonElement>('button:not(:disabled)')
        : null;
    scale(pressed, 0.975);
  });
  document.addEventListener('keyup', (event) => {
    if ([' ', 'Enter'].includes(event.key)) release();
  });
}
