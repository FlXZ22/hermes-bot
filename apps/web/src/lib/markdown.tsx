import React, { useState } from "react";
import { CursorIcon } from "@grok-bot/theme";

interface CodeBlockProps {
  lang: string;
  code: string;
}

const CodeBlock: React.FC<CodeBlockProps> = ({ lang, code }) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard?.writeText(code).catch(() => undefined);
    setCopied(true);
    setTimeout(() => setCopied(false), 1600);
  };

  return (
    <div className="gb-codeblock">
      <div className="gb-codeblock__bar">
        <span>{lang || "text"}</span>
        <button type="button" className="gb-btn gb-btn--ghost gb-btn--sm" onClick={handleCopy}>
          <CursorIcon name={copied ? "check" : "copy"} size={12} />
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre>
        <code>{code}</code>
      </pre>
    </div>
  );
};

function renderInline(text: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  const regex = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*|\[([^\]]+)\]\((https?:\/\/[^\s)]+)\))/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let key = 0;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    if (token.startsWith("`")) {
      nodes.push(
        <code key={key++} className="gb-inline-code">
          {token.slice(1, -1)}
        </code>
      );
    } else if (token.startsWith("**")) {
      nodes.push(<strong key={key++}>{token.slice(2, -2)}</strong>);
    } else if (token.startsWith("*")) {
      nodes.push(<em key={key++}>{token.slice(1, -1)}</em>);
    } else if (match[2] && match[3]) {
      nodes.push(
        <a key={key++} href={match[3]} target="_blank" rel="noreferrer noopener">
          {match[2]}
        </a>
      );
    }
    lastIndex = match.index + token.length;
  }
  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }
  return nodes;
}

export const MarkdownBody: React.FC<{ content: string }> = React.memo(({ content }) => {
  if (!content) return null;

  const blocks: React.ReactNode[] = [];
  const lines = content.replace(/\r\n/g, "\n").split("\n");
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block
    if (line.trimStart().startsWith("```")) {
      const lang = line.trimStart().slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      if (i < lines.length) i++;
      blocks.push(<CodeBlock key={key++} lang={lang} code={codeLines.join("\n")} />);
      continue;
    }

    // Blank line
    if (!line.trim()) {
      i++;
      continue;
    }

    // Headings
    const headingMatch = /^(#{1,3})\s+(.+)$/.exec(line);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const body = renderInline(headingMatch[2]);
      if (level === 1) blocks.push(<h1 key={key++}>{body}</h1>);
      else if (level === 2) blocks.push(<h2 key={key++}>{body}</h2>);
      else blocks.push(<h3 key={key++}>{body}</h3>);
      i++;
      continue;
    }

    // Blockquote
    if (line.trimStart().startsWith("> ")) {
      const quoteLines: string[] = [];
      while (i < lines.length && lines[i].trimStart().startsWith("> ")) {
        quoteLines.push(lines[i].trimStart().slice(2));
        i++;
      }
      blocks.push(<blockquote key={key++}>{renderInline(quoteLines.join(" "))}</blockquote>);
      continue;
    }

    // Table
    if (line.includes("|") && i + 1 < lines.length && /^\s*\|?[\s:-]+\|/.test(lines[i + 1])) {
      const parseRow = (r: string) =>
        r
          .trim()
          .replace(/^\|/, "")
          .replace(/\|$/, "")
          .split("|")
          .map((c) => c.trim());
      const headers = parseRow(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
        rows.push(parseRow(lines[i]));
        i++;
      }
      blocks.push(
        <table key={key++}>
          <thead>
            <tr>
              {headers.map((h, idx) => (
                <th key={idx}>{renderInline(h)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, rIdx) => (
              <tr key={rIdx}>
                {r.map((cell, cIdx) => (
                  <td key={cIdx}>{renderInline(cell)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
      continue;
    }

    // Unordered list
    if (/^\s*[-*+]\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*[-*+]\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*[-*+]\s+/, ""));
        i++;
      }
      blocks.push(
        <ul key={key++}>
          {items.map((it, idx) => (
            <li key={idx}>{renderInline(it)}</li>
          ))}
        </ul>
      );
      continue;
    }

    // Ordered list
    if (/^\s*\d+\.\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*\d+\.\s+/, ""));
        i++;
      }
      blocks.push(
        <ol key={key++}>
          {items.map((it, idx) => (
            <li key={idx}>{renderInline(it)}</li>
          ))}
        </ol>
      );
      continue;
    }

    // Paragraph
    const paraLines: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !lines[i].trimStart().startsWith("```") &&
      !/^#{1,3}\s+/.test(lines[i]) &&
      !/^\s*[-*+]\s+/.test(lines[i]) &&
      !/^\s*\d+\.\s+/.test(lines[i]) &&
      !lines[i].trimStart().startsWith("> ")
    ) {
      paraLines.push(lines[i]);
      i++;
    }
    blocks.push(<p key={key++}>{renderInline(paraLines.join("\n"))}</p>);
  }

  return <>{blocks}</>;
});

MarkdownBody.displayName = "MarkdownBody";
