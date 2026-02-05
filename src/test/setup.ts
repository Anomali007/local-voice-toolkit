/// <reference types="vitest" />
import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri APIs for testing
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-global-shortcut', () => ({
  register: vi.fn(),
  unregister: vi.fn(),
  isRegistered: vi.fn(),
}));
