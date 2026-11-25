import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { JobLog, JobStatus, JobView, Snapshot } from "./types";

export function useApi<T>(path: string | null, intervalMs = 0) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [loading, setLoading] = useState(Boolean(path));
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const reload = useCallback(async () => {
    if (!path) return;
    try {
      const result = await api<T>(path);
      if (mounted.current) {
        setData(result);
        setError(null);
      }
    } catch (e) {
      if (mounted.current) setError(e as Error);
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [path]);

  useEffect(() => {
    if (!path) return;
    setLoading(true);
    void reload();
    if (intervalMs > 0) {
      const id = window.setInterval(() => void reload(), intervalMs);
      return () => window.clearInterval(id);
    }
  }, [path, intervalMs, reload]);

  return { data, error, loading, reload, setData };
}

export function useLiveSystem(initial?: Snapshot[]) {
  const [history, setHistory] = useState<Snapshot[]>(initial ?? []);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const source = new EventSource("/api/system/stream");
    source.addEventListener("snapshot", (event) => {
      try {
        const snapshot = JSON.parse((event as MessageEvent).data) as Snapshot;
        setConnected(true);
        setHistory((prev) => {
          const next = [...prev, snapshot];
          return next.length > 150 ? next.slice(next.length - 150) : next;
        });
      } catch {
        /* ignore */
      }
    });
    source.onerror = () => setConnected(false);
    return () => source.close();
  }, []);

  const latest = history.length ? history[history.length - 1] : null;
  return { history, latest, connected };
}

export interface JobStreamState {
  job: JobView | null;
  logs: JobLog[];
  terminal: boolean;
}

export function useJobStream(jobId: string | null, onTerminal?: (job: JobView) => void) {
  const [state, setState] = useState<JobStreamState>({ job: null, logs: [], terminal: false });
  const terminalRef = useRef(false);
  const callbackRef = useRef(onTerminal);
  callbackRef.current = onTerminal;

  useEffect(() => {
    if (!jobId) {
      setState({ job: null, logs: [], terminal: false });
      terminalRef.current = false;
      return;
    }
    terminalRef.current = false;
    setState({ job: null, logs: [], terminal: false });
    const source = new EventSource(`/api/jobs/${jobId}/stream`);

    const finish = (job: JobView) => {
      if (terminalRef.current) return;
      terminalRef.current = true;
      setState((prev) => ({ ...prev, job, terminal: true }));
      callbackRef.current?.(job);
    };

    source.addEventListener("state", (event) => {
      try {
        const job = JSON.parse((event as MessageEvent).data) as JobView;
        setState((prev) => ({ ...prev, job }));
        if (["succeeded", "failed", "cancelled"].includes(job.status)) finish(job);
      } catch {
        /* ignore */
      }
    });

    source.addEventListener("job", (event) => {
      try {
        const payload = JSON.parse((event as MessageEvent).data);
        if (payload.type === "log") {
          setState((prev) => ({ ...prev, logs: [...prev.logs, payload.log as JobLog] }));
        } else if (payload.type === "progress") {
          setState((prev) =>
            prev.job
              ? { ...prev, job: { ...prev.job, stage: payload.stage, progress: payload.progress } }
              : prev,
          );
        } else if (payload.type === "status") {
          setState((prev) => {
            if (!prev.job) return prev;
            const job: JobView = { ...prev.job, status: payload.status as JobStatus, error: payload.error };
            if (["succeeded", "failed", "cancelled"].includes(job.status)) finish(job);
            return { ...prev, job };
          });
        }
      } catch {
        /* ignore */
      }
    });

    source.onerror = () => {
      // Stream closed after a terminal event, or the server went away. Poll once
      // so a missed terminal frame can never leave the UI spinning.
      if (terminalRef.current) {
        source.close();
        return;
      }
      void api<{ job: JobView }>(`/jobs/${jobId}`)
        .then(({ job }) => {
          setState((prev) => ({ ...prev, job }));
          if (["succeeded", "failed", "cancelled"].includes(job.status)) finish(job);
        })
        .catch(() => undefined);
    };

    return () => source.close();
  }, [jobId]);

  return state;
}

export function useDebounced<T>(value: T, delay = 200): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(value), delay);
    return () => window.clearTimeout(id);
  }, [value, delay]);
  return debounced;
}
