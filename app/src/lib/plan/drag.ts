// Drag and drop for plan entries (calendar, board list), with mouse and finger.
// Mouse and pen start dragging after a few pixels; a finger needs a long press (so scrolling
// still works), then the entry follows it. Drop targets are elements with `data-drop="<key>"`;
// the one under the pointer gets `data-over` for styling, and `ondrop(key)` gets its key.
// A tap without dragging stays a normal click.

export type DragOptions = {
  /** Off: the element is just clickable. */
  enabled: boolean;
  ondrop: (target: string) => void;
};

const LONG_PRESS = 320;
const MOUSE_SLOP = 6;
const TOUCH_SLOP = 10;
const EDGE = 72;

export function draggable(node: HTMLElement, options: DragOptions) {
  let opts = options;
  let start: { x: number; y: number; id: number; touch: boolean } | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let ghost: HTMLElement | null = null;
  let grab = { x: 0, y: 0 };
  let over: HTMLElement | null = null;
  let last = { x: 0, y: 0 };
  let frame = 0;
  let justDragged = false;

  const active = () => ghost !== null;

  function begin(x: number, y: number) {
    if (!start) return;
    const r = node.getBoundingClientRect();
    grab = { x: start.x - r.left, y: start.y - r.top };
    ghost = node.cloneNode(true) as HTMLElement;
    ghost.removeAttribute("id");
    ghost.classList.add("drag-ghost");
    Object.assign(ghost.style, { position: "fixed", left: "0", top: "0", width: `${r.width}px`, margin: "0", zIndex: "1000", pointerEvents: "none" });
    document.body.appendChild(ghost);
    document.body.classList.add("is-dragging");
    node.classList.add("drag-source");
    try {
      node.setPointerCapture(start.id);
    } catch {
      /* pointer already gone */
    }
    navigator.vibrate?.(12);
    moveTo(x, y);
    frame = requestAnimationFrame(autoScroll);
  }

  function moveTo(x: number, y: number) {
    last = { x, y };
    if (!ghost) return;
    ghost.style.transform = `translate(${x - grab.x}px, ${y - grab.y}px) rotate(-2deg)`;
    const target = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest<HTMLElement>("[data-drop]") ?? null;
    if (target !== over) {
      over?.removeAttribute("data-over");
      over = target;
      over?.setAttribute("data-over", "");
    }
  }

  // Near the top or bottom edge the page (or the scrolling container) scrolls along.
  function autoScroll() {
    if (!ghost) return;
    const h = window.innerHeight;
    const dy = last.y < EDGE ? -Math.ceil((EDGE - last.y) / 6) : last.y > h - EDGE ? Math.ceil((last.y - (h - EDGE)) / 6) : 0;
    if (dy) {
      window.scrollBy(0, dy);
      moveTo(last.x, last.y);
    }
    frame = requestAnimationFrame(autoScroll);
  }

  function end(drop: boolean) {
    if (timer) clearTimeout(timer);
    timer = null;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    cancelAnimationFrame(frame);
    if (ghost) {
      const key = over?.dataset.drop;
      over?.removeAttribute("data-over");
      ghost.remove();
      ghost = null;
      over = null;
      document.body.classList.remove("is-dragging");
      node.classList.remove("drag-source");
      justDragged = true;
      setTimeout(() => (justDragged = false), 0);
      if (drop && key) opts.ondrop(key);
    }
    start = null;
  }

  function down(ev: PointerEvent) {
    if (!opts.enabled || ev.button !== 0) return;
    start = { x: ev.clientX, y: ev.clientY, id: ev.pointerId, touch: ev.pointerType === "touch" };
    // Listen on the window: a fast pointer leaves the element before the drag has begun.
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    if (start.touch) timer = setTimeout(() => begin(ev.clientX, ev.clientY), LONG_PRESS);
  }

  function move(ev: PointerEvent) {
    if (!start || ev.pointerId !== start.id) return;
    if (active()) {
      ev.preventDefault();
      moveTo(ev.clientX, ev.clientY);
      return;
    }
    const d = Math.hypot(ev.clientX - start.x, ev.clientY - start.y);
    if (start.touch) {
      // Moving before the long press is over means scrolling.
      if (d > TOUCH_SLOP) end(false);
    } else if (d > MOUSE_SLOP) {
      begin(ev.clientX, ev.clientY);
    }
  }

  const up = (ev: PointerEvent) => start && ev.pointerId === start.id && end(true);
  const cancel = () => end(false);
  // While dragging with a finger the page must not scroll (needs a non-passive listener).
  const touchmove = (ev: TouchEvent) => active() && ev.preventDefault();
  // A drag is not a click; a long press does not open the context menu.
  const click = (ev: MouseEvent) => {
    if (justDragged) {
      ev.preventDefault();
      ev.stopImmediatePropagation();
    }
  };
  const menu = (ev: Event) => opts.enabled && ev.preventDefault();
  const key = (ev: KeyboardEvent) => ev.key === "Escape" && end(false);

  node.addEventListener("pointerdown", down);
  node.addEventListener("touchmove", touchmove, { passive: false });
  node.addEventListener("click", click, true);
  node.addEventListener("contextmenu", menu);
  window.addEventListener("keydown", key);

  return {
    update(next: DragOptions) {
      opts = next;
    },
    destroy() {
      end(false);
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("touchmove", touchmove);
      node.removeEventListener("click", click, true);
      node.removeEventListener("contextmenu", menu);
      window.removeEventListener("keydown", key);
    },
  };
}
