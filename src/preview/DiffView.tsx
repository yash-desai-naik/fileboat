import { MergeView, unifiedMergeView } from "@codemirror/merge";
import { codeFolding, syntaxHighlighting } from "@codemirror/language";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView, highlightSpecialChars, lineNumbers } from "@codemirror/view";
import { useEffect, useRef } from "react";
import { chrome, describe, syntax } from "./CodeView";

interface Props {
  before: string;
  after: string;
  /** The file name, for the language. */
  name: string;
  /** Side by side, or the changes inline in one editor. */
  split: boolean;
}

const diffTheme = EditorView.theme({
  ".cm-changedLine, .cm-insertedLine": { backgroundColor: "var(--edit-added)" },
  ".cm-deletedChunk, .cm-deletedLine": { backgroundColor: "var(--edit-deleted)" },
  ".cm-changedText": { background: "none", backgroundColor: "color-mix(in srgb, var(--git-added) 30%, transparent)" },
  ".cm-deletedChunk .cm-deletedText, del": { backgroundColor: "color-mix(in srgb, var(--git-deleted) 30%, transparent)", textDecoration: "none" },
  ".cm-collapsedLines": { backgroundColor: "var(--bg-hover)", color: "var(--text-3)" },
  "&.cm-merge-a .cm-changedLine": { backgroundColor: "var(--edit-deleted)" },
});

function base(lang: Extension): Extension[] {
  return [
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    lineNumbers(),
    codeFolding(),
    highlightSpecialChars(),
    syntaxHighlighting(syntax),
    chrome,
    diffTheme,
    lang,
  ];
}

/** Read-only diff of two versions of a file (CodeMirror's merge view), unchanged runs collapsed. */
export function DiffView({ before, after, name, split }: Props) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let stale = false;
    let destroy = () => {};
    const build = (lang: Extension) => {
      if (stale || !host.current) return;
      destroy();
      host.current.replaceChildren();
      const collapse = { margin: 3, minSize: 6 };
      if (split) {
        const view = new MergeView({
          a: { doc: before, extensions: base(lang) },
          b: { doc: after, extensions: base(lang) },
          parent: host.current,
          collapseUnchanged: collapse,
          highlightChanges: true,
          gutter: true,
        });
        destroy = () => view.destroy();
      } else {
        const view = new EditorView({
          parent: host.current,
          state: EditorState.create({
            doc: after,
            extensions: [...base(lang), unifiedMergeView({ original: before, mergeControls: false, collapseUnchanged: collapse, highlightChanges: true })],
          }),
        });
        destroy = () => view.destroy();
      }
    };
    // Plain first, highlighted once the language has loaded.
    build([]);
    const desc = describe(name);
    if (desc) void desc.load().then((support) => build(support));
    return () => {
      stale = true;
      destroy();
    };
  }, [before, after, name, split]);

  return <div className={`pv-diff${split ? " split" : ""}`} ref={host} />;
}

export default DiffView;
