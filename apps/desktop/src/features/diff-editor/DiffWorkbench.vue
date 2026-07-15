<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { monaco } from "./monaco";
import { desktopApi } from "../../shared/api";
import { errorMessage } from "../../shared/errors";
import type { ConfigItemDto, DiffSessionDto, FileRevisionDto, TextDocumentDto } from "../../shared/bindings";

const LOCAL = "__local__";
const WORKTREE = "__worktree__";
const props = defineProps<{ repository: string; item: ConfigItemDto }>();
const emit = defineEmits<{ repositorySaved: []; dirtyChanged: [value: boolean] }>();

const host = ref<HTMLDivElement>();
const sideBySide = ref(true);
const ignoreWhitespace = ref(false);
const dirty = ref(false);
const savedMessage = ref("已加载");
const loading = ref(false);
const error = ref("");
const history = ref<FileRevisionDto[]>([]);
const leftVersion = ref(LOCAL);
const rightVersion = ref(WORKTREE);
const hasDifferences = ref<boolean>();
const session = ref<DiffSessionDto>();
const historyDocuments = new Map<string, TextDocumentDto>();
let worktreeDraft = "";
let comparisonSequence = 0;
let editor: monaco.editor.IStandaloneDiffEditor | undefined;
let original: monaco.editor.ITextModel | undefined;
let modified: monaco.editor.ITextModel | undefined;
let modifiedSubscription: monaco.IDisposable | undefined;
let diffSubscription: monaco.IDisposable | undefined;

const rightEditable = computed(() => rightVersion.value === WORKTREE);
const diffLabel = computed(() => {
  if (dirty.value) return "有未保存修改";
  if (loading.value) return "正在加载";
  if (hasDifferences.value === false) return "无差异";
  if (!rightEditable.value) return "历史版本只读对比";
  return savedMessage.value;
});
const leftTitle = computed(() => versionTitle(leftVersion.value, "本机当前文件"));
const rightTitle = computed(() => versionTitle(rightVersion.value, "仓库工作副本"));

onMounted(() => {
  if (!host.value) return;
  editor = monaco.editor.createDiffEditor(host.value, {
    automaticLayout: true,
    renderSideBySide: sideBySide.value,
    useInlineViewWhenSpaceIsLimited: true,
    renderSideBySideInlineBreakpoint: 700,
    originalEditable: false,
    readOnly: false,
    minimap: { enabled: false },
    renderOverviewRuler: true,
    renderMarginRevertIcon: true,
    colorDecorators: false,
    diffAlgorithm: "advanced",
    padding: { top: 12, bottom: 12 },
    fontSize: 13,
    scrollBeyondLastLine: false,
  });
  diffSubscription = editor.onDidUpdateDiff(() => {
    hasDifferences.value = (editor?.getLineChanges()?.length ?? 0) > 0;
  });
  loadDocuments();
});

watch(() => [props.repository, props.item.id], () => loadDocuments());
watch([leftVersion, rightVersion], (_next, previous) => {
  if (previous?.[1] === WORKTREE && modified) {
    worktreeDraft = modified.getValue();
  }
  renderComparison();
});
watch([sideBySide, ignoreWhitespace], () => {
  editor?.updateOptions({
    renderSideBySide: sideBySide.value,
    ignoreTrimWhitespace: ignoreWhitespace.value,
  });
});
watch(dirty, (value) => emit("dirtyChanged", value));

async function loadDocuments() {
  if (!editor) return;
  const sequence = ++comparisonSequence;
  loading.value = true;
  error.value = "";
  hasDifferences.value = undefined;
  try {
    const [opened, revisions] = await Promise.all([
      desktopApi.openDiff(props.repository, props.item.id),
      desktopApi.diffHistory(props.repository, props.item.id),
    ]);
    if (sequence !== comparisonSequence) return;
    session.value = opened;
    history.value = revisions;
    historyDocuments.clear();
    worktreeDraft = opened.repository.content;
    leftVersion.value = LOCAL;
    rightVersion.value = WORKTREE;
    dirty.value = false;
    savedMessage.value = "已加载";
    await renderComparison();
  } catch (reason) {
    if (sequence !== comparisonSequence) return;
    error.value = errorMessage(reason);
    editor.setModel(null);
  } finally {
    if (sequence === comparisonSequence) loading.value = false;
  }
}

async function historicalDocument(revision: string) {
  const key = `${props.repository}\u0000${props.item.id}\u0000${revision}`;
  const cached = historyDocuments.get(key);
  if (cached) return cached;
  const document = await desktopApi.openDiffRevision(props.repository, props.item.id, revision);
  historyDocuments.set(key, document);
  return document;
}

async function documentFor(version: string, side: "left" | "right") {
  if (!session.value) throw new Error("对比会话尚未加载");
  if (version === LOCAL) return session.value.local;
  if (version === WORKTREE) {
    return side === "right"
      ? { ...session.value.repository, content: worktreeDraft }
      : session.value.repository;
  }
  return historicalDocument(version);
}

