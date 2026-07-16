<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { monaco } from "../diff-editor/monaco";

const editorHost = ref<HTMLDivElement>();
let editor: monaco.editor.IStandaloneDiffEditor | undefined;
let original: monaco.editor.ITextModel | undefined;
let modified: monaco.editor.ITextModel | undefined;
let observer: MutationObserver | undefined;

function currentTheme() {
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
}
function setTheme(theme: "light" | "dark") {
  document.documentElement.dataset.theme = theme;
  monaco.editor.setTheme("envweave-" + theme);
}

onMounted(() => {
  setTheme(currentTheme());
  if (editorHost.value) {
    original = monaco.editor.createModel("export EDITOR=vim\nalias ll='ls -lah'\nexport LANG=en_US.UTF-8\n", "shell");
    modified = monaco.editor.createModel("export EDITOR=nvim\nalias ll='eza -lah'\nexport LANG=zh_CN.UTF-8\nexport GOPATH=$HOME/go\n", "shell");
    editor = monaco.editor.createDiffEditor(editorHost.value, {
      automaticLayout: true, readOnly: false, renderSideBySide: true,
      minimap: { enabled: false }, fontSize: 12, scrollBeyondLastLine: false,
    });
    editor.setModel({ original, modified });
  }
  observer = new MutationObserver(() => monaco.editor.setTheme("envweave-" + currentTheme()));
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
});
onBeforeUnmount(() => {
  observer?.disconnect(); editor?.dispose(); original?.dispose(); modified?.dispose();
});
</script>

