import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Markdown } from "./Markdown";

const openUrlMock = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
}));

describe("Markdown", () => {
  beforeEach(() => {
    openUrlMock.mockReset();
  });

  it("renders headings, lists, and paragraphs as real elements", () => {
    render(<Markdown content={"# Title\n\n- one\n- two"} copyLabel="Copy" />);
    expect(screen.getByRole("heading", { level: 1, name: "Title" })).toBeInTheDocument();
    expect(screen.getByText("one")).toBeInTheDocument();
    expect(screen.getByText("two")).toBeInTheDocument();
  });

  it("renders a GFM table", () => {
    render(<Markdown content={"| A | B |\n|---|---|\n| 1 | 2 |"} copyLabel="Copy" />);
    expect(screen.getByRole("table")).toBeInTheDocument();
  });

  it("a safe https link opens via the system browser on click, never a WebView navigation", () => {
    render(<Markdown content="[BRUTE](https://example.com)" copyLabel="Copy" />);
    const link = screen.getByText("BRUTE");
    fireEvent.click(link);
    expect(openUrlMock).toHaveBeenCalledWith("https://example.com");
  });

  it("an unsafe (localhost) link is rendered as inert text, never a clickable anchor", () => {
    render(<Markdown content="[click me](http://localhost:1420/)" copyLabel="Copy" />);
    const span = screen.getByText("click me");
    expect(span.tagName).toBe("SPAN");
    expect(span).toHaveClass("md-unsafe-link");
    fireEvent.click(span);
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("a javascript: link is rejected the same way", () => {
    render(<Markdown content="[bad](javascript:alert(1))" copyLabel="Copy" />);
    expect(screen.getByText("bad")).toHaveClass("md-unsafe-link");
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("renders a fenced code block with a language label and a copy button", () => {
    render(<Markdown content={"```javascript\nconsole.log(1);\n```"} copyLabel="Copy" />);
    expect(screen.getByText("javascript")).toBeInTheDocument();
    expect(screen.getByText("Copy")).toBeInTheDocument();
  });

  it("the copy button copies the code block's exact text", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    render(<Markdown content={"```python\nprint('hi')\n```"} copyLabel="Copy" />);
    fireEvent.click(screen.getByText("Copy"));
    expect(writeText).toHaveBeenCalledWith("print('hi')");
  });

  it("inline code (not a fenced block) has no copy button or language header", () => {
    render(<Markdown content="Use `const x = 1` here." copyLabel="Copy" />);
    expect(screen.queryByText("Copy")).not.toBeInTheDocument();
  });
});
