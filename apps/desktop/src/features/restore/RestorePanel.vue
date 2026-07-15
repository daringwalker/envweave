<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { desktopApi } from "../../shared/api";
import { errorMessage } from "../../shared/errors";
import type { MigrationPackageStepDto, RestorePlanDto, RestoreRunDto } from "../../shared/bindings";

const props = defineProps<{ repository: string; aurHelper?: string }>();
const emit = defineEmits<{ pendingChanged: [count: number] }>();
const plan = ref<RestorePlanDto>();
const busy = ref(false);
const message = ref("");
const filter = ref("all");
const selectedIds = ref<string[]>([]);
const run = ref<RestoreRunDto>();
const pendingRuns = ref<RestoreRunDto[]>([]);
const pendingBusy = ref("");
const packageSteps = ref<MigrationPackageStepDto[]>([]);
const packageWarnings = ref<string[]>([]);
const selectedPackages = ref<string[]>([]);

const labels: Record<string, string> = {
  ready: "可恢复",
  review: "需确认",
  skipped: "无需执行 · 已跳过",
  inapplicable: "不适用",
  blocked: "阻塞",
};
const visible = computed(() =>
  plan.value?.steps.filter((step) => filter.value === "all" || step.disposition === filter.value) ?? [],
);
const availableTools = computed(() => plan.value?.facts.tools.filter((tool) => tool.available) ?? []);
const restoreCount = computed(() => selectedIds.value.length);
const installablePackages = computed(() => packageSteps.value.filter((step) => step.action && step.disposition !== "blocked"));
const packageCount = computed(() => selectedPackages.value.length);

function packageKey(step: MigrationPackageStepDto) {
  const value = step.package;
  return `${value.provider}:${value.kind}:${value.appId ?? value.name}`;
}

function togglePackage(id: string, checked: boolean) {
  selectedPackages.value = checked
    ? [...selectedPackages.value, id]
    : selectedPackages.value.filter((value) => value !== id);
}

function toggleSelection(id: string, checked: boolean) {
  selectedIds.value = checked
    ? [...selectedIds.value, id]
    : selectedIds.value.filter((value) => value !== id);
}

async function inspect() {
  busy.value = true;
  message.value = "";
  try {
    const migration = await desktopApi.migrationPreflight(props.repository, props.aurHelper);
    plan.value = migration.configuration;
    packageSteps.value = migration.packages;
    packageWarnings.value = migration.packageWarnings;
    selectedPackages.value = migration.packages
      .filter((step) => step.disposition === "ready" && step.action)
      .map(packageKey);
    selectedIds.value = plan.value.steps
      .filter((step) => step.disposition === "ready")
      .map((step) => step.id);
  } catch (error) {
    message.value = errorMessage(error);
  } finally {
    busy.value = false;
  }
}

async function installPackages() {
  const chosen = installablePackages.value.filter((step) => selectedPackages.value.includes(packageKey(step)));
  if (!chosen.length || !window.confirm(`将按顺序安装 ${chosen.length} 个缺失软件包。第三方源和系统授权步骤已在列表中标记，确定继续吗？`)) return;
  busy.value = true;
  message.value = "";
  try {
    let completed = 0;
    for (const step of chosen) {
      if (!step.action) continue;
      await desktopApi.installPackage(step.package, props.aurHelper);
      completed += 1;
      message.value = `软件准备进度：${completed}/${chosen.length} · ${step.package.name}`;
    }
    await inspect();
    message.value = `软件准备完成：已安装 ${completed} 个软件包`;
  } catch (error) {
    message.value = `软件准备已停止：${errorMessage(error)}；再次执行会从仍缺失的软件包继续`;
    await inspect();
  } finally {
    busy.value = false;
  }
}

async function loadIncomplete() {
  try {
    pendingRuns.value = await desktopApi.incompleteRestores(props.repository);
    emit("pendingChanged", pendingRuns.value.length);
  } catch (error) {
    message.value = errorMessage(error);
  }
}

async function recoverPending(pending: RestoreRunDto) {
  if (!window.confirm("将按事务备份把涉及的配置恢复到中断前状态，确定继续吗？")) return;
  pendingBusy.value = pending.id;
  message.value = "";
  try {
    run.value = await desktopApi.recoverRestore(props.repository, pending.id);
    message.value = run.value.status === "rolled-back"
      ? "中断事务已安全回滚"
      : "部分配置无法自动回滚，请查看事务详情并从备份页手动恢复";
    await Promise.all([loadIncomplete(), inspect()]);
  } catch (error) {
    message.value = errorMessage(error);
  } finally {
    pendingBusy.value = "";
  }
}

