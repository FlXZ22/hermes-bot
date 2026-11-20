import React, { memo, startTransition, useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import remend from "remend";
import { Fragment, jsx, jsxs } from "react/jsx-runtime";
import { toJsxRuntime } from "hast-util-to-jsx-runtime";
import { renderInWorker } from "./renderWorker";
import { CursorIcon } from "@grok-bot/theme";

const highlightCache = new Map<string, string>();

export const CodeBlock = memo(function CodeBlock({ lang, code }: { lang: string; code: string }) {
  const [result, setResult] = useState<{ code: string; lang: string; html: string }>();
  useEffect(() => {
    if (!lang || code.length > 50_000) return;
    const key = JSON.stringify([lang, code]);
    const cached = highlightCache.get(key);
    if (cached !== undefined) {
      setResult({ code, lang, html: cached });
      return;
    }
    // Wait for a short pause so growing code blocks stay cheap to display.
    let cancel: (() => void) | undefined;
    const timer = setTimeout(() => {
      cancel = renderInWorker({ kind: "highlight", code, language: lang }, html => {
        if (typeof html !== "string") return;
        if (html.length <= 150_000) {
          if (highlightCache.size >= 48) highlightCache.delete(highlightCache.keys().next().value!);
          highlightCache.set(key, html);
        }
        setResult({ code, lang, html });
      });
    }, 85);
    return () => { clearTimeout(timer); cancel?.(); };
  }, [code, lang]);
  const highlighted = result?.code === code && result.lang === lang ? result.html : null;
  const [copyState, setCopyState] = useState("Copy");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
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
  a({ href, children }) {
    if (href?.startsWith("#")) return <a href={href}>{children}</a>;
    const destination = externalUrl(href);
    return destination ? <a href={destination} target="_blank" rel="noopener noreferrer">{children}</a> : <span>{children}</span>;
  },
  img({ src, alt }) {
    const destination = externalUrl(src);
    return destination ? <RemoteImage key={destination} src={destination} alt={alt ?? ""} /> : <span>{alt}</span>;
  },
};
function externalUrl(value?: string) {
  if (!value) return undefined;
  try {
    const url = new URL(value);
    return (url.protocol === "https:" || url.protocol === "http:") && !url.username && !url.password
      ? url.href : undefined;
  } catch { return undefined; }
}

function RemoteImage({ src, alt }: { src: string; alt: string }) {
  const [loaded, setLoaded] = useState(false);
  return loaded
    ? <img src={src} alt={alt} loading="lazy" referrerPolicy="no-referrer" />
    : <button type="button" className="gb-btn gb-btn--ghost gb-btn--sm" onClick={() => setLoaded(true)}>
        Load image{alt ? `: ${alt}` : ""}
      </button>;
}

const MarkdownBlock = memo(function MarkdownBlock({ serialized }: { serialized: string }) {
  return <div className="gb-markdown-block">{toJsxRuntime({ type: "root", children: [JSON.parse(serialized)] }, {
    Fragment, jsx, jsxs, components, passKeys: true,
  })}</div>;
});

const plugins = [remarkGfm];
const inlineLimit = 4096;
const previewLimit = 1_000_000;

export const MarkdownBody = memo(function MarkdownBody({ content, streaming = false }: { content: string; streaming?: boolean }) {
  const input = content.slice(0, previewLimit);
  const large = input.length > inlineLimit;
  const [blocks, setBlocks] = useState<string[] | null>(null);
  const [failed, setFailed] = useState(false);
  const latest = useRef({ kind: "markdown" as const, content: input, streaming });
  const requestNext = useRef<() => void>(() => {});
  latest.current = { kind: "markdown", content: input, streaming };
  useEffect(() => {
    if (!large) { setBlocks(null); return; }
    let disposed = false;
    let inFlight = false;
    let previous: typeof latest.current | undefined;
    let cancel: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => {
      if (disposed || inFlight || timer || (previous?.content === latest.current.content && previous.streaming === latest.current.streaming)) return;
      timer = setTimeout(() => {
        timer = undefined;
        inFlight = true;
        previous = latest.current;
        cancel = renderInWorker(previous, result => {
          inFlight = false;
          if (disposed) return;
          startTransition(() => {
            setFailed(!Array.isArray(result));
            if (Array.isArray(result)) setBlocks(result);
          });
          schedule();
        });
      }, latest.current.streaming ? 110 : 0);
    };
    requestNext.current = schedule;
    schedule();
    return () => {
      disposed = true; clearTimeout(timer); cancel?.();
      requestNext.current = () => {};
    };
  }, [large]);
  useEffect(() => { requestNext.current(); }, [input, streaming]);

  const inline = useMemo(() => {
    const preview = input.slice(0, inlineLimit);
    return streaming || large ? remend(preview, { katex: false }) : preview;
  }, [input, streaming, large]);

  return <div className="gb-markdown" data-chunked={large && blocks && !failed ? "true" : undefined}>
    {large && failed ? <div className="gb-markdown-fallback">{input}</div>
      : large && blocks
      ? blocks.map((block, index) => <MarkdownBlock key={index} serialized={block} />)
      : <ReactMarkdown remarkPlugins={plugins} components={components} skipHtml>{inline}</ReactMarkdown>}
    {content.length > previewLimit && <p>Preview limited. Use Copy message for the full response.</p>}
  </div>;
});
