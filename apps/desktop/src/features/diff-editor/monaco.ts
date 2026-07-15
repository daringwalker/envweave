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

export { monaco };
