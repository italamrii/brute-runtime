import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ConfidenceBadge } from "./Badges";
import { I18nProvider } from "../i18n/I18nContext";

function renderBadge(confidence: Parameters<typeof ConfidenceBadge>[0]["confidence"]) {
  return render(
    <I18nProvider>
      <ConfidenceBadge confidence={confidence} />
    </I18nProvider>,
  );
}

describe("ConfidenceBadge", () => {
  it("never renders a bare universal score — every tier gets its own labeled badge", () => {
    renderBadge("measured");
    expect(screen.getByText("measured")).toHaveClass("badge-measured");
  });

  it("labels an unavailable value honestly rather than hiding it", () => {
    renderBadge("unavailable");
    expect(screen.getByText("unavailable")).toHaveClass("badge-unavailable");
  });

  it("labels an inferred value distinctly from a measured one", () => {
    renderBadge("inferred");
    const el = screen.getByText("inferred");
    expect(el).toHaveClass("badge-inferred");
    expect(el).not.toHaveClass("badge-measured");
  });

  it("renders catalog provenance with its own label, not as 'unknown'", () => {
    renderBadge("catalog");
    expect(screen.getByText("catalog")).toBeInTheDocument();
  });
});
