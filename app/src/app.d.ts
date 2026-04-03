/// <reference types="@sveltejs/kit" />

declare global {
  namespace App {
    // interface Error {}
    // interface Locals {}
    // interface PageData {}
    // interface Platform {}
  }

  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export {};
