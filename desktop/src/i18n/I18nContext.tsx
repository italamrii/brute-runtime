import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { strings, type Lang } from "./strings";

const STORAGE_KEY = "brute.language";

function loadInitialLang(): Lang {
  const stored = window.localStorage.getItem(STORAGE_KEY);
  return stored === "ar" || stored === "en" ? stored : "en";
}

interface I18nValue {
  lang: Lang;
  dir: "ltr" | "rtl";
  setLang: (lang: Lang) => void;
  t: (key: string) => string;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(loadInitialLang);
  const dir = lang === "ar" ? "rtl" : "ltr";

  const setLang = (next: Lang) => {
    setLangState(next);
    window.localStorage.setItem(STORAGE_KEY, next);
  };

  const value = useMemo<I18nValue>(
    () => ({
      lang,
      dir,
      setLang,
      t: (key: string) => strings[lang][key] ?? strings.en[key] ?? key,
    }),
    [lang, dir],
  );

  return (
    <I18nContext.Provider value={value}>
      <div dir={dir} lang={lang}>
        {children}
      </div>
    </I18nContext.Provider>
  );
}

export function useI18n(): I18nValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used within I18nProvider");
  return ctx;
}
