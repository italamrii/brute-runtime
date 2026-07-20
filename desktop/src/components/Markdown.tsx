import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Light as SyntaxHighlighter } from "react-syntax-highlighter";
import jsLang from "react-syntax-highlighter/dist/esm/languages/hljs/javascript";
import tsLang from "react-syntax-highlighter/dist/esm/languages/hljs/typescript";
import pyLang from "react-syntax-highlighter/dist/esm/languages/hljs/python";
import rustLang from "react-syntax-highlighter/dist/esm/languages/hljs/rust";
import bashLang from "react-syntax-highlighter/dist/esm/languages/hljs/bash";
import jsonLang from "react-syntax-highlighter/dist/esm/languages/hljs/json";
import cssLang from "react-syntax-highlighter/dist/esm/languages/hljs/css";
import xmlLang from "react-syntax-highlighter/dist/esm/languages/hljs/xml";
import sqlLang from "react-syntax-highlighter/dist/esm/languages/hljs/sql";
import cLang from "react-syntax-highlighter/dist/esm/languages/hljs/c";
import cppLang from "react-syntax-highlighter/dist/esm/languages/hljs/cpp";
import { atomOneDark } from "react-syntax-highlighter/dist/esm/styles/hljs";
import { openUrl } from "@tauri-apps/plugin-opener";
import { checkUrlSafety } from "../lib/urlSafety";

SyntaxHighlighter.registerLanguage("javascript", jsLang);
SyntaxHighlighter.registerLanguage("typescript", tsLang);
SyntaxHighlighter.registerLanguage("python", pyLang);
SyntaxHighlighter.registerLanguage("rust", rustLang);
SyntaxHighlighter.registerLanguage("bash", bashLang);
SyntaxHighlighter.registerLanguage("json", jsonLang);
SyntaxHighlighter.registerLanguage("css", cssLang);
SyntaxHighlighter.registerLanguage("xml", xmlLang);
SyntaxHighlighter.registerLanguage("html", xmlLang);
SyntaxHighlighter.registerLanguage("sql", sqlLang);
SyntaxHighlighter.registerLanguage("c", cLang);
SyntaxHighlighter.registerLanguage("cpp", cppLang);

/**
 * A code fence's copy button - no dependency, just the Clipboard API
 * (already same-origin-safe, no network involved).
 */
function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="md-copy-btn"
      onClick={() => {
        void navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? "✓" : label}
    </button>
  );
}

/**
 * Renders Markdown as real React elements (react-markdown never uses
 * dangerouslySetInnerHTML, so there is no HTML-injection surface from
 * model output). Every link is intercepted here rather than left as a
 * normal anchor: it is validated by the same checkUrlSafety() used for
 * the Discover Models "official source" flow, and opened via the
 * system browser (a separate OS process) - never a WebView navigation,
 * never a new-window anchor target. A link that fails the safety check
 * is rendered as inert text instead of a clickable element.
 */
export function Markdown({ content, copyLabel }: { content: string; copyLabel: string }) {
  return (
    <div className="md-body">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
        a: ({ href, children }) => {
          const result = href ? checkUrlSafety(href) : { safe: false };
          if (!result.safe) {
            return <span className="md-unsafe-link">{children}</span>;
          }
          return (
            <a
              href={href}
              onClick={(e) => {
                e.preventDefault();
                void openUrl(href!);
              }}
            >
              {children}
            </a>
          );
        },
        code: ({ className, children, ...props }) => {
          const match = /language-(\w+)/.exec(className ?? "");
          const isBlock = Boolean(match);
          const text = String(children).replace(/\n$/, "");
          if (!isBlock) {
            return (
              <code className={className} {...props}>
                {children}
              </code>
            );
          }
          return (
            <div className="md-code-block">
              <div className="md-code-block-header">
                <span className="md-code-lang">{match?.[1] ?? "text"}</span>
                <CopyButton text={text} label={copyLabel} />
              </div>
              <SyntaxHighlighter language={match?.[1]} style={atomOneDark} customStyle={{ margin: 0 }}>
                {text}
              </SyntaxHighlighter>
            </div>
          );
        },
      }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
