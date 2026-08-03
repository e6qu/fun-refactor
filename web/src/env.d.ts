/// <reference types="vite/client" />

// Monaco reads its worker factory off the global; the shipped types do not declare it.
declare global {
  interface Window {
    MonacoEnvironment?: { getWorker: (id: string, label: string) => Worker };
  }
}
export {};
