import * as monaco from "monaco-editor/esm/vs/editor/editor.api.js";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import "monaco-editor/esm/vs/basic-languages/shell/shell.contribution.js";

const workerHost = self as typeof self & {
  MonacoEnvironment: { getWorker(workerId: string, label: string): Worker };
};

workerHost.MonacoEnvironment = {
  getWorker(_workerId: string, _label: string) {
    return new EditorWorker();
  },
};

monaco.editor.defineTheme("envweave-light", {
  base: "vs",
  inherit: true,
  rules: [],
  colors: {
    "editor.background": "#FCFDFC",
    "editor.foreground": "#223029",
    "editorGutter.background": "#F7F9F7",
    "editorLineNumber.foreground": "#5F6E65",
    "editorLineNumber.activeForeground": "#405048",
    "editor.lineHighlightBackground": "#F2F7F3",
    "editor.selectionBackground": "#BDE4CA",
    "editor.inactiveSelectionBackground": "#DCEDE1",
    "diffEditor.insertedLineBackground": "#DFF3E640",
    "diffEditor.insertedTextBackground": "#9BD7AE70",
    "diffEditor.removedLineBackground": "#FCE7E640",
    "diffEditor.removedTextBackground": "#E9A8A470",
    "diffEditor.border": "#D9E2DC",
  },
});

monaco.editor.defineTheme("envweave-dark", {
  base: "vs-dark",
  inherit: true,
  rules: [],
  colors: {
    "editor.background": "#111914",
    "editor.foreground": "#DDE8E1",
    "editorGutter.background": "#141D18",
    "editorLineNumber.foreground": "#91A198",
    "editorLineNumber.activeForeground": "#B9C8BF",
    "editor.lineHighlightBackground": "#1B2821",
    "editor.selectionBackground": "#315F43",
    "editor.inactiveSelectionBackground": "#294735",
    "diffEditor.insertedLineBackground": "#234B324D",
    "diffEditor.insertedTextBackground": "#34794C80",
    "diffEditor.removedLineBackground": "#4D29284D",
    "diffEditor.removedTextBackground": "#8A454580",
    "diffEditor.border": "#33443A",
  },
});

export { monaco };
