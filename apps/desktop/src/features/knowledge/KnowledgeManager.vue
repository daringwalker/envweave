<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { desktopApi } from "../../shared/api";
import type { KnowledgeApplicationDto, KnowledgeCatalogDto, KnowledgeConfigDto } from "../../shared/bindings";
import { errorMessage } from "../../shared/errors";

const props = defineProps<{ repository: string }>();
const emit = defineEmits<{ close: [] }>();
const catalog = ref<KnowledgeCatalogDto>();
const selectedId = ref("");
const search = ref("");
const treeSelection = ref(localStorage.getItem("envweave.knowledgeSelection") ?? "all");
const expandedCategories = ref(loadExpandedCategories());
const localApplicationIds = ref(new Set<string>());
const detectingLocal = ref(false);
const busy = ref(false);
const error = ref("");
const message = ref("");
const editing = ref(false);
const editingOriginalId = ref("");
const deleteConfirm = ref(false);
const draft = ref<KnowledgeApplicationDto>(emptyApplication());
const packagesText = ref("");
const executablesText = ref("");

const categoryLabels: Record<string, string> = { shell: "Shell 与命令行", terminal: "终端", editor: "编辑器", "version-control": "版本控制", "developer-tools": "开发工具", runtime: "运行时", container: "容器", cloud: "云与集群", desktop: "桌面环境", "window-manager": "窗口管理器与合成器", "file-manager": "文件管理", browser: "浏览器", communication: "通信", media: "影音", security: "安全与连接", system: "系统与全局配置", other: "其他" };
const categoryOrder = ["shell", "terminal", "editor", "version-control", "developer-tools", "runtime", "container", "cloud", "desktop", "window-manager", "file-manager", "browser", "communication", "media", "security", "system", "other"];
const roleLabels: Record<string, string> = { config: "配置", history: "历史", flags: "启动参数", state: "状态", extensions: "扩展", credentials: "凭据" };

const allApplications = computed(() => catalog.value?.applications ?? []);
const builtinCount = computed(() => allApplications.value.filter((item) => item.source === "builtin").length);
const userCount = computed(() => allApplications.value.filter((item) => item.source === "user").length);
const localCount = computed(() => allApplications.value.filter((item) => localApplicationIds.value.has(item.id)).length);
const categoryGroups = computed(() => {
  const groups = new Map<string, KnowledgeApplicationDto[]>();
  for (const item of allApplications.value) {
    const entries = groups.get(item.category) ?? [];
    entries.push(item); groups.set(item.category, entries);
  }
  return [...groups.entries()]
    .sort(([left], [right]) => categoryIndex(left) - categoryIndex(right))
    .map(([category, items]) => [category, items.sort((left, right) => left.name.localeCompare(right.name, "zh-CN"))] as const);
});
const searchResults = computed(() => {
  const query = search.value.trim().toLowerCase();
  if (!query) return [];
  return allApplications.value.filter((item) => `${item.name} ${item.id} ${categoryLabels[item.category] ?? item.category} ${item.packages.join(" ")} ${item.executables.join(" ")} ${item.configs.map((config) => `${config.path} ${config.role} ${config.scope}`).join(" ")}`.toLowerCase().includes(query));
});
const selected = computed(() => allApplications.value.find((item) => item.id === selectedId.value));
const viewItems = computed(() => {
  if (treeSelection.value === "user") return allApplications.value.filter((item) => item.source === "user");
  if (treeSelection.value === "local") return allApplications.value.filter((item) => localApplicationIds.value.has(item.id));
  if (treeSelection.value.startsWith("category:")) return allApplications.value.filter((item) => item.category === treeSelection.value.slice(9));
  return allApplications.value;
});
const viewTitle = computed(() => {
  if (treeSelection.value === "user") return "用户自定义";
  if (treeSelection.value === "local") return "本机发现";
  if (treeSelection.value.startsWith("category:")) {
    const category = treeSelection.value.slice(9); return categoryLabels[category] ?? category;
  }
  return "全部知识条目";
});
const viewDescription = computed(() => {
  if (treeSelection.value === "user") return "用户创建或覆盖的知识条目，可直接编辑和删除。";
  if (treeSelection.value === "local") return detectingLocal.value ? "正在关联本机已有配置…" : "智能扫描在本机实际发现配置位置的程序。";
  if (treeSelection.value === "category:system") return "系统级配置默认不选择，恢复前必须审阅权限和机器相关风险。";
  if (treeSelection.value.startsWith("category:")) return "选择程序查看它支持的软件包、命令和配置位置。";
  return "按类别浏览全部内置和用户知识条目。";
});

