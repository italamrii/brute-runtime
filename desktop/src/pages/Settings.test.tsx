import { afterEach, describe, expect, it } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { AppStatusProvider } from "../lib/AppStatusContext";
import { Settings } from "./Settings";

function renderSettings(lang: "en" | "ar") {
  window.localStorage.setItem("brute.language", lang);
  return render(
    <I18nProvider>
      <AppStatusProvider>
        <Settings />
      </AppStatusProvider>
    </I18nProvider>,
  );
}

describe("Settings page — About panel", () => {
  afterEach(() => {
    cleanup();
    window.localStorage.clear();
  });

  it("shows the product name, logo, and privacy slogan in English", async () => {
    renderSettings("en");
    expect(await screen.findByText("BRUTE Runtime")).toBeInTheDocument();
    expect(screen.getByText("Your data never leaves your device.")).toBeInTheDocument();
    const logo = screen.getByAltText(/BRUTE Runtime/);
    expect(logo.tagName).toBe("IMG");
    expect(logo).toHaveClass("about-logo");
  });

  it("shows the product name, logo, and privacy slogan in Arabic", async () => {
    renderSettings("ar");
    expect(await screen.findAllByText("BRUTE Runtime")).not.toHaveLength(0);
    expect(screen.getByText("بياناتك ما تطلع من جهازك.")).toBeInTheDocument();
  });

  it("shows the beta description in English", async () => {
    renderSettings("en");
    await screen.findByText("BRUTE Runtime");
    expect(screen.getByText("Free beta running locally on your device")).toBeInTheDocument();
    expect(screen.getByText("No accounts, no tracking, and no data uploads")).toBeInTheDocument();
  });

  it("shows the beta description in Arabic", async () => {
    renderSettings("ar");
    await screen.findAllByText("BRUTE Runtime");
    expect(screen.getByText("نسخة تجريبية مجانية تعمل محليًا على جهازك")).toBeInTheDocument();
    expect(screen.getByText("لا حسابات، لا تتبع، ولا رفع للبيانات")).toBeInTheDocument();
  });

  it("shows version, network, privacy, and signing as scannable rows in English", async () => {
    renderSettings("en");
    await screen.findByText("BRUTE Runtime");
    expect(screen.getByText("Version")).toBeInTheDocument();
    expect(screen.getByText("0.1.0")).toBeInTheDocument();
    expect(screen.getByText("Network")).toBeInTheDocument();
    expect(screen.getByText("No network activity")).toBeInTheDocument();
    expect(screen.getByText("Privacy")).toBeInTheDocument();
    expect(screen.getByText("Local only")).toBeInTheDocument();
    expect(screen.getByText("Signing")).toBeInTheDocument();
    expect(screen.getByText("This beta is not digitally signed yet.")).toBeInTheDocument();
  });

  it("shows version, network, privacy, and signing as scannable rows in Arabic", async () => {
    renderSettings("ar");
    await screen.findAllByText("BRUTE Runtime");
    expect(screen.getByText("الإصدار")).toBeInTheDocument();
    expect(screen.getByText("0.1.0")).toBeInTheDocument();
    expect(screen.getByText("الشبكة")).toBeInTheDocument();
    expect(screen.getByText("لا يوجد نشاط شبكة")).toBeInTheDocument();
    expect(screen.getByText("الخصوصية")).toBeInTheDocument();
    expect(screen.getByText("محلي فقط")).toBeInTheDocument();
    expect(screen.getByText("التوقيع")).toBeInTheDocument();
    expect(screen.getByText("نسخة تجريبية غير موقعة رقميًا حاليًا.")).toBeInTheDocument();
  });

  it("shows the author credit, subtly, in English", async () => {
    renderSettings("en");
    await screen.findByText("BRUTE Runtime");
    const credit = screen.getByText("By Engineer Abdullah Alamri");
    expect(credit).toBeInTheDocument();
    expect(credit).toHaveClass("about-credit");
  });

  it("shows the author credit, subtly, in Arabic", async () => {
    renderSettings("ar");
    await screen.findAllByText("BRUTE Runtime");
    const credit = screen.getByText("بواسطة المهندس عبدالله العمري");
    expect(credit).toBeInTheDocument();
    expect(credit).toHaveClass("about-credit");
  });

  it("renders the page under an RTL container in Arabic and LTR in English", async () => {
    const { container } = renderSettings("ar");
    await screen.findAllByText("BRUTE Runtime");
    expect(container.querySelector('[dir="rtl"]')).toBeInTheDocument();
    cleanup();

    const { container: enContainer } = renderSettings("en");
    await screen.findByText("BRUTE Runtime");
    expect(enContainer.querySelector('[dir="ltr"]')).toBeInTheDocument();
  });

  it("keeps technical values (version, local-state path) forced ltr inside the Arabic RTL page", async () => {
    renderSettings("ar");
    await screen.findAllByText("BRUTE Runtime");

    const version = screen.getByText("0.1.0");
    expect(version).toHaveAttribute("dir", "ltr");

    const localStatePath = screen.getByText("%LOCALAPPDATA%\\BruteRuntime\\");
    expect(localStatePath).toHaveAttribute("dir", "ltr");

    const libraryInput = screen.getByLabelText("موقع المكتبة الافتراضي") as HTMLInputElement;
    expect(libraryInput).toHaveAttribute("dir", "ltr");
    expect(libraryInput.value).toMatch(/BruteRuntime/);
  });
});
