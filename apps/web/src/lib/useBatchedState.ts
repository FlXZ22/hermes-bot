import { useCallback, useEffect, useRef, useState, type SetStateAction } from "react";

/** Preserve event order while coalescing token bursts into roughly 30 UI commits/sec. */
export function useBatchedState<T>(initial: T) {
  const [value, commit] = useState(initial);
  const pending = useRef<SetStateAction<T>[]>([]);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const setValue = useCallback((update: SetStateAction<T>) => {
    pending.current.push(update);
    if (timer.current !== null) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      const updates = pending.current.splice(0);
      commit(previous => updates.reduce<T>((next, action) => typeof action === "function"
        ? (action as (prev: T) => T)(next) : action, previous));
    }, 32);
  }, []);
  useEffect(() => () => { if (timer.current !== null) clearTimeout(timer.current); timer.current = null; pending.current = []; }, []);
  return [value, setValue] as const;
}