function categoryIndex(category: string) { const index = categoryOrder.indexOf(category); return index < 0 ? categoryOrder.length : index; }
function loadExpandedCategories() {
  try {
    const value: unknown = JSON.parse(localStorage.getItem("envweave.knowledgeExpanded") ?? "[]");
    return new Set(Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []);
  } catch { return new Set<string>(); }
}
function emptyConfig(): KnowledgeConfigDto { return { id: "config", path: "~/.config/", role: "config", scope: "user", platforms: [], sensitive: false, recommended: true, description: "" }; }
function emptyApplication(): KnowledgeApplicationDto { return { id: "", name: "", category: "other", packages: [], executables: [], configs: [emptyConfig()], source: "user" }; }
function cloneApplication(item: KnowledgeApplicationDto): KnowledgeApplicationDto { return { ...item, source: "user", packages: [...item.packages], executables: [...item.executables], configs: item.configs.map((config) => ({ ...config, platforms: [...config.platforms] })) }; }
function words(value: string) { return value.split(/[,\s]+/).map((item) => item.trim()).filter(Boolean); }
function persistTree() {
  localStorage.setItem("envweave.knowledgeSelection", treeSelection.value);
  localStorage.setItem("envweave.knowledgeExpanded", JSON.stringify([...expandedCategories.value]));
}
function toggleCategory(category: string) {
  const next = new Set(expandedCategories.value); next.has(category) ? next.delete(category) : next.add(category); expandedCategories.value = next; persistTree();
}
function showOverview(selection: string) { treeSelection.value = selection; selectedId.value = ""; editing.value = false; deleteConfirm.value = false; persistTree(); }
function showCategory(category: string) {
  const next = new Set(expandedCategories.value); next.add(category); expandedCategories.value = next; showOverview(`category:${category}`);
}
function select(item: KnowledgeApplicationDto) {
  selectedId.value = item.id; treeSelection.value = `app:${item.id}`; editing.value = false; deleteConfirm.value = false; error.value = ""; message.value = "";
  const next = new Set(expandedCategories.value); next.add(item.category); expandedCategories.value = next; persistTree();
}

async function detectLocal() {
  detectingLocal.value = true;
  try {
    const scan = await desktopApi.scanConfigurations(props.repository);
    localApplicationIds.value = new Set(scan.candidates.map((candidate) => candidate.applicationId));
  } catch {
    localApplicationIds.value = new Set();
  } finally { detectingLocal.value = false; }
}
async function load() {
  busy.value = true; error.value = "";
  try {
    catalog.value = await desktopApi.listKnowledge();
    if (treeSelection.value.startsWith("app:")) {
      const id = treeSelection.value.slice(4); const item = allApplications.value.find((entry) => entry.id === id);
      if (item) selectedId.value = id; else treeSelection.value = "all";
    }
    void detectLocal();
  } catch (reason) { error.value = errorMessage(reason); }
  finally { busy.value = false; }
}
function createApplication() { draft.value = emptyApplication(); packagesText.value = ""; executablesText.value = ""; editing.value = true; editingOriginalId.value = ""; deleteConfirm.value = false; }
function editApplication(item: KnowledgeApplicationDto) {
  draft.value = cloneApplication(item); packagesText.value = item.packages.join(", "); executablesText.value = item.executables.join(", "); editing.value = true; editingOriginalId.value = item.id; deleteConfirm.value = false; error.value = ""; message.value = item.source === "builtin" ? "保存后会创建用户版本，并覆盖同 ID 的内置条目。" : "";
}
function addConfig() { const config = emptyConfig(); config.id = `config-${draft.value.configs.length + 1}`; draft.value.configs.push(config); }
async function save() {
  busy.value = true; error.value = ""; message.value = "";
  try {
    draft.value.packages = words(packagesText.value); draft.value.executables = words(executablesText.value); catalog.value = await desktopApi.saveKnowledge(draft.value); editing.value = false;
    const saved = allApplications.value.find((item) => item.id === draft.value.id); if (saved) select(saved); message.value = "用户知识条目已保存，下一次智能扫描会立即使用。";
  } catch (reason) { error.value = errorMessage(reason); }
  finally { busy.value = false; }
}
async function remove(item: KnowledgeApplicationDto) {
  if (!deleteConfirm.value) { deleteConfirm.value = true; return; }
  busy.value = true; error.value = "";
  try {
    catalog.value = await desktopApi.deleteKnowledge(item.id); deleteConfirm.value = false; const restored = allApplications.value.find((entry) => entry.id === item.id);
    if (restored) select(restored); else showOverview("user"); message.value = "用户条目已删除；如果存在同 ID 内置条目，现已恢复使用内置版本。";
  } catch (reason) { error.value = errorMessage(reason); }
  finally { busy.value = false; }
}

