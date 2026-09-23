export interface ExpandedSnippet {
  text: string;
  selectionFrom: number;
  selectionTo: number;
}

interface Placeholder {
  number: number;
  from: number;
  to: number;
}

export function expandCatalogSnippet(
  snippet: string,
  selectedText: string,
): ExpandedSnippet {
  const pattern = /\$\{([1-9]\d*):([^}]*)\}|\$0/g;
  const placeholders: Placeholder[] = [];
  let text = "";
  let source = 0;
  let finalCursor: number | null = null;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(snippet))) {
    text += snippet.slice(source, match.index);
    if (match[0] === "$0") {
      finalCursor ??= text.length;
    } else {
      const number = Number(match[1]);
      const value =
        number === 1 && selectedText.length > 0
          ? selectedText
          : (match[2] ?? "");
      const from = text.length;
      text += value;
      placeholders.push({ number, from, to: text.length });
    }
    source = pattern.lastIndex;
  }
  text += snippet.slice(source);

  const first = placeholders.sort(
    (left, right) => left.number - right.number,
  )[0];
  return {
    text,
    selectionFrom: first?.from ?? finalCursor ?? text.length,
    selectionTo: first?.to ?? finalCursor ?? text.length,
  };
}
