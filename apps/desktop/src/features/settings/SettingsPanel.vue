<script setup lang="ts">
import { ref } from "vue";
import { desktopApi } from "../../shared/api";
import { errorMessage } from "../../shared/errors";
import KnowledgeManager from "../knowledge/KnowledgeManager.vue";

type ThemePreference = "system" | "light" | "dark";
const props = defineProps<{ repository: string; aurHelper: string; themePreference: ThemePreference }>();
const emit = defineEmits<{ aurChanged: [value: string]; themeChanged: [value: ThemePreference] }>();
const helper = ref(props.aurHelper);
const name = ref("");
const email = ref("");
const message = ref("");
const knowledgeOpen = ref(false);

function saveHelper() {
  localStorage.setItem("envweave.aurHelper", helper.value);
  emit("aurChanged", helper.value);
  message.value = "AUR 助手设置已保存";
}
function changeTheme(value: ThemePreference) {
  emit("themeChanged", value);
  message.value = value === "system" ? "外观将跟随系统" : `已切换为${value === "dark" ? "深色" : "浅色"}模式`;
}
async function saveIdentity() {
  try { message.value = (await desktopApi.gitSetIdentity(props.repository, name.value, email.value)).message; }
  catch (reason) { message.value = errorMessage(reason); }
}
</script>

<template>
  <section class="panel settings">
    <header class="panel-header"><div><p class="eyebrow">应用设置</p><h1>设置</h1></div></header>
    <p v-if="message" class="notice">{{ message }}</p>
    <div class="setting-group appearance-setting"><h2>外观</h2><p>选择舒适的界面配色，跟随系统时会自动响应系统主题变化。</p><div class="theme-options" role="group" aria-label="界面主题"><button v-for="option in ([['system','跟随系统'],['light','浅色'],['dark','深色']] as const)" :key="option[0]" type="button" :class="{ active: themePreference===option[0] }" @click="changeTheme(option[0])">{{ option[1] }}</button></div></div>
    <div class="setting-group knowledge-setting"><h2>配置知识库</h2><p>查看智能扫描支持的全部应用，并使用独立 TOML 文件添加或覆盖用户知识。</p><button class="button primary" @click="knowledgeOpen=true">管理知识库</button></div>
    <div class="setting-group"><h2>Arch Linux</h2><label>AUR 助手<select v-model="helper"><option value="">不自动安装 AUR</option><option value="paru">paru</option><option value="yay">yay</option></select></label><button class="button secondary" @click="saveHelper">保存</button></div>
    <div class="setting-group"><h2>当前仓库 Git 身份</h2><label>用户名<input v-model="name" placeholder="Your Name"></label><label>邮箱<input v-model="email" type="email" placeholder="you@example.com"></label><button class="button primary" :disabled="!name.trim()||!email.trim()" @click="saveIdentity">保存身份</button></div>
    <div class="setting-group"><h2>安全</h2><p>EnvWeave 不保存 Git、Apple ID 或软件源凭据。认证由系统 SSH Agent、Git Credential Helper、App Store 和包管理器接管。</p></div>
  </section>
  <KnowledgeManager v-if="knowledgeOpen" :repository="repository" @close="knowledgeOpen=false" />
</template>
