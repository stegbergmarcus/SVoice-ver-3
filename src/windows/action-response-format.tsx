import type { ReactNode } from "react";
import { parseActionResponse, type InlinePart } from "./action-response-parser";

function renderParts(parts: InlinePart[], keyPrefix: string): ReactNode {
  return parts.map((part, i) =>
    part.kind === "strong" ? (
      <strong key={`${keyPrefix}-${i}`}>{part.text}</strong>
    ) : (
      <span key={`${keyPrefix}-${i}`}>{part.text}</span>
    ),
  );
}

export function FormattedActionResponse({ text }: { text: string }) {
  const blocks = parseActionResponse(text);

  return (
    <div className="action-popup-response-content">
      {blocks.map((block, i) => {
        if (block.kind === "heading") {
          return (
            <div key={i} className="action-popup-response-heading">
              {renderParts(block.parts, `h-${i}`)}
            </div>
          );
        }
        if (block.kind === "list") {
          return (
            <ul key={i} className="action-popup-response-list">
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>
                  {renderParts(item, `li-${i}-${itemIndex}`)}
                </li>
              ))}
            </ul>
          );
        }
        return (
          <p key={i} className="action-popup-response-paragraph">
            {renderParts(block.parts, `p-${i}`)}
          </p>
        );
      })}
    </div>
  );
}
