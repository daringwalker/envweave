<script setup lang="ts">
import { defineAsyncComponent, defineComponent, onBeforeUnmount, onMounted, ref } from "vue";
import RepositoryPicker from "./features/repository/RepositoryPicker.vue";
import ConfigPanel from "./features/dotfiles/ConfigPanel.vue";
import PackagesPanel from "./features/packages/PackagesPanel.vue";
import SyncPanel from "./features/sync/SyncPanel.vue";
import BackupsPanel from "./features/backups/BackupsPanel.vue";
import OverviewPanel from "./features/overview/OverviewPanel.vue";
import SettingsPanel from "./features/settings/SettingsPanel.vue";
import RestorePanel from "./features/restore/RestorePanel.vue";
import { desktopApi } from "./shared/api";
import type { AppSnapshotDto, ConfigItemDto, RepositoryInspectionDto } from "./shared/bindings";

const snapshot = ref<AppSnapshotDto>();
const themeQa=import.meta.env.DEV&&new URLSearchParams(window.location.search).has("theme-qa");
const ThemeShowcase=import.meta.env.DEV
  ?defineAsyncComponent(()=>import("./features/theme/ThemeShowcase.vue"))
  :defineComponent({name:"ThemeShowcaseDisabled",render:()=>null});
const DiffWorkbench = defineAsyncComponent(
  () => import("./features/diff-editor/DiffWorkbench.vue"),
);
const active=ref("配置文件");const repository=ref<RepositoryInspectionDto>();const selectedItem=ref<ConfigItemDto>();const restoringRepository=ref(true);const configRevision=ref(0);const diffDirty=ref(false);
const pendingRestoreCount=ref(0);
const aurHelper=ref(localStorage.getItem("envweave.aurHelper")??"");
const lastRepositoryKey="envweave.lastRepository";
type ThemePreference = "system" | "light" | "dark";
type ResolvedTheme = "light" | "dark";
const savedTheme=localStorage.getItem("envweave.theme");
const themePreference=ref<ThemePreference>(savedTheme==="light"||savedTheme==="dark"?savedTheme:"system");
const systemTheme=window.matchMedia("(prefers-color-scheme: dark)");
const resolvedTheme=ref<ResolvedTheme>("light");

function applyTheme(value:ThemePreference){
  const resolved:ResolvedTheme=value==="dark"||(value==="system"&&systemTheme.matches)?"dark":"light";
  resolvedTheme.value=resolved;
  document.documentElement.dataset.theme=resolved;
}
function setTheme(value:ThemePreference){
  themePreference.value=value;
  value==="system"?localStorage.removeItem("envweave.theme"):localStorage.setItem("envweave.theme",value);
  applyTheme(value);
}
function followSystemTheme(){if(themePreference.value==="system")applyTheme("system");}
applyTheme(themePreference.value);

onMounted(async () => {
  systemTheme.addEventListener("change",followSystemTheme);
  if(themeQa){restoringRepository.value=false;return;}
  const snapshotTask=desktopApi.snapshot().then((value)=>{snapshot.value=value;}).catch(()=>{snapshot.value={appName:"EnvWeave",version:"web-preview",platform:"preview"};});
  const path=localStorage.getItem(lastRepositoryKey);
  const repositoryTask=path?desktopApi.inspectRepository(path).then((value)=>{if(value.hasManifest)repository.value=value;else localStorage.removeItem(lastRepositoryKey);}).catch(()=>localStorage.removeItem(lastRepositoryKey)):Promise.resolve();
  await Promise.all([snapshotTask,repositoryTask]);restoringRepository.value=false;
});
onBeforeUnmount(()=>systemTheme.removeEventListener("change",followSystemTheme));

const navigation = [
  ["概览", "grid"], ["配置文件", "file"], ["恢复向导", "restore"], ["软件包", "package"],
  ["同步", "sync"], ["备份", "backup"], ["设置", "settings"],
];
function allowDiscardDraft(){return !diffDirty.value||window.confirm("对比编辑器中有未保存修改，确定放弃并继续吗？");}
function selectRepository(value:RepositoryInspectionDto){if(!allowDiscardDraft())return;repository.value=value;selectedItem.value=undefined;diffDirty.value=false;localStorage.setItem(lastRepositoryKey,value.path);}
</script>

<template>
  <ThemeShowcase v-if="themeQa" />
  <div v-else class="app-shell">
    <aside class="sidebar">
      <div class="brand"><span class="brand-mark">E</span><div><strong>EnvWeave</strong><small>环境迁移管理器</small></div></div>
      <nav>
        <button v-for="([label, icon]) in navigation" :key="label" :class="{ active: active === label }" @click="active=label">
          <span class="nav-icon">{{ icon === "file" ? "◇" : icon === "package" ? "◫" : "○" }}</span>{{ label }}
        </button>
      </nav>
      <div class="sidebar-status">
        <span class="status-dot"></span>
        <div><strong>MVP 本地版</strong><small>{{ snapshot?.platform ?? "正在连接 Rust…" }} · v{{ snapshot?.version ?? "—" }}</small></div>
      </div>
    </aside>
    <main class="main-content">
      <RepositoryPicker :current="repository" @selected="selectRepository" />
      <button v-if="repository&&pendingRestoreCount" class="interrupted-restore-banner" @click="active='恢复向导'"><strong>检测到 {{ pendingRestoreCount }} 个未完成恢复事务</strong><span>打开恢复向导检查并回滚到中断前状态 →</span></button>
      <div v-if="!repository&&restoringRepository" class="welcome panel"><div class="welcome-mark">E</div><h1>正在恢复上次仓库</h1><p>正在校验仓库路径和 EnvWeave 清单…</p></div>
      <div v-else-if="!repository" class="welcome panel"><div class="welcome-mark">E</div><h1>开始编织你的开发环境</h1><p>选择已有 EnvWeave 仓库，或选择一个空目录并初始化。配置、软件包和 Git 历史都由你自己的仓库掌控。</p></div>
      <template v-else>
        <div v-show="active==='配置文件'" class="kept-page"><ConfigPanel :repository="repository.path" :refresh-revision="configRevision" :allow-selection="allowDiscardDraft" @select="selectedItem=$event"/><DiffWorkbench v-if="selectedItem&&selectedItem.kind==='file'" :repository="repository.path" :item="selectedItem" :theme="resolvedTheme" @repository-saved="configRevision++" @dirty-changed="diffDirty=$event"/></div>
        <OverviewPanel v-show="active==='概览'" :repository="repository.path" />
        <RestorePanel v-show="active==='恢复向导'" :repository="repository.path" :aur-helper="aurHelper" @pending-changed="pendingRestoreCount=$event" />
        <PackagesPanel v-show="active==='软件包'" :repository="repository.path" :aur-helper="aurHelper" />
        <SyncPanel v-show="active==='同步'" :repository="repository.path" />
        <BackupsPanel v-show="active==='备份'" :repository="repository.path" />
        <div v-show="active==='设置'" class="kept-page"><SettingsPanel :repository="repository.path" :aur-helper="aurHelper" :theme-preference="themePreference" @aur-changed="aurHelper=$event" @theme-changed="setTheme" /></div>
      </template>
    </main>
  </div>
</template>