async function keepPending(pending: RestoreRunDto) {
  if (!window.confirm("这会保留中断后当前磁盘上的文件，并停止提示该事务。只有确认当前配置正确时才应继续。确定保留吗？")) return;
  pendingBusy.value = pending.id;
  message.value = "";
  try {
    run.value = await desktopApi.keepCurrentRestore(props.repository, pending.id);
    message.value = "已保留当前文件状态；事务备份仍可在备份页面手动恢复";
    await Promise.all([loadIncomplete(), inspect()]);
  } catch (error) {
    message.value = errorMessage(error);
  } finally {
    pendingBusy.value = "";
  }
}

function runTime(value: string) {
  return new Date(Number(value)).toLocaleString();
}

async function execute() {
  if (!restoreCount.value || !window.confirm(`将恢复 ${restoreCount.value} 项用户配置。所有目标都会先备份，确定继续吗？`)) return;
  busy.value = true;
  message.value = "";
  run.value = undefined;
  try {
    if (!plan.value) return;
    run.value = await desktopApi.executeRestore(props.repository, plan.value.id, selectedIds.value);
    const outcome = run.value.status === "completed"
      ? `恢复完成：${run.value.items.filter((item) => item.status === "applied").length} 项已应用`
      : run.value.status === "rollback-failed"
        ? "恢复失败且部分内容无法自动回滚，请立即检查事务详情和备份"
        : "恢复过程中发生错误，已自动回滚已应用的配置";
    await Promise.all([inspect(), loadIncomplete()]);
    message.value = outcome;
  } catch (error) {
    message.value = errorMessage(error);
  } finally {
    busy.value = false;
  }
}

async function refresh() {
  await Promise.all([inspect(), loadIncomplete()]);
}
watch(() => props.repository, refresh);
onMounted(refresh);
</script>

