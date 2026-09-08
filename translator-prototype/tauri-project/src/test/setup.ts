import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";

// 每个用例后卸载组件，避免用例间 DOM 污染
afterEach(() => {
  cleanup();
});
