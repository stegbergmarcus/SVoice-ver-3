export type InlinePart =
  | { kind: "text"; text: string }
  | { kind: "strong"; text: string };

export type ResponseBlock =
  | { kind: "paragraph"; parts: InlinePart[] }
  | { kind: "heading"; parts: InlinePart[] }
  | { kind: "list"; items: InlinePart[][] };

export function parseInline(text: string): InlinePart[] {
  const parts: InlinePart[] = [];
  const pattern = /\*\*([^*]+)\*\*/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push({ kind: "text", text: text.slice(lastIndex, match.index) });
    }
    parts.push({ kind: "strong", text: match[1] });
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < text.length) {
    parts.push({ kind: "text", text: text.slice(lastIndex) });
  }

  return parts.length > 0 ? parts : [{ kind: "text", text }];
}

function stripHeadingSyntax(line: string): string {
  const trimmed = line.trim();
  const heading = trimmed.match(/^#{1,4}\s+(.+)$/);
  if (heading) return heading[1].trim();

  const strongHeading = trimmed.match(/^\*\*([^*]+)\*\*:?$/);
  if (strongHeading) return strongHeading[1].trim();

  return trimmed;
}

export function parseActionResponse(text: string): ResponseBlock[] {
  const blocks: ResponseBlock[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let paragraph: string[] = [];
  let listItems: InlinePart[][] = [];

  const flushParagraph = () => {
    if (paragraph.length === 0) return;
    blocks.push({
      kind: "paragraph",
      parts: parseInline(paragraph.join(" ").trim()),
    });
    paragraph = [];
  };

  const flushList = () => {
    if (listItems.length === 0) return;
    blocks.push({ kind: "list", items: listItems });
    listItems = [];
  };

  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) {
      flushParagraph();
      flushList();
      continue;
    }

    const bullet = line.match(/^[-*•]\s+(.+)$/);
    if (bullet) {
      flushParagraph();
      listItems.push(parseInline(bullet[1].trim()));
      continue;
    }

    const looksLikeHeading =
      /^#{1,4}\s+/.test(line) ||
      (/^\*\*[^*]+\*\*:?$/.test(line) && line.length <= 80);
    if (looksLikeHeading) {
      flushParagraph();
      flushList();
      blocks.push({ kind: "heading", parts: parseInline(stripHeadingSyntax(line)) });
      continue;
    }

    flushList();
    paragraph.push(line);
  }

  flushParagraph();
  flushList();
  return blocks;
}
