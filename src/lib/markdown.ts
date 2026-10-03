/** Markdown minimal pour les réponses de l'assistant : tout est échappé avant d'être mis en forme. */

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Gras, italique, code et liens dans une ligne déjà échappée. */
function inline(text: string): string {
  // Le code d'abord, mis de côté : son contenu ne doit pas être reformaté.
  const codes: string[] = [];
  let out = text.replace(/`([^`\n]+)`/g, (_, c) => {
    codes.push(`<code>${c}</code>`);
    return `\u0000${codes.length - 1}\u0000`;
  });
  out = out
    .replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[\s(])\*([^*\s][^*\n]*)\*(?=[\s).,;:!?]|$)/g, "$1<em>$2</em>")
    .replace(/\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" data-url="$2">$1</a>');
  return out.replace(/\u0000(\d+)\u0000/g, (_, i) => codes[+i]);
}

/** Convertit du Markdown simple en HTML sûr (titres, listes, blocs de code, paragraphes). */
export function renderMarkdown(source: string): string {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const html: string[] = [];
  let paragraph: string[] = [];
  let list: { tag: "ul" | "ol"; items: string[] } | null = null;

  const flush = () => {
    if (paragraph.length) html.push(`<p>${paragraph.map(inline).join("<br>")}</p>`);
    if (list) html.push(`<${list.tag}>${list.items.map((i) => `<li>${inline(i)}</li>`).join("")}</${list.tag}>`);
    paragraph = [];
    list = null;
  };

  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const fence = raw.match(/^\s*```(.*)$/);
    if (fence) {
      flush();
      const code: string[] = [];
      // Bloc non refermé (réponse en cours d'écriture) : il va jusqu'à la fin.
      for (i++; i < lines.length && !/^\s*```\s*$/.test(lines[i]); i++) code.push(lines[i]);
      html.push(`<pre><code>${escape(code.join("\n"))}</code></pre>`);
      continue;
    }
    const line = escape(raw);
    const heading = line.match(/^(#{1,4})\s+(.*)$/);
    const bullet = line.match(/^\s*[-*•]\s+(.*)$/);
    const numbered = line.match(/^\s*\d+[.)]\s+(.*)$/);
    if (!line.trim()) {
      flush();
    } else if (heading) {
      flush();
      html.push(`<h4>${inline(heading[2])}</h4>`);
    } else if (bullet || numbered) {
      const tag = bullet ? "ul" : "ol";
      if (paragraph.length || (list && list.tag !== tag)) flush();
      list ??= { tag, items: [] };
      list.items.push((bullet ?? numbered)![1]);
    } else if (list && /^\s{2,}\S/.test(raw)) {
      // Suite indentée d'un élément de liste.
      list.items[list.items.length - 1] += " " + line.trim();
    } else {
      if (list) flush();
      paragraph.push(line);
    }
  }
  flush();
  return html.join("");
}
