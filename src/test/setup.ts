// Every test starts with no stand-in for the Rust side; a test that needs one calls mockIPC (or src/test/tauri.ts)
import { afterEach } from "vitest";
import { clearMocks } from "@tauri-apps/api/mocks";

// jsdom has no matchMedia, which the settings store asks for the system's light or dark look: always light here
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as MediaQueryList;
}

afterEach(() => clearMocks());