<template>
  <main class="theme-showcase">
    <header class="theme-showcase-header">
      <div><p class="eyebrow">开发环境 · 模拟数据</p><h1>EnvWeave 主题验收台</h1><p>集中展示常用控件、业务状态、弹窗与代码对比，便于检查文字清晰度和颜色一致性。</p></div>
      <div class="theme-options" aria-label="预览主题"><button :class="{active:currentTheme()==='light'}" @click="setTheme('light')">浅色</button><button :class="{active:currentTheme()==='dark'}" @click="setTheme('dark')">深色</button></div>
    </header>

    <section class="qa-grid">
      <article class="panel qa-panel">
        <header class="panel-header"><div><p class="eyebrow">基础控件</p><h1>文字、按钮与表单</h1></div><div class="button-row"><button class="button secondary">次要操作</button><button class="button primary">主要操作</button></div></header>
        <div class="qa-content qa-controls"><label>仓库名称<input value="workstation-dotfiles"></label><label>包管理器<select><option>Arch Linux · pacman</option></select></label><button class="button scan-button">智能扫描</button><button class="button danger">危险操作</button></div>
        <p class="notice">操作成功：配置清单已经更新。</p><p class="notice warning">需要确认：检测到机器相关配置。</p><p class="failure-list">读取失败：文件权限不足，请检查访问权限。</p>
        <div class="qa-badges"><span class="status-pill">已同步</span><span class="status-pill modified">有修改</span><span class="provider-badge">pacman</span><span class="risk">系统级</span><span class="sensitive-badge">敏感</span><span class="machine-bound-badge">机器相关</span></div>
      </article>

      <article class="panel qa-panel">
        <header class="panel-header"><div><p class="eyebrow">配置文件</p><h1>扫描结果与选择状态</h1></div><button class="button secondary">批量添加 3 项</button></header>
        <div class="config-list">
          <div class="config-row selected"><span class="file-glyph">◇</span><span class="config-name"><strong>.zshrc</strong><small>~/.zshrc · Zsh 主配置</small></span><span class="status-pill modified">已修改</span><span class="row-actions"><button>对比</button><button>采集</button></span></div>
          <div class="config-row"><span class="file-glyph">▣</span><span class="config-name"><strong>KDE Plasma</strong><small>~/.config/plasma-org.kde.plasma.desktop-appletsrc</small></span><span class="status-pill">一致</span></div>
          <div class="config-row"><span class="file-glyph">◇</span><span class="config-name"><strong>Git config</strong><small>~/.gitconfig · 用户级 Git 配置</small></span><span class="sensitive-badge">敏感</span></div>
        </div>
        <div class="summary-strip"><strong>18</strong><span>已管理配置</span><input placeholder="搜索配置路径"></div>
      </article>

      <article class="panel qa-panel">
        <header class="panel-header"><div><p class="eyebrow">软件包</p><h1>应用与来源</h1></div><button class="button primary">保存清单</button></header>
        <div class="package-grid qa-packages"><article><input type="checkbox" checked><span class="provider-badge">pacman</span><div><strong>neovim</strong><small>0.11.2 · 显式安装</small></div></article><article><input type="checkbox" checked><span class="provider-badge">flatpak</span><div><strong>org.gimp.GIMP</strong><small>Flathub · 桌面应用</small></div></article><article class="unchecked"><input type="checkbox"><span class="provider-badge">desktop</span><div><strong>Obsidian.AppImage</strong><small>便携应用 · 来源待补充</small></div></article></div>
        <div class="portable-source qa-content"><label>下载页面<input value="https://obsidian.md/download"></label><label>直接下载链接<input value="https://example.com/app.AppImage"></label><small>便携程序来源会随软件包清单一起迁移。</small></div>
      </article>

      <article class="panel qa-panel">
        <header class="panel-header"><div><p class="eyebrow">Git 同步</p><h1>仓库状态</h1></div><div class="button-row"><button class="button secondary">拉取</button><button class="button primary">推送</button></div></header>
        <div class="git-overview"><div><small>分支</small><strong>main</strong></div><div><small>本地修改</small><strong>3</strong></div><div><small>领先</small><strong>2</strong></div><div><small>落后</small><strong>0</strong></div></div>
        <div class="changed-list"><div><code>M</code><span>files/zshrc</span></div><div><code>A</code><span>packages.toml</span></div><div><code>D</code><span>files/old.conf</span></div></div>
        <section class="rebase-conflict-panel"><header><div><p class="eyebrow">同步已暂停</p><h2>正在处理 Git 变基冲突</h2></div><span>1 个冲突文件</span></header><p>编辑冲突文件并保留正确内容，继续时只暂存冲突文件。</p><div class="rebase-conflict-list"><div><code>UU</code><span>files/zshrc</span></div></div><footer><button class="button secondary">重新检查</button><span></span><button class="button danger">中止变基</button><button class="button primary">标记已解决并继续</button></footer></section>
        <div class="commit-bar"><input value="chore: migrate workstation configuration"><button class="button primary">提交</button></div>
      </article>

      <article class="panel qa-panel qa-wide">
        <header class="panel-header"><div><p class="eyebrow">恢复向导</p><h1>迁移预检与执行状态</h1></div><span class="dry-run-badge">安全预演</span></header>
        <div class="machine-facts"><article><small>平台</small><strong>Arch Linux</strong><span>x86_64</span></article><article><small>桌面</small><strong>KDE Plasma</strong><span>Wayland</span></article><article><small>主机</small><strong>framework-13</strong><span>新设备</span></article><article><small>仓库</small><strong>可恢复</strong><span>18 项配置</span></article></div>
        <div class="restore-summary"><button class="active"><strong>12</strong><span>可执行</span></button><button><strong>3</strong><span>需确认</span></button><button><strong>1</strong><span>已跳过</span></button><button><strong>1</strong><span>不适用</span></button><button><strong>1</strong><span>阻塞</span></button></div>
        <div class="restore-steps qa-restore"><article><span class="step-state">可执行</span><div class="step-main"><strong>恢复 Zsh 配置</strong><code>~/.config/zsh</code><small>shell.zsh · 合并覆盖</small></div><div class="step-impact"><div class="impact-counts"><span class="create">新增 3</span><span class="update">覆盖 2</span><span class="preserve">保留 4</span></div><ul><li>合并仓库内容，并保留仅存在于本机的路径</li></ul></div></article><article class="review"><span class="step-state"><label class="review-choice"><input type="checkbox">需确认</label></span><div class="step-main"><strong>恢复 KDE 面板布局</strong><code>~/.config/plasma-workspace</code><small>desktop.kde · 镜像替换</small></div><div class="step-impact"><div class="impact-counts"><span class="update">覆盖 5</span><span class="delete">删除 2</span><span class="preserve">保留 1</span></div><details class="deletion-preview" open><summary>查看删除清单</summary><code>applets/old-panel.conf</code><code>cache/stale-layout.json</code></details><ul><li>将删除 2 个仅存在于本机的路径</li></ul></div></article><article><span class="step-state">可执行</span><div class="step-main"><strong>补充终端配置</strong><code>~/.config/kitty</code><small>terminal.kitty · 保留已有</small></div><div class="step-impact"><div class="impact-counts"><span class="create">新增 2</span><span class="preserve">保留 8</span></div><ul><li>只补充缺失路径，不覆盖本机已有内容</li></ul></div></article><article class="blocked"><span class="step-state">阻塞</span><div class="step-main"><strong>恢复系统级配置</strong><code>/etc/environment</code><small>需要管理员权限</small></div><ul><li>保持当前文件</li></ul></article></div>
      </article>

      <article class="panel qa-panel qa-wide">
        <header class="panel-header"><div><p class="eyebrow">多版本可视化对比</p><h1>.zshrc · 本机与仓库</h1></div><div class="diff-status dirty"><span></span>有未保存修改</div></header>
        <div class="toolbar"><div class="segmented"><button class="active">并排</button><button>行内</button></div><label class="check"><input type="checkbox">忽略空白</label><span class="history-empty">历史版本：a81e21f</span><span class="toolbar-spacer"></span><button class="icon-button">↑</button><button class="icon-button">↓</button></div>
        <div class="editor-labels"><div>本机当前文件<small>UTF-8 · LF</small></div><div>仓库工作副本<small>可编辑</small></div></div>
        <div ref="editorHost" class="qa-editor"></div>
        <footer class="workbench-footer"><span>4 行 · Shell</span><button class="button primary">保存到仓库</button></footer>
      </article>

      <article class="knowledge-dialog qa-knowledge">
        <header class="knowledge-header"><div><p class="eyebrow">配置知识库</p><h1>按类别浏览应用</h1><p>内置规则与用户扩展规则同时显示。</p></div><button class="close-button">×</button></header>
        <div class="knowledge-toolbar"><input value="terminal"><select><option>全部平台</option></select><span>126 个应用</span><button class="button primary">新建规则</button></div>
        <div class="knowledge-content"><aside class="knowledge-tree"><div class="tree-quick-links"><button class="active"><span>全部应用</span><small>126</small></button></div><div class="tree-category"><div class="tree-category-row active"><button class="tree-disclosure">⌄</button><button class="tree-category-name"><span>终端与 Shell</span><small>18</small></button></div><div class="tree-children"><button class="tree-app active"><span><strong>Konsole</strong><small>KDE 终端</small></span><em class="builtin">内置</em></button><button class="tree-app"><span><strong>WezTerm</strong><small>跨平台终端</small></span><em class="user">用户</em></button></div></div></aside><section class="knowledge-detail"><header><div><span class="category-badge">终端</span><h2>Konsole</h2><code>org.kde.konsole</code></div><button class="button secondary">复制并编辑</button></header><div class="knowledge-facts"><div><small>检测包</small><strong>konsole</strong></div><div><small>支持平台</small><strong>Linux</strong></div></div><h3>配置位置</h3><div class="knowledge-paths"><article><div><strong>~/.config/konsolerc</strong><small>主配置</small></div><span class="role-badge">配置</span></article><article><div><strong>~/.local/share/konsole</strong><small>配色与 Profile</small></div><span class="role-badge">目录</span></article></div></section></div>
      </article>

      <article class="discovery-dialog qa-discovery">
        <header class="discovery-header"><div><p class="eyebrow">智能扫描</p><h1>发现 8 个配置位置</h1><p>单击左侧配置即可在右侧预览。</p></div><button class="close-button">×</button></header>
        <div class="discovery-summary"><div><strong>42</strong><span>软件包</span></div><div><strong>8</strong><span>配置位置</span></div><div><strong>3</strong><span>准备添加</span></div><input value="shell"></div>
        <div class="selection-toolbar"><button>选择推荐项</button><button>全部非敏感项</button><span></span><button>重新扫描</button></div>
        <div class="scan-feedback"><p class="scan-message">扫描完成，配置内容仅在选择后读取。</p></div>
        <div class="discovery-content"><div class="discovery-results"><section class="discovery-group"><header><strong>Zsh</strong><small>pacman · PATH</small></header><div class="candidate-row selected preview-active"><input type="checkbox" checked><span class="candidate-icon">◇</span><span class="candidate-info"><strong>~/.zshrc</strong><small>Zsh 主配置</small></span><span class="role-badge">配置</span><span class="preview-indicator">›</span></div><div class="candidate-row sensitive"><input type="checkbox"><span class="candidate-icon">◇</span><span class="candidate-info"><strong>~/.zsh_history</strong><small>命令历史</small></span><span class="risk">敏感</span><span class="preview-indicator">›</span></div><div class="candidate-row managed"><input type="checkbox" checked disabled><span class="candidate-icon">▣</span><span class="candidate-info"><strong>~/.config/zsh</strong><small>已加入仓库</small></span><span class="managed-badge">已管理</span></div></section></div><aside class="preview-pane"><header><div><strong>Zsh</strong><small>~/.zshrc</small></div><button class="close-button">×</button></header><pre class="preview-code"><code># Shell configuration
