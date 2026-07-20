import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { TechnicalValue } from "./TechnicalValue";

describe("TechnicalValue", () => {
  it("forces ltr direction regardless of the surrounding page direction", () => {
    render(
      <div dir="rtl">
        <TechnicalValue>C:\Users\asdks\AppData\Local\BRUTE Runtime\runtime\cpu</TechnicalValue>
      </div>,
    );
    const el = screen.getByText(/BRUTE Runtime/);
    expect(el).toHaveAttribute("dir", "ltr");
  });

  it("isolates the value from the surrounding bidi context without altering its text", () => {
    const raw = "\\\\?\\C:\\Models\\qwen.gguf";
    render(<TechnicalValue>{raw}</TechnicalValue>);
    const el = screen.getByText(raw);
    expect(el).toHaveStyle({ unicodeBidi: "isolate" });
    // The fix must be presentation-only - the exact original string (not
    // a copy with inserted bidi control characters) is what a user's
    // clipboard would receive when they select and copy this element.
    expect(el.textContent).toBe(raw);
  });

  it("preserves the caller's className so existing visual styling (mono, kv-row-value) still applies", () => {
    render(<TechnicalValue className="kv-row-value mono">abc123</TechnicalValue>);
    expect(screen.getByText("abc123")).toHaveClass("kv-row-value", "mono");
  });

  it("renders as a different element when asked, e.g. a table cell", () => {
    render(
      <table>
        <tbody>
          <tr>
            <TechnicalValue as="td" className="mono">
              Q4_K_M
            </TechnicalValue>
          </tr>
        </tbody>
      </table>,
    );
    const cell = screen.getByText("Q4_K_M");
    expect(cell.tagName).toBe("TD");
    expect(cell).toHaveAttribute("dir", "ltr");
  });

  it("forwards a title attribute for full-value tooltips on truncated hashes", () => {
    const fullHash = "a".repeat(64);
    render(<TechnicalValue title={fullHash}>{fullHash.slice(0, 12)}…</TechnicalValue>);
    expect(screen.getByTitle(fullHash)).toBeInTheDocument();
  });

  it("keeps a profile ID ltr and unreordered under Arabic RTL", () => {
    const profileId = "profile-instance-f052be34d631ff2889844a7551eea020";
    render(
      <div dir="rtl">
        <TechnicalValue>{profileId}</TechnicalValue>
      </div>,
    );
    const el = screen.getByText(profileId);
    expect(el).toHaveAttribute("dir", "ltr");
    expect(el.textContent).toBe(profileId);
  });

  it("keeps a raw enum value ltr under Arabic RTL", () => {
    render(
      <div dir="rtl">
        <TechnicalValue>local_unverified_source</TechnicalValue>
      </div>,
    );
    expect(screen.getByText("local_unverified_source")).toHaveAttribute("dir", "ltr");
  });

  it("keeps a model identifier ltr under Arabic RTL", () => {
    render(
      <div dir="rtl">
        <TechnicalValue>Qwen2.5-0.5B-Instruct-Q4_K_M</TechnicalValue>
      </div>,
    );
    expect(screen.getByText("Qwen2.5-0.5B-Instruct-Q4_K_M")).toHaveAttribute("dir", "ltr");
  });

  it("renders identically (still dir=ltr) when the surrounding page is English LTR, not just Arabic RTL", () => {
    const path = "\\\\?\\C:\\Models\\qwen.gguf";
    render(
      <div dir="ltr">
        <TechnicalValue>{path}</TechnicalValue>
      </div>,
    );
    const el = screen.getByText(path);
    expect(el).toHaveAttribute("dir", "ltr");
    expect(el.textContent).toBe(path);
  });
});
