<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../../shared/api";
import { errorMessage } from "../../shared/errors";
import type { ConfigItemDto } from "../../shared/bindings";
import DiscoveryPanel from "../discovery/DiscoveryPanel.vue";
const props=defineProps<{repository:string,refreshRevision?:number,allowSelection?:()=>boolean}>();
const emit=defineEmits<{select:[item:ConfigItemDto]}>();
const items=ref<ConfigItemDto[]>([]);const busy=ref(false);const message=ref("");const selected=ref("");
const showDiscovery=ref(false);
const discoveryMounted=ref(false);
const labels:Record<string,string>={"in-sync":"已同步",modified:"有差异","missing-target":"本机缺失","missing-repository":"仓库缺失","type-mismatch":"类型冲突"};
async function refresh(){busy.value=true;message.value="";try{items.value=await desktopApi.listConfigs(props.repository);}catch(e){message.value=errorMessage(e);}finally{busy.value=false;}}
async function add(directory:boolean){const target=await open({multiple:false,directory,title:directory?"选择配置目录":"选择配置文件"});if(!target)return;busy.value=true;try{let item:ConfigItemDto;try{item=await desktopApi.addConfig(props.repository,target);}catch(e){const code=e&&typeof e==="object"&&"code"in e?String(e.code):"";if(code!=="config.sensitive_confirmation_required"||!confirm(`${errorMessage(e)}\n\n敏感内容进入 Git 仓库后可能被同步到远程。仍要继续吗？`))throw e;item=await desktopApi.addConfig(props.repository,target,true);}items.value.push(item);choose(item);message.value="配置已添加并完成首次收集";}catch(e){message.value=errorMessage(e);}finally{busy.value=false;}}
function choose(item:ConfigItemDto){if(props.allowSelection&&!props.allowSelection())return;selected.value=item.id;emit("select",item);}
async function operate(item:ConfigItemDto,action:"capture"|"apply"){if(selected.value===item.id&&props.allowSelection&&!props.allowSelection())return;busy.value=true;try{const result=action==="capture"?await desktopApi.captureConfig(props.repository,item.id):await desktopApi.applyConfig(props.repository,item.id);message.value=result.message;await refresh();}catch(e){message.value=errorMessage(e);}finally{busy.value=false;}}
async function remove(item:ConfigItemDto){if(selected.value===item.id&&props.allowSelection&&!props.allowSelection())return;if(!confirm(`从仓库删除“${item.name}”？本机文件不会被删除。`))return;try{message.value=(await desktopApi.removeConfig(props.repository,item.id)).message;selected.value="";await refresh();}catch(e){message.value=errorMessage(e);}}
async function discovered(added:ConfigItemDto[]){await refresh();message.value=`已从扫描结果添加 ${added.length} 个配置项`;if(added[0])choose(added[0]);}
function openDiscovery(){discoveryMounted.value=true;showDiscovery.value=true;}
watch(()=>props.repository,refresh);watch(()=>props.refreshRevision,refresh);onMounted(refresh);
</script>
<template><section class="panel config-panel"><header class="panel-header"><div><p class="eyebrow">配置清单</p><h1>配置文件</h1></div><div class="button-row"><button class="button scan-button" @click="openDiscovery">⌕ 智能扫描</button><button class="button secondary" @click="add(true)">添加目录</button><button class="button secondary" @click="add(false)">添加文件</button></div></header>
<p v-if="message" class="notice">{{message}}</p><div v-if="!items.length&&!busy" class="empty-state"><strong>仓库中还没有配置</strong><span>从本机选择文件或目录，EnvWeave 会复制到仓库并建立可移植映射。</span></div>
<div v-else class="config-list"><article v-for="item in items" :key="item.id" :class="['config-row',{selected:selected===item.id}]" @click="choose(item)"><span class="file-glyph">{{item.kind==='directory'?'▣':'◇'}}</span><div class="config-name"><strong>{{item.name}} <span v-if="item.scope==='system'" class="system-scope-badge" title="系统级配置目前仅支持采集、查看和对比">系统级</span><span v-if="item.portability==='machine-bound'" class="machine-bound-badge" title="新机器恢复时默认跳过">机器相关</span><span v-if="item.sensitive" class="sensitive-badge">敏感</span></strong><small>{{item.target}} → {{item.source}}</small></div><span :class="['status-pill',item.status]">{{labels[item.status]??item.status}}</span><div class="row-actions"><button @click.stop="operate(item,'capture')">收集</button><button :disabled="item.scope==='system'" :title="item.scope==='system'?'需要受控的管理员权限支持，当前版本暂不允许直接恢复':''" @click.stop="operate(item,'apply')">应用</button><button @click.stop="remove(item)">删除</button></div></article></div><DiscoveryPanel v-if="discoveryMounted" v-show="showDiscovery" :repository="repository" @close="showDiscovery=false" @added="discovered"/></section></template>
