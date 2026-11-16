import { HighlightStyle, LanguageDescription, codeFolding, foldGutter, syntaxHighlighting } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView, highlightSpecialChars, lineNumbers } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";
import { useEffect, useRef } from "react";

/** Token colours come from the theme (`--syn-*`), so one style serves every theme (as in Tableboat). */
const syntax = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.modifier, t.controlKeyword], color: "var(--syn-keyword)" },
  { tag: [t.typeName, t.className, t.namespace], color: "var(--syn-type)" },
  { tag: [t.standard(t.name), t.function(t.variableName), t.function(t.propertyName), t.macroName], color: "var(--syn-function)" },
  { tag: [t.string, t.special(t.string), t.regexp], color: "var(--syn-string)" },
  { tag: [t.number], color: "var(--syn-number)" },
  { tag: [t.bool, t.null, t.atom], color: "var(--syn-literal)" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.variableName, t.propertyName, t.special(t.name), t.attributeName], color: "var(--syn-property)" },
  { tag: [t.tagName], color: "var(--syn-keyword)" },
  { tag: [t.heading], color: "var(--syn-function)", fontWeight: "700" },
  { tag: [t.emphasis], fontStyle: "italic" },
  { tag: [t.strong], fontWeight: "700" },
  { tag: [t.link, t.url], color: "var(--syn-string)", textDecoration: "underline" },
  { tag: [t.meta, t.processingInstruction], color: "var(--syn-comment)" },
  { tag: [t.operator, t.punctuation, t.bracket, t.separator], color: "var(--syn-operator)" },
  { tag: t.invalid, color: "var(--syn-invalid)" },
]);

const chrome = EditorView.theme({
  "&": { height: "100%", backgroundColor: "transparent", color: "var(--syn-operator)", fontSize: "var(--fs-sm)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.55" },
  ".cm-content": { padding: "6px 0" },
  ".cm-gutters": { backgroundColor: "var(--bg-sidebar)", color: "var(--editor-gutter)", border: "none" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 8px 0 12px" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--selection)",
  },
  ".cm-foldPlaceholder": { backgroundColor: "transparent", border: "none", color: "var(--text-3)" },
  ".cm-foldGutter .cm-gutterElement": { color: "var(--text-3)", cursor: "pointer" },
});

/** The language for a file name ("main.rs", "Dockerfile", "package.json"), if CodeMirror has one. */
function describe(name: string): LanguageDescription | null {
  return LanguageDescription.matchFilename(languages, name);
}

interface Props {
  text: string;
  /** The file name, to pick the language. */
  name: string;
  wrap: boolean;
}

/** Read-only code with highlighting, line numbers and folding; languages load when first needed. */
export function CodeView({ text, name, wrap }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const language = useRef(new Compartment());
  const wrapping = useRef(new Compartment());

  useEffect(() => {
    const v = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: "",
        extensions: [
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          lineNumbers(),
          codeFolding(),
          foldGutter(),
          highlightSpecialChars(),
          syntaxHighlighting(syntax),
          chrome,
          language.current.of([]),
          wrapping.current.of([]),
        ],
      }),
    });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
  }, []);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } });
  }, [text]);

  useEffect(() => {
    let stale = false;
    const v = view.current;
    if (!v) return;
    const desc = describe(name);
    if (!desc) {
      v.dispatch({ effects: language.current.reconfigure([]) });
      return;
    }
    void desc.load().then((support) => {
      if (!stale && view.current === v) v.dispatch({ effects: language.current.reconfigure(support) });
    });
    return () => {
      stale = true;
    };
  }, [name]);

  useEffect(() => {
    view.current?.dispatch({ effects: wrapping.current.reconfigure(wrap ? EditorView.lineWrapping : []) });
  }, [wrap]);

  return <div className="pv-code" ref={host} />;
}

export default CodeView;
