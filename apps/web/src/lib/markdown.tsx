import React, { memo, useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import remend from "remend";
import type { HLJSApi } from "highlight.js";
let highlighterPromise: Promise<HLJSApi> | undefined;
import { CursorIcon } from "@grok-bot/theme";

export const CodeBlock = memo(function CodeBlock({ lang, code }: { lang: string; code: string }) {
  const [highlighter, setHighlighter] = useState<HLJSApi | null>(null);
  useEffect(() => {
    let mounted = true;
    highlighterPromise ??= import("highlight.js/lib/common").then(module => module.default);
    void highlighterPromise.then(api => { if (mounted) setHighlighter(api); }).catch(() => { highlighterPromise = undefined; });
    return () => { mounted = false; };
  }, []);
  const [copyState, setCopyState] = useState("Copy");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  // Never auto-detect languages or highlight unbounded payloads on the main thread.
  const highlighted = useMemo(() => {
    if (code.length > 50_000 || !lang || !highlighter?.getLanguage(lang)) return null;
    try { return highlighter.highlight(code, { language: lang, ignoreIllegals: true }).value; } catch { return null; }
  }, [code, lang, highlighter]);
  const copy = async () => {
    try { await navigator.clipboard.writeText(code); setCopyState("Copied"); }
    catch { setCopyState("Copy failed"); }
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopyState("Copy"), 1600);
  };
  return <div className="gb-codeblock">
    <div className="gb-codeblock__bar"><span>{lang || "text"}</span>
      <button type="button" className="gb-btn gb-btn--ghost gb-btn--sm" onClick={() => void copy()} aria-label="Copy code">
        <CursorIcon name={copyState === "Copied" ? "check" : "copy"} size={12} /><span aria-live="polite">{copyState}</span>
      </button>
    </div>
    <pre>{highlighted === null ? <code>{code}</code> : <code dangerouslySetInnerHTML={{ __html: highlighted }} />}</pre>
  </div>;
});

const components: Components = {
  pre({ children }) {
    const child = React.Children.toArray(children)[0];
    if (React.isValidElement<{ className?: string; children?: React.ReactNode }>(child)) {
      return <CodeBlock lang={/language-([^\s]+)/.exec(child.props.className ?? "")?.[1] ?? ""} code={String(child.props.children ?? "").replace(/\n$/, "")} />;
    }
    return <pre>{children}</pre>;
  },
  code({ children, className }) { return <code className={className ?? "gb-inline-code"}>{children}</code>; },
  table({ children }) { return <div className="gb-markdown-table" tabIndex={0} role="region" aria-label="Response table"><table>{children}</table></div>; },
  a({ href, children }) { return href ? <a href={href} target="_blank" rel="noopener noreferrer">{children}</a> : <span>{children}</span>; },
  img({ src, alt }) { return src ? <img src={src} alt={alt ?? ""} loading="lazy" /> : <span>{alt}</span>; },
};
const plugins = [remarkGfm];
export const MarkdownBody = memo(function MarkdownBody({ content, streaming = false }: { content: string; streaming?: boolean }) {
  const markdown = useMemo(() => streaming ? remend(content, { katex: false }) : content, [content, streaming]);
  return <div className="gb-markdown"><ReactMarkdown remarkPlugins={plugins} components={components} skipHtml>{markdown}</ReactMarkdown></div>;
});