<template>
  <section class="panel restore-panel">
    <header class="panel-header">
      <div><p class="eyebrow">新机器恢复</p><h1>迁移预检与恢复计划</h1></div>
      <div class="button-row"><span class="dry-run-badge">系统级配置默认隔离</span><button class="button" :disabled="busy" @click="inspect">重新预检</button><button v-if="installablePackages.length" class="button primary" :disabled="busy||!packageCount" @click="installPackages">{{ busy ? "处理中…" : `先安装 ${packageCount} 项` }}</button><button class="button primary" :disabled="busy||!restoreCount||!!packageCount" @click="execute">{{ busy ? "处理中…" : `恢复配置 ${restoreCount} 项` }}</button></div>
    </header>
    <p v-if="message" class="notice warning">{{ message }}</p>
    <section v-if="pendingRuns.length" class="pending-restores">
      <header><div><strong>检测到 {{ pendingRuns.length }} 个未完成恢复事务</strong><span>应用可能在配置写入期间退出。建议使用事务备份回到一致状态。</span></div></header>
      <article v-for="pending in pendingRuns" :key="pending.id">
        <div><strong>{{ runTime(pending.createdAtEpochMs) }}</strong><small>事务 {{ pending.id }} · {{ pending.items.length }} 个已记录步骤</small></div>
        <div class="button-row"><button class="button secondary" :disabled="!!pendingBusy" @click="keepPending(pending)">保留当前状态</button><button class="button primary" :disabled="!!pendingBusy" @click="recoverPending(pending)">{{ pendingBusy===pending.id ? "处理中…" : "回滚到中断前" }}</button></div>
      </article>
    </section>
    <section v-if="packageSteps.length||packageWarnings.length" class="migration-package-stage">
      <header><div><p class="eyebrow">阶段 1 · 软件准备</p><strong>{{ packageSteps.length ? `发现 ${packageSteps.length} 个缺失软件包` : "软件清单已满足" }}</strong></div><span>默认选择可安全安装项；取消勾选表示本次跳过</span></header>
      <p v-for="warning in packageWarnings" :key="warning" class="package-stage-warning">{{ warning }}</p>
      <div v-if="packageSteps.length" class="migration-package-list">
        <article v-for="step in packageSteps" :key="packageKey(step)" :class="step.disposition">
          <label><input v-if="step.action&&step.disposition!=='blocked'" type="checkbox" :checked="selectedPackages.includes(packageKey(step))" @change="togglePackage(packageKey(step), ($event.target as HTMLInputElement).checked)"><span>{{ step.disposition==='ready'?'可安装':step.disposition==='review'?'需确认':'阻塞' }}</span></label>
          <div><strong>{{ step.package.name }}</strong><small>{{ step.package.provider }} · {{ step.package.kind }}<template v-if="step.action"> · {{ step.action.commandPreview }}</template></small></div>
          <ul><li v-for="reason in step.reasons" :key="reason">{{ reason }}</li></ul>
        </article>
      </div>
    </section>
    <div v-if="busy&&!plan" class="empty-state"><strong>正在识别当前机器</strong><span>检查系统、架构、桌面环境、Shell、权限和恢复工具。</span></div>
    <template v-else-if="plan">
      <p class="restore-stage-title">阶段 2 · 配置事务</p>
      <div class="machine-facts">
        <article><small>系统</small><strong>{{ plan.facts.distribution || plan.facts.os }} {{ plan.facts.distributionVersion }}</strong><span>{{ plan.facts.architecture }}</span></article>
        <article><small>桌面与 Shell</small><strong>{{ plan.facts.desktop || "未检测到桌面" }}</strong><span>{{ plan.facts.shell || "未知 Shell" }}</span></article>
        <article><small>用户目录</small><strong>{{ plan.facts.home }}</strong><span>权限：{{ plan.facts.privilegeTool || "无受控提权工具" }}</span></article>
        <article><small>可用工具</small><strong>{{ availableTools.length }}</strong><span>{{ availableTools.map((tool) => tool.name).join(" · ") || "未检测到" }}</span></article>
      </div>
      <div class="restore-summary">
        <button :class="{active:filter==='all'}" @click="filter='all'"><strong>{{ plan.steps.length }}</strong><span>全部</span></button>
        <button :class="{active:filter==='ready'}" @click="filter='ready'"><strong>{{ plan.counts.ready }}</strong><span>可恢复</span></button>
        <button :class="{active:filter==='review'}" @click="filter='review'"><strong>{{ plan.counts.review }}</strong><span>需确认</span></button>
        <button :class="{active:filter==='skipped'}" @click="filter='skipped'"><strong>{{ plan.counts.skipped }}</strong><span>已隔离</span></button>
        <button :class="{active:filter==='blocked'}" @click="filter='blocked'"><strong>{{ plan.counts.blocked }}</strong><span>阻塞</span></button>
      </div>
      <div v-if="!plan.steps.length" class="empty-state"><strong>仓库中没有配置项</strong><span>先收集配置和软件包，再生成新机器恢复计划。</span></div>
      <div v-else class="restore-steps">
        <article v-for="step in visible" :key="step.id" :class="step.disposition">
          <span class="step-state"><label v-if="step.disposition==='ready'||step.disposition==='review'" class="review-choice"><input type="checkbox" :checked="selectedIds.includes(step.id)" @change="toggleSelection(step.id, ($event.target as HTMLInputElement).checked)">{{ labels[step.disposition] }}</label><template v-else>{{ labels[step.disposition] }}</template></span>
          <div class="step-main"><strong>{{ step.name }}</strong><code>{{ step.target }}</code><small>{{ step.applicationId }}<template v-if="step.dependencies.length"> · 依赖 {{ step.dependencies.join("、") }}</template></small></div>
          <ul><li v-for="reason in step.reasons" :key="reason">{{ reason }}</li></ul>
        </article>
      </div>
      <div v-if="run" class="restore-result" :class="run.status"><strong>{{ run.status === "completed" ? "事务恢复完成" : run.status === "rollback-failed" ? "事务需要人工恢复" : "事务已回滚" }}</strong><span>记录 {{ run.id }} · {{ run.items.length }} 项；可在备份页面单独恢复历史内容。</span></div>
      <footer class="restore-footer"><span>按依赖顺序执行；每项写入前创建备份，任一失败将停止并自动回滚本批次。</span><strong>机器绑定、平台不匹配和系统级项目不会自动恢复</strong></footer>
    </template>
  </section>
</template>