async function renderComparison() {
  if (!editor || !session.value) return;
  const sequence = ++comparisonSequence;
  loading.value = true;
  error.value = "";
  hasDifferences.value = undefined;
  try {
    const [left, right] = await Promise.all([
      documentFor(leftVersion.value, "left"),
      documentFor(rightVersion.value, "right"),
    ]);
    if (sequence !== comparisonSequence) return;
    modifiedSubscription?.dispose();
    original?.dispose();
    modified?.dispose();
    original = monaco.editor.createModel(left.content, "shell", uniqueUri("left"));
    modified = monaco.editor.createModel(right.content, "shell", uniqueUri("right"));
    editor.updateOptions({ readOnly: !rightEditable.value });
    editor.setModel({ original, modified });
    modifiedSubscription = modified.onDidChangeContent(() => {
      if (rightVersion.value !== WORKTREE || !session.value || !modified) return;
      worktreeDraft = modified.getValue();
      dirty.value = worktreeDraft !== session.value.repository.content;
    });
    dirty.value = rightVersion.value === WORKTREE
      && worktreeDraft !== session.value.repository.content;
  } catch (reason) {
    if (sequence !== comparisonSequence) return;
    error.value = errorMessage(reason);
  } finally {
    if (sequence === comparisonSequence) loading.value = false;
  }
}

function uniqueUri(side: string) {
  return monaco.Uri.parse(`envweave://${side}/${props.item.id}?${Date.now()}-${Math.random()}`);
}

function versionTitle(value: string, fallback: string) {
  if (value === LOCAL || value === WORKTREE) return fallback;
  const revision = history.value.find((item) => item.commit === value);
  return revision ? `${revision.shortCommit} · ${revision.subject}` : "Git 历史版本";
}

function revisionDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString("zh-CN", { dateStyle: "short", timeStyle: "short" });
}

function nextDiff() { editor?.goToDiff("next"); }
function previousDiff() { editor?.goToDiff("previous"); }

async function saveRepository() {
  if (!session.value || !modified || rightVersion.value !== WORKTREE) return;
  try {
    const saved = await desktopApi.saveRepositoryText(
      props.repository,
      props.item.id,
      session.value.repository.revision,
      modified.getValue(),
    );
    session.value.repository = saved;
    worktreeDraft = saved.content;
    dirty.value = false;
    savedMessage.value = "仓库版本已安全保存";
    hasDifferences.value = undefined;
    // Re-bind the same models so Monaco immediately recalculates a now-empty diff.
    if (editor && original && modified) {
      editor.setModel(null);
      editor.setModel({ original, modified });
    }
    emit("repositorySaved");
  } catch (reason) {
    error.value = errorMessage(reason);
  }
}

onBeforeUnmount(() => {
  emit("dirtyChanged", false);
  modifiedSubscription?.dispose();
  diffSubscription?.dispose();
  editor?.dispose();
  original?.dispose();
  modified?.dispose();
});
</script>

<template>
  <section class="workbench">
    <header class="workbench-header">
      <div><p class="eyebrow">多版本可视化对比</p><h1>{{ item.name }} · {{ item.target }}</h1></div>
      <div class="diff-status" :class="{ dirty }"><span></span>{{ diffLabel }}</div>
    </header>
    <div class="toolbar" aria-label="对比工具栏">
      <div class="segmented">
        <button :class="{ active: sideBySide }" @click="sideBySide = true">并排</button>
        <button :class="{ active: !sideBySide }" @click="sideBySide = false">行内</button>
      </div>
      <label class="check"><input v-model="ignoreWhitespace" type="checkbox" />忽略空白</label>
      <span v-if="!history.length" class="history-empty">该文件还没有 Git 提交历史</span>
      <span class="toolbar-spacer"></span>
      <button class="icon-button" title="上一个差异" @click="previousDiff">↑</button>
      <button class="icon-button" title="下一个差异" @click="nextDiff">↓</button>
    </div>
    <div class="editor-labels version-labels">
      <div>
        <span>左侧基准</span>
        <select v-model="leftVersion" :disabled="loading">
          <option :value="LOCAL">本机当前文件</option>
          <option v-for="revision in history" :key="revision.commit" :value="revision.commit">
            {{ revision.shortCommit }} · {{ revision.subject }} · {{ revisionDate(revision.authoredAt) }}
          </option>
        </select>
        <small :title="leftTitle">{{ leftTitle }} · 只读</small>
      </div>
      <div>
        <span>右侧版本</span>
        <select v-model="rightVersion" :disabled="loading">
          <option :value="WORKTREE">仓库工作副本（可编辑）</option>
          <option v-for="revision in history" :key="revision.commit" :value="revision.commit">
            {{ revision.shortCommit }} · {{ revision.subject }} · {{ revisionDate(revision.authoredAt) }}
          </option>
        </select>
        <small :title="rightTitle">{{ rightTitle }} · {{ rightEditable ? "可编辑" : "只读" }}</small>
      </div>
    </div>
    <p v-if="error" class="notice warning">{{ error }}</p>
    <div ref="host" class="editor-host" :class="{ loading }"></div>
    <footer class="workbench-footer">
      <span>UTF-8 · {{ session?.repository.lineEnding ?? "—" }} · 历史读取不会修改 Git 工作区</span>
      <button class="button primary" :disabled="!dirty || !rightEditable" @click="saveRepository">保存工作副本</button>
    </footer>
  </section>
</template>
