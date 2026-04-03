import { appState, decrementPendingRequests, incrementPendingRequests } from '$lib/state.svelte';

export class ApiError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(message: string, code: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.code = code;
    this.status = status;
  }
}

let baseUrl = 'http://localhost:3000';

function buildUrl(path: string): URL {
  const url = new URL(`${baseUrl}${path}`);
  if (appState.sandboxMode) {
    url.searchParams.set('sandbox', '1');
  }
  return url;
}

async function resolveApiPortFromTauri(): Promise<number | null> {
  if (typeof window === 'undefined' || !window.__TAURI_INTERNALS__) {
    return null;
  }

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<number>('get_api_port');
  } catch {
    return null;
  }
}

export async function initializeApiClient(): Promise<void> {
  const port = await resolveApiPortFromTauri();
  if (port !== null) {
    baseUrl = `http://127.0.0.1:${port}`;
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  incrementPendingRequests();
  const url = buildUrl(path);

  let response: Response;

  try {
    response = await fetch(url.toString(), {
      method,
      headers: {
        'content-type': 'application/json'
      },
      body: body === undefined ? undefined : JSON.stringify(body)
    });
  } finally {
    decrementPendingRequests();
  }

  if (!response.ok) {
    let payload: { message?: string; code?: string } | null = null;

    try {
      payload = (await response.json()) as { message?: string; code?: string };
    } catch {
      payload = null;
    }

    throw new ApiError(
      payload?.message ?? `${method} ${path} failed`,
      payload?.code ?? 'request_failed',
      response.status
    );
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

async function download(path: string): Promise<{ blob: Blob; fileName: string | null }> {
  incrementPendingRequests();
  const url = buildUrl(path);

  let response: Response;
  try {
    response = await fetch(url.toString(), {
      method: 'GET'
    });
  } finally {
    decrementPendingRequests();
  }

  if (!response.ok) {
    let payload: { message?: string; code?: string } | null = null;

    try {
      payload = (await response.json()) as { message?: string; code?: string };
    } catch {
      payload = null;
    }

    throw new ApiError(
      payload?.message ?? `GET ${path} failed`,
      payload?.code ?? 'request_failed',
      response.status
    );
  }

  const disposition = response.headers.get('content-disposition') ?? '';
  const fileNameMatch = disposition.match(/filename="?([^\";]+)"?/i);
  const fileName = fileNameMatch?.[1] ?? null;

  return {
    blob: await response.blob(),
    fileName
  };
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body: unknown) => request<T>('POST', path, body),
  put: <T>(path: string, body: unknown) => request<T>('PUT', path, body),
  del: <T>(path: string) => request<T>('DELETE', path),
  download,
  url: (path: string) => buildUrl(path).toString()
};
