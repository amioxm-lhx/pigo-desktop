import { ssrRenderAttrs } from "vue/server-renderer";
import { useSSRContext } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const __pageData = JSON.parse('{"title":"运行时核心","description":"","frontmatter":{},"headers":[],"relativePath":"zh-CN/spec/03-runtime/README.md","filePath":"zh-CN/spec/03-runtime/README.md","lastUpdated":1793980871000}');
const _sfc_main = { name: "zh-CN/spec/03-runtime/README.md" };
function _sfc_ssrRender(_ctx, _push, _parent, _attrs, $props, $setup, $data, $options) {
  _push(`<div${ssrRenderAttrs(_attrs)}><h1 id="运行时核心" tabindex="-1">运行时核心 <a class="header-anchor" href="#运行时核心" aria-label="Permalink to &quot;运行时核心&quot;">​</a></h1><blockquote><p><strong>翻译说明：</strong> 本页是与 <a href="/spec/03-runtime/README">英文源规格</a> 一一对应的机器辅助翻译。代码、协议字段和标识符保持原文；如翻译与英文源事实有歧义，以英文版本为准。</p></blockquote><table tabindex="0"><thead><tr><th>医生</th><th>描述</th></tr></thead><tbody><tr><td><a href="/zh-CN/spec/03-runtime/01-ipc-protocol">01-ipc-protocol.md</a></td><td>Electron IPC 协议</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/02-agent-runtime">02-agent-runtime.md</a></td><td>pi 代理运行时</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/03-tools-and-permissions">03-工具和权限.md</a></td><td>工具和权限</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/04-data-storage">04-数据存储.md</a></td><td>本地存储模型</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/05-host-core-rust">05-host-core-rust.md</a></td><td>Rust 主机核心</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/06-host-rpc-protocol">06-host-rpc-protocol.md</a></td><td>主机 JSON-RPC 协议</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/07-process-model">07-process-model.md</a></td><td>流程拓扑和生命周期</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/08-error-codes">08-错误代码.md</a></td><td>共享错误代码注册表</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/09-logging-and-observability">09-logging-and-observability.md</a></td><td>日志记录和审计</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/10-session-state-machine">10-会话状态机.md</a></td><td>Session/turn 状态机</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/11-provider-model-system">11-provider-model-system.md</a></td><td>通用提供商和模型覆盖范围</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/12-provider-config-schema">12-provider-config-schema.md</a></td><td>提供程序配置/SQL/主机方法</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/13-model-catalog-and-selection">13-model-catalog-and-selection.md</a></td><td>模型目录、选择器、选择</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/14-secrets-storage">14-秘密-存储.md</a></td><td>密钥存储和编辑</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/15-workspace-ignore-rules">15-workspace-ignore-rules.md</a></td><td>工作区忽略和拒绝名单</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/16-tool-result-limits">16-tool-result-limits.md</a></td><td>工具结果大小限制</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/17-asktool-questions">17-asktool-questions.md</a></td><td>交互式多问题工具</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/18-line-anchored-edit-contract">18-line-anchored-edit-contract.md</a></td><td>按行锚定的 Edit 合约</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/19-remote-agent-control-protocol">19-remote-agent-control-protocol.md</a></td><td>远程 Agent 控制协议</td></tr><tr><td><a href="/zh-CN/spec/03-runtime/20-staff-employees">20-staff-employees.md</a></td><td>员工、凭据与共享记忆</td></tr></tbody></table></div>`);
}
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("zh-CN/spec/03-runtime/README.md");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const README = /* @__PURE__ */ _export_sfc(_sfc_main, [["ssrRender", _sfc_ssrRender]]);
export {
  __pageData,
  README as default
};
