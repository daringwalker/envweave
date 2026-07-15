<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { desktopApi } from "../../shared/api";
import { errorMessage } from "../../shared/errors";
import type { ConfigItemDto, DiscoveryCandidateDto, DiscoveryScanDto, PreviewFileDto, TextDocumentDto } from "../../shared/bindings";

const props = defineProps<{ repository: string }>();
const emit = defineEmits<{ close: []; added: [items: ConfigItemDto[]] }>();
const scan = ref<DiscoveryScanDto>();
const selected = ref(new Set<string>());
const search = ref("");
const busy = ref(true);
const message = ref("正在扫描已安装软件和常见配置位置…");
const failures = ref<string[]>([]);
const previewRoot = ref<DiscoveryCandidateDto>();
const previewFiles = ref<PreviewFileDto[]>([]);
const previewDocument = ref<TextDocumentDto>();
const previewError = ref("");
const previewBusy = ref(false);
let previewRequest = 0;
const roleLabels: Record<string,string> = { config:"配置", history:"历史", flags:"启动参数", state:"状态", extensions:"扩展", credentials:"凭据" };

const filtered = computed(() => {
  const query = search.value.trim().toLowerCase();
  return scan.value?.candidates.filter((item) => !query || `${item.applicationName} ${item.target} ${item.description}`.toLowerCase().includes(query)) ?? [];
});
const groups = computed(() => {
  const result = new Map<string, DiscoveryCandidateDto[]>();
  for (const item of filtered.value) {
    const group = result.get(item.applicationName) ?? [];
    group.push(item); result.set(item.applicationName, group);
  }
  return [...result.entries()];
});
const selectable = computed(() => scan.value?.candidates.filter((item) => !item.managed) ?? []);

