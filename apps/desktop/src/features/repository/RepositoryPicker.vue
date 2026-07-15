<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../shared/api";
import type { RepositoryInspectionDto } from "../../shared/bindings";
import { errorMessage } from "../../shared/errors";

const props = defineProps<{ current?: RepositoryInspectionDto }>();
const emit = defineEmits<{ selected: [inspection: RepositoryInspectionDto] }>();

const inspection = ref<RepositoryInspectionDto>();
const shown = computed(() => props.current ?? inspection.value);
const error = ref("");
const busy = ref(false);
const cloneDialogOpen = ref(false);
const cloneRemote = ref("");
const cloneParent = ref("");
const remoteInput = ref<HTMLInputElement>();

const cloneName = computed(() => repositoryName(cloneRemote.value));
const cloneDestination = computed(() => {
  const parent = cloneParent.value.replace(/\/+$/, "");
  return parent && cloneName.value ? `${parent}/${cloneName.value}` : "";
});

function repositoryName(remote: string) {
  const normalized = remote.trim().replace(/\/+$/, "");
  const name = normalized.split(/[/:]/).pop()?.replace(/\.git$/i, "").trim();
  return name || "envweave-config";
}

async function chooseRepository() {
  error.value = "";
  const selected = await open({ directory: true, multiple: false, title: "选择 EnvWeave 仓库" });
  if (!selected) return;
  busy.value = true;
  try {
    inspection.value = await desktopApi.inspectRepository(selected);
    if (inspection.value.hasManifest) emit("selected", inspection.value);
  } catch (reason) {
    error.value = errorMessage(reason);
  } finally {
    busy.value = false;
  }
}

async function initializeRepository() {
  if (!inspection.value) return;
  busy.value = true;
  error.value = "";
  try {
    inspection.value = await desktopApi.createRepository(inspection.value.path);
    emit("selected", inspection.value);
  } catch (reason) {
    error.value = errorMessage(reason);
  } finally {
    busy.value = false;
  }
}

async function showCloneDialog() {
  error.value = "";
  cloneDialogOpen.value = true;
  await nextTick();
  remoteInput.value?.focus();
}

function closeCloneDialog() {
  if (!busy.value) cloneDialogOpen.value = false;
}

async function chooseCloneParent() {
  const parent = await open({ directory: true, multiple: false, title: "选择克隆目标的父目录" });
  if (parent) cloneParent.value = parent;
}

async function cloneRepository() {
  const remote = cloneRemote.value.trim();
  const destination = cloneDestination.value;
  if (!remote || !destination) return;

  busy.value = true;
  error.value = "";
  try {
    inspection.value = await desktopApi.cloneRepository(remote, destination);
    if (!inspection.value.hasManifest) {
      error.value = `仓库已克隆到 ${destination}，但其中没有 envweave.toml。请选择该目录进行初始化。`;
      cloneDialogOpen.value = false;
      return;
    }
    cloneDialogOpen.value = false;
    emit("selected", inspection.value);
  } catch (reason) {
    error.value = errorMessage(reason);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="repository-card">
    <div>
      <p class="eyebrow">当前仓库</p>
      <strong>{{ shown?.path ?? "尚未选择" }}</strong>
      <p v-if="shown" class="repository-meta">
        {{ shown.hasManifest ? "EnvWeave 清单有效" : "未发现 envweave.toml" }}
        · {{ shown.hasGit ? "Git 已初始化" : "未初始化 Git" }}
      </p>
      <p v-if="error" class="error-text">{{ error }}</p>
    </div>
    <div class="button-row">
      <button v-if="shown && !shown.hasManifest" class="button primary" :disabled="busy" @click="initializeRepository">
        初始化仓库
      </button>
      <button class="button secondary" :disabled="busy" @click="showCloneDialog">克隆仓库</button>
      <button class="button secondary" :disabled="busy" @click="chooseRepository">
        {{ busy ? "处理中…" : "选择目录" }}
      </button>
    </div>
  </section>

  <div v-if="cloneDialogOpen" class="modal-backdrop" @click.self="closeCloneDialog">
    <form class="clone-dialog" role="dialog" aria-modal="true" aria-labelledby="clone-title" @submit.prevent="cloneRepository">
      <header>
        <div><p class="eyebrow">Git 远程仓库</p><h2 id="clone-title">克隆配置仓库</h2></div>
        <button type="button" class="dialog-close" :disabled="busy" aria-label="关闭" @click="closeCloneDialog">×</button>
      </header>
      <label class="clone-field">
        <span>远程仓库地址</span>
        <input ref="remoteInput" v-model="cloneRemote" :disabled="busy" autocomplete="off" placeholder="https://github.com/user/dotfiles.git">
      </label>
      <label class="clone-field">
        <span>保存到</span>
        <div class="clone-path-row">
          <input v-model="cloneParent" :disabled="busy" placeholder="选择目标父目录" readonly>
          <button type="button" class="button secondary" :disabled="busy" @click="chooseCloneParent">浏览…</button>
        </div>
      </label>
      <p v-if="cloneDestination" class="clone-destination">将创建目录：<code>{{ cloneDestination }}</code></p>
      <p v-if="error" class="notice warning">{{ error }}</p>
      <footer>
        <button type="button" class="button secondary" :disabled="busy" @click="closeCloneDialog">取消</button>
        <button type="submit" class="button primary" :disabled="busy || !cloneRemote.trim() || !cloneParent">
          {{ busy ? "正在克隆…" : "开始克隆" }}
        </button>
      </footer>
    </form>
  </div>
</template>
