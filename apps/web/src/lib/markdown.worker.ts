import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkRehype from "remark-rehype";
import remarkGfm from "remark-gfm";
import hljs from "highlight.js/lib/common";
import remend from "remend";

const processor = unified().use(remarkParse).use(remarkGfm).use(remarkRehype).freeze();

export type RenderRequest =
  | { kind: "markdown"; content: string; streaming: boolean }
  | { kind: "highlight"; code: string; language: string };

self.onmessage = (event: MessageEvent<{ id: number; request: RenderRequest }>) => {
  const { id, request } = event.data;
  try {
    let result: string[] | string | null;
    if (request.kind === "highlight") {
      const { code, language } = request;
      result = hljs.getLanguage(language) && code.length <= 50_000
        ? hljs.highlight(code, { language, ignoreIllegals: true }).value : null;
    } else {
      const input = request.streaming ? remend(request.content, { katex: false }) : request.content;
      const tree = processor.runSync(processor.parse(input));
      // Positions change as text arrives. Excluding them lets the UI retain
      // completed blocks even when references require a fresh parse.
      result = tree.children
        .filter(block => block.type !== "text" || block.value.trim())
        .map(block => JSON.stringify(block, (key, value) => key === "position" ? undefined : value));
    }
    self.postMessage({ id, result });
  } catch {
    self.postMessage({ id, result: null });
  }
};
