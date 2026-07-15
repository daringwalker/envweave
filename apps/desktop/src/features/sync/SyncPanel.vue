<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { desktopApi } from "../../shared/api";
import type { GitStatusDto } from "../../shared/bindings";
import { errorMessage } from "../../shared/errors";

const props = defineProps<{ repository: string }>();
const status = ref<GitStatusDto>();
const commitMessage = ref("EnvWeave 更新");
const remoteUrl = ref("");
const error = ref("");
const message = ref("");
const busy = ref(false);

const hasPushDestination = computed(() => Boolean(status.value?.upstream || status.value?.originUrl));
const rebaseBlocked = computed(() => Boolean(status.value?.rebaseInProgress));
const conflictPaths = computed(() => new Set(status.value?.conflicts.map((item) => item.path) ?? []));

function beginOperation() {
  busy.value = true;
  error.value = "";
  message.value = "";
}

async function refresh() {
  beginOperation();
  try {
    status.value = await desktopApi.gitStatus(props.repository);
    if (!remoteUrl.value && status.value.originUrl) remoteUrl.value = status.value.originUrl;
  } catch (reason) {
    error.value = errorMessage(reason);
  } finally {
    busy.value = false;
  }
}

async function commit() {
  beginOperation();
  try {
    status.value = await desktopApi.gitCommit(props.repository, commitMessage.value);
    commitMessage.value = "EnvWeave 更新";
    message.value = "变更已提交";
  } catch (reason) {
    error.value = errorMessage(reason);
  } finally {
    busy.value = false;
  }
}

async function remote(action: "fetch" | "pull" | "push") {
  beginOperation();
  try {
    status.value = action === "fetch"
      ? await desktopApi.gitFetch(props.repository)
      : action === "pull"
        ? await desktopApi.gitPull(props.repository)
        : await desktopApi.gitPush(props.repository);
    message.value = action === "fetch" ? "远程状态已更新" : action === "pull" ? "拉取完成" : "推送完成";
  } catch (reason) {
    error.value = errorMessage(reason);
    try { status.value = await desktopApi.gitStatus(props.repository); } catch { /* Preserve the operation error. */ }
  } finally {
    busy.value = false;
  }
}

async function rebase(action: "continue" | "abort") {
  if (action === "abort" && !confirm("中止变基会放弃这次拉取过程中产生的修改，并恢复到变基开始前。确定继续？")) return;
  beginOperation();
  try {
    status.value = action === "continue"
      ? await desktopApi.gitContinueRebase(props.repository)
      : await desktopApi.gitAbortRebase(props.repository);
    message.value = action === "continue" ? "变基已继续并完成" : "已中止变基，仓库已恢复到变基前状态";
  } catch (reason) {
    error.value = errorMessage(reason);
    try { status.value = await desktopApi.gitStatus(props.repository); } catch { /* Preserve the operation error. */ }
  } finally {
    busy.value = false;
  }
}

async function setOrigin() {
  if (!remoteUrl.value.trim()) return;
  beginOperation();
  try {
    status.value = await desktopApi.gitSetOrigin(props.repository, remoteUrl.value.trim());
    remoteUrl.value = status.value.originUrl ?? remoteUrl.value.trim();
    message.value = "远程仓库 origin 已保存；现在可以推送了";
  } catch (reason) {
    error.value = errorMessage(reason);
  } finally {
    busy.value = false;
  }
}

watch(() => props.repository, () => {
  remoteUrl.value = "";
  void refresh();
});
onMounted(refresh);
</script>

<template>
  <section class="panel">
    <header class="panel-header">
      <div><p class="eyebrow">版本管理</p><h1>Git 同步</h1></div>
      <div class="button-row">
        <button class="button secondary" :disabled="busy || rebaseBlocked || !hasPushDestination" @click="remote('fetch')">获取</button>
        <button class="button secondary" :disabled="busy || rebaseBlocked || !status?.upstream" @click="remote('pull')">拉取并变基</button>
        <button class="button primary" :disabled="busy || rebaseBlocked || !hasPushDestination || !status?.branch" @click="remote('push')">
          {{ busy ? "处理中…" : "推送" }}
        </button>
      </div>
    </header>

    <p v-if="error" class="notice warning">{{ error }}</p>
    <p v-else-if="message" class="notice">{{ message }}</p>
    <p v-if="status && !hasPushDestination" class="notice warning">
      尚未配置远程仓库。请在下方填写仓库地址并点击“保存 origin”，之后才能获取或推送。
    </p>
    <p v-else-if="status?.originUrl && !status.upstream" class="notice">
      已配置 origin。首次推送会自动创建远程分支，并建立后续同步所需的上游跟踪关系。
    </p>

    <section v-if="status?.rebaseInProgress" class="rebase-conflict-panel">
      <header><div><p class="eyebrow">同步已暂停</p><h2>正在处理 Git 变基冲突</h2></div><span>{{ status.conflicts.length }} 个冲突文件</span></header>
      <p v-if="status.conflicts.length">请使用对比编辑器或其他文本编辑器修改下列文件，删除冲突标记并保留正确内容。点击继续时，EnvWeave 只会暂存这些冲突文件。</p>
      <p v-else>冲突文件已经暂存，可以继续完成变基。如果 Git 遇到下一组冲突，列表会自动更新。</p>
      <div v-if="status.conflicts.length" class="rebase-conflict-list"><div v-for="file in status.conflicts" :key="file.path"><code>{{ file.code }}</code><span>{{ file.path }}</span></div></div>
      <footer><button class="button secondary" :disabled="busy" @click="refresh">重新检查</button><span></span><button class="button danger" :disabled="busy" @click="rebase('abort')">中止变基</button><button class="button primary" :disabled="busy" @click="rebase('continue')">标记已解决并继续</button></footer>
    </section>

    <div v-if="status" class="git-overview">
      <div><small>当前分支</small><strong>{{ status.branch ?? "尚无提交" }}</strong></div>
      <div><small>上游分支</small><strong>{{ status.upstream ?? "首次推送时建立" }}</strong></div>
      <div><small>领先 / 落后</small><strong>{{ status.ahead }} / {{ status.behind }}</strong></div>
      <div><small>工作区变更</small><strong>{{ status.changed.length }}</strong></div>
    </div>

    <div v-if="status?.changed.length" class="changed-list">
      <div v-for="file in status.changed" :key="file.path" :class="{ conflicted: conflictPaths.has(file.path) }"><code>{{ file.code }}</code><span>{{ file.path }}</span></div>
    </div>

    <div class="commit-bar">
      <input v-model="commitMessage" aria-label="提交说明">
      <button class="button primary" :disabled="busy || rebaseBlocked || !status?.changed.length || !commitMessage.trim()" @click="commit">提交全部变更</button>
    </div>
    <div class="commit-bar remote-bar">
      <input v-model="remoteUrl" placeholder="https://github.com/user/dotfiles.git" aria-label="origin 远程仓库地址">
      <button class="button secondary" :disabled="busy || rebaseBlocked || !remoteUrl.trim()" @click="setOrigin">
        {{ status?.originUrl ? "更新 origin" : "保存 origin" }}
      </button>
    </div>
  </section>
</template>
