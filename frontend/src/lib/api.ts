export class ApiError extends Error {
  code: string;
  status: number;
  constructor(message: string, code = "error", status = 0) {
    super(message);
    this.code = code;
    this.status = status;
  }
}

async function parseError(res: Response): Promise<ApiError> {
  let code = "http_error";
  let message = `Request failed with ${res.status}`;
  try {
    const body = await res.json();
    if (body && typeof body === "object") {
      code = body.code ?? code;
      message = body.message ?? message;
    }
  } catch {
    /* non-JSON error body */
  }
  return new ApiError(message, code, res.status);
}

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`/api${path}`, {
    ...init,
    headers: {
      ...(init?.body ? { "content-type": "application/json" } : {}),
      ...(init?.headers ?? {}),
    },
  });
  if (!res.ok) throw await parseError(res);
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

/** Consume an SSE response, dispatching named events with parsed JSON data. */
export async function consumeSse(
  res: Response,
  onEvent: (event: string, data: unknown) => void,
): Promise<void> {
  if (!res.ok) throw await parseError(res);
  if (!res.body) return;
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let boundary = buffer.indexOf("\n\n");
    while (boundary !== -1) {
      const frame = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      let event = "message";
      const dataLines: string[] = [];
      for (const line of frame.split("\n")) {
        if (line.startsWith("event:")) event = line.slice(6).trim();
        else if (line.startsWith("data:")) dataLines.push(line.slice(5).trim());
      }
      if (dataLines.length) {
        try {
          onEvent(event, JSON.parse(dataLines.join("\n")));
        } catch {
          /* ignore malformed frames (comments, keep-alives) */
        }
      }
      boundary = buffer.indexOf("\n\n");
    }
  }
}

export async function sseStream(
  path: string,
  init: RequestInit,
  onEvent: (event: string, data: unknown) => void,
): Promise<void> {
  const res = await fetch(`/api${path}`, init);
  await consumeSse(res, onEvent);
}

export function downloadText(filename: string, text: string, type = "text/markdown;charset=utf-8") {
  const blob = new Blob([text], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export async function fetchText(path: string): Promise<string> {
  const res = await fetch(`/api${path}`);
  if (!res.ok) throw await parseError(res);
  return res.text();
}
