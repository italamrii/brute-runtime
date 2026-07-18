import { describe, expect, it, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { I18nProvider, useI18n } from "./I18nContext";

function Probe() {
  const { lang, dir, setLang, t } = useI18n();
  return (
    <div>
      <span data-testid="lang">{lang}</span>
      <span data-testid="dir">{dir}</span>
      <span data-testid="slogan">{t("slogan")}</span>
      <span data-testid="missing">{t("this_key_does_not_exist")}</span>
      <button onClick={() => setLang("ar")}>switch</button>
    </div>
  );
}

describe("I18nProvider", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("defaults to English LTR", () => {
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId("lang")).toHaveTextContent("en");
    expect(screen.getByTestId("dir")).toHaveTextContent("ltr");
  });

  it("switches direction to RTL when Arabic is selected, without corrupting other text", () => {
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    fireEvent.click(screen.getByText("switch"));
    expect(screen.getByTestId("dir")).toHaveTextContent("rtl");
    expect(screen.getByTestId("slogan")).toHaveTextContent("بياناتك ما تطلع من جهازك");
  });

  it("falls back to the raw key for a missing translation instead of rendering blank", () => {
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId("missing")).toHaveTextContent("this_key_does_not_exist");
  });
});
