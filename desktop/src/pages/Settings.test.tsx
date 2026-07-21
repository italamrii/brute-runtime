import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { AppStatusProvider } from "../lib/AppStatusContext";
import { Settings } from "./Settings";
import type { Preferences } from "../lib/types";

vi.mock("../lib/api", () => ({
  resolveRuntime: vi.fn(),
  getPreferences: vi.fn(),
  savePreferences: vi.fn(),
  resetPreferences: vi.fn(),
}));

import { getPreferences, resetPreferences, resolveRuntime, savePreferences } from "../lib/api";

function defaultPreferences(): Preferences {
  return {
    schema_version: "preferences-v1",
    language: "both",
    use_case: "general_assistant",
    priority: "balanced",
    arabic_priority: false,
    english_priority: false,
    memory_conservative_mode: false,
    cpu_only: false,
    gpu_preference: "no_preference",
    offline_only: false,
    permitted_licenses: [],
    commercial_use_required: false,
    preferred_families: [],
    excluded_families: [],
    max_download_size_bytes: null,
    max_ram_bytes: null,
    max_vram_bytes: null,
    updated_at_rfc3339: "2026-01-01T00:00:00Z",
  };
}

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
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(resolveRuntime).mockResolvedValue({
      source: "bundled",
      binary_dir: "C:\\Program Files\\BRUTE Runtime\\runtime\\cpu",
      cli_verified: true,
      bench_verified: true,
      detail: "Bundled runtime verified.",
    });
    vi.mocked(getPreferences).mockResolvedValue(defaultPreferences());
    vi.mocked(savePreferences).mockResolvedValue(undefined);
    vi.mocked(resetPreferences).mockResolvedValue(defaultPreferences());
  });

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

describe("Settings page — local preference profile", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(resolveRuntime).mockResolvedValue({
      source: "bundled",
      binary_dir: "C:\\Program Files\\BRUTE Runtime\\runtime\\cpu",
      cli_verified: true,
      bench_verified: true,
      detail: "Bundled runtime verified.",
    });
    vi.mocked(getPreferences).mockResolvedValue(defaultPreferences());
    vi.mocked(savePreferences).mockResolvedValue(undefined);
    vi.mocked(resetPreferences).mockResolvedValue(defaultPreferences());
  });

  afterEach(() => {
    cleanup();
    window.localStorage.clear();
  });

  it("loads and shows the real saved preferences, not fabricated defaults", async () => {
    vi.mocked(getPreferences).mockResolvedValue({ ...defaultPreferences(), language: "arabic", use_case: "coding" });
    renderSettings("en");
    const languageSelect = (await screen.findByLabelText("Model language")) as HTMLSelectElement;
    await waitFor(() => expect(languageSelect.value).toBe("arabic"));
    expect((screen.getByLabelText("Use") as HTMLSelectElement).value).toBe("coding");
  });

  it("saves immediately when a simple preference changes", async () => {
    renderSettings("en");
    const prioritySelect = (await screen.findByLabelText("Priority")) as HTMLSelectElement;
    fireEvent.change(prioritySelect, { target: { value: "fastest" } });

    await waitFor(() => expect(savePreferences).toHaveBeenCalled());
    const saved = vi.mocked(savePreferences).mock.calls[0][0];
    expect(saved.priority).toBe("fastest");
  });

  it("keeps advanced preferences hidden until explicitly shown", async () => {
    renderSettings("en");
    await screen.findByLabelText("Model language");
    expect(screen.queryByLabelText(/CPU-only/)).not.toBeInTheDocument();

    fireEvent.click(screen.getByText("Show advanced preferences"));
    expect(await screen.findByLabelText(/CPU-only/)).toBeInTheDocument();
    expect(screen.getByLabelText(/Maximum RAM to use/)).toBeInTheDocument();
  });

  it("parses comma-separated family/license lists into arrays", async () => {
    renderSettings("en");
    await screen.findByLabelText("Model language");
    fireEvent.click(screen.getByText("Show advanced preferences"));

    const licensesInput = await screen.findByLabelText(/Permitted licenses/);
    fireEvent.change(licensesInput, { target: { value: "apache-2.0, mit ,  mit" } });

    await waitFor(() => expect(savePreferences).toHaveBeenCalled());
    const saved = vi.mocked(savePreferences).mock.calls[0][0];
    expect(saved.permitted_licenses).toEqual(["apache-2.0", "mit", "mit"]);
  });

  it("converts GB input to bytes for memory/download limits", async () => {
    renderSettings("en");
    await screen.findByLabelText("Model language");
    fireEvent.click(screen.getByText("Show advanced preferences"));

    const maxRamInput = await screen.findByLabelText(/Maximum RAM to use/);
    fireEvent.change(maxRamInput, { target: { value: "8" } });

    await waitFor(() => expect(savePreferences).toHaveBeenCalled());
    const saved = vi.mocked(savePreferences).mock.calls[0][0];
    expect(saved.max_ram_bytes).toBe(8_000_000_000);
  });

  it("resets to defaults after confirmation", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(getPreferences).mockResolvedValue({ ...defaultPreferences(), cpu_only: true });
    renderSettings("en");
    await screen.findByLabelText("Model language");
    fireEvent.click(screen.getByText("Show advanced preferences"));
    await screen.findByLabelText(/CPU-only/);

    fireEvent.click(screen.getByText("Reset preferences to defaults"));
    await waitFor(() => expect(resetPreferences).toHaveBeenCalled());
    confirmSpy.mockRestore();
  });

  it("does not reset without confirmation", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    renderSettings("en");
    await screen.findByLabelText("Model language");
    fireEvent.click(screen.getByText("Show advanced preferences"));
    await screen.findByLabelText(/CPU-only/);

    fireEvent.click(screen.getByText("Reset preferences to defaults"));
    expect(resetPreferences).not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  it("keeps license/family text inputs and number inputs forced ltr in the Arabic RTL page", async () => {
    renderSettings("ar");
    await screen.findByLabelText("لغة النموذج");
    fireEvent.click(screen.getByText("إظهار التفضيلات المتقدمة"));

    const licensesInput = await screen.findByLabelText(/التراخيص المسموحة/);
    expect(licensesInput).toHaveAttribute("dir", "ltr");
    const maxRamInput = screen.getByLabelText(/أقصى استخدام للذاكرة/);
    expect(maxRamInput).toHaveAttribute("dir", "ltr");
  });
});