export EDITOR=nvim
alias ll='eza -lah'
source ~/.config/zsh/plugins.zsh</code></pre><footer>LF · 96 B · 只读预览</footer></aside></div>
        <footer class="discovery-footer"><span>敏感和系统级配置默认不选。</span><button class="button primary">批量添加 3 项</button></footer>
      </article>
    </section>
  </main>
</template>

<style>
body:has(.theme-showcase){overflow:auto}.theme-showcase{min-height:100vh;overflow:visible;padding:22px;color:var(--theme-text);background:var(--theme-shell)}
.theme-showcase-header{display:flex;max-width:1500px;align-items:center;justify-content:space-between;margin:0 auto 14px;padding:16px 18px;background:var(--theme-surface);border:1px solid var(--theme-border);border-radius:12px;box-shadow:var(--theme-shadow)}
.theme-showcase-header h1{margin:0;font-size:21px}.theme-showcase-header p:last-child{margin:5px 0 0;color:var(--theme-text-muted);font-size:11px}
.qa-grid{display:grid;max-width:1500px;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px;margin:auto}.qa-panel{min-height:auto;overflow:hidden}.qa-wide,.qa-knowledge,.qa-discovery{grid-column:1/-1}.qa-content{padding:12px 14px}.qa-controls{display:flex;gap:9px;align-items:end}.qa-controls label{display:flex;min-width:0;flex:1;flex-direction:column;gap:5px;color:var(--theme-text-muted);font-size:9px}.qa-controls input,.qa-controls select{width:100%;padding:7px;border:1px solid var(--theme-border-strong);border-radius:6px}.qa-panel>.notice,.qa-panel>.failure-list{margin:7px 14px}.qa-badges{display:flex;gap:6px;flex-wrap:wrap;padding:10px 14px 14px}.qa-packages{grid-template-columns:repeat(3,minmax(0,1fr))}.qa-packages article{min-width:0}.qa-restore{max-height:none}.qa-editor{height:290px}.qa-knowledge{width:auto;height:520px}.qa-discovery{width:auto;height:620px}.qa-discovery .discovery-content{min-height:0}.qa-discovery .discovery-footer{display:flex}.qa-discovery .preview-pane .close-button{color:#aebbb3}.qa-discovery .preview-pane .close-button:hover{color:#e7eee9;background:#233129}
@media(max-width:1050px){.qa-grid{grid-template-columns:1fr}.qa-wide,.qa-knowledge,.qa-discovery{grid-column:auto}.qa-controls{align-items:stretch;flex-direction:column}.qa-packages{grid-template-columns:1fr}.qa-knowledge,.qa-discovery{width:100%}}
</style>
