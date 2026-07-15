<script setup lang="ts">
import { computed, ref } from "vue";
import { desktopApi } from "../../shared/api";
import { errorMessage } from "../../shared/errors";
import type { InstallActionDto, PackageDto, PackageScanDto } from "../../shared/bindings";

const props = defineProps<{ repository: string; aurHelper?: string }>();
const scan = ref<PackageScanDto>();
const busy = ref(false);
const filter = ref("");
const selected = ref(new Set<string>());
const plan = ref<InstallActionDto[]>([]);
const message = ref("");

const visible = computed(() => scan.value?.packages.filter((item) => {
  const query = filter.value.toLowerCase();
  return item.name.toLowerCase().includes(query)
    || item.provider.toLowerCase().includes(query)
    || (item.executablePath ?? "").toLowerCase().includes(query);
}) ?? []);
const portableCount = computed(() => scan.value?.packages.filter((item) => item.provider === "portable").length ?? 0);
const flatpakCount = computed(() => scan.value?.packages.filter((item) => item.provider === "flatpak").length ?? 0);

function key(item: Pick<PackageDto, "provider" | "kind" | "name" | "appId">) {
  return `${item.provider}:${item.kind}:${item.appId ?? item.name}`;
}

function providerLabel(provider: string) {
  return ({ portable: "便携", flatpak: "Flatpak", brew: "Homebrew", mas: "App Store", pacman: "pacman", aur: "AUR" } as Record<string, string>)[provider] ?? provider;
}

async function run() {
  busy.value = true;
  message.value = "";
  try {
    scan.value = await desktopApi.scanPackages(props.repository);
    selected.value = new Set(scan.value.packages.map(key));
    message.value = `扫描完成：${scan.value.packages.length} 项；Desktop Entry 发现 ${portableCount.value} 个便携应用，Flatpak ${flatpakCount.value} 项`;
  } catch (error) {
    message.value = errorMessage(error);
  } finally {
    busy.value = false;
  }
}

function toggle(id: string) {
  const next = new Set(selected.value);
  next.has(id) ? next.delete(id) : next.add(id);
  selected.value = next;
}

async function save() {
  if (!scan.value) return;
  try {
    const result = await desktopApi.savePackages(
      props.repository,
      scan.value.packages.filter((item) => selected.value.has(key(item))),
    );
    message.value = result.message;
  } catch (error) {
    message.value = errorMessage(error);
  }
}

async function buildPlan() {
  busy.value = true;
  try {
    plan.value = await desktopApi.packagePlan(props.repository, props.aurHelper);
    message.value = plan.value.length ? `发现 ${plan.value.length} 个可自动安装的缺失软件包` : "当前系统已满足可自动安装的软件清单；便携应用请在恢复向导中查看来源";
  } catch (error) {
    message.value = errorMessage(error);
  } finally {
    busy.value = false;
  }
}

async function install(action: InstallActionDto) {
  if (action.thirdParty && !confirm(`“${action.package.name}”来自第三方源，继续安装？`)) return;
  if (!action.thirdParty && !confirm(`将执行：${action.commandPreview}\n\n确认安装？`)) return;
  busy.value = true;
  try {
    message.value = (await desktopApi.installPackage(action.package, props.aurHelper)).message;
    plan.value = plan.value.filter((value) => key(value.package) !== key(action.package));
  } catch (error) {
    message.value = errorMessage(error);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="panel">
    <header class="panel-header">
      <div><p class="eyebrow">软件迁移</p><h1>已安装软件与便携应用</h1></div>
      <div class="button-row"><button class="button secondary" :disabled="busy" @click="buildPlan">检查缺失</button><button class="button primary" :disabled="busy" @click="run">{{ busy ? "处理中…" : "扫描本机" }}</button></div>
    </header>
    <p v-if="message" class="notice">{{ message }}</p>
    <div v-if="scan" class="summary-strip"><strong>{{ scan.packages.length }}</strong> 个软件资产 · 已选择 {{ selected.size }} 个 · 便携 {{ portableCount }} · Flatpak {{ flatpakCount }}<button class="button secondary" @click="save">保存清单</button><input v-model="filter" placeholder="筛选名称、来源或路径"></div>
    <p v-for="warning in scan?.warnings" :key="warning" class="notice warning">{{ warning }}</p>
    <div v-if="plan.length" class="install-plan"><article v-for="action in plan" :key="key(action.package)"><div><strong>{{ action.package.name }}</strong><small>{{ action.commandPreview }}</small></div><span v-if="action.thirdParty" class="risk">第三方</span><button class="button primary" @click="install(action)">安装</button></article></div>
    <div v-if="!scan&&!plan.length" class="empty-state"><strong>尚未扫描</strong><span>Linux 会扫描 pacman、Flatpak 和 Desktop Entry；AppImage 与解压运行的便携程序可补充下载页面或直接下载链接。</span></div>
    <div v-if="scan" class="package-grid asset-grid">
      <article v-for="item in visible" :key="key(item)" :class="{ unchecked: !selected.has(key(item)), portable: item.provider==='portable' }" @click="toggle(key(item))">
        <input type="checkbox" :checked="selected.has(key(item))" @click.stop="toggle(key(item))">
        <span class="provider-badge">{{ providerLabel(item.provider) }}</span>
        <div class="package-main"><strong>{{ item.name }}</strong><small>{{ item.kind }}<template v-if="item.version"> · {{ item.version }}</template><template v-if="item.repository"> · {{ item.repository }}</template></small><code v-if="item.executablePath">{{ item.executablePath }}</code></div>
        <div v-if="item.provider==='portable'" class="portable-source" @click.stop>
          <label>下载页面<input v-model.trim="item.sourcePage" placeholder="可选：软件官网或发布页"></label>
          <label>直接下载链接<input v-model.trim="item.downloadUrl" placeholder="可选：安装文件直链"></label>
          <small>{{ item.desktopFile ? `来自 ${item.desktopFile}` : "手动记录" }}</small>
        </div>
      </article>
    </div>
  </section>
</template>
