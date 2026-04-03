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

const API_PORT_INIT_TIMEOUT_MS = 30_000;
const API_PORT_INIT_RETRY_MS = 200;

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

function inTauriDesktop(): boolean {
  return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__);
}

function formatNetworkFailureMessage(method: string, url: string, error: unknown): string {
  const reason = error instanceof Error ? error.message : String(error);
  const commonHint = inTauriDesktop()
    ? 'Embedded API appears unreachable from the desktop webview. Restart the app and check desktop diagnostics in your RustyGene data directory.'
    : 'API appears unreachable. Verify the API process is running and reachable from the current origin.';

  return `Network request failed for ${method} ${url}: ${reason}. ${commonHint}`;
}

async function fetchWithDiagnostics(url: string, init: RequestInit & { method: string }): Promise<Response> {
  try {
    return await fetch(url, init);
  } catch (error) {
    if (inTauriDesktop()) {
      const refreshedPort = await resolveApiPortFromTauri();
      if (refreshedPort !== null) {
        const refreshedBase = `http://127.0.0.1:${refreshedPort}`;
        const original = new URL(url);
        const refreshedUrl = new URL(`${original.pathname}${original.search}`, refreshedBase).toString();

        if (refreshedBase !== baseUrl) {
          baseUrl = refreshedBase;
        }

        try {
          await delay(120);
          return await fetch(refreshedUrl, init);
        } catch {
          // Fall through to formatted error below.
        }
      }
    }

    throw new Error(formatNetworkFailureMessage(init.method, url, error));
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function resolveApiPortWithRetry(): Promise<number | null> {
  const deadline = Date.now() + API_PORT_INIT_TIMEOUT_MS;

  while (Date.now() < deadline) {
    const port = await resolveApiPortFromTauri();
    if (port !== null) {
      return port;
    }
    await delay(API_PORT_INIT_RETRY_MS);
  }

  return null;
}

export async function initializeApiClient(): Promise<void> {
  const port = await resolveApiPortWithRetry();
  if (port !== null) {
    baseUrl = `http://127.0.0.1:${port}`;
    return;
  }

  if (inTauriDesktop()) {
    throw new Error(
      `Embedded API did not become ready within ${API_PORT_INIT_TIMEOUT_MS / 1000} seconds`
    );
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  incrementPendingRequests();
  const url = buildUrl(path);

  let response: Response;

  try {
    response = await fetchWithDiagnostics(url.toString(), {
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

async function upload<T>(path: string, formData: FormData): Promise<T> {
  incrementPendingRequests();
  const url = buildUrl(path);

  let response: Response;

  try {
    response = await fetchWithDiagnostics(url.toString(), {
      method: 'POST',
      body: formData
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
      payload?.message ?? `POST ${path} failed`,
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
    response = await fetchWithDiagnostics(url.toString(), {
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
  postFormData: <T>(path: string, formData: FormData) => upload<T>(path, formData),
  put: <T>(path: string, body: unknown) => request<T>('PUT', path, body),
  del: <T>(path: string) => request<T>('DELETE', path),
  download,
  url: (path: string) => buildUrl(path).toString()
};