async function runScan() {
  closePreview();
  busy.value = true; failures.value = [];
  try {
    scan.value = await desktopApi.scanConfigurations(props.repository);
    selected.value = new Set(scan.value.candidates.filter((item) => item.recommended && item.scope !== "system" && !item.sensitive && !item.managed).map((item) => item.id));
    message.value = scan.value.candidates.length ? `发现 ${scan.value.candidates.length} 个配置位置` : "没有发现知识库中的配置文件";
  } catch (reason) { message.value = errorMessage(reason); }
  finally { busy.value = false; }
}
function toggle(item: DiscoveryCandidateDto) {
  if (item.managed) return;
  const next = new Set(selected.value); next.has(item.id) ? next.delete(item.id) : next.add(item.id); selected.value = next;
}
function selectRecommended() { selected.value = new Set(selectable.value.filter((item) => item.recommended && item.scope !== "system" && !item.sensitive).map((item) => item.id)); }
function selectAllSafe() { selected.value = new Set(selectable.value.filter((item) => item.scope !== "system" && !item.sensitive).map((item) => item.id)); }
function clearSelection() { selected.value = new Set(); }
async function preview(item: DiscoveryCandidateDto) {
  if (item.sensitive && !confirm(`“${item.target}”可能包含主机、令牌或凭据。确认在屏幕上显示其内容？`)) return;
  const request = ++previewRequest;
  previewRoot.value = item; previewFiles.value = []; previewDocument.value = undefined; previewError.value = ""; previewBusy.value = true;
  try {
    const files = await desktopApi.previewConfiguration(item.path);
    if (request !== previewRequest || previewRoot.value?.id !== item.id) return;
    previewFiles.value = files;
    if (!files[0]) {
      previewError.value = "目录中没有可预览的文本配置，或文件超过预览限制。";
      return;
    }
    const document = await desktopApi.readConfigurationPreview(item.path, files[0].path);
    if (request === previewRequest && previewRoot.value?.id === item.id) previewDocument.value = document;
  } catch (reason) {
    if (request === previewRequest) previewError.value = errorMessage(reason);
  } finally {
    if (request === previewRequest) previewBusy.value = false;
  }
}
async function readPreview(file: PreviewFileDto) {
  const root = previewRoot.value;
  if (!root) return;
  const request = ++previewRequest;
  previewDocument.value = undefined; previewBusy.value = true; previewError.value = "";
  try {
    const document = await desktopApi.readConfigurationPreview(root.path, file.path);
    if (request === previewRequest && previewRoot.value?.id === root.id) previewDocument.value = document;
  } catch (reason) {
    if (request === previewRequest) previewError.value = errorMessage(reason);
  } finally {
    if (request === previewRequest) previewBusy.value = false;
  }
}
function closePreview() { previewRequest++; previewRoot.value = undefined; previewFiles.value = []; previewDocument.value = undefined; previewError.value = ""; previewBusy.value = false; }
function formatSize(size: number) { return size < 1024 ? `${size} B` : `${(size / 1024).toFixed(size < 10240 ? 1 : 0)} KB`; }
async function addSelected() {
  if (!scan.value || !selected.value.size) return;
  const chosen = scan.value.candidates.filter((item) => selected.value.has(item.id));
  const sensitive = chosen.filter((item) => item.sensitive);
  if (sensitive.length && !confirm(`选择中包含 ${sensitive.length} 个敏感配置。它们可能含有主机、令牌或凭据，确认加入普通 Git 仓库？`)) return;
  busy.value = true; failures.value = [];
  try {
    const result = await desktopApi.addDiscoveredConfigurations(props.repository, chosen);
    failures.value = result.failed.map((failure) => `${failure.path}：${failure.message}`);
    message.value = `成功添加 ${result.added.length} 项，跳过 ${result.skipped.length} 项${result.failed.length ? `，失败 ${result.failed.length} 项` : ""}`;
    emit("added", result.added);
    const addedTargets = new Set(result.added.map((item) => item.target));
    for (const item of scan.value.candidates) if (addedTargets.has(item.target)) item.managed = true;
    selected.value = new Set();
  } catch (reason) { message.value = errorMessage(reason); }
  finally { busy.value = false; }
}
onMounted(runScan);
watch(() => props.repository, runScan);
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <section class="discovery-dialog" role="dialog" aria-modal="true" aria-label="智能扫描配置">
      <header class="discovery-header"><div><p class="eyebrow">配置知识库</p><h1>智能扫描配置</h1><p>扫描时只检查路径；单击左侧配置即可在右侧按需预览内容。</p></div><div class="button-row"><button class="button secondary" :disabled="busy" @click="runScan">重新扫描</button><button class="close-button" aria-label="关闭" @click="emit('close')">×</button></div></header>
      <div class="discovery-summary"><div><strong>{{ scan?.packageCount ?? 0 }}</strong><span>已识别软件包</span></div><div><strong>{{ scan?.candidates.length ?? 0 }}</strong><span>现有配置位置</span></div><div><strong>{{ selected.size }}</strong><span>准备添加</span></div><input v-model="search" placeholder="搜索软件或配置路径"></div>
      <div class="selection-toolbar"><button @click="selectRecommended">选择推荐项</button><button @click="selectAllSafe">选择全部非敏感项</button><button @click="clearSelection">清空</button><span></span><button class="rescan" :disabled="busy" @click="runScan">重新扫描</button></div>
      <div class="scan-feedback"><p class="scan-message" :class="{ busy }">{{ message }}</p><details v-if="scan?.warnings.length" class="scan-warnings"><summary>扫描提示（{{scan.warnings.length}}）</summary><p v-for="warning in scan.warnings" :key="warning">{{warning}}</p></details><div v-if="failures.length" class="failure-list"><p v-for="failure in failures" :key="failure">{{failure}}</p></div></div>
      <div class="discovery-content">
        <div v-if="groups.length" class="discovery-results">
          <section v-for="[name, items] in groups" :key="name" class="discovery-group"><header><strong>{{ name }}</strong><small>{{ items[0]?.detectedBy.join(' · ') }}</small></header>
            <div v-for="item in items" :key="item.id" :class="['candidate-row',{ managed:item.managed,sensitive:item.sensitive,selected:selected.has(item.id),'preview-active':previewRoot?.id===item.id }]" tabindex="0" :aria-label="`预览 ${item.target}`" @click="preview(item)" @keydown.enter.self.prevent="preview(item)" @keydown.space.self.prevent="preview(item)">
              <input type="checkbox" :checked="selected.has(item.id)||item.managed" :disabled="item.managed" :aria-label="`选择 ${item.target}`" @click.stop @change.stop="toggle(item)"><span class="candidate-icon">{{item.kind==='directory'?'▣':'◇'}}</span><span class="candidate-info"><strong>{{item.target}}</strong><small>{{item.description}}</small></span><span class="role-badge">{{roleLabels[item.role]??item.role}}</span><span v-if="item.scope==='system'" class="risk">系统级</span><span v-if="item.sensitive" class="risk">敏感</span><span v-if="item.managed" class="managed-badge">已管理</span><span class="preview-indicator" aria-hidden="true">›</span>
            </div>
          </section>
        </div>
        <div v-else-if="!busy" class="empty-state"><strong>没有发现可迁移配置</strong><span>知识库只显示本机实际存在的文件。你仍可使用“添加文件/目录”管理自定义位置。</span></div>
        <aside class="preview-pane">
          <template v-if="previewRoot"><header><div><strong>{{previewRoot.applicationName}}</strong><small>{{previewRoot.target}}</small></div><button class="close-button" aria-label="关闭预览" @click="closePreview">×</button></header>
            <div v-if="previewFiles.length>1" class="preview-files"><button v-for="file in previewFiles" :key="file.path" :class="{active:previewDocument?.path===file.path}" @click="readPreview(file)"><span>{{file.relativePath}}</span><small>{{formatSize(file.size)}}</small></button></div>
            <p v-if="previewError" class="preview-error">{{previewError}}</p><div v-if="previewBusy" class="preview-loading">正在读取…</div><pre v-else-if="previewDocument" class="preview-code"><code>{{previewDocument.content}}</code></pre>
            <footer v-if="previewDocument">{{previewDocument.lineEnding}} · {{formatSize(previewDocument.content.length)}} · 只读预览</footer>
          </template>
          <div v-else class="preview-empty"><span class="candidate-icon">◇</span><strong>选择配置查看内容</strong><small>单击左侧任意文件或目录，内容会在这里自动更新。</small></div>
        </aside>
      </div>
      <footer class="discovery-footer"><span>敏感和系统级配置默认不选；系统级目前可采集、查看和对比，暂不允许直接恢复。</span><div class="button-row"><button class="button secondary" @click="emit('close')">完成</button><button class="button primary" :disabled="busy||!selected.size" @click="addSelected">批量添加 {{selected.size}} 项</button></div></footer>
    </section>
  </div>
</template>