watch(() => props.repository, () => void detectLocal());
onMounted(load);
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <section class="knowledge-dialog" role="dialog" aria-modal="true" aria-label="配置知识库管理">
      <header class="knowledge-header">
        <div><p class="eyebrow">智能扫描数据源</p><h1>配置知识库</h1><p>按类别浏览内置支持项，并使用用户 TOML 扩充或覆盖。</p></div>
        <div class="button-row"><button class="button primary" :disabled="busy" @click="createApplication">＋ 新建条目</button><button class="close-button" aria-label="关闭" @click="emit('close')">×</button></div>
      </header>
      <div class="knowledge-toolbar"><input v-model="search" placeholder="搜索程序、软件包或配置路径"><span>内置 {{ builtinCount }}</span><span>用户 {{ userCount }}</span></div>
      <div class="knowledge-feedback"><p v-if="error" class="notice warning">{{ error }}</p><p v-else-if="message" class="notice">{{ message }}</p><details v-if="catalog?.warnings.length" class="scan-warnings"><summary>{{ catalog.warnings.length }} 个知识库文件未能加载</summary><p v-for="warning in catalog.warnings" :key="warning">{{ warning }}</p></details></div>

      <div class="knowledge-content">
        <aside class="knowledge-tree" aria-label="知识库分类">
          <template v-if="search.trim()">
            <h3>搜索结果 <small>{{ searchResults.length }}</small></h3>
            <button v-for="item in searchResults" :key="item.id" class="tree-app search-result" :class="{active:selectedId===item.id&&!editing}" @click="select(item)"><span><strong>{{ item.name }}</strong><small>{{ categoryLabels[item.category] ?? item.category }} / {{ item.id }}</small></span><em :class="item.source">{{ item.source === 'builtin' ? '内置' : '用户' }}</em></button>
            <div v-if="!busy&&!searchResults.length" class="tree-empty">没有匹配条目</div>
          </template>
          <template v-else>
            <div class="tree-quick-links">
              <button :class="{active:treeSelection==='all'&&!editing}" @click="showOverview('all')"><span>全部条目</span><small>{{ allApplications.length }}</small></button>
              <button :class="{active:treeSelection==='local'&&!editing}" @click="showOverview('local')"><span>{{ detectingLocal ? '正在识别本机…' : '本机发现' }}</span><small>{{ localCount }}</small></button>
              <button :class="{active:treeSelection==='user'&&!editing}" @click="showOverview('user')"><span>用户自定义</span><small>{{ userCount }}</small></button>
            </div>
            <h3>程序类别 <small>{{ categoryGroups.length }}</small></h3>
            <section v-for="[category,items] in categoryGroups" :key="category" class="tree-category">
              <div class="tree-category-row" :class="{active:treeSelection===`category:${category}`&&!editing}">
                <button class="tree-disclosure" :aria-label="`${expandedCategories.has(category)?'折叠':'展开'}${categoryLabels[category]??category}`" @click="toggleCategory(category)">{{ expandedCategories.has(category) ? '⌄' : '›' }}</button>
                <button class="tree-category-name" @click="showCategory(category)"><span>{{ categoryLabels[category] ?? category }}</span><small>{{ items.length }}</small></button>
              </div>
              <div v-if="expandedCategories.has(category)" class="tree-children">
                <button v-for="item in items" :key="item.id" class="tree-app" :class="{active:selectedId===item.id&&!editing}" @click="select(item)"><span><strong>{{ item.name }}</strong><small>{{ item.configs.length }} 个位置</small></span><em :class="item.source">{{ item.source === 'builtin' ? '内置' : '用户' }}</em></button>
              </div>
            </section>
          </template>
        </aside>

        <main v-if="editing" class="knowledge-editor">
          <div class="knowledge-editor-title"><div><small>用户知识条目</small><h2>{{ draft.name || "新建条目" }}</h2></div><button class="button secondary" :disabled="busy" @click="editing=false">取消</button></div>
          <div class="knowledge-form-grid">
            <label>应用 ID<input v-model.trim="draft.id" :disabled="busy||Boolean(editingOriginalId)" placeholder="例如 my-tool"><small>小写字母、数字、- 或 _；保存后不可修改</small></label>
            <label>显示名称<input v-model.trim="draft.name" :disabled="busy" placeholder="例如 My Tool"></label>
            <label>程序类别<select v-model="draft.category" :disabled="busy"><option v-for="category in categoryOrder" :key="category" :value="category">{{ categoryLabels[category] }}</option></select></label>
            <label>软件包名称<input v-model="packagesText" :disabled="busy" placeholder="brew、pacman 或 AUR 包名，逗号分隔"></label>
            <label>可执行命令<input v-model="executablesText" :disabled="busy" placeholder="命令名，逗号分隔"></label>
          </div>
          <div class="knowledge-config-heading"><div><strong>配置位置</strong><small>至少保留一个；支持 ~/ 或绝对路径</small></div><button class="button secondary" :disabled="busy" @click="addConfig">＋ 添加位置</button></div>
          <div class="knowledge-configs"><article v-for="(config,index) in draft.configs" :key="index">
            <div class="config-fields"><label>配置 ID<input v-model.trim="config.id" :disabled="busy"></label><label class="path-field">路径<input v-model.trim="config.path" :disabled="busy" placeholder="~/.config/example"></label><label>用途<select v-model="config.role" :disabled="busy"><option v-for="(label,value) in roleLabels" :key="value" :value="value">{{ label }}</option></select></label><label>作用域<select v-model="config.scope" :disabled="busy"><option value="user">用户级</option><option value="system">系统级</option></select></label><button class="icon-button" :disabled="busy||draft.configs.length===1" title="删除位置" @click="draft.configs.splice(index,1)">×</button></div>
            <label>说明<input v-model.trim="config.description" :disabled="busy" placeholder="这个位置保存什么配置"></label>
            <div class="config-options"><label><input v-model="config.platforms" type="checkbox" value="macos"> macOS</label><label><input v-model="config.platforms" type="checkbox" value="linux"> Linux</label><label><input v-model="config.recommended" type="checkbox"> 默认推荐</label><label><input v-model="config.sensitive" type="checkbox"> 可能敏感</label></div>
          </article></div>
          <footer><span>保存为独立 TOML 文件，可在用户目录中继续手动编辑。</span><button class="button primary" :disabled="busy||!draft.id||!draft.name||!draft.configs.length" @click="save">{{ busy ? "正在保存…" : "保存条目" }}</button></footer>
        </main>

        <main v-else-if="selected" class="knowledge-detail">
          <header><div><span :class="['source-badge',selected.source]">{{ selected.source === "builtin" ? "内置只读" : "用户条目" }}</span><span class="category-badge">{{ categoryLabels[selected.category] ?? selected.category }}</span><h2>{{ selected.name }}</h2><code>{{ selected.id }}</code></div><div class="button-row"><button class="button secondary" @click="editApplication(selected)">{{ selected.source === "builtin" ? "创建用户覆盖" : "编辑" }}</button><button v-if="selected.source==='user'" class="button danger" :disabled="busy" @click="remove(selected)">{{ deleteConfirm ? "再次点击确认删除" : "删除" }}</button></div></header>
          <div class="knowledge-facts"><div><small>关联软件包</small><strong>{{ selected.packages.join(', ') || '未指定' }}</strong></div><div><small>关联命令</small><strong>{{ selected.executables.join(', ') || '未指定' }}</strong></div></div>
          <h3>配置位置</h3><div class="knowledge-paths"><article v-for="config in selected.configs" :key="config.id"><div><strong>{{ config.path }}</strong><small>{{ config.description || config.id }}</small></div><div class="knowledge-tags"><span>{{ roleLabels[config.role] ?? config.role }}</span><span :class="{sensitive:config.scope==='system'}">{{ config.scope === 'system' ? '系统级' : '用户级' }}</span><span v-for="platform in config.platforms" :key="platform">{{ platform }}</span><span v-if="config.recommended">推荐</span><span v-if="config.sensitive" class="sensitive">敏感</span></div></article></div>
          <footer><span>用户知识库目录</span><code>{{ catalog?.directory }}</code></footer>
        </main>

        <main v-else class="knowledge-overview">
          <header><div><p class="eyebrow">知识库分类</p><h2>{{ viewTitle }}</h2><p>{{ viewDescription }}</p></div><strong>{{ viewItems.length }}</strong></header>
          <div v-if="viewItems.length" class="knowledge-app-grid"><button v-for="item in viewItems" :key="item.id" @click="select(item)"><span><strong>{{ item.name }}</strong><small>{{ categoryLabels[item.category] ?? item.category }} · {{ item.configs.length }} 个位置</small></span><em :class="item.source">{{ item.source === 'builtin' ? '内置' : '用户' }}</em></button></div>
          <div v-else class="empty-state"><strong>{{ detectingLocal ? "正在识别本机配置…" : "此分类暂无条目" }}</strong><span v-if="treeSelection==='local'&&!detectingLocal">智能扫描只列出本机实际存在知识库配置的程序。</span></div>
        </main>
      </div>
    </section>
  </div>
</template>
