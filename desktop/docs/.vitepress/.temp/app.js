import { ssrRenderAttrs, ssrRenderSlot, ssrInterpolate, ssrRenderAttr, ssrRenderList, ssrRenderComponent, ssrRenderVNode, ssrRenderClass, renderToString } from "vue/server-renderer";
import { defineComponent, mergeProps, useSSRContext, getCurrentInstance, hasInjectionContext, inject, watch, getCurrentScope, onScopeDispose, onMounted, nextTick, isRef, toValue, toRef as toRef$1, readonly, customRef, ref, shallowRef, watchEffect, computed, unref, reactive, onUnmounted, markRaw, h, watchPostEffect, onUpdated, resolveComponent, createVNode, resolveDynamicComponent, withCtx, renderSlot, createTextVNode, toDisplayString, openBlock, createBlock, createCommentVNode, Fragment, renderList, defineAsyncComponent, provide, toHandlers, withKeys, onBeforeUnmount, useSlots, createSSRApp } from "vue";
import { _ as _export_sfc } from "./plugin-vue_export-helper.1tPrXgE0.js";
const _sfc_main$17 = /* @__PURE__ */ defineComponent({
  __name: "VPBadge",
  __ssrInlineRender: true,
  props: {
    text: {},
    type: { default: "tip" }
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<span${ssrRenderAttrs(mergeProps({
        class: ["VPBadge", __props.type]
      }, _attrs))}>`);
      ssrRenderSlot(_ctx.$slots, "default", {}, () => {
        _push(`${ssrInterpolate(__props.text)}`);
      }, _push, _parent);
      _push(`</span>`);
    };
  }
});
const _sfc_setup$17 = _sfc_main$17.setup;
_sfc_main$17.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPBadge.vue");
  return _sfc_setup$17 ? _sfc_setup$17(props, ctx) : void 0;
};
function deserializeFunctions(r) {
  return Array.isArray(r) ? r.map(deserializeFunctions) : typeof r == "object" && r !== null ? Object.keys(r).reduce((t, n) => (t[n] = deserializeFunctions(r[n]), t), {}) : typeof r == "string" && r.startsWith("_vp-fn_") ? new Function(`return ${r.slice(7)}`)() : r;
}
const siteData = deserializeFunctions(JSON.parse(`{"lang":"en-US","dir":"ltr","title":"PI-Desktop","description":"Local-first AI coding agent documentation","base":"/","head":[],"router":{"prefetchLinks":true},"appearance":true,"themeConfig":{"logo":"/app-icon.png","siteTitle":"PI-Desktop","search":{"provider":"local"},"socialLinks":[{"icon":"github","link":"https://github.com/vastsa/PI-Desktop"}],"editLink":{"pattern":"https://github.com/vastsa/PI-Desktop/edit/main/docs/:path","text":"Edit this page on GitHub"},"outline":{"level":"deep","label":"On this page"},"docFooter":{"prev":"Previous","next":"Next"},"footer":{"message":"Built for local-first development. <a href=\\"https://aiuo.net\\" target=\\"_blank\\" rel=\\"noreferrer\\">AIUO.NET</a>","copyright":"Copyright © 2026 PI-Desktop contributors"},"nav":[{"text":"Guide","link":"/guide/"},{"text":"Specs","link":"/spec/README"},{"text":"ADRs","link":"/adr/README"},{"text":"Plugin guide","link":"/plugin-development"},{"text":"Privacy policy","link":"/privacy-policy"},{"text":"GitHub","link":"https://github.com/vastsa/PI-Desktop"}],"sidebar":{"/guide/":[{"text":"Guide","items":[{"text":"Start here","link":"/guide/"},{"text":"Screens","link":"/guide/screenshots"}]}],"/plugin-development":[{"text":"Plugin authoring","items":[{"text":"Zero to one","link":"/plugin-development"},{"text":"Plugin System","link":"/spec/07-plugins/README"},{"text":"01. Plugin System","link":"/spec/07-plugins/01-plugin-system"},{"text":"02. Plugin Manifest Schema","link":"/spec/07-plugins/02-plugin-manifest-schema"},{"text":"03. Plugin API","link":"/spec/07-plugins/03-plugin-api"},{"text":"04. Plugin Security","link":"/spec/07-plugins/04-plugin-security"},{"text":"05. Plugin Lifecycle","link":"/spec/07-plugins/05-plugin-lifecycle"},{"text":"06. Plugin Packaging","link":"/spec/07-plugins/06-plugin-packaging"},{"text":"07. Plugin Marketplace","link":"/spec/07-plugins/07-plugin-marketplace"},{"text":"08. Plugin Signing and Updates","link":"/spec/07-plugins/08-plugin-signing-updates"},{"text":"09. Plugin Command Palette","link":"/spec/07-plugins/09-plugin-command-palette"},{"text":"10. Plugin Developer Experience","link":"/spec/07-plugins/10-plugin-devex"},{"text":"11. Plugin Storage Isolation","link":"/spec/07-plugins/11-plugin-storage-isolation"},{"text":"12. Plugin IPC and Host Services","link":"/spec/07-plugins/12-plugin-ipc-and-host-services"},{"text":"13. Plugin Permissions Matrix","link":"/spec/07-plugins/13-plugin-permissions-matrix"},{"text":"14. Plugin Roadmap","link":"/spec/07-plugins/14-plugin-roadmap"},{"text":"15. Plugin Center","link":"/spec/07-plugins/15-plugin-center"},{"text":"16. Trusted Extensions","link":"/spec/07-plugins/16-trusted-extensions"}]}],"/project/":[{"text":"Project records","items":[{"text":"Project Tracking","link":"/project/README"},{"text":"Documentation and Code Alignment Audit (2026-07-30)","link":"/project/2026-07-30-docs-code-audit"},{"text":"Documentation redesign visual verification","link":"/project/2026-08-13-docs-redesign-verification"},{"text":"PI-Desktop Project Board","link":"/project/BOARD"},{"text":"Plan Checkpoint and Shell Implementation Plan","link":"/project/plan-mode-implementation-plan"}]}],"/spec/":[{"text":"Start here","items":[{"text":"PI-Desktop Spec","link":"/spec/README"},{"text":"PI-Desktop Baseline Freeze","link":"/spec/00-baseline"},{"text":"PI-Desktop Spec Navigation","link":"/spec/NAV"}]},{"text":"Product","items":[{"text":"Product & Scope","link":"/spec/01-product/README"},{"text":"00. Overview","link":"/spec/01-product/00-overview"},{"text":"01. Product Scope","link":"/spec/01-product/01-product-scope"},{"text":"02. Non-Goals","link":"/spec/01-product/02-non-goals"}]},{"text":"Architecture","items":[{"text":"Architecture & Engineering","link":"/spec/02-architecture/README"},{"text":"01. Architecture","link":"/spec/02-architecture/01-architecture"},{"text":"02. Tech Stack","link":"/spec/02-architecture/02-tech-stack"},{"text":"03. Repo Structure","link":"/spec/02-architecture/03-repo-structure"},{"text":"Documentation site","link":"/spec/02-architecture/04-documentation-site"},{"text":"Remote Agent Control Target Architecture","link":"/spec/02-architecture/05-remote-agent-control"}]},{"text":"Runtime","collapsed":true,"items":[{"text":"Runtime Core","link":"/spec/03-runtime/README"},{"text":"01. IPC Protocol","link":"/spec/03-runtime/01-ipc-protocol"},{"text":"02. Agent Runtime","link":"/spec/03-runtime/02-agent-runtime"},{"text":"03. Tools and Permissions","link":"/spec/03-runtime/03-tools-and-permissions"},{"text":"04. Data Storage (Schema v16)","link":"/spec/03-runtime/04-data-storage"},{"text":"05. Rust Host Core","link":"/spec/03-runtime/05-host-core-rust"},{"text":"06. Host RPC Protocol","link":"/spec/03-runtime/06-host-rpc-protocol"},{"text":"07. Process Model","link":"/spec/03-runtime/07-process-model"},{"text":"08. Error Codes","link":"/spec/03-runtime/08-error-codes"},{"text":"09. Logging and Observability","link":"/spec/03-runtime/09-logging-and-observability"},{"text":"10. Session, Plan, and Goal State Machine","link":"/spec/03-runtime/10-session-state-machine"},{"text":"11. Provider & Model System","link":"/spec/03-runtime/11-provider-model-system"},{"text":"12. Provider Config Schema","link":"/spec/03-runtime/12-provider-config-schema"},{"text":"13. Model Catalog & Selection","link":"/spec/03-runtime/13-model-catalog-and-selection"},{"text":"14. Secrets Storage","link":"/spec/03-runtime/14-secrets-storage"},{"text":"15. Workspace Ignore Rules","link":"/spec/03-runtime/15-workspace-ignore-rules"},{"text":"16. Tool Result Limits & Truncation","link":"/spec/03-runtime/16-tool-result-limits"},{"text":"17. asktool Interactive Questions","link":"/spec/03-runtime/17-asktool-questions"},{"text":"18. Line-Anchored Edit Contract","link":"/spec/03-runtime/18-line-anchored-edit-contract"},{"text":"19. Remote Agent Control Protocol","link":"/spec/03-runtime/19-remote-agent-control-protocol"},{"text":"20. Staff Employees","link":"/spec/03-runtime/20-staff-employees"}]},{"text":"Experience","collapsed":true,"items":[{"text":"UX","link":"/spec/04-ux/README"},{"text":"01. UI Information Architecture","link":"/spec/04-ux/01-ui-ia"},{"text":"02. i18n (English-first)","link":"/spec/04-ux/02-i18n-english-first"},{"text":"03. Permission UX","link":"/spec/04-ux/03-permission-ux"},{"text":"04. Builtin Commands","link":"/spec/04-ux/04-builtin-commands"},{"text":"05. Onboarding","link":"/spec/04-ux/05-onboarding"},{"text":"06. Settings Information Architecture","link":"/spec/04-ux/06-settings-ia"},{"text":"07. UI Design System","link":"/spec/04-ux/07-ui-design-system"},{"text":"08. Component Spec","link":"/spec/04-ux/08-component-spec"},{"text":"09. Interaction Patterns","link":"/spec/04-ux/09-interaction-patterns"},{"text":"10. WorkBuddy Benchmark — UX Spec Proposal","link":"/spec/04-ux/10-workbuddy-benchmark-ux"},{"text":"11. asktool Question Card","link":"/spec/04-ux/11-asktool-question-card"},{"text":"Composer Prompt Enhancement","link":"/spec/04-ux/12-prompt-enhancement"}]},{"text":"Security","collapsed":true,"items":[{"text":"05. Security","link":"/spec/05-security/README"},{"text":"01. Security","link":"/spec/05-security/01-security"},{"text":"Remote Agent Control Security Specification","link":"/spec/05-security/02-remote-control-security"}]},{"text":"Delivery","collapsed":true,"items":[{"text":"Delivery & Acceptance","link":"/spec/06-delivery/README"},{"text":"01. MVP Milestones","link":"/spec/06-delivery/01-mvp-milestones"},{"text":"02. Acceptance Criteria","link":"/spec/06-delivery/02-acceptance-criteria"},{"text":"03. AI-Assisted Development Workflow","link":"/spec/06-delivery/03-ai-development-workflow"},{"text":"04. E2E Test Plan","link":"/spec/06-delivery/04-e2e-test-plan"},{"text":"05. Change Checklist","link":"/spec/06-delivery/05-change-checklist"},{"text":"06. Desktop Release Runbook","link":"/spec/06-delivery/06-release-runbook"},{"text":"Remote Agent Control Rollout and Acceptance","link":"/spec/06-delivery/07-remote-control-rollout"}]},{"text":"Plugins","collapsed":true,"items":[{"text":"Plugin System","link":"/spec/07-plugins/README"},{"text":"01. Plugin System","link":"/spec/07-plugins/01-plugin-system"},{"text":"02. Plugin Manifest Schema","link":"/spec/07-plugins/02-plugin-manifest-schema"},{"text":"03. Plugin API","link":"/spec/07-plugins/03-plugin-api"},{"text":"04. Plugin Security","link":"/spec/07-plugins/04-plugin-security"},{"text":"05. Plugin Lifecycle","link":"/spec/07-plugins/05-plugin-lifecycle"},{"text":"06. Plugin Packaging","link":"/spec/07-plugins/06-plugin-packaging"},{"text":"07. Plugin Marketplace","link":"/spec/07-plugins/07-plugin-marketplace"},{"text":"08. Plugin Signing and Updates","link":"/spec/07-plugins/08-plugin-signing-updates"},{"text":"09. Plugin Command Palette","link":"/spec/07-plugins/09-plugin-command-palette"},{"text":"10. Plugin Developer Experience","link":"/spec/07-plugins/10-plugin-devex"},{"text":"11. Plugin Storage Isolation","link":"/spec/07-plugins/11-plugin-storage-isolation"},{"text":"12. Plugin IPC and Host Services","link":"/spec/07-plugins/12-plugin-ipc-and-host-services"},{"text":"13. Plugin Permissions Matrix","link":"/spec/07-plugins/13-plugin-permissions-matrix"},{"text":"14. Plugin Roadmap","link":"/spec/07-plugins/14-plugin-roadmap"},{"text":"15. Plugin Center","link":"/spec/07-plugins/15-plugin-center"},{"text":"16. Trusted Extensions","link":"/spec/07-plugins/16-trusted-extensions"}]},{"text":"Decisions & metadata","collapsed":true,"items":[{"text":"Meta","link":"/spec/08-meta/README"},{"text":"Decisions Log","link":"/spec/08-meta/decisions-log"},{"text":"Open Questions","link":"/spec/08-meta/open-questions"}]}],"/adr/":[{"text":"Architecture decisions","items":[{"text":"ADR index","link":"/adr/README"},{"text":"Documentation site","link":"/adr/0079-vitepress-documentation-site"},{"text":"Latest decisions","link":"/spec/08-meta/decisions-log"}]},{"text":"All decisions","collapsed":true,"items":[{"text":"ADR 0001: Use Electron as the desktop shell","link":"/adr/0001-use-electron"},{"text":"ADR 0002: Use the pi Agent Harness as the kernel","link":"/adr/0002-use-pi-agent-harness"},{"text":"ADR 0003: Hybrid runtime — Rust host core + Node pi agent sidecar","link":"/adr/0003-agent-in-main-process"},{"text":"ADR 0004: No remote Gateway in the MVP","link":"/adr/0004-no-gateway-in-mvp"},{"text":"ADR 0005: User-installable plugin system","link":"/adr/0005-user-installable-plugin-system"},{"text":"ADR 0006: Postpone the plugin marketplace; build the local plugin runtime first","link":"/adr/0006-plugin-marketplace-postponed"},{"text":"ADR 0007: Plugin distribution package format uses .piplug (zip)","link":"/adr/0007-plugin-package-format"},{"text":"ADR 0008: Plugin runtime targets isolation in a separate process","link":"/adr/0008-plugin-runtime-isolation-target"},{"text":"ADR 0009: English-first globalization","link":"/adr/0009-english-first-globalization"},{"text":"ADR 0010: Use Rust as backend host core","link":"/adr/0010-rust-backend-host-core"},{"text":"ADR 0011: Freeze host RPC, storage ownership, and mode defaults","link":"/adr/0011-host-rpc-and-storage-defaults"},{"text":"ADR 0012: Universal provider/model coverage via pi-ai + OpenAI-compatible extensibility","link":"/adr/0012-universal-provider-model-coverage"},{"text":"ADR 0013: Consolidate settings navigation into four destinations","link":"/adr/0013-compact-settings-directory"},{"text":"ADR 0014: Adopt host-owned storage schema v2","link":"/adr/0014-host-owned-storage-schema-v2"},{"text":"ADR 0015: Make settings content responsive to window width","link":"/adr/0015-responsive-settings-content"},{"text":"ADR 0016: Organize the sidebar around retained multi-project tabs","link":"/adr/0016-sidebar-organization-and-multi-project-tabs"},{"text":"ADR 0017: Remove composer workspace context rail","link":"/adr/0017-remove-composer-workspace-context-rail"},{"text":"ADR 0018: Carry thinking mode through the complete session pipeline","link":"/adr/0018-end-to-end-thinking-mode"},{"text":"ADR 0019: Work panel subsystems (embedded browser, git review, file browsing)","link":"/adr/0019-work-panel-subsystems"},{"text":"ADR 0020: Configuration provider studio","link":"/adr/0020-configuration-provider-studio"},{"text":"ADR 0021: Platform application chrome","link":"/adr/0021-platform-application-chrome"},{"text":"ADR 0022: Application Update Delivery","link":"/adr/0022-application-update-delivery"},{"text":"ADR 0023: Independent Conversation Session Fork","link":"/adr/0023-independent-conversation-session-fork"},{"text":"ADR 0024: Composer Slash Commands and @ File References","link":"/adr/0024-composer-commands-and-file-references"},{"text":"ADR 0025: Keep Application Menus out of Windows/Linux Windows","link":"/adr/0025-menu-free-windows-linux-chrome"},{"text":"ADR 0026: Move the Projects Index into Settings as an Archive","link":"/adr/0026-project-archive-in-settings"},{"text":"ADR 0027: Make pi-ai authoritative for model metadata","link":"/adr/0027-pi-model-catalog-authority"},{"text":"ADR 0028: Scope work-panel runtime contexts to conversations","link":"/adr/0028-session-scoped-work-panel-contexts"},{"text":"ADR 0029: Separate native-window and work-panel resize ownership","link":"/adr/0029-separate-window-and-panel-resize-ownership"},{"text":"ADR 0030: Turn-boundary context checkpoint compaction","link":"/adr/0030-turn-boundary-context-checkpoint-compaction"},{"text":"ADR 0031: Keep composer prompt rows free of brand icons","link":"/adr/0031-icon-free-composer-prompt-row"},{"text":"ADR 0032: Reserve native width for the docked work panel","link":"/adr/0032-reserve-native-width-for-the-docked-work-panel"},{"text":"ADR 0033: Internal-dock work panel (no native window expansion)","link":"/adr/0033-internal-dock-work-panel"},{"text":"ADR 0034: Merge the command palette into global search","link":"/adr/0034-merge-command-palette-into-global-search"},{"text":"ADR 0035: Surface the OS locale through the preload bridge","link":"/adr/0035-surface-os-locale-via-preload"},{"text":"ADR 0036: Split Settings into AI and Shortcuts destinations","link":"/adr/0036-settings-ia-ai-and-shortcuts-tabs"},{"text":"ADR 0037: Resolve project instructions in Electron main","link":"/adr/0037-project-instruction-chain"},{"text":"ADR 0038: Bridge plugin-declared MCP servers in Electron main","link":"/adr/0038-plugin-mcp-bridge"},{"text":"ADR 0039: Activate plugin skills and ship plugin authoring as a first-party devkit","link":"/adr/0039-plugin-skills-activation-and-devkit"},{"text":"ADR 0040: Resident plugin services and the inter-plugin message bus","link":"/adr/0040-plugin-resident-services-and-message-bus"},{"text":"ADR 0041: Bound host runtime resources and decouple message persistence","link":"/adr/0041-bounded-host-runtime-and-persistence-outbox"},{"text":"ADR 0042: Message-scoped inline review cards","link":"/adr/0042-message-scoped-inline-review-cards"},{"text":"ADR 0043: Message-owned review snapshots and guarded rollback","link":"/adr/0043-message-owned-review-snapshots-and-rollback"},{"text":"ADR 0044: Session-bound project instruction preflight","link":"/adr/0044-session-bound-project-instruction-preflight"},{"text":"ADR 0045: Bash tool inherits the user's login-shell PATH","link":"/adr/0045-bash-inherits-user-login-path"},{"text":"ADR 0046: Categorized process log files","link":"/adr/0046-categorized-process-logs"},{"text":"ADR 0047: Context usage inspector with exact and estimated token sources","link":"/adr/0047-context-usage-inspector"},{"text":"ADR 0048: Lazy per-turn tool activation","link":"/adr/0048-lazy-per-turn-tool-activation"},{"text":"ADR 0049: Recover automatic context compaction failures with a retained tail","link":"/adr/0049-context-compaction-failure-recovery"},{"text":"ADR 0050: Bounded provider stream recovery and diagnostics","link":"/adr/0050-bounded-provider-stream-recovery"},{"text":"ADR 0051: Isolate host RPC stdio from the Tokio blocking pool","link":"/adr/0051-host-rpc-stdio-resource-isolation"},{"text":"ADR 0052: Plan operating state and approval boundary","link":"/adr/0052-plan-operating-state-and-approval-boundary"},{"text":"ADR 0053: Plan checkpoint artifact, approval, and execution epoch","link":"/adr/0053-plan-checkpoint-artifact-and-execution-epoch"},{"text":"ADR 0054: Selectable command shell catalog and execution identity","link":"/adr/0054-selectable-command-shell-catalog"},{"text":"ADR 0055: Agent-only mode; Chat becomes an internal read-only profile","link":"/adr/0055-agent-only-mode"},{"text":"ADR 0056: User-owned MCP servers and skills, with a shared activation scope","link":"/adr/0056-extension-activation-scope"},{"text":"ADR 0057: Permission-gated external paths and portable native search","link":"/adr/0057-permission-gated-external-paths-and-portable-search"},{"text":"ADR 0058: Extensions Page Density and Theme-Readable Button Surfaces","link":"/adr/0058-extensions-page-density-and-button-contrast"},{"text":"ADR 0059: Persist Composer Clipboard Files in Session Scratch","link":"/adr/0059-composer-clipboard-files-in-session-scratch"},{"text":"ADR 0060: Archive the Regenerate Branch Under the RPC Lock","link":"/adr/0060-regenerate-branch-archive-under-the-rpc-lock"},{"text":"ADR 0061: Imperceptible background context compaction","link":"/adr/0061-imperceptible-background-context-compaction"},{"text":"ADR 0062: Bounded Subagents Behind a Task Tool","link":"/adr/0062-bounded-subagents-behind-a-task-tool"},{"text":"ADR 0063: A Managed Surface for Global Subagent Definitions","link":"/adr/0063-subagent-management-ui"},{"text":"ADR 0064: Codex-parity context compaction","link":"/adr/0064-codex-parity-context-compaction"},{"text":"ADR 0065: Smooth shell layout and stream feedback","link":"/adr/0065-smooth-shell-layout-and-stream-feedback"},{"text":"ADR 0066: Empty home direct bottom composer","link":"/adr/0066-empty-home-direct-bottom-composer"},{"text":"ADR 0067: ChatGPT-inspired empty-home starter guidance","link":"/adr/0067-chatgpt-inspired-empty-home-starters"},{"text":"ADR 0068: Add a keyboard entry point for the work panel","link":"/adr/0068-work-panel-keyboard-entry"},{"text":"ADR 0069: Make native-tool path mistakes recoverable","link":"/adr/0069-recoverable-native-tool-path-errors"},{"text":"ADR 0070: Separate Composer File-Reference Display from Prompt Serialization","link":"/adr/0070-separate-composer-file-reference-display-from-prompt"},{"text":"ADR 0071: Adopt an Apple-Inspired Global Corner Hierarchy","link":"/adr/0071-apple-inspired-global-corner-hierarchy"},{"text":"ADR 0072: Add a global plugin launcher","link":"/adr/0072-global-plugin-launcher"},{"text":"ADR 0073: Stage next-turn composer configuration and preserve stopped throughput","link":"/adr/0073-next-turn-composer-configuration-and-stopped-throughput"},{"text":"ADR 0074: Native notification permission for plugins","link":"/adr/0074-native-notification-permission-for-plugins"},{"text":"ADR 0075: Manual reload for development-plugin permission ceilings","link":"/adr/0075-manual-development-plugin-reload"},{"text":"ADR 0076: Capture the Windows-reserved plugin launcher chord in host-core","link":"/adr/0076-windows-reserved-global-shortcut-fallback"},{"text":"ADR 0077: Add an interactive multi-question asktool","link":"/adr/0077-asktool-interactive-multi-question-flow"},{"text":"ADR 0078: Cross-platform tray-resident minimize","link":"/adr/0078-cross-platform-tray-resident-minimize"},{"text":"ADR 0079: Use VitePress for the bilingual documentation site","link":"/adr/0079-vitepress-documentation-site"},{"text":"ADR 0080: Prewarm the global plugin launcher after boot","link":"/adr/0080-prewarm-global-plugin-launcher"},{"text":"ADR 0081: Host-owned cross-platform plugin panel chrome","link":"/adr/0081-host-owned-plugin-panel-chrome"},{"text":"ADR 0082: Localized and page-adaptive plugin panel chrome","link":"/adr/0082-localized-plugin-panel-chrome"},{"text":"ADR 0083: Custom global UI font","link":"/adr/0083-custom-global-ui-font"},{"text":"ADR 0084: Defer new-task session creation until the first message","link":"/adr/0084-deferred-new-task-session-creation"},{"text":"ADR 0085: Make the work panel shortcut a toggle","link":"/adr/0085-work-panel-shortcut-toggle"},{"text":"ADR 0086: Keep macOS on the regular activation policy","link":"/adr/0086-macos-regular-activation-policy"},{"text":"ADR 0087: Replace textual Edit matching with a line-anchored, tag-verified contract","link":"/adr/0087-line-anchored-edit-contract"},{"text":"ADR 0088: Plugin file access is declared per mode, and deletion is recoverable","link":"/adr/0088-declared-file-scope-for-plugins"},{"text":"ADR 0089: Proactive Background Subagent Delegation","link":"/adr/0089-proactive-background-subagent-delegation"},{"text":"ADR 0090: User-Configurable Close Behavior with Close-to-Tray","link":"/adr/0090-user-configurable-close-behavior-close-to-tray"},{"text":"ADR 0091: Route provider rate limits through bounded same-turn retry","link":"/adr/0091-rate-limit-same-turn-retry"},{"text":"ADR 0092: Use a plugin-owned surface with a host window-control capsule","link":"/adr/0092-plugin-owned-panel-surface"},{"text":"ADR 0093: Keep a strict 46px plugin drag band with a minimal capsule","link":"/adr/0093-plugin-panel-strict-drag-band"},{"text":"ADR 0094: Admit one desktop instance per data directory","link":"/adr/0094-single-instance-per-data-directory"},{"text":"ADR 0095: Sign in with a vendor account instead of pasting an API key","link":"/adr/0095-vendor-account-oauth-login"},{"text":"ADR 0096: Flatten the Settings directory and colocate marketplace source configuration","link":"/adr/0096-flat-settings-directory-and-marketplace-context"},{"text":"ADR 0097: Place global defaults under the AI settings destination","link":"/adr/0097-place-global-defaults-under-ai-settings"},{"text":"ADR 0098: Treat every vendor OAuth account as an independent provider row","link":"/adr/0098-multiple-vendor-oauth-accounts"},{"text":"ADR 0099: Add titled visual clusters to the Settings directory","link":"/adr/0099-titled-settings-navigation-clusters"},{"text":"ADR 0100: Make builtin subagents inherit the parent permission mode","link":"/adr/0100-builtin-subagents-inherit-parent-permission-mode"},{"text":"ADR 0101: Model-aware image attachment transport","link":"/adr/0101-model-aware-image-attachments"},{"text":"ADR 0102: Publisher-owned plugin source with a Git-hosted artifact store","link":"/adr/0102-publisher-owned-plugin-source-and-git-hosted-artifacts"},{"text":"ADR 0103: Compact context usage summary","link":"/adr/0103-compact-context-usage-summary"},{"text":"ADR 0104: Plugin-contributed work panel views","link":"/adr/0104-plugin-contributed-work-panel-views"},{"text":"ADR 0105: Ship Files as a bundled plugin; keep Review in the host","link":"/adr/0105-files-as-a-bundled-plugin"},{"text":"ADR 0106: Keep only five core builtin commands","link":"/adr/0106-core-five-builtin-commands"},{"text":"ADR 0107: Make current-session task notification suppression atomic","link":"/adr/0107-atomic-viewing-context-for-task-notifications"},{"text":"ADR 0108: Remove the built-in interactive terminal","link":"/adr/0108-remove-built-in-interactive-terminal"},{"text":"ADR 0109: Open Files entries with the OS-associated application","link":"/adr/0109-open-files-with-the-os-associated-application"},{"text":"ADR 0110: Version the plugin panel chrome spacing contract","link":"/adr/0110-plugin-panel-chrome-spacing-contract"},{"text":"ADR 0111: Reveal Files in the OS File Manager","link":"/adr/0111-reveal-files-in-file-manager"},{"text":"ADR 0112: Agent Capability Management Roots and Settings IA","link":"/adr/0112-agent-capability-management-roots-and-settings-ia"},{"text":"ADR 0113: Persist the New Task empty slot immediately and deduplicate it by message count","link":"/adr/0113-immediate-empty-session-slot-and-deduplication"},{"text":"ADR 0114: Persist Provider Model Bindings and Thinking Configuration","link":"/adr/0114-provider-model-bindings-and-thinking-configuration"},{"text":"ADR 0115: Keep plugin clipboard history host-owned and in memory","link":"/adr/0115-plugin-clipboard-history-is-host-owned"},{"text":"ADR 0116: Add OpenCode Go as a Fixed Provider Preset","link":"/adr/0116-opencode-go-provider-preset"},{"text":"ADR 0117: Preserve the Windows taskbar entry for native minimize","link":"/adr/0117-windows-taskbar-native-minimize"},{"text":"ADR 0118: Keep queued prompts renderer-owned and stop runs at turn boundaries","link":"/adr/0118-renderer-owned-queued-prompts"},{"text":"ADR 0119: Event-Driven Subagent Timeouts","link":"/adr/0119-event-driven-subagent-timeouts"},{"text":"ADR 0120: Bounded Session History Windows","link":"/adr/0120-bounded-session-history-windows"},{"text":"ADR 0121: Keep Composer prompt enhancement one-shot and main-owned","link":"/adr/0121-one-shot-composer-prompt-enhancement"},{"text":"ADR 0122: Reserve native width while the work panel is visible","link":"/adr/0122-reserve-native-width-while-work-panel-visible"},{"text":"ADR 0123: Use native taskbar minimize for Windows/Linux window controls","link":"/adr/0123-native-taskbar-minimize-controls"},{"text":"ADR 0124: Bind Temporary Sessions to Their Own Scratch Workspace","link":"/adr/0124-temporary-session-scratch-workspace"},{"text":"ADR 0125: Renderer Ships Derived Brand Marks and Minified Output","link":"/adr/0125-renderer-derived-brand-marks-and-minified-output"},{"text":"ADR 0126: Agent Capability Pages Are One Workbench That Can Author","link":"/adr/0126-agent-capability-workbench"},{"text":"ADR 0127: Transcript Layout Index and Identity-Based Truncation","link":"/adr/0127-transcript-layout-index-and-identity-truncation"},{"text":"ADR 0128: Share one bounded budget for transient provider failures","link":"/adr/0128-bounded-transient-provider-retry"},{"text":"ADR 0129: The Subagent Idle Watchdog Bounds Silence, Not Slowness","link":"/adr/0129-idle-watchdog-bounds-silence"},{"text":"ADR 0130: Bounded Mounted Transcript Window","link":"/adr/0130-bounded-mounted-transcript-window"},{"text":"ADR 0131: Spill Large Composer Text Pastes into Session Scratch","link":"/adr/0131-large-text-paste-session-reference"},{"text":"ADR 0132: Attribute cross-display window moves to the user","link":"/adr/0132-attribute-cross-display-window-moves-to-the-user"},{"text":"ADR 0133: Use models.dev as the primary model catalog with pi-ai fallback","link":"/adr/0133-models-dev-primary-catalog-with-pi-fallback"},{"text":"ADR 0134: Use models.dev as the sole model metadata source with a local snapshot","link":"/adr/0134-models-dev-sole-model-metadata-source"},{"text":"ADR 0135: Retry unchanged edited prompts","link":"/adr/0135-retry-unchanged-edited-prompts"},{"text":"ADR 0136: Preserve the active task boundary across context compaction","link":"/adr/0136-active-task-boundary-across-compaction"},{"text":"ADR 0137: Retained Session Panes","link":"/adr/0137-retained-session-panes"},{"text":"ADR 0138: Subagent Peer Messaging","link":"/adr/0138-subagent-peer-messaging"},{"text":"ADR 0140: Fold the Three Peer Tools Into One Peer Tool","link":"/adr/0140-peer-tool-consolidation"},{"text":"ADR 0141: Make Expanded Sidebar Width User-Resizable","link":"/adr/0141-sidebar-width-resize"},{"text":"ADR 0142: Allow non-loopback HTTP MCP endpoints with explicit risk disclosure","link":"/adr/0142-allow-non-loopback-http-mcp"},{"text":"ADR 0143: Make Session Titles User-Renamable","link":"/adr/0143-user-renamable-session-titles"},{"text":"ADR 0144: Allow User-Configured Thinking-Level Overrides","link":"/adr/0144-user-configurable-thinking-level-overrides"},{"text":"ADR 0145: Publish Native macOS Intel Artifacts","link":"/adr/0145-native-macos-intel-release-lane"},{"text":"ADR 0146: Assign outer and inner work-panel resize ownership by boundary","link":"/adr/0146-separate-work-panel-and-chat-resize-ownership"},{"text":"ADR 0147: A2A Protocol Stack for Subagent Coordination","link":"/adr/0147-a2a-protocol-stack"},{"text":"ADR 0148: Explicitly disable application keyboard shortcuts","link":"/adr/0148-explicitly-disable-keyboard-shortcuts"},{"text":"ADR 0149: Calm transcript running-status motion","link":"/adr/0149-calm-transcript-running-status-motion"},{"text":"ADR 0150: Inline SVG empty-home agent mark","link":"/adr/0150-inline-svg-empty-home-agent-mark"},{"text":"ADR 0151: Keep the work panel inside the fixed application window","link":"/adr/0151-internal-work-panel-dock"},{"text":"ADR 0152: Eight-frame empty-home mascot GIF","link":"/adr/0152-eight-frame-empty-home-mascot-gif"},{"text":"ADR 0153: Checkpoint the streaming reply beside the transcript","link":"/adr/0153-inflight-reply-checkpoint"},{"text":"ADR 0154: Reveal the New Task empty destination before host IO","link":"/adr/0154-reveal-new-task-empty-destination"},{"text":"ADR 0155: Add Zhipu / Z.AI Named Endpoint Presets","link":"/adr/0155-zhipu-endpoint-presets"},{"text":"ADR 0156: Simplify the Add-Provider Common Path","link":"/adr/0156-simplify-add-provider-common-path"},{"text":"ADR 0157: Main-owned GitHub issue feedback","link":"/adr/0157-main-owned-github-issue-feedback"},{"text":"ADR 0158: Keep approval cards focused and remember the selected mode","link":"/adr/0158-approval-card-focus-and-remembered-mode"},{"text":"ADR 0159: Generated plugin settings and plugin-local shortcuts","link":"/adr/0159-plugin-generated-settings-and-local-shortcuts"},{"text":"ADR 0160: Shipped locale registry and searchable language picker","link":"/adr/0160-shipped-locale-registry-and-language-picker"},{"text":"ADR 0161: Searchable theme picker matching language","link":"/adr/0161-searchable-theme-picker"},{"text":"ADR 0162: Cross-session A2A addressing","link":"/adr/0162-cross-session-a2a-addressing"},{"text":"ADR 0163: Transcript File References Render as Previewable Chips","link":"/adr/0163-transcript-file-reference-chips"},{"text":"ADR 0164: Parent agents collaborate across conversations","link":"/adr/0164-parent-cross-conversation-a2a"},{"text":"ADR 0165: Withdraw the A2A / Peer coordination stack","link":"/adr/0165-withdraw-a2a-peer-stack"},{"text":"ADR 0166: Parent-judged subagent lifetime","link":"/adr/0166-parent-judged-subagent-lifetime"},{"text":"ADR 0167: Agent-chosen Bash timeout","link":"/adr/0167-agent-chosen-bash-timeout"},{"text":"ADR 0168: Main-owned http(s)/mailto allowlist for openExternal","link":"/adr/0168-main-owned-open-external-allowlist"},{"text":"ADR 0169: Classified file preview and live workspace events for plugin views","link":"/adr/0169-classified-file-preview-and-live-workspace-events"},{"text":"ADR 0170: Ship the work-panel browser as a bundled plugin over public CDP","link":"/adr/0170-work-panel-browser-as-bundled-plugin"},{"text":"ADR 0171: Host-owned completed-turn token history","link":"/adr/0171-host-owned-completed-turn-token-history"},{"text":"ADR 0172: Contained in-chat image display","link":"/adr/0172-contained-in-chat-image-display"},{"text":"ADR 0173: Plugin-owned token usage dashboard","link":"/adr/0173-plugin-owned-token-usage-dashboard"},{"text":"ADR 0174: Host-owned plugin completions and session context","link":"/adr/0174-plugin-host-owned-completion-and-session-context"},{"text":"ADR 0175: Explain quiet active turns with live agent activity status","link":"/adr/0175-live-agent-activity-status"},{"text":"ADR 0176: Per-provider User-Agent override","link":"/adr/0176-per-provider-user-agent"},{"text":"ADR 0177: User-configurable outbound proxy","link":"/adr/0177-user-configurable-outbound-proxy"},{"text":"ADR 0178: Per-provider custom HTTP headers","link":"/adr/0178-per-provider-custom-headers"},{"text":"ADR 0179: Import model configuration from local agent stores","link":"/adr/0179-import-model-configuration"},{"text":"ADR 0180: Custom global UI type scale","link":"/adr/0180-custom-reading-font-size"},{"text":"ADR 0181: Main-owned picker capabilities","link":"/adr/0181-main-owned-picker-capabilities"},{"text":"ADR 0182: Traditional Chinese shell locale","link":"/adr/0182-traditional-chinese-shell-locale"},{"text":"ADR 0183: P0 international shell locales","link":"/adr/0183-p0-international-shell-locales"},{"text":"ADR 0184: Dock the context usage inspector in the composer toolbar","link":"/adr/0184-composer-context-usage-inspector"},{"text":"ADR 0185: Korean shell locale","link":"/adr/0185-korean-shell-locale"},{"text":"ADR 0186: Summarize First-Turn Session Titles with a Main-Owned One-Shot","link":"/adr/0186-session-auto-title-summary"},{"text":"ADR 0187: Focus-Aware Native Task Notifications","link":"/adr/0187-focus-aware-native-task-notifications"},{"text":"ADR 0188: Preserve distinct credentials during model configuration import","link":"/adr/0188-preserve-distinct-import-credentials"},{"text":"ADR 0189: Parent fatal error aborts leftover delegates","link":"/adr/0189-parent-fatal-error-aborts-leftover-delegates"},{"text":"ADR 0190: Host-gated large-file and dropped-file access","link":"/adr/0190-host-gated-large-file-and-drop-access"},{"text":"ADR 0191: Label Both macOS Release Architectures","link":"/adr/0191-label-both-macos-release-architectures"},{"text":"ADR 0192: Alias a configured model and make model ids copyable","link":"/adr/0192-model-alias"},{"text":"ADR 0193: Last-request occupancy in the context inspector","link":"/adr/0193-last-request-context-occupancy"},{"text":"ADR 0194: Optional subagent thinking override","link":"/adr/0194-subagent-thinking-parameter-omission"},{"text":"ADR 0195: Viewport-fixed work panel toggle","link":"/adr/0195-viewport-fixed-work-panel-toggle"},{"text":"ADR 0196: Show provider retry causes in the active-turn status","link":"/adr/0196-retry-cause-in-active-turn-status"},{"text":"ADR 0197: Publish a Windows Portable Executable","link":"/adr/0197-windows-portable-exe"},{"text":"ADR 0198: Name every quiet interval on the live activity row","link":"/adr/0198-quiet-interval-activity-phases"},{"text":"ADR 0200: Host-Owned Plugin Session Import and Ownership API","link":"/adr/0200-plugin-owned-session-api"},{"text":"ADR 0201: Explicit Plugin Project IDs and Host-Owned Session Refresh","link":"/adr/0201-plugin-project-ids-and-session-refresh"},{"text":"ADR 0202: Expose Effective Subagent Thinking Metadata","link":"/adr/0202-effective-subagent-thinking-metadata"},{"text":"ADR 0203: Local MCP Control Plane for Desktop Operations","link":"/adr/0203-local-mcp-control-plane"},{"text":"ADR 0204: Explicit Unsigned macOS First-Launch Helper","link":"/adr/0204-unsigned-macos-first-launch-helper"},{"text":"ADR 0205: Remote Agent Control Uses a Dedicated Host Boundary","link":"/adr/0205-remote-agent-control-boundary"},{"text":"ADR 0206: Extend provider retries and show bounded progress","link":"/adr/0206-ten-provider-retries-and-progress-status"},{"text":"ADR 0207: Allow Three Same-Path Mutation Recovery Failures","link":"/adr/0207-three-mutation-recovery-failures"},{"text":"ADR 0208: Plugin Desktop Control Requires Native User Consent","link":"/adr/0208-plugin-desktop-control-native-consent"},{"text":"ADR 0209: PowerShell 7 as a selectable Windows command shell","link":"/adr/0209-powershell-7-selectable-windows-shell"},{"text":"ADR 0210: Subagent output-token cap","link":"/adr/0210-subagent-output-token-cap"},{"text":"ADR 0211: Plan-Safe Plugin Actions for Read-Only Inspection","link":"/adr/0211-plan-safe-plugin-actions"},{"text":"ADR 0212: Remove diagnostic timing log streams","link":"/adr/0212-remove-diagnostic-timing-log-streams"},{"text":"ADR 0213: Persist the Host-owned turn queue in host-core","link":"/adr/0213-persist-host-owned-turn-queue"},{"text":"ADR 0214: Trusted extensions run in the Agent sidecar","link":"/adr/0214-trusted-extensions"},{"text":"ADR 0215: Agent extensions are a plugin contribution","link":"/adr/0215-agent-extensions-as-plugin-contribution"},{"text":"ADR 0216: Truncate Regenerates Under the RPC Lock","link":"/adr/0216-truncate-regenerates-under-the-rpc-lock"},{"text":"ADR 0217: Host Stdout Sender Must Not Outlive Serve","link":"/adr/0217-host-stdout-sender-must-not-outlive-serve"},{"text":"ADR 0218: Effective Image-Input Overrides Across Composer and Transport","link":"/adr/0218-effective-image-input-overrides"},{"text":"ADR 0219: User-Invoked Skills in the Composer Slash Menu","link":"/adr/0219-user-invoked-skills-in-composer"},{"text":"ADR 0220: Keep Windows Work-Panel Chrome Single-Purpose","link":"/adr/0220-windows-work-panel-toolbar"},{"text":"ADR 0221: Render Canonical Thinking-Level Values Without Translation","link":"/adr/0221-canonical-thinking-level-values"},{"text":"ADR 0222: Native File and Folder Drops in the Composer","link":"/adr/0222-composer-native-file-folder-drops"},{"text":"ADR 0223: Context Usage Display Preference","link":"/adr/0223-context-usage-display-preference"},{"text":"ADR 0224: Right Panel Tab Strip and Data-Driven Add Menu","link":"/adr/0224-right-panel-tab-strip"},{"text":"ADR 0225: Restore Deferred Tools from Effective Session Context","link":"/adr/0225-restore-deferred-tools-from-effective-session-context"},{"text":"ADR 0226: Reserve Chat Width for Composer Controls","link":"/adr/0226-reserve-chat-width-for-composer-controls"},{"text":"ADR 0227: Project group manual ordering","link":"/adr/0227-project-group-manual-ordering"},{"text":"ADR 0228: Long-press the project title to reorder","link":"/adr/0228-long-press-project-title-reorder"},{"text":"ADR 0229: Press-and-move project title reorder","link":"/adr/0229-press-and-move-project-title-reorder"},{"text":"ADR 0230: Skill Ships with the Agent Core Tool Set","link":"/adr/0230-skill-ships-with-the-agent-core-tool-set"},{"text":"ADR 0231: Ideographic Comma Opens the Composer Slash Menu","link":"/adr/0231-ideographic-comma-opens-the-slash-menu"},{"text":"ADR 0232: Keep macOS DMG Opening Guidance Text-Only","link":"/adr/0232-macos-dmg-text-only-opening-guidance"},{"text":"ADR 0233: Renderer-Owned Multi-Folder Project Creation","link":"/adr/0233-renderer-owned-multi-folder-project-creation"},{"text":"ADR 0234: Keep project memory host-owned and path-scoped","link":"/adr/0234-project-owned-memory-context"},{"text":"ADR 0235: Preserve Domain Facades and Enforce Architecture Budgets","link":"/adr/0235-domain-facades-and-architecture-budgets"},{"text":"ADR 0236: Restore Archived Projects When Session Import Adds a Bound Session","link":"/adr/0236-restore-archived-project-on-session-import"},{"text":"ADR 0237: Keep Session Orchestration in an Official Plugin","link":"/adr/0237-session-orchestrator-as-a-plugin"},{"text":"ADR 0238: Prioritize MainChat in the three-column shell","link":"/adr/0238-three-column-width-priority"},{"text":"ADR 0239: Host-owned session collaboration messages","link":"/adr/0239-session-collaboration-messages"},{"text":"ADR 0240: Independent session discovery and navigable collaboration projections","link":"/adr/0240-independent-session-discovery-and-navigable-projections"},{"text":"ADR 0241: Ship the file view as a vendored, updatable plugin","link":"/adr/0241-vendored-updatable-file-view-plugin"},{"text":"ADR 0242: Delta-only coalesced streaming updates","link":"/adr/0242-delta-only-streaming-updates"},{"text":"ADR 0243: Skill market public-HTTPS catalog fetch","link":"/adr/0243-skill-market-public-https-catalog"},{"text":"ADR 0244: Bound dependency installation for imported extensions","link":"/adr/0244-imported-extension-dependency-boundary"},{"text":"ADR 0245: Harden the MCP market public-network boundary","link":"/adr/0245-mcp-market-public-network-boundary"},{"text":"ADR 0246: Opt-in subagent inheritance of the parent tool catalog","link":"/adr/0246-subagent-opt-in-parent-tool-inherit"},{"text":"ADR 0247: Git clone accepts only syntactically public hosts","link":"/adr/0247-git-clone-public-hostname"},{"text":"ADR 0248 — Package theme assets and contributed window backgrounds","link":"/adr/0248-plugin-theme-assets-and-window-background"},{"text":"ADR 0249: ChatGPT-Style Logical Project Groups","link":"/adr/0249-chatgpt-style-logical-project-groups"},{"text":"ADR 0249 — Plugin runtime theme APIs and sidebar image token","link":"/adr/0249-plugin-runtime-theme-apis"},{"text":"ADR 0250 — Structured, bounded, and redacted process logs","link":"/adr/0250-structured-bounded-redacted-process-logs"},{"text":"ADR 0251 — pi-go Go agent sidecar","link":"/adr/0251-pi-go-agent-sidecar"},{"text":"0252. Staff employees: referenced credentials and promotion-gated shared memory","link":"/adr/0252-staff-employees-and-shared-memory"},{"text":"0253. Site sessions: a real signed-in browser instead of a forged request","link":"/adr/0253-site-sessions-for-staff-collection"},{"text":"0254. Middle-manager boards: the rows employees file, and the one door that turns a proposal into work","link":"/adr/0254-middle-manager-boards"},{"text":"ADR 0255: Host-owned browser chrome over a Tauri child webview","link":"/adr/0255-tauri-browser-guest"},{"text":"ADR 0256: Reopen the builtin command registry with guardrails","link":"/adr/0256-reopen-builtin-command-registry"},{"text":"ADR active-turn-steering: Bind Composer steering to the active durable turn","link":"/adr/active-turn-steering"},{"text":"ADR: Show pinned conversations in a global sidebar section","link":"/adr/global-sidebar-pins"},{"text":"ADR: Separate Subagent Model Opt-In from Definition Pins","link":"/adr/subagent-model-opt-in"}]}]}},"locales":{"root":{"label":"English","lang":"en"},"zh-CN":{"label":"简体中文","lang":"zh-CN","title":"PI-Desktop 文档","description":"本地优先的 AI 编程代理文档","themeConfig":{"nav":[{"text":"快速开始","link":"/zh-CN/guide/"},{"text":"规格","link":"/zh-CN/spec/README"},{"text":"ADR","link":"/zh-CN/adr/"},{"text":"插件开发","link":"/zh-CN/plugin-development"},{"text":"隐私政策（英文）","link":"/privacy-policy"},{"text":"GitHub","link":"https://github.com/vastsa/PI-Desktop"}],"sidebar":{"/zh-CN/guide/":[{"text":"指南","items":[{"text":"快速开始","link":"/zh-CN/guide/"},{"text":"界面截图","link":"/zh-CN/guide/screenshots"}]}],"/zh-CN/plugin-development":[{"text":"插件开发","items":[{"text":"从零到一","link":"/zh-CN/plugin-development"},{"text":"插件系统","link":"/zh-CN/spec/07-plugins/README"},{"text":"01. 插件系统","link":"/zh-CN/spec/07-plugins/01-plugin-system"},{"text":"02. 插件Manifest Schema","link":"/zh-CN/spec/07-plugins/02-plugin-manifest-schema"},{"text":"03. 插件 API","link":"/zh-CN/spec/07-plugins/03-plugin-api"},{"text":"04. 插件安全","link":"/zh-CN/spec/07-plugins/04-plugin-security"},{"text":"05. 插件生命周期","link":"/zh-CN/spec/07-plugins/05-plugin-lifecycle"},{"text":"06. 插件打包","link":"/zh-CN/spec/07-plugins/06-plugin-packaging"},{"text":"07. 插件市场","link":"/zh-CN/spec/07-plugins/07-plugin-marketplace"},{"text":"08. 插件签名和更新","link":"/zh-CN/spec/07-plugins/08-plugin-signing-updates"},{"text":"09. 插件命令面板","link":"/zh-CN/spec/07-plugins/09-plugin-command-palette"},{"text":"10. 插件开发者体验","link":"/zh-CN/spec/07-plugins/10-plugin-devex"},{"text":"11. 插件存储隔离","link":"/zh-CN/spec/07-plugins/11-plugin-storage-isolation"},{"text":"12. 插件 IPC 和主机服务","link":"/zh-CN/spec/07-plugins/12-plugin-ipc-and-host-services"},{"text":"13. 插件权限矩阵","link":"/zh-CN/spec/07-plugins/13-plugin-permissions-matrix"},{"text":"14. 插件路线图","link":"/zh-CN/spec/07-plugins/14-plugin-roadmap"},{"text":"15. 插件中心","link":"/zh-CN/spec/07-plugins/15-plugin-center"},{"text":"16. 受信任扩展","link":"/zh-CN/spec/07-plugins/16-trusted-extensions"}]}],"/zh-CN/spec/":[{"text":"从这里开始","items":[{"text":"PI-Desktop 规格","link":"/zh-CN/spec/README"},{"text":"PI-Desktop 基线冻结","link":"/zh-CN/spec/00-baseline"},{"text":"PI-Desktop 规格导航","link":"/zh-CN/spec/NAV"}]},{"text":"产品","items":[{"text":"产品及范围","link":"/zh-CN/spec/01-product/README"},{"text":"00. 概述","link":"/zh-CN/spec/01-product/00-overview"},{"text":"01. 产品范围","link":"/zh-CN/spec/01-product/01-product-scope"},{"text":"02. 非目标","link":"/zh-CN/spec/01-product/02-non-goals"}]},{"text":"架构","items":[{"text":"架构与工程","link":"/zh-CN/spec/02-architecture/README"},{"text":"01. 架构","link":"/zh-CN/spec/02-architecture/01-architecture"},{"text":"02. 技术栈","link":"/zh-CN/spec/02-architecture/02-tech-stack"},{"text":"03. 仓库结构","link":"/zh-CN/spec/02-architecture/03-repo-structure"},{"text":"文档站点","link":"/zh-CN/spec/02-architecture/04-documentation-site"},{"text":"远程 Agent 控制目标架构","link":"/zh-CN/spec/02-architecture/05-remote-agent-control"}]},{"text":"运行时","collapsed":true,"items":[{"text":"运行时核心","link":"/zh-CN/spec/03-runtime/README"},{"text":"01. IPC 协议","link":"/zh-CN/spec/03-runtime/01-ipc-protocol"},{"text":"02. Agent 运行时","link":"/zh-CN/spec/03-runtime/02-agent-runtime"},{"text":"03. 工具和权限","link":"/zh-CN/spec/03-runtime/03-tools-and-permissions"},{"text":"04. 数据存储（架构 v16）","link":"/zh-CN/spec/03-runtime/04-data-storage"},{"text":"05. Rust 主机核心","link":"/zh-CN/spec/03-runtime/05-host-core-rust"},{"text":"06. 主机 RPC 协议","link":"/zh-CN/spec/03-runtime/06-host-rpc-protocol"},{"text":"07. 进程模型","link":"/zh-CN/spec/03-runtime/07-process-model"},{"text":"08. 错误代码","link":"/zh-CN/spec/03-runtime/08-error-codes"},{"text":"09. 日志记录和可观测性","link":"/zh-CN/spec/03-runtime/09-logging-and-observability"},{"text":"10. 会话、Plan 和 Goal 状态机","link":"/zh-CN/spec/03-runtime/10-session-state-machine"},{"text":"11. 提供商和模型系统","link":"/zh-CN/spec/03-runtime/11-provider-model-system"},{"text":"12. 提供商配置架构","link":"/zh-CN/spec/03-runtime/12-provider-config-schema"},{"text":"13. 模型目录及选择","link":"/zh-CN/spec/03-runtime/13-model-catalog-and-selection"},{"text":"14. 密钥存储","link":"/zh-CN/spec/03-runtime/14-secrets-storage"},{"text":"15. 工作区忽略规则","link":"/zh-CN/spec/03-runtime/15-workspace-ignore-rules"},{"text":"16. 工具结果限制和截断","link":"/zh-CN/spec/03-runtime/16-tool-result-limits"},{"text":"17. asktool 互动问题","link":"/zh-CN/spec/03-runtime/17-asktool-questions"},{"text":"18. 行锚定编辑契约","link":"/zh-CN/spec/03-runtime/18-line-anchored-edit-contract"},{"text":"19. 远程 Agent 控制协议","link":"/zh-CN/spec/03-runtime/19-remote-agent-control-protocol"},{"text":"20. 员工（Staff Employees）","link":"/zh-CN/spec/03-runtime/20-staff-employees"}]},{"text":"用户体验","collapsed":true,"items":[{"text":"用户体验","link":"/zh-CN/spec/04-ux/README"},{"text":"01. UI信息架构","link":"/zh-CN/spec/04-ux/01-ui-ia"},{"text":"02. i18n（英语优先）","link":"/zh-CN/spec/04-ux/02-i18n-english-first"},{"text":"03. 权限用户体验","link":"/zh-CN/spec/04-ux/03-permission-ux"},{"text":"04. 内置命令","link":"/zh-CN/spec/04-ux/04-builtin-commands"},{"text":"05. 新手引导","link":"/zh-CN/spec/04-ux/05-onboarding"},{"text":"06. 设置信息架构","link":"/zh-CN/spec/04-ux/06-settings-ia"},{"text":"07. UI设计系统","link":"/zh-CN/spec/04-ux/07-ui-design-system"},{"text":"08. 组件规格","link":"/zh-CN/spec/04-ux/08-component-spec"},{"text":"09. 交互模式","link":"/zh-CN/spec/04-ux/09-interaction-patterns"},{"text":"10. WorkBuddy 基准 — 用户体验规范提案","link":"/zh-CN/spec/04-ux/10-workbuddy-benchmark-ux"},{"text":"11. asktool问题卡","link":"/zh-CN/spec/04-ux/11-asktool-question-card"},{"text":"输入框提示词增强","link":"/zh-CN/spec/04-ux/12-prompt-enhancement"}]},{"text":"安全","collapsed":true,"items":[{"text":"05. 安全","link":"/zh-CN/spec/05-security/README"},{"text":"01. 安全","link":"/zh-CN/spec/05-security/01-security"},{"text":"远程 Agent 控制安全规格","link":"/zh-CN/spec/05-security/02-remote-control-security"}]},{"text":"交付","collapsed":true,"items":[{"text":"交付及验收","link":"/zh-CN/spec/06-delivery/README"},{"text":"01. MVP 里程碑","link":"/zh-CN/spec/06-delivery/01-mvp-milestones"},{"text":"02. 验收标准","link":"/zh-CN/spec/06-delivery/02-acceptance-criteria"},{"text":"03. AI 辅助开发工作流程","link":"/zh-CN/spec/06-delivery/03-ai-development-workflow"},{"text":"04. E2E 测试 Plan","link":"/zh-CN/spec/06-delivery/04-e2e-test-plan"},{"text":"05. 更改清单","link":"/zh-CN/spec/06-delivery/05-change-checklist"},{"text":"06. 桌面发布手册","link":"/zh-CN/spec/06-delivery/06-release-runbook"},{"text":"远程 Agent 控制交付与验收","link":"/zh-CN/spec/06-delivery/07-remote-control-rollout"}]},{"text":"插件","collapsed":true,"items":[{"text":"插件系统","link":"/zh-CN/spec/07-plugins/README"},{"text":"01. 插件系统","link":"/zh-CN/spec/07-plugins/01-plugin-system"},{"text":"02. 插件Manifest Schema","link":"/zh-CN/spec/07-plugins/02-plugin-manifest-schema"},{"text":"03. 插件 API","link":"/zh-CN/spec/07-plugins/03-plugin-api"},{"text":"04. 插件安全","link":"/zh-CN/spec/07-plugins/04-plugin-security"},{"text":"05. 插件生命周期","link":"/zh-CN/spec/07-plugins/05-plugin-lifecycle"},{"text":"06. 插件打包","link":"/zh-CN/spec/07-plugins/06-plugin-packaging"},{"text":"07. 插件市场","link":"/zh-CN/spec/07-plugins/07-plugin-marketplace"},{"text":"08. 插件签名和更新","link":"/zh-CN/spec/07-plugins/08-plugin-signing-updates"},{"text":"09. 插件命令面板","link":"/zh-CN/spec/07-plugins/09-plugin-command-palette"},{"text":"10. 插件开发者体验","link":"/zh-CN/spec/07-plugins/10-plugin-devex"},{"text":"11. 插件存储隔离","link":"/zh-CN/spec/07-plugins/11-plugin-storage-isolation"},{"text":"12. 插件 IPC 和主机服务","link":"/zh-CN/spec/07-plugins/12-plugin-ipc-and-host-services"},{"text":"13. 插件权限矩阵","link":"/zh-CN/spec/07-plugins/13-plugin-permissions-matrix"},{"text":"14. 插件路线图","link":"/zh-CN/spec/07-plugins/14-plugin-roadmap"},{"text":"15. 插件中心","link":"/zh-CN/spec/07-plugins/15-plugin-center"},{"text":"16. 受信任扩展","link":"/zh-CN/spec/07-plugins/16-trusted-extensions"}]},{"text":"决策与元数据","collapsed":true,"items":[{"text":"元数据","link":"/zh-CN/spec/08-meta/README"},{"text":"决策日志","link":"/zh-CN/spec/08-meta/decisions-log"},{"text":"开放式问题","link":"/zh-CN/spec/08-meta/open-questions"}]},{"text":"兼容性参考","collapsed":true,"items":[{"text":"规格地图","link":"/zh-CN/spec/index"}]}],"/zh-CN/adr/":[{"text":"架构决策记录","items":[{"text":"ADR 索引","link":"/zh-CN/adr/"},{"text":"文档站决策","link":"/adr/0079-vitepress-documentation-site"},{"text":"最新决策","link":"/zh-CN/spec/08-meta/decisions-log"}]},{"text":"全部英文决策","collapsed":true,"items":[{"text":"ADR 0001: Use Electron as the desktop shell","link":"/adr/0001-use-electron"},{"text":"ADR 0002: Use the pi Agent Harness as the kernel","link":"/adr/0002-use-pi-agent-harness"},{"text":"ADR 0003: Hybrid runtime — Rust host core + Node pi agent sidecar","link":"/adr/0003-agent-in-main-process"},{"text":"ADR 0004: No remote Gateway in the MVP","link":"/adr/0004-no-gateway-in-mvp"},{"text":"ADR 0005: User-installable plugin system","link":"/adr/0005-user-installable-plugin-system"},{"text":"ADR 0006: Postpone the plugin marketplace; build the local plugin runtime first","link":"/adr/0006-plugin-marketplace-postponed"},{"text":"ADR 0007: Plugin distribution package format uses .piplug (zip)","link":"/adr/0007-plugin-package-format"},{"text":"ADR 0008: Plugin runtime targets isolation in a separate process","link":"/adr/0008-plugin-runtime-isolation-target"},{"text":"ADR 0009: English-first globalization","link":"/adr/0009-english-first-globalization"},{"text":"ADR 0010: Use Rust as backend host core","link":"/adr/0010-rust-backend-host-core"},{"text":"ADR 0011: Freeze host RPC, storage ownership, and mode defaults","link":"/adr/0011-host-rpc-and-storage-defaults"},{"text":"ADR 0012: Universal provider/model coverage via pi-ai + OpenAI-compatible extensibility","link":"/adr/0012-universal-provider-model-coverage"},{"text":"ADR 0013: Consolidate settings navigation into four destinations","link":"/adr/0013-compact-settings-directory"},{"text":"ADR 0014: Adopt host-owned storage schema v2","link":"/adr/0014-host-owned-storage-schema-v2"},{"text":"ADR 0015: Make settings content responsive to window width","link":"/adr/0015-responsive-settings-content"},{"text":"ADR 0016: Organize the sidebar around retained multi-project tabs","link":"/adr/0016-sidebar-organization-and-multi-project-tabs"},{"text":"ADR 0017: Remove composer workspace context rail","link":"/adr/0017-remove-composer-workspace-context-rail"},{"text":"ADR 0018: Carry thinking mode through the complete session pipeline","link":"/adr/0018-end-to-end-thinking-mode"},{"text":"ADR 0019: Work panel subsystems (embedded browser, git review, file browsing)","link":"/adr/0019-work-panel-subsystems"},{"text":"ADR 0020: Configuration provider studio","link":"/adr/0020-configuration-provider-studio"},{"text":"ADR 0021: Platform application chrome","link":"/adr/0021-platform-application-chrome"},{"text":"ADR 0022: Application Update Delivery","link":"/adr/0022-application-update-delivery"},{"text":"ADR 0023: Independent Conversation Session Fork","link":"/adr/0023-independent-conversation-session-fork"},{"text":"ADR 0024: Composer Slash Commands and @ File References","link":"/adr/0024-composer-commands-and-file-references"},{"text":"ADR 0025: Keep Application Menus out of Windows/Linux Windows","link":"/adr/0025-menu-free-windows-linux-chrome"},{"text":"ADR 0026: Move the Projects Index into Settings as an Archive","link":"/adr/0026-project-archive-in-settings"},{"text":"ADR 0027: Make pi-ai authoritative for model metadata","link":"/adr/0027-pi-model-catalog-authority"},{"text":"ADR 0028: Scope work-panel runtime contexts to conversations","link":"/adr/0028-session-scoped-work-panel-contexts"},{"text":"ADR 0029: Separate native-window and work-panel resize ownership","link":"/adr/0029-separate-window-and-panel-resize-ownership"},{"text":"ADR 0030: Turn-boundary context checkpoint compaction","link":"/adr/0030-turn-boundary-context-checkpoint-compaction"},{"text":"ADR 0031: Keep composer prompt rows free of brand icons","link":"/adr/0031-icon-free-composer-prompt-row"},{"text":"ADR 0032: Reserve native width for the docked work panel","link":"/adr/0032-reserve-native-width-for-the-docked-work-panel"},{"text":"ADR 0033: Internal-dock work panel (no native window expansion)","link":"/adr/0033-internal-dock-work-panel"},{"text":"ADR 0034: Merge the command palette into global search","link":"/adr/0034-merge-command-palette-into-global-search"},{"text":"ADR 0035: Surface the OS locale through the preload bridge","link":"/adr/0035-surface-os-locale-via-preload"},{"text":"ADR 0036: Split Settings into AI and Shortcuts destinations","link":"/adr/0036-settings-ia-ai-and-shortcuts-tabs"},{"text":"ADR 0037: Resolve project instructions in Electron main","link":"/adr/0037-project-instruction-chain"},{"text":"ADR 0038: Bridge plugin-declared MCP servers in Electron main","link":"/adr/0038-plugin-mcp-bridge"},{"text":"ADR 0039: Activate plugin skills and ship plugin authoring as a first-party devkit","link":"/adr/0039-plugin-skills-activation-and-devkit"},{"text":"ADR 0040: Resident plugin services and the inter-plugin message bus","link":"/adr/0040-plugin-resident-services-and-message-bus"},{"text":"ADR 0041: Bound host runtime resources and decouple message persistence","link":"/adr/0041-bounded-host-runtime-and-persistence-outbox"},{"text":"ADR 0042: Message-scoped inline review cards","link":"/adr/0042-message-scoped-inline-review-cards"},{"text":"ADR 0043: Message-owned review snapshots and guarded rollback","link":"/adr/0043-message-owned-review-snapshots-and-rollback"},{"text":"ADR 0044: Session-bound project instruction preflight","link":"/adr/0044-session-bound-project-instruction-preflight"},{"text":"ADR 0045: Bash tool inherits the user's login-shell PATH","link":"/adr/0045-bash-inherits-user-login-path"},{"text":"ADR 0046: Categorized process log files","link":"/adr/0046-categorized-process-logs"},{"text":"ADR 0047: Context usage inspector with exact and estimated token sources","link":"/adr/0047-context-usage-inspector"},{"text":"ADR 0048: Lazy per-turn tool activation","link":"/adr/0048-lazy-per-turn-tool-activation"},{"text":"ADR 0049: Recover automatic context compaction failures with a retained tail","link":"/adr/0049-context-compaction-failure-recovery"},{"text":"ADR 0050: Bounded provider stream recovery and diagnostics","link":"/adr/0050-bounded-provider-stream-recovery"},{"text":"ADR 0051: Isolate host RPC stdio from the Tokio blocking pool","link":"/adr/0051-host-rpc-stdio-resource-isolation"},{"text":"ADR 0052: Plan operating state and approval boundary","link":"/adr/0052-plan-operating-state-and-approval-boundary"},{"text":"ADR 0053: Plan checkpoint artifact, approval, and execution epoch","link":"/adr/0053-plan-checkpoint-artifact-and-execution-epoch"},{"text":"ADR 0054: Selectable command shell catalog and execution identity","link":"/adr/0054-selectable-command-shell-catalog"},{"text":"ADR 0055: Agent-only mode; Chat becomes an internal read-only profile","link":"/adr/0055-agent-only-mode"},{"text":"ADR 0056: User-owned MCP servers and skills, with a shared activation scope","link":"/adr/0056-extension-activation-scope"},{"text":"ADR 0057: Permission-gated external paths and portable native search","link":"/adr/0057-permission-gated-external-paths-and-portable-search"},{"text":"ADR 0058: Extensions Page Density and Theme-Readable Button Surfaces","link":"/adr/0058-extensions-page-density-and-button-contrast"},{"text":"ADR 0059: Persist Composer Clipboard Files in Session Scratch","link":"/adr/0059-composer-clipboard-files-in-session-scratch"},{"text":"ADR 0060: Archive the Regenerate Branch Under the RPC Lock","link":"/adr/0060-regenerate-branch-archive-under-the-rpc-lock"},{"text":"ADR 0061: Imperceptible background context compaction","link":"/adr/0061-imperceptible-background-context-compaction"},{"text":"ADR 0062: Bounded Subagents Behind a Task Tool","link":"/adr/0062-bounded-subagents-behind-a-task-tool"},{"text":"ADR 0063: A Managed Surface for Global Subagent Definitions","link":"/adr/0063-subagent-management-ui"},{"text":"ADR 0064: Codex-parity context compaction","link":"/adr/0064-codex-parity-context-compaction"},{"text":"ADR 0065: Smooth shell layout and stream feedback","link":"/adr/0065-smooth-shell-layout-and-stream-feedback"},{"text":"ADR 0066: Empty home direct bottom composer","link":"/adr/0066-empty-home-direct-bottom-composer"},{"text":"ADR 0067: ChatGPT-inspired empty-home starter guidance","link":"/adr/0067-chatgpt-inspired-empty-home-starters"},{"text":"ADR 0068: Add a keyboard entry point for the work panel","link":"/adr/0068-work-panel-keyboard-entry"},{"text":"ADR 0069: Make native-tool path mistakes recoverable","link":"/adr/0069-recoverable-native-tool-path-errors"},{"text":"ADR 0070: Separate Composer File-Reference Display from Prompt Serialization","link":"/adr/0070-separate-composer-file-reference-display-from-prompt"},{"text":"ADR 0071: Adopt an Apple-Inspired Global Corner Hierarchy","link":"/adr/0071-apple-inspired-global-corner-hierarchy"},{"text":"ADR 0072: Add a global plugin launcher","link":"/adr/0072-global-plugin-launcher"},{"text":"ADR 0073: Stage next-turn composer configuration and preserve stopped throughput","link":"/adr/0073-next-turn-composer-configuration-and-stopped-throughput"},{"text":"ADR 0074: Native notification permission for plugins","link":"/adr/0074-native-notification-permission-for-plugins"},{"text":"ADR 0075: Manual reload for development-plugin permission ceilings","link":"/adr/0075-manual-development-plugin-reload"},{"text":"ADR 0076: Capture the Windows-reserved plugin launcher chord in host-core","link":"/adr/0076-windows-reserved-global-shortcut-fallback"},{"text":"ADR 0077: Add an interactive multi-question asktool","link":"/adr/0077-asktool-interactive-multi-question-flow"},{"text":"ADR 0078: Cross-platform tray-resident minimize","link":"/adr/0078-cross-platform-tray-resident-minimize"},{"text":"ADR 0079: Use VitePress for the bilingual documentation site","link":"/adr/0079-vitepress-documentation-site"},{"text":"ADR 0080: Prewarm the global plugin launcher after boot","link":"/adr/0080-prewarm-global-plugin-launcher"},{"text":"ADR 0081: Host-owned cross-platform plugin panel chrome","link":"/adr/0081-host-owned-plugin-panel-chrome"},{"text":"ADR 0082: Localized and page-adaptive plugin panel chrome","link":"/adr/0082-localized-plugin-panel-chrome"},{"text":"ADR 0083: Custom global UI font","link":"/adr/0083-custom-global-ui-font"},{"text":"ADR 0084: Defer new-task session creation until the first message","link":"/adr/0084-deferred-new-task-session-creation"},{"text":"ADR 0085: Make the work panel shortcut a toggle","link":"/adr/0085-work-panel-shortcut-toggle"},{"text":"ADR 0086: Keep macOS on the regular activation policy","link":"/adr/0086-macos-regular-activation-policy"},{"text":"ADR 0087: Replace textual Edit matching with a line-anchored, tag-verified contract","link":"/adr/0087-line-anchored-edit-contract"},{"text":"ADR 0088: Plugin file access is declared per mode, and deletion is recoverable","link":"/adr/0088-declared-file-scope-for-plugins"},{"text":"ADR 0089: Proactive Background Subagent Delegation","link":"/adr/0089-proactive-background-subagent-delegation"},{"text":"ADR 0090: User-Configurable Close Behavior with Close-to-Tray","link":"/adr/0090-user-configurable-close-behavior-close-to-tray"},{"text":"ADR 0091: Route provider rate limits through bounded same-turn retry","link":"/adr/0091-rate-limit-same-turn-retry"},{"text":"ADR 0092: Use a plugin-owned surface with a host window-control capsule","link":"/adr/0092-plugin-owned-panel-surface"},{"text":"ADR 0093: Keep a strict 46px plugin drag band with a minimal capsule","link":"/adr/0093-plugin-panel-strict-drag-band"},{"text":"ADR 0094: Admit one desktop instance per data directory","link":"/adr/0094-single-instance-per-data-directory"},{"text":"ADR 0095: Sign in with a vendor account instead of pasting an API key","link":"/adr/0095-vendor-account-oauth-login"},{"text":"ADR 0096: Flatten the Settings directory and colocate marketplace source configuration","link":"/adr/0096-flat-settings-directory-and-marketplace-context"},{"text":"ADR 0097: Place global defaults under the AI settings destination","link":"/adr/0097-place-global-defaults-under-ai-settings"},{"text":"ADR 0098: Treat every vendor OAuth account as an independent provider row","link":"/adr/0098-multiple-vendor-oauth-accounts"},{"text":"ADR 0099: Add titled visual clusters to the Settings directory","link":"/adr/0099-titled-settings-navigation-clusters"},{"text":"ADR 0100: Make builtin subagents inherit the parent permission mode","link":"/adr/0100-builtin-subagents-inherit-parent-permission-mode"},{"text":"ADR 0101: Model-aware image attachment transport","link":"/adr/0101-model-aware-image-attachments"},{"text":"ADR 0102: Publisher-owned plugin source with a Git-hosted artifact store","link":"/adr/0102-publisher-owned-plugin-source-and-git-hosted-artifacts"},{"text":"ADR 0103: Compact context usage summary","link":"/adr/0103-compact-context-usage-summary"},{"text":"ADR 0104: Plugin-contributed work panel views","link":"/adr/0104-plugin-contributed-work-panel-views"},{"text":"ADR 0105: Ship Files as a bundled plugin; keep Review in the host","link":"/adr/0105-files-as-a-bundled-plugin"},{"text":"ADR 0106: Keep only five core builtin commands","link":"/adr/0106-core-five-builtin-commands"},{"text":"ADR 0107: Make current-session task notification suppression atomic","link":"/adr/0107-atomic-viewing-context-for-task-notifications"},{"text":"ADR 0108: Remove the built-in interactive terminal","link":"/adr/0108-remove-built-in-interactive-terminal"},{"text":"ADR 0109: Open Files entries with the OS-associated application","link":"/adr/0109-open-files-with-the-os-associated-application"},{"text":"ADR 0110: Version the plugin panel chrome spacing contract","link":"/adr/0110-plugin-panel-chrome-spacing-contract"},{"text":"ADR 0111: Reveal Files in the OS File Manager","link":"/adr/0111-reveal-files-in-file-manager"},{"text":"ADR 0112: Agent Capability Management Roots and Settings IA","link":"/adr/0112-agent-capability-management-roots-and-settings-ia"},{"text":"ADR 0113: Persist the New Task empty slot immediately and deduplicate it by message count","link":"/adr/0113-immediate-empty-session-slot-and-deduplication"},{"text":"ADR 0114: Persist Provider Model Bindings and Thinking Configuration","link":"/adr/0114-provider-model-bindings-and-thinking-configuration"},{"text":"ADR 0115: Keep plugin clipboard history host-owned and in memory","link":"/adr/0115-plugin-clipboard-history-is-host-owned"},{"text":"ADR 0116: Add OpenCode Go as a Fixed Provider Preset","link":"/adr/0116-opencode-go-provider-preset"},{"text":"ADR 0117: Preserve the Windows taskbar entry for native minimize","link":"/adr/0117-windows-taskbar-native-minimize"},{"text":"ADR 0118: Keep queued prompts renderer-owned and stop runs at turn boundaries","link":"/adr/0118-renderer-owned-queued-prompts"},{"text":"ADR 0119: Event-Driven Subagent Timeouts","link":"/adr/0119-event-driven-subagent-timeouts"},{"text":"ADR 0120: Bounded Session History Windows","link":"/adr/0120-bounded-session-history-windows"},{"text":"ADR 0121: Keep Composer prompt enhancement one-shot and main-owned","link":"/adr/0121-one-shot-composer-prompt-enhancement"},{"text":"ADR 0122: Reserve native width while the work panel is visible","link":"/adr/0122-reserve-native-width-while-work-panel-visible"},{"text":"ADR 0123: Use native taskbar minimize for Windows/Linux window controls","link":"/adr/0123-native-taskbar-minimize-controls"},{"text":"ADR 0124: Bind Temporary Sessions to Their Own Scratch Workspace","link":"/adr/0124-temporary-session-scratch-workspace"},{"text":"ADR 0125: Renderer Ships Derived Brand Marks and Minified Output","link":"/adr/0125-renderer-derived-brand-marks-and-minified-output"},{"text":"ADR 0126: Agent Capability Pages Are One Workbench That Can Author","link":"/adr/0126-agent-capability-workbench"},{"text":"ADR 0127: Transcript Layout Index and Identity-Based Truncation","link":"/adr/0127-transcript-layout-index-and-identity-truncation"},{"text":"ADR 0128: Share one bounded budget for transient provider failures","link":"/adr/0128-bounded-transient-provider-retry"},{"text":"ADR 0129: The Subagent Idle Watchdog Bounds Silence, Not Slowness","link":"/adr/0129-idle-watchdog-bounds-silence"},{"text":"ADR 0130: Bounded Mounted Transcript Window","link":"/adr/0130-bounded-mounted-transcript-window"},{"text":"ADR 0131: Spill Large Composer Text Pastes into Session Scratch","link":"/adr/0131-large-text-paste-session-reference"},{"text":"ADR 0132: Attribute cross-display window moves to the user","link":"/adr/0132-attribute-cross-display-window-moves-to-the-user"},{"text":"ADR 0133: Use models.dev as the primary model catalog with pi-ai fallback","link":"/adr/0133-models-dev-primary-catalog-with-pi-fallback"},{"text":"ADR 0134: Use models.dev as the sole model metadata source with a local snapshot","link":"/adr/0134-models-dev-sole-model-metadata-source"},{"text":"ADR 0135: Retry unchanged edited prompts","link":"/adr/0135-retry-unchanged-edited-prompts"},{"text":"ADR 0136: Preserve the active task boundary across context compaction","link":"/adr/0136-active-task-boundary-across-compaction"},{"text":"ADR 0137: Retained Session Panes","link":"/adr/0137-retained-session-panes"},{"text":"ADR 0138: Subagent Peer Messaging","link":"/adr/0138-subagent-peer-messaging"},{"text":"ADR 0140: Fold the Three Peer Tools Into One Peer Tool","link":"/adr/0140-peer-tool-consolidation"},{"text":"ADR 0141: Make Expanded Sidebar Width User-Resizable","link":"/adr/0141-sidebar-width-resize"},{"text":"ADR 0142: Allow non-loopback HTTP MCP endpoints with explicit risk disclosure","link":"/adr/0142-allow-non-loopback-http-mcp"},{"text":"ADR 0143: Make Session Titles User-Renamable","link":"/adr/0143-user-renamable-session-titles"},{"text":"ADR 0144: Allow User-Configured Thinking-Level Overrides","link":"/adr/0144-user-configurable-thinking-level-overrides"},{"text":"ADR 0145: Publish Native macOS Intel Artifacts","link":"/adr/0145-native-macos-intel-release-lane"},{"text":"ADR 0146: Assign outer and inner work-panel resize ownership by boundary","link":"/adr/0146-separate-work-panel-and-chat-resize-ownership"},{"text":"ADR 0147: A2A Protocol Stack for Subagent Coordination","link":"/adr/0147-a2a-protocol-stack"},{"text":"ADR 0148: Explicitly disable application keyboard shortcuts","link":"/adr/0148-explicitly-disable-keyboard-shortcuts"},{"text":"ADR 0149: Calm transcript running-status motion","link":"/adr/0149-calm-transcript-running-status-motion"},{"text":"ADR 0150: Inline SVG empty-home agent mark","link":"/adr/0150-inline-svg-empty-home-agent-mark"},{"text":"ADR 0151: Keep the work panel inside the fixed application window","link":"/adr/0151-internal-work-panel-dock"},{"text":"ADR 0152: Eight-frame empty-home mascot GIF","link":"/adr/0152-eight-frame-empty-home-mascot-gif"},{"text":"ADR 0153: Checkpoint the streaming reply beside the transcript","link":"/adr/0153-inflight-reply-checkpoint"},{"text":"ADR 0154: Reveal the New Task empty destination before host IO","link":"/adr/0154-reveal-new-task-empty-destination"},{"text":"ADR 0155: Add Zhipu / Z.AI Named Endpoint Presets","link":"/adr/0155-zhipu-endpoint-presets"},{"text":"ADR 0156: Simplify the Add-Provider Common Path","link":"/adr/0156-simplify-add-provider-common-path"},{"text":"ADR 0157: Main-owned GitHub issue feedback","link":"/adr/0157-main-owned-github-issue-feedback"},{"text":"ADR 0158: Keep approval cards focused and remember the selected mode","link":"/adr/0158-approval-card-focus-and-remembered-mode"},{"text":"ADR 0159: Generated plugin settings and plugin-local shortcuts","link":"/adr/0159-plugin-generated-settings-and-local-shortcuts"},{"text":"ADR 0160: Shipped locale registry and searchable language picker","link":"/adr/0160-shipped-locale-registry-and-language-picker"},{"text":"ADR 0161: Searchable theme picker matching language","link":"/adr/0161-searchable-theme-picker"},{"text":"ADR 0162: Cross-session A2A addressing","link":"/adr/0162-cross-session-a2a-addressing"},{"text":"ADR 0163: Transcript File References Render as Previewable Chips","link":"/adr/0163-transcript-file-reference-chips"},{"text":"ADR 0164: Parent agents collaborate across conversations","link":"/adr/0164-parent-cross-conversation-a2a"},{"text":"ADR 0165: Withdraw the A2A / Peer coordination stack","link":"/adr/0165-withdraw-a2a-peer-stack"},{"text":"ADR 0166: Parent-judged subagent lifetime","link":"/adr/0166-parent-judged-subagent-lifetime"},{"text":"ADR 0167: Agent-chosen Bash timeout","link":"/adr/0167-agent-chosen-bash-timeout"},{"text":"ADR 0168: Main-owned http(s)/mailto allowlist for openExternal","link":"/adr/0168-main-owned-open-external-allowlist"},{"text":"ADR 0169: Classified file preview and live workspace events for plugin views","link":"/adr/0169-classified-file-preview-and-live-workspace-events"},{"text":"ADR 0170: Ship the work-panel browser as a bundled plugin over public CDP","link":"/adr/0170-work-panel-browser-as-bundled-plugin"},{"text":"ADR 0171: Host-owned completed-turn token history","link":"/adr/0171-host-owned-completed-turn-token-history"},{"text":"ADR 0172: Contained in-chat image display","link":"/adr/0172-contained-in-chat-image-display"},{"text":"ADR 0173: Plugin-owned token usage dashboard","link":"/adr/0173-plugin-owned-token-usage-dashboard"},{"text":"ADR 0174: Host-owned plugin completions and session context","link":"/adr/0174-plugin-host-owned-completion-and-session-context"},{"text":"ADR 0175: Explain quiet active turns with live agent activity status","link":"/adr/0175-live-agent-activity-status"},{"text":"ADR 0176: Per-provider User-Agent override","link":"/adr/0176-per-provider-user-agent"},{"text":"ADR 0177: User-configurable outbound proxy","link":"/adr/0177-user-configurable-outbound-proxy"},{"text":"ADR 0178: Per-provider custom HTTP headers","link":"/adr/0178-per-provider-custom-headers"},{"text":"ADR 0179: Import model configuration from local agent stores","link":"/adr/0179-import-model-configuration"},{"text":"ADR 0180: Custom global UI type scale","link":"/adr/0180-custom-reading-font-size"},{"text":"ADR 0181: Main-owned picker capabilities","link":"/adr/0181-main-owned-picker-capabilities"},{"text":"ADR 0182: Traditional Chinese shell locale","link":"/adr/0182-traditional-chinese-shell-locale"},{"text":"ADR 0183: P0 international shell locales","link":"/adr/0183-p0-international-shell-locales"},{"text":"ADR 0184: Dock the context usage inspector in the composer toolbar","link":"/adr/0184-composer-context-usage-inspector"},{"text":"ADR 0185: Korean shell locale","link":"/adr/0185-korean-shell-locale"},{"text":"ADR 0186: Summarize First-Turn Session Titles with a Main-Owned One-Shot","link":"/adr/0186-session-auto-title-summary"},{"text":"ADR 0187: Focus-Aware Native Task Notifications","link":"/adr/0187-focus-aware-native-task-notifications"},{"text":"ADR 0188: Preserve distinct credentials during model configuration import","link":"/adr/0188-preserve-distinct-import-credentials"},{"text":"ADR 0189: Parent fatal error aborts leftover delegates","link":"/adr/0189-parent-fatal-error-aborts-leftover-delegates"},{"text":"ADR 0190: Host-gated large-file and dropped-file access","link":"/adr/0190-host-gated-large-file-and-drop-access"},{"text":"ADR 0191: Label Both macOS Release Architectures","link":"/adr/0191-label-both-macos-release-architectures"},{"text":"ADR 0192: Alias a configured model and make model ids copyable","link":"/adr/0192-model-alias"},{"text":"ADR 0193: Last-request occupancy in the context inspector","link":"/adr/0193-last-request-context-occupancy"},{"text":"ADR 0194: Optional subagent thinking override","link":"/adr/0194-subagent-thinking-parameter-omission"},{"text":"ADR 0195: Viewport-fixed work panel toggle","link":"/adr/0195-viewport-fixed-work-panel-toggle"},{"text":"ADR 0196: Show provider retry causes in the active-turn status","link":"/adr/0196-retry-cause-in-active-turn-status"},{"text":"ADR 0197: Publish a Windows Portable Executable","link":"/adr/0197-windows-portable-exe"},{"text":"ADR 0198: Name every quiet interval on the live activity row","link":"/adr/0198-quiet-interval-activity-phases"},{"text":"ADR 0200: Host-Owned Plugin Session Import and Ownership API","link":"/adr/0200-plugin-owned-session-api"},{"text":"ADR 0201: Explicit Plugin Project IDs and Host-Owned Session Refresh","link":"/adr/0201-plugin-project-ids-and-session-refresh"},{"text":"ADR 0202: Expose Effective Subagent Thinking Metadata","link":"/adr/0202-effective-subagent-thinking-metadata"},{"text":"ADR 0203: Local MCP Control Plane for Desktop Operations","link":"/adr/0203-local-mcp-control-plane"},{"text":"ADR 0204: Explicit Unsigned macOS First-Launch Helper","link":"/adr/0204-unsigned-macos-first-launch-helper"},{"text":"ADR 0205: Remote Agent Control Uses a Dedicated Host Boundary","link":"/adr/0205-remote-agent-control-boundary"},{"text":"ADR 0206: Extend provider retries and show bounded progress","link":"/adr/0206-ten-provider-retries-and-progress-status"},{"text":"ADR 0207: Allow Three Same-Path Mutation Recovery Failures","link":"/adr/0207-three-mutation-recovery-failures"},{"text":"ADR 0208: Plugin Desktop Control Requires Native User Consent","link":"/adr/0208-plugin-desktop-control-native-consent"},{"text":"ADR 0209: PowerShell 7 as a selectable Windows command shell","link":"/adr/0209-powershell-7-selectable-windows-shell"},{"text":"ADR 0210: Subagent output-token cap","link":"/adr/0210-subagent-output-token-cap"},{"text":"ADR 0211: Plan-Safe Plugin Actions for Read-Only Inspection","link":"/adr/0211-plan-safe-plugin-actions"},{"text":"ADR 0212: Remove diagnostic timing log streams","link":"/adr/0212-remove-diagnostic-timing-log-streams"},{"text":"ADR 0213: Persist the Host-owned turn queue in host-core","link":"/adr/0213-persist-host-owned-turn-queue"},{"text":"ADR 0214: Trusted extensions run in the Agent sidecar","link":"/adr/0214-trusted-extensions"},{"text":"ADR 0215: Agent extensions are a plugin contribution","link":"/adr/0215-agent-extensions-as-plugin-contribution"},{"text":"ADR 0216: Truncate Regenerates Under the RPC Lock","link":"/adr/0216-truncate-regenerates-under-the-rpc-lock"},{"text":"ADR 0217: Host Stdout Sender Must Not Outlive Serve","link":"/adr/0217-host-stdout-sender-must-not-outlive-serve"},{"text":"ADR 0218: Effective Image-Input Overrides Across Composer and Transport","link":"/adr/0218-effective-image-input-overrides"},{"text":"ADR 0219: User-Invoked Skills in the Composer Slash Menu","link":"/adr/0219-user-invoked-skills-in-composer"},{"text":"ADR 0220: Keep Windows Work-Panel Chrome Single-Purpose","link":"/adr/0220-windows-work-panel-toolbar"},{"text":"ADR 0221: Render Canonical Thinking-Level Values Without Translation","link":"/adr/0221-canonical-thinking-level-values"},{"text":"ADR 0222: Native File and Folder Drops in the Composer","link":"/adr/0222-composer-native-file-folder-drops"},{"text":"ADR 0223: Context Usage Display Preference","link":"/adr/0223-context-usage-display-preference"},{"text":"ADR 0224: Right Panel Tab Strip and Data-Driven Add Menu","link":"/adr/0224-right-panel-tab-strip"},{"text":"ADR 0225: Restore Deferred Tools from Effective Session Context","link":"/adr/0225-restore-deferred-tools-from-effective-session-context"},{"text":"ADR 0226: Reserve Chat Width for Composer Controls","link":"/adr/0226-reserve-chat-width-for-composer-controls"},{"text":"ADR 0227: Project group manual ordering","link":"/adr/0227-project-group-manual-ordering"},{"text":"ADR 0228: Long-press the project title to reorder","link":"/adr/0228-long-press-project-title-reorder"},{"text":"ADR 0229: Press-and-move project title reorder","link":"/adr/0229-press-and-move-project-title-reorder"},{"text":"ADR 0230: Skill Ships with the Agent Core Tool Set","link":"/adr/0230-skill-ships-with-the-agent-core-tool-set"},{"text":"ADR 0231: Ideographic Comma Opens the Composer Slash Menu","link":"/adr/0231-ideographic-comma-opens-the-slash-menu"},{"text":"ADR 0232: Keep macOS DMG Opening Guidance Text-Only","link":"/adr/0232-macos-dmg-text-only-opening-guidance"},{"text":"ADR 0233: Renderer-Owned Multi-Folder Project Creation","link":"/adr/0233-renderer-owned-multi-folder-project-creation"},{"text":"ADR 0234: Keep project memory host-owned and path-scoped","link":"/adr/0234-project-owned-memory-context"},{"text":"ADR 0235: Preserve Domain Facades and Enforce Architecture Budgets","link":"/adr/0235-domain-facades-and-architecture-budgets"},{"text":"ADR 0236: Restore Archived Projects When Session Import Adds a Bound Session","link":"/adr/0236-restore-archived-project-on-session-import"},{"text":"ADR 0237: Keep Session Orchestration in an Official Plugin","link":"/adr/0237-session-orchestrator-as-a-plugin"},{"text":"ADR 0238: Prioritize MainChat in the three-column shell","link":"/adr/0238-three-column-width-priority"},{"text":"ADR 0239: Host-owned session collaboration messages","link":"/adr/0239-session-collaboration-messages"},{"text":"ADR 0240: Independent session discovery and navigable collaboration projections","link":"/adr/0240-independent-session-discovery-and-navigable-projections"},{"text":"ADR 0241: Ship the file view as a vendored, updatable plugin","link":"/adr/0241-vendored-updatable-file-view-plugin"},{"text":"ADR 0242: Delta-only coalesced streaming updates","link":"/adr/0242-delta-only-streaming-updates"},{"text":"ADR 0243: Skill market public-HTTPS catalog fetch","link":"/adr/0243-skill-market-public-https-catalog"},{"text":"ADR 0244: Bound dependency installation for imported extensions","link":"/adr/0244-imported-extension-dependency-boundary"},{"text":"ADR 0245: Harden the MCP market public-network boundary","link":"/adr/0245-mcp-market-public-network-boundary"},{"text":"ADR 0246: Opt-in subagent inheritance of the parent tool catalog","link":"/adr/0246-subagent-opt-in-parent-tool-inherit"},{"text":"ADR 0247: Git clone accepts only syntactically public hosts","link":"/adr/0247-git-clone-public-hostname"},{"text":"ADR 0248 — Package theme assets and contributed window backgrounds","link":"/adr/0248-plugin-theme-assets-and-window-background"},{"text":"ADR 0249: ChatGPT-Style Logical Project Groups","link":"/adr/0249-chatgpt-style-logical-project-groups"},{"text":"ADR 0249 — Plugin runtime theme APIs and sidebar image token","link":"/adr/0249-plugin-runtime-theme-apis"},{"text":"ADR 0250 — Structured, bounded, and redacted process logs","link":"/adr/0250-structured-bounded-redacted-process-logs"},{"text":"ADR 0251 — pi-go Go agent sidecar","link":"/adr/0251-pi-go-agent-sidecar"},{"text":"0252. Staff employees: referenced credentials and promotion-gated shared memory","link":"/adr/0252-staff-employees-and-shared-memory"},{"text":"0253. Site sessions: a real signed-in browser instead of a forged request","link":"/adr/0253-site-sessions-for-staff-collection"},{"text":"0254. Middle-manager boards: the rows employees file, and the one door that turns a proposal into work","link":"/adr/0254-middle-manager-boards"},{"text":"ADR 0255: Host-owned browser chrome over a Tauri child webview","link":"/adr/0255-tauri-browser-guest"},{"text":"ADR 0256: Reopen the builtin command registry with guardrails","link":"/adr/0256-reopen-builtin-command-registry"},{"text":"ADR active-turn-steering: Bind Composer steering to the active durable turn","link":"/adr/active-turn-steering"},{"text":"ADR: Show pinned conversations in a global sidebar section","link":"/adr/global-sidebar-pins"},{"text":"ADR: Separate Subagent Model Opt-In from Definition Pins","link":"/adr/subagent-model-opt-in"}]}]},"search":{"provider":"local","options":{"translations":{"button":{"buttonText":"搜索文档","buttonAriaLabel":"搜索文档"},"modal":{"noResultsText":"没有找到相关结果","resetButtonTitle":"清除查询","footer":{"selectText":"选择","navigateText":"切换","closeText":"关闭"}}}}},"outline":{"level":"deep","label":"本页目录"},"docFooter":{"prev":"上一页","next":"下一页"},"lastUpdated":{"text":"最后更新于"},"returnToTopLabel":"返回顶部","sidebarMenuLabel":"目录","darkModeSwitchLabel":"外观","langMenuLabel":"切换语言","editLink":{"pattern":"https://github.com/vastsa/PI-Desktop/edit/main/docs/:path","text":"在 GitHub 上编辑此页"},"footer":{"message":"为本地优先开发而构建。 <a href=\\"https://aiuo.net\\" target=\\"_blank\\" rel=\\"noreferrer\\">AIUO.NET</a>","copyright":"Copyright © 2026 PI-Desktop 贡献者"}}}},"scrollOffset":134,"cleanUrls":true}`));
function tryOnScopeDispose(fn) {
  if (getCurrentScope()) {
    onScopeDispose(fn);
    return true;
  }
  return false;
}
const localProvidedStateMap = /* @__PURE__ */ new WeakMap();
const injectLocal = (...args) => {
  var _a;
  const key = args[0];
  const instance = (_a = getCurrentInstance()) == null ? void 0 : _a.proxy;
  if (instance == null && !hasInjectionContext())
    throw new Error("injectLocal must be called in setup");
  if (instance && localProvidedStateMap.has(instance) && key in localProvidedStateMap.get(instance))
    return localProvidedStateMap.get(instance)[key];
  return inject(...args);
};
const isClient = typeof window !== "undefined" && typeof document !== "undefined";
typeof WorkerGlobalScope !== "undefined" && globalThis instanceof WorkerGlobalScope;
const notNullish = (val) => val != null;
const toString = Object.prototype.toString;
const isObject = (val) => toString.call(val) === "[object Object]";
const noop = () => {
};
const isIOS = /* @__PURE__ */ getIsIOS();
function getIsIOS() {
  var _a, _b;
  return isClient && ((_a = window == null ? void 0 : window.navigator) == null ? void 0 : _a.userAgent) && (/iP(?:ad|hone|od)/.test(window.navigator.userAgent) || ((_b = window == null ? void 0 : window.navigator) == null ? void 0 : _b.maxTouchPoints) > 2 && /iPad|Macintosh/.test(window == null ? void 0 : window.navigator.userAgent));
}
function createFilterWrapper(filter, fn) {
  function wrapper(...args) {
    return new Promise((resolve, reject) => {
      Promise.resolve(filter(() => fn.apply(this, args), { fn, thisArg: this, args })).then(resolve).catch(reject);
    });
  }
  return wrapper;
}
const bypassFilter = (invoke) => {
  return invoke();
};
function debounceFilter(ms, options = {}) {
  let timer;
  let maxTimer;
  let lastRejector = noop;
  const _clearTimeout = (timer2) => {
    clearTimeout(timer2);
    lastRejector();
    lastRejector = noop;
  };
  let lastInvoker;
  const filter = (invoke) => {
    const duration = toValue(ms);
    const maxDuration = toValue(options.maxWait);
    if (timer)
      _clearTimeout(timer);
    if (duration <= 0 || maxDuration !== void 0 && maxDuration <= 0) {
      if (maxTimer) {
        _clearTimeout(maxTimer);
        maxTimer = null;
      }
      return Promise.resolve(invoke());
    }
    return new Promise((resolve, reject) => {
      lastRejector = options.rejectOnCancel ? reject : resolve;
      lastInvoker = invoke;
      if (maxDuration && !maxTimer) {
        maxTimer = setTimeout(() => {
          if (timer)
            _clearTimeout(timer);
          maxTimer = null;
          resolve(lastInvoker());
        }, maxDuration);
      }
      timer = setTimeout(() => {
        if (maxTimer)
          _clearTimeout(maxTimer);
        maxTimer = null;
        resolve(invoke());
      }, duration);
    });
  };
  return filter;
}
function throttleFilter(...args) {
  let lastExec = 0;
  let timer;
  let isLeading = true;
  let lastRejector = noop;
  let lastValue;
  let ms;
  let trailing;
  let leading;
  let rejectOnCancel;
  if (!isRef(args[0]) && typeof args[0] === "object")
    ({ delay: ms, trailing = true, leading = true, rejectOnCancel = false } = args[0]);
  else
    [ms, trailing = true, leading = true, rejectOnCancel = false] = args;
  const clear = () => {
    if (timer) {
      clearTimeout(timer);
      timer = void 0;
      lastRejector();
      lastRejector = noop;
    }
  };
  const filter = (_invoke) => {
    const duration = toValue(ms);
    const elapsed = Date.now() - lastExec;
    const invoke = () => {
      return lastValue = _invoke();
    };
    clear();
    if (duration <= 0) {
      lastExec = Date.now();
      return invoke();
    }
    if (elapsed > duration && (leading || !isLeading)) {
      lastExec = Date.now();
      invoke();
    } else if (trailing) {
      lastValue = new Promise((resolve, reject) => {
        lastRejector = rejectOnCancel ? reject : resolve;
        timer = setTimeout(() => {
          lastExec = Date.now();
          isLeading = true;
          resolve(invoke());
          clear();
        }, Math.max(0, duration - elapsed));
      });
    }
    if (!leading && !timer)
      timer = setTimeout(() => isLeading = true, duration);
    isLeading = false;
    return lastValue;
  };
  return filter;
}
function pausableFilter(extendFilter = bypassFilter, options = {}) {
  const {
    initialState = "active"
  } = options;
  const isActive2 = toRef(initialState === "active");
  function pause() {
    isActive2.value = false;
  }
  function resume() {
    isActive2.value = true;
  }
  const eventFilter = (...args) => {
    if (isActive2.value)
      extendFilter(...args);
  };
  return { isActive: readonly(isActive2), pause, resume, eventFilter };
}
function pxValue(px) {
  return px.endsWith("rem") ? Number.parseFloat(px) * 16 : Number.parseFloat(px);
}
function getLifeCycleTarget(target) {
  return getCurrentInstance();
}
function toArray(value) {
  return Array.isArray(value) ? value : [value];
}
function toRef(...args) {
  if (args.length !== 1)
    return toRef$1(...args);
  const r = args[0];
  return typeof r === "function" ? readonly(customRef(() => ({ get: r, set: noop }))) : ref(r);
}
function useDebounceFn(fn, ms = 200, options = {}) {
  return createFilterWrapper(
    debounceFilter(ms, options),
    fn
  );
}
function useThrottleFn(fn, ms = 200, trailing = false, leading = true, rejectOnCancel = false) {
  return createFilterWrapper(
    throttleFilter(ms, trailing, leading, rejectOnCancel),
    fn
  );
}
function watchWithFilter(source, cb, options = {}) {
  const {
    eventFilter = bypassFilter,
    ...watchOptions
  } = options;
  return watch(
    source,
    createFilterWrapper(
      eventFilter,
      cb
    ),
    watchOptions
  );
}
function watchPausable(source, cb, options = {}) {
  const {
    eventFilter: filter,
    initialState = "active",
    ...watchOptions
  } = options;
  const { eventFilter, pause, resume, isActive: isActive2 } = pausableFilter(filter, { initialState });
  const stop = watchWithFilter(
    source,
    cb,
    {
      ...watchOptions,
      eventFilter
    }
  );
  return { stop, pause, resume, isActive: isActive2 };
}
function tryOnMounted(fn, sync = true, target) {
  const instance = getLifeCycleTarget();
  if (instance)
    onMounted(fn, target);
  else if (sync)
    fn();
  else
    nextTick(fn);
}
function watchDebounced(source, cb, options = {}) {
  const {
    debounce = 0,
    maxWait = void 0,
    ...watchOptions
  } = options;
  return watchWithFilter(
    source,
    cb,
    {
      ...watchOptions,
      eventFilter: debounceFilter(debounce, { maxWait })
    }
  );
}
function watchImmediate(source, cb, options) {
  return watch(
    source,
    cb,
    {
      ...options,
      immediate: true
    }
  );
}
function computedAsync(evaluationCallback, initialState, optionsOrRef) {
  let options;
  if (isRef(optionsOrRef)) {
    options = {
      evaluating: optionsOrRef
    };
  } else {
    options = {};
  }
  const {
    lazy = false,
    evaluating = void 0,
    shallow = true,
    onError = noop
  } = options;
  const started = shallowRef(!lazy);
  const current = shallow ? shallowRef(initialState) : ref(initialState);
  let counter = 0;
  watchEffect(async (onInvalidate) => {
    if (!started.value)
      return;
    counter++;
    const counterAtBeginning = counter;
    let hasFinished = false;
    if (evaluating) {
      Promise.resolve().then(() => {
        evaluating.value = true;
      });
    }
    try {
      const result = await evaluationCallback((cancelCallback) => {
        onInvalidate(() => {
          if (evaluating)
            evaluating.value = false;
          if (!hasFinished)
            cancelCallback();
        });
      });
      if (counterAtBeginning === counter)
        current.value = result;
    } catch (e) {
      onError(e);
    } finally {
      if (evaluating && counterAtBeginning === counter)
        evaluating.value = false;
      hasFinished = true;
    }
  });
  if (lazy) {
    return computed(() => {
      started.value = true;
      return current.value;
    });
  } else {
    return current;
  }
}
const defaultWindow = isClient ? window : void 0;
function unrefElement(elRef) {
  var _a;
  const plain = toValue(elRef);
  return (_a = plain == null ? void 0 : plain.$el) != null ? _a : plain;
}
function useEventListener(...args) {
  const cleanups = [];
  const cleanup = () => {
    cleanups.forEach((fn) => fn());
    cleanups.length = 0;
  };
  const register = (el, event, listener, options) => {
    el.addEventListener(event, listener, options);
    return () => el.removeEventListener(event, listener, options);
  };
  const firstParamTargets = computed(() => {
    const test = toArray(toValue(args[0])).filter((e) => e != null);
    return test.every((e) => typeof e !== "string") ? test : void 0;
  });
  const stopWatch = watchImmediate(
    () => {
      var _a, _b;
      return [
        (_b = (_a = firstParamTargets.value) == null ? void 0 : _a.map((e) => unrefElement(e))) != null ? _b : [defaultWindow].filter((e) => e != null),
        toArray(toValue(firstParamTargets.value ? args[1] : args[0])),
        toArray(unref(firstParamTargets.value ? args[2] : args[1])),
        // @ts-expect-error - TypeScript gets the correct types, but somehow still complains
        toValue(firstParamTargets.value ? args[3] : args[2])
      ];
    },
    ([raw_targets, raw_events, raw_listeners, raw_options]) => {
      cleanup();
      if (!(raw_targets == null ? void 0 : raw_targets.length) || !(raw_events == null ? void 0 : raw_events.length) || !(raw_listeners == null ? void 0 : raw_listeners.length))
        return;
      const optionsClone = isObject(raw_options) ? { ...raw_options } : raw_options;
      cleanups.push(
        ...raw_targets.flatMap(
          (el) => raw_events.flatMap(
            (event) => raw_listeners.map((listener) => register(el, event, listener, optionsClone))
          )
        )
      );
    },
    { flush: "post" }
  );
  const stop = () => {
    stopWatch();
    cleanup();
  };
  tryOnScopeDispose(cleanup);
  return stop;
}
function useMounted() {
  const isMounted = shallowRef(false);
  const instance = getCurrentInstance();
  if (instance) {
    onMounted(() => {
      isMounted.value = true;
    }, instance);
  }
  return isMounted;
}
function useSupported(callback) {
  const isMounted = useMounted();
  return computed(() => {
    isMounted.value;
    return Boolean(callback());
  });
}
function createKeyPredicate(keyFilter) {
  if (typeof keyFilter === "function")
    return keyFilter;
  else if (typeof keyFilter === "string")
    return (event) => event.key === keyFilter;
  else if (Array.isArray(keyFilter))
    return (event) => keyFilter.includes(event.key);
  return () => true;
}
function onKeyStroke(...args) {
  let key;
  let handler;
  let options = {};
  if (args.length === 3) {
    key = args[0];
    handler = args[1];
    options = args[2];
  } else if (args.length === 2) {
    if (typeof args[1] === "object") {
      key = true;
      handler = args[0];
      options = args[1];
    } else {
      key = args[0];
      handler = args[1];
    }
  } else {
    key = true;
    handler = args[0];
  }
  const {
    target = defaultWindow,
    eventName = "keydown",
    passive = false,
    dedupe = false
  } = options;
  const predicate = createKeyPredicate(key);
  const listener = (e) => {
    if (e.repeat && toValue(dedupe))
      return;
    if (predicate(e))
      handler(e);
  };
  return useEventListener(target, eventName, listener, passive);
}
const ssrWidthSymbol = Symbol("vueuse-ssr-width");
function useSSRWidth() {
  const ssrWidth = hasInjectionContext() ? injectLocal(ssrWidthSymbol, null) : null;
  return typeof ssrWidth === "number" ? ssrWidth : void 0;
}
function useMediaQuery(query, options = {}) {
  const { window: window2 = defaultWindow, ssrWidth = useSSRWidth() } = options;
  const isSupported = useSupported(() => window2 && "matchMedia" in window2 && typeof window2.matchMedia === "function");
  const ssrSupport = shallowRef(typeof ssrWidth === "number");
  const mediaQuery = shallowRef();
  const matches = shallowRef(false);
  const handler = (event) => {
    matches.value = event.matches;
  };
  watchEffect(() => {
    if (ssrSupport.value) {
      ssrSupport.value = !isSupported.value;
      const queryStrings = toValue(query).split(",");
      matches.value = queryStrings.some((queryString) => {
        const not = queryString.includes("not all");
        const minWidth = queryString.match(/\(\s*min-width:\s*(-?\d+(?:\.\d*)?[a-z]+\s*)\)/);
        const maxWidth = queryString.match(/\(\s*max-width:\s*(-?\d+(?:\.\d*)?[a-z]+\s*)\)/);
        let res = Boolean(minWidth || maxWidth);
        if (minWidth && res) {
          res = ssrWidth >= pxValue(minWidth[1]);
        }
        if (maxWidth && res) {
          res = ssrWidth <= pxValue(maxWidth[1]);
        }
        return not ? !res : res;
      });
      return;
    }
    if (!isSupported.value)
      return;
    mediaQuery.value = window2.matchMedia(toValue(query));
    matches.value = mediaQuery.value.matches;
  });
  useEventListener(mediaQuery, "change", handler, { passive: true });
  return computed(() => matches.value);
}
const _global = typeof globalThis !== "undefined" ? globalThis : typeof window !== "undefined" ? window : typeof global !== "undefined" ? global : typeof self !== "undefined" ? self : {};
const globalKey = "__vueuse_ssr_handlers__";
const handlers = /* @__PURE__ */ getHandlers();
function getHandlers() {
  if (!(globalKey in _global))
    _global[globalKey] = _global[globalKey] || {};
  return _global[globalKey];
}
function getSSRHandler(key, fallback) {
  return handlers[key] || fallback;
}
function usePreferredDark(options) {
  return useMediaQuery("(prefers-color-scheme: dark)", options);
}
function guessSerializerType(rawInit) {
  return rawInit == null ? "any" : rawInit instanceof Set ? "set" : rawInit instanceof Map ? "map" : rawInit instanceof Date ? "date" : typeof rawInit === "boolean" ? "boolean" : typeof rawInit === "string" ? "string" : typeof rawInit === "object" ? "object" : !Number.isNaN(rawInit) ? "number" : "any";
}
const StorageSerializers = {
  boolean: {
    read: (v) => v === "true",
    write: (v) => String(v)
  },
  object: {
    read: (v) => JSON.parse(v),
    write: (v) => JSON.stringify(v)
  },
  number: {
    read: (v) => Number.parseFloat(v),
    write: (v) => String(v)
  },
  any: {
    read: (v) => v,
    write: (v) => String(v)
  },
  string: {
    read: (v) => v,
    write: (v) => String(v)
  },
  map: {
    read: (v) => new Map(JSON.parse(v)),
    write: (v) => JSON.stringify(Array.from(v.entries()))
  },
  set: {
    read: (v) => new Set(JSON.parse(v)),
    write: (v) => JSON.stringify(Array.from(v))
  },
  date: {
    read: (v) => new Date(v),
    write: (v) => v.toISOString()
  }
};
const customStorageEventName = "vueuse-storage";
function useStorage(key, defaults, storage, options = {}) {
  var _a;
  const {
    flush = "pre",
    deep = true,
    listenToStorageChanges = true,
    writeDefaults = true,
    mergeDefaults = false,
    shallow,
    window: window2 = defaultWindow,
    eventFilter,
    onError = (e) => {
      console.error(e);
    },
    initOnMounted
  } = options;
  const data = (shallow ? shallowRef : ref)(typeof defaults === "function" ? defaults() : defaults);
  const keyComputed = computed(() => toValue(key));
  if (!storage) {
    try {
      storage = getSSRHandler("getDefaultStorage", () => {
        var _a2;
        return (_a2 = defaultWindow) == null ? void 0 : _a2.localStorage;
      })();
    } catch (e) {
      onError(e);
    }
  }
  if (!storage)
    return data;
  const rawInit = toValue(defaults);
  const type = guessSerializerType(rawInit);
  const serializer = (_a = options.serializer) != null ? _a : StorageSerializers[type];
  const { pause: pauseWatch, resume: resumeWatch } = watchPausable(
    data,
    () => write(data.value),
    { flush, deep, eventFilter }
  );
  watch(keyComputed, () => update(), { flush });
  if (window2 && listenToStorageChanges) {
    tryOnMounted(() => {
      if (storage instanceof Storage)
        useEventListener(window2, "storage", update, { passive: true });
      else
        useEventListener(window2, customStorageEventName, updateFromCustomEvent);
      if (initOnMounted)
        update();
    });
  }
  if (!initOnMounted)
    update();
  function dispatchWriteEvent(oldValue, newValue) {
    if (window2) {
      const payload = {
        key: keyComputed.value,
        oldValue,
        newValue,
        storageArea: storage
      };
      window2.dispatchEvent(storage instanceof Storage ? new StorageEvent("storage", payload) : new CustomEvent(customStorageEventName, {
        detail: payload
      }));
    }
  }
  function write(v) {
    try {
      const oldValue = storage.getItem(keyComputed.value);
      if (v == null) {
        dispatchWriteEvent(oldValue, null);
        storage.removeItem(keyComputed.value);
      } else {
        const serialized = serializer.write(v);
        if (oldValue !== serialized) {
          storage.setItem(keyComputed.value, serialized);
          dispatchWriteEvent(oldValue, serialized);
        }
      }
    } catch (e) {
      onError(e);
    }
  }
  function read(event) {
    const rawValue = event ? event.newValue : storage.getItem(keyComputed.value);
    if (rawValue == null) {
      if (writeDefaults && rawInit != null)
        storage.setItem(keyComputed.value, serializer.write(rawInit));
      return rawInit;
    } else if (!event && mergeDefaults) {
      const value = serializer.read(rawValue);
      if (typeof mergeDefaults === "function")
        return mergeDefaults(value, rawInit);
      else if (type === "object" && !Array.isArray(value))
        return { ...rawInit, ...value };
      return value;
    } else if (typeof rawValue !== "string") {
      return rawValue;
    } else {
      return serializer.read(rawValue);
    }
  }
  function update(event) {
    if (event && event.storageArea !== storage)
      return;
    if (event && event.key == null) {
      data.value = rawInit;
      return;
    }
    if (event && event.key !== keyComputed.value)
      return;
    pauseWatch();
    try {
      if ((event == null ? void 0 : event.newValue) !== serializer.write(data.value))
        data.value = read(event);
    } catch (e) {
      onError(e);
    } finally {
      if (event)
        nextTick(resumeWatch);
      else
        resumeWatch();
    }
  }
  function updateFromCustomEvent(event) {
    update(event.detail);
  }
  return data;
}
const CSS_DISABLE_TRANS = "*,*::before,*::after{-webkit-transition:none!important;-moz-transition:none!important;-o-transition:none!important;-ms-transition:none!important;transition:none!important}";
function useColorMode(options = {}) {
  const {
    selector = "html",
    attribute = "class",
    initialValue = "auto",
    window: window2 = defaultWindow,
    storage,
    storageKey = "vueuse-color-scheme",
    listenToStorageChanges = true,
    storageRef,
    emitAuto,
    disableTransition = true
  } = options;
  const modes = {
    auto: "",
    light: "light",
    dark: "dark",
    ...options.modes || {}
  };
  const preferredDark = usePreferredDark({ window: window2 });
  const system = computed(() => preferredDark.value ? "dark" : "light");
  const store = storageRef || (storageKey == null ? toRef(initialValue) : useStorage(storageKey, initialValue, storage, { window: window2, listenToStorageChanges }));
  const state = computed(() => store.value === "auto" ? system.value : store.value);
  const updateHTMLAttrs = getSSRHandler(
    "updateHTMLAttrs",
    (selector2, attribute2, value) => {
      const el = typeof selector2 === "string" ? window2 == null ? void 0 : window2.document.querySelector(selector2) : unrefElement(selector2);
      if (!el)
        return;
      const classesToAdd = /* @__PURE__ */ new Set();
      const classesToRemove = /* @__PURE__ */ new Set();
      let attributeToChange = null;
      if (attribute2 === "class") {
        const current = value.split(/\s/g);
        Object.values(modes).flatMap((i) => (i || "").split(/\s/g)).filter(Boolean).forEach((v) => {
          if (current.includes(v))
            classesToAdd.add(v);
          else
            classesToRemove.add(v);
        });
      } else {
        attributeToChange = { key: attribute2, value };
      }
      if (classesToAdd.size === 0 && classesToRemove.size === 0 && attributeToChange === null)
        return;
      let style;
      if (disableTransition) {
        style = window2.document.createElement("style");
        style.appendChild(document.createTextNode(CSS_DISABLE_TRANS));
        window2.document.head.appendChild(style);
      }
      for (const c of classesToAdd) {
        el.classList.add(c);
      }
      for (const c of classesToRemove) {
        el.classList.remove(c);
      }
      if (attributeToChange) {
        el.setAttribute(attributeToChange.key, attributeToChange.value);
      }
      if (disableTransition) {
        window2.getComputedStyle(style).opacity;
        document.head.removeChild(style);
      }
    }
  );
  function defaultOnChanged(mode) {
    var _a;
    updateHTMLAttrs(selector, attribute, (_a = modes[mode]) != null ? _a : mode);
  }
  function onChanged(mode) {
    if (options.onChanged)
      options.onChanged(mode, defaultOnChanged);
    else
      defaultOnChanged(mode);
  }
  watch(state, onChanged, { flush: "post", immediate: true });
  tryOnMounted(() => onChanged(state.value));
  const auto = computed({
    get() {
      return emitAuto ? store.value : state.value;
    },
    set(v) {
      store.value = v;
    }
  });
  return Object.assign(auto, { store, system, state });
}
function useDark(options = {}) {
  const {
    valueDark = "dark",
    valueLight = ""
  } = options;
  const mode = useColorMode({
    ...options,
    onChanged: (mode2, defaultHandler) => {
      var _a;
      if (options.onChanged)
        (_a = options.onChanged) == null ? void 0 : _a.call(options, mode2 === "dark", defaultHandler, mode2);
      else
        defaultHandler(mode2);
    },
    modes: {
      dark: valueDark,
      light: valueLight
    }
  });
  const system = computed(() => mode.system.value);
  const isDark = computed({
    get() {
      return mode.value === "dark";
    },
    set(v) {
      const modeVal = v ? "dark" : "light";
      if (system.value === modeVal)
        mode.value = "auto";
      else
        mode.value = modeVal;
    }
  });
  return isDark;
}
function resolveElement(el) {
  if (typeof Window !== "undefined" && el instanceof Window)
    return el.document.documentElement;
  if (typeof Document !== "undefined" && el instanceof Document)
    return el.documentElement;
  return el;
}
const ARRIVED_STATE_THRESHOLD_PIXELS = 1;
function useScroll(element, options = {}) {
  const {
    throttle = 0,
    idle = 200,
    onStop = noop,
    onScroll = noop,
    offset = {
      left: 0,
      right: 0,
      top: 0,
      bottom: 0
    },
    eventListenerOptions = {
      capture: false,
      passive: true
    },
    behavior = "auto",
    window: window2 = defaultWindow,
    onError = (e) => {
      console.error(e);
    }
  } = options;
  const internalX = shallowRef(0);
  const internalY = shallowRef(0);
  const x = computed({
    get() {
      return internalX.value;
    },
    set(x2) {
      scrollTo2(x2, void 0);
    }
  });
  const y = computed({
    get() {
      return internalY.value;
    },
    set(y2) {
      scrollTo2(void 0, y2);
    }
  });
  function scrollTo2(_x, _y) {
    var _a, _b, _c, _d;
    if (!window2)
      return;
    const _element = toValue(element);
    if (!_element)
      return;
    (_c = _element instanceof Document ? window2.document.body : _element) == null ? void 0 : _c.scrollTo({
      top: (_a = toValue(_y)) != null ? _a : y.value,
      left: (_b = toValue(_x)) != null ? _b : x.value,
      behavior: toValue(behavior)
    });
    const scrollContainer = ((_d = _element == null ? void 0 : _element.document) == null ? void 0 : _d.documentElement) || (_element == null ? void 0 : _element.documentElement) || _element;
    if (x != null)
      internalX.value = scrollContainer.scrollLeft;
    if (y != null)
      internalY.value = scrollContainer.scrollTop;
  }
  const isScrolling = shallowRef(false);
  const arrivedState = reactive({
    left: true,
    right: false,
    top: true,
    bottom: false
  });
  const directions = reactive({
    left: false,
    right: false,
    top: false,
    bottom: false
  });
  const onScrollEnd = (e) => {
    if (!isScrolling.value)
      return;
    isScrolling.value = false;
    directions.left = false;
    directions.right = false;
    directions.top = false;
    directions.bottom = false;
    onStop(e);
  };
  const onScrollEndDebounced = useDebounceFn(onScrollEnd, throttle + idle);
  const setArrivedState = (target) => {
    var _a;
    if (!window2)
      return;
    const el = ((_a = target == null ? void 0 : target.document) == null ? void 0 : _a.documentElement) || (target == null ? void 0 : target.documentElement) || unrefElement(target);
    const { display, flexDirection, direction } = getComputedStyle(el);
    const directionMultipler = direction === "rtl" ? -1 : 1;
    const scrollLeft = el.scrollLeft;
    directions.left = scrollLeft < internalX.value;
    directions.right = scrollLeft > internalX.value;
    const left = Math.abs(scrollLeft * directionMultipler) <= (offset.left || 0);
    const right = Math.abs(scrollLeft * directionMultipler) + el.clientWidth >= el.scrollWidth - (offset.right || 0) - ARRIVED_STATE_THRESHOLD_PIXELS;
    if (display === "flex" && flexDirection === "row-reverse") {
      arrivedState.left = right;
      arrivedState.right = left;
    } else {
      arrivedState.left = left;
      arrivedState.right = right;
    }
    internalX.value = scrollLeft;
    let scrollTop = el.scrollTop;
    if (target === window2.document && !scrollTop)
      scrollTop = window2.document.body.scrollTop;
    directions.top = scrollTop < internalY.value;
    directions.bottom = scrollTop > internalY.value;
    const top = Math.abs(scrollTop) <= (offset.top || 0);
    const bottom = Math.abs(scrollTop) + el.clientHeight >= el.scrollHeight - (offset.bottom || 0) - ARRIVED_STATE_THRESHOLD_PIXELS;
    if (display === "flex" && flexDirection === "column-reverse") {
      arrivedState.top = bottom;
      arrivedState.bottom = top;
    } else {
      arrivedState.top = top;
      arrivedState.bottom = bottom;
    }
    internalY.value = scrollTop;
  };
  const onScrollHandler = (e) => {
    var _a;
    if (!window2)
      return;
    const eventTarget = (_a = e.target.documentElement) != null ? _a : e.target;
    setArrivedState(eventTarget);
    isScrolling.value = true;
    onScrollEndDebounced(e);
    onScroll(e);
  };
  useEventListener(
    element,
    "scroll",
    throttle ? useThrottleFn(onScrollHandler, throttle, true, false) : onScrollHandler,
    eventListenerOptions
  );
  tryOnMounted(() => {
    try {
      const _element = toValue(element);
      if (!_element)
        return;
      setArrivedState(_element);
    } catch (e) {
      onError(e);
    }
  });
  useEventListener(
    element,
    "scrollend",
    onScrollEnd,
    eventListenerOptions
  );
  return {
    x,
    y,
    isScrolling,
    arrivedState,
    directions,
    measure() {
      const _element = toValue(element);
      if (window2 && _element)
        setArrivedState(_element);
    }
  };
}
function useLocalStorage(key, initialValue, options = {}) {
  const { window: window2 = defaultWindow } = options;
  return useStorage(key, initialValue, window2 == null ? void 0 : window2.localStorage, options);
}
function checkOverflowScroll(ele) {
  const style = window.getComputedStyle(ele);
  if (style.overflowX === "scroll" || style.overflowY === "scroll" || style.overflowX === "auto" && ele.clientWidth < ele.scrollWidth || style.overflowY === "auto" && ele.clientHeight < ele.scrollHeight) {
    return true;
  } else {
    const parent = ele.parentNode;
    if (!parent || parent.tagName === "BODY")
      return false;
    return checkOverflowScroll(parent);
  }
}
function preventDefault(rawEvent) {
  const e = rawEvent || window.event;
  const _target = e.target;
  if (checkOverflowScroll(_target))
    return false;
  if (e.touches.length > 1)
    return true;
  if (e.preventDefault)
    e.preventDefault();
  return false;
}
const elInitialOverflow = /* @__PURE__ */ new WeakMap();
function useScrollLock(element, initialState = false) {
  const isLocked = shallowRef(initialState);
  let stopTouchMoveListener = null;
  let initialOverflow = "";
  watch(toRef(element), (el) => {
    const target = resolveElement(toValue(el));
    if (target) {
      const ele = target;
      if (!elInitialOverflow.get(ele))
        elInitialOverflow.set(ele, ele.style.overflow);
      if (ele.style.overflow !== "hidden")
        initialOverflow = ele.style.overflow;
      if (ele.style.overflow === "hidden")
        return isLocked.value = true;
      if (isLocked.value)
        return ele.style.overflow = "hidden";
    }
  }, {
    immediate: true
  });
  const lock = () => {
    const el = resolveElement(toValue(element));
    if (!el || isLocked.value)
      return;
    if (isIOS) {
      stopTouchMoveListener = useEventListener(
        el,
        "touchmove",
        (e) => {
          preventDefault(e);
        },
        { passive: false }
      );
    }
    el.style.overflow = "hidden";
    isLocked.value = true;
  };
  const unlock = () => {
    const el = resolveElement(toValue(element));
    if (!el || !isLocked.value)
      return;
    if (isIOS)
      stopTouchMoveListener == null ? void 0 : stopTouchMoveListener();
    el.style.overflow = initialOverflow;
    elInitialOverflow.delete(el);
    isLocked.value = false;
  };
  tryOnScopeDispose(unlock);
  return computed({
    get() {
      return isLocked.value;
    },
    set(v) {
      if (v)
        lock();
      else unlock();
    }
  });
}
function useSessionStorage(key, initialValue, options = {}) {
  const { window: window2 = defaultWindow } = options;
  return useStorage(key, initialValue, window2 == null ? void 0 : window2.sessionStorage, options);
}
function useWindowScroll(options = {}) {
  const { window: window2 = defaultWindow, ...rest } = options;
  return useScroll(window2, rest);
}
function useWindowSize(options = {}) {
  const {
    window: window2 = defaultWindow,
    initialWidth = Number.POSITIVE_INFINITY,
    initialHeight = Number.POSITIVE_INFINITY,
    listenOrientation = true,
    includeScrollbar = true,
    type = "inner"
  } = options;
  const width = shallowRef(initialWidth);
  const height = shallowRef(initialHeight);
  const update = () => {
    if (window2) {
      if (type === "outer") {
        width.value = window2.outerWidth;
        height.value = window2.outerHeight;
      } else if (type === "visual" && window2.visualViewport) {
        const { width: visualViewportWidth, height: visualViewportHeight, scale } = window2.visualViewport;
        width.value = Math.round(visualViewportWidth * scale);
        height.value = Math.round(visualViewportHeight * scale);
      } else if (includeScrollbar) {
        width.value = window2.innerWidth;
        height.value = window2.innerHeight;
      } else {
        width.value = window2.document.documentElement.clientWidth;
        height.value = window2.document.documentElement.clientHeight;
      }
    }
  };
  update();
  tryOnMounted(update);
  const listenerOptions = { passive: true };
  useEventListener("resize", update, listenerOptions);
  if (window2 && type === "visual" && window2.visualViewport) {
    useEventListener(window2.visualViewport, "resize", update, listenerOptions);
  }
  if (listenOrientation) {
    const matches = useMediaQuery("(orientation: portrait)");
    watch(matches, () => update());
  }
  return { width, height };
}
const __vite_import_meta_env__ = {};
const EXTERNAL_URL_RE = /^(?:[a-z]+:|\/\/)/i;
const APPEARANCE_KEY = "vitepress-theme-appearance";
const HASH_RE = /#.*$/;
const HASH_OR_QUERY_RE = /[?#].*$/;
const INDEX_OR_EXT_RE = /(?:(^|\/)index)?\.(?:md|html)$/;
const inBrowser = typeof document !== "undefined";
const notFoundPageData = {
  relativePath: "404.md",
  filePath: "",
  title: "404",
  description: "Not Found",
  headers: [],
  frontmatter: { sidebar: false, layout: "page" },
  lastUpdated: 0,
  isNotFound: true
};
function isActive(currentPath, matchPath, asRegex = false) {
  if (matchPath === void 0) {
    return false;
  }
  currentPath = normalize(`/${currentPath}`);
  if (asRegex) {
    return new RegExp(matchPath).test(currentPath);
  }
  if (normalize(matchPath) !== currentPath) {
    return false;
  }
  const hashMatch = matchPath.match(HASH_RE);
  if (hashMatch) {
    return (inBrowser ? location.hash : "") === hashMatch[0];
  }
  return true;
}
function normalize(path) {
  return decodeURI(path).replace(HASH_OR_QUERY_RE, "").replace(INDEX_OR_EXT_RE, "$1");
}
function isExternal(path) {
  return EXTERNAL_URL_RE.test(path);
}
function getLocaleForPath(siteData2, relativePath) {
  return Object.keys((siteData2 == null ? void 0 : siteData2.locales) || {}).find((key) => key !== "root" && !isExternal(key) && isActive(relativePath, `/${key}/`, true)) || "root";
}
function resolveSiteDataByRoute(siteData2, relativePath) {
  var _a, _b, _c, _d, _e, _f, _g;
  const localeIndex = getLocaleForPath(siteData2, relativePath);
  return Object.assign({}, siteData2, {
    localeIndex,
    lang: ((_a = siteData2.locales[localeIndex]) == null ? void 0 : _a.lang) ?? siteData2.lang,
    dir: ((_b = siteData2.locales[localeIndex]) == null ? void 0 : _b.dir) ?? siteData2.dir,
    title: ((_c = siteData2.locales[localeIndex]) == null ? void 0 : _c.title) ?? siteData2.title,
    titleTemplate: ((_d = siteData2.locales[localeIndex]) == null ? void 0 : _d.titleTemplate) ?? siteData2.titleTemplate,
    description: ((_e = siteData2.locales[localeIndex]) == null ? void 0 : _e.description) ?? siteData2.description,
    head: mergeHead(siteData2.head, ((_f = siteData2.locales[localeIndex]) == null ? void 0 : _f.head) ?? []),
    themeConfig: {
      ...siteData2.themeConfig,
      ...(_g = siteData2.locales[localeIndex]) == null ? void 0 : _g.themeConfig
    }
  });
}
function createTitle(siteData2, pageData) {
  const title = pageData.title || siteData2.title;
  const template = pageData.titleTemplate ?? siteData2.titleTemplate;
  if (typeof template === "string" && template.includes(":title")) {
    return template.replace(/:title/g, title);
  }
  const templateString = createTitleTemplate(siteData2.title, template);
  if (title === templateString.slice(3)) {
    return title;
  }
  return `${title}${templateString}`;
}
function createTitleTemplate(siteTitle, template) {
  if (template === false) {
    return "";
  }
  if (template === true || template === void 0) {
    return ` | ${siteTitle}`;
  }
  if (siteTitle === template) {
    return "";
  }
  return ` | ${template}`;
}
function hasTag(head, tag) {
  const [tagType, tagAttrs] = tag;
  if (tagType !== "meta")
    return false;
  const keyAttr = Object.entries(tagAttrs)[0];
  if (keyAttr == null)
    return false;
  return head.some(([type, attrs]) => type === tagType && attrs[keyAttr[0]] === keyAttr[1]);
}
function mergeHead(prev, curr) {
  return [...prev.filter((tagAttrs) => !hasTag(curr, tagAttrs)), ...curr];
}
const INVALID_CHAR_REGEX = /[\u0000-\u001F"#$&*+,:;<=>?[\]^`{|}\u007F]/g;
const DRIVE_LETTER_REGEX = /^[a-z]:/i;
function sanitizeFileName(name) {
  const match = DRIVE_LETTER_REGEX.exec(name);
  const driveLetter = match ? match[0] : "";
  return driveLetter + name.slice(driveLetter.length).replace(INVALID_CHAR_REGEX, "_").replace(/(^|\/)_+(?=[^/]*$)/, "$1");
}
const KNOWN_EXTENSIONS = /* @__PURE__ */ new Set();
function treatAsHtml(filename) {
  var _a;
  if (KNOWN_EXTENSIONS.size === 0) {
    const extraExts = typeof process === "object" && ((_a = process.env) == null ? void 0 : _a.VITE_EXTRA_EXTENSIONS) || (__vite_import_meta_env__ == null ? void 0 : __vite_import_meta_env__.VITE_EXTRA_EXTENSIONS) || "";
    ("3g2,3gp,aac,ai,apng,au,avif,bin,bmp,cer,class,conf,crl,css,csv,dll,doc,eps,epub,exe,gif,gz,ics,ief,jar,jpe,jpeg,jpg,js,json,jsonld,m4a,man,mid,midi,mjs,mov,mp2,mp3,mp4,mpe,mpeg,mpg,mpp,oga,ogg,ogv,ogx,opus,otf,p10,p7c,p7m,p7s,pdf,png,ps,qt,roff,rtf,rtx,ser,svg,t,tif,tiff,tr,ts,tsv,ttf,txt,vtt,wav,weba,webm,webp,woff,woff2,xhtml,xml,yaml,yml,zip" + (extraExts && typeof extraExts === "string" ? "," + extraExts : "")).split(",").forEach((ext2) => KNOWN_EXTENSIONS.add(ext2));
  }
  const ext = filename.split(".").pop();
  return ext == null || !KNOWN_EXTENSIONS.has(ext.toLowerCase());
}
function escapeRegExp(str) {
  return str.replace(/[|\\{}()[\]^$+*?.]/g, "\\$&").replace(/-/g, "\\x2d");
}
const dataSymbol = Symbol();
const siteDataRef = shallowRef(siteData);
function initData(route) {
  const site = computed(() => resolveSiteDataByRoute(siteDataRef.value, route.data.relativePath));
  const appearance = site.value.appearance;
  const isDark = appearance === "force-dark" ? ref(true) : appearance === "force-auto" ? usePreferredDark() : appearance ? useDark({
    storageKey: APPEARANCE_KEY,
    initialValue: () => appearance === "dark" ? "dark" : "auto",
    ...typeof appearance === "object" ? appearance : {}
  }) : ref(false);
  const hashRef = ref(inBrowser ? location.hash : "");
  if (inBrowser) {
    window.addEventListener("hashchange", () => {
      hashRef.value = location.hash;
    });
  }
  watch(() => route.data, () => {
    hashRef.value = inBrowser ? location.hash : "";
  });
  return {
    site,
    theme: computed(() => site.value.themeConfig),
    page: computed(() => route.data),
    frontmatter: computed(() => route.data.frontmatter),
    params: computed(() => route.data.params),
    lang: computed(() => site.value.lang),
    dir: computed(() => route.data.frontmatter.dir || site.value.dir),
    localeIndex: computed(() => site.value.localeIndex || "root"),
    title: computed(() => createTitle(site.value, route.data)),
    description: computed(() => route.data.description || site.value.description),
    isDark,
    hash: computed(() => hashRef.value)
  };
}
function useData$1() {
  const data = inject(dataSymbol);
  if (!data) {
    throw new Error("vitepress data not properly injected in app");
  }
  return data;
}
function joinPath(base, path) {
  return `${base}${path}`.replace(/\/+/g, "/");
}
function withBase(path) {
  return EXTERNAL_URL_RE.test(path) || !path.startsWith("/") ? path : joinPath(siteDataRef.value.base, path);
}
function pathToFile(path) {
  let pagePath = path.replace(/\.html$/, "");
  pagePath = decodeURIComponent(pagePath);
  pagePath = pagePath.replace(/\/$/, "/index");
  {
    if (inBrowser) {
      const base = "/";
      pagePath = sanitizeFileName(pagePath.slice(base.length).replace(/\//g, "_") || "index") + ".md";
      let pageHash = __VP_HASH_MAP__[pagePath.toLowerCase()];
      if (!pageHash) {
        pagePath = pagePath.endsWith("_index.md") ? pagePath.slice(0, -9) + ".md" : pagePath.slice(0, -3) + "_index.md";
        pageHash = __VP_HASH_MAP__[pagePath.toLowerCase()];
      }
      if (!pageHash)
        return null;
      pagePath = `${base}${"assets"}/${pagePath}.${pageHash}.js`;
    } else {
      pagePath = `./${sanitizeFileName(pagePath.slice(1).replace(/\//g, "_"))}.md.js`;
    }
  }
  return pagePath;
}
let contentUpdatedCallbacks = [];
function onContentUpdated(fn) {
  contentUpdatedCallbacks.push(fn);
  onUnmounted(() => {
    contentUpdatedCallbacks = contentUpdatedCallbacks.filter((f) => f !== fn);
  });
}
function getScrollOffset() {
  let scrollOffset = siteDataRef.value.scrollOffset;
  let offset = 0;
  let padding = 24;
  if (typeof scrollOffset === "object" && "padding" in scrollOffset) {
    padding = scrollOffset.padding;
    scrollOffset = scrollOffset.selector;
  }
  if (typeof scrollOffset === "number") {
    offset = scrollOffset;
  } else if (typeof scrollOffset === "string") {
    offset = tryOffsetSelector(scrollOffset, padding);
  } else if (Array.isArray(scrollOffset)) {
    for (const selector of scrollOffset) {
      const res = tryOffsetSelector(selector, padding);
      if (res) {
        offset = res;
        break;
      }
    }
  }
  return offset;
}
function tryOffsetSelector(selector, padding) {
  const el = document.querySelector(selector);
  if (!el)
    return 0;
  const bot = el.getBoundingClientRect().bottom;
  if (bot < 0)
    return 0;
  return bot + padding;
}
const RouterSymbol = Symbol();
const fakeHost = "http://a.com";
const getDefaultRoute = () => ({
  path: "/",
  component: null,
  data: notFoundPageData
});
function createRouter(loadPageModule, fallbackComponent) {
  const route = reactive(getDefaultRoute());
  const router = {
    route,
    go
  };
  async function go(href = inBrowser ? location.href : "/") {
    var _a, _b;
    href = normalizeHref(href);
    if (await ((_a = router.onBeforeRouteChange) == null ? void 0 : _a.call(router, href)) === false)
      return;
    if (inBrowser && href !== normalizeHref(location.href)) {
      history.replaceState({ scrollPosition: window.scrollY }, "");
      history.pushState({}, "", href);
    }
    await loadPage(href);
    await ((_b = router.onAfterRouteChange ?? router.onAfterRouteChanged) == null ? void 0 : _b(href));
  }
  let latestPendingPath = null;
  async function loadPage(href, scrollPosition = 0, isRetry = false) {
    var _a, _b;
    if (await ((_a = router.onBeforePageLoad) == null ? void 0 : _a.call(router, href)) === false)
      return;
    const targetLoc = new URL(href, fakeHost);
    const pendingPath = latestPendingPath = targetLoc.pathname;
    try {
      let page = await loadPageModule(pendingPath);
      if (!page) {
        throw new Error(`Page not found: ${pendingPath}`);
      }
      if (latestPendingPath === pendingPath) {
        latestPendingPath = null;
        const { default: comp, __pageData } = page;
        if (!comp) {
          throw new Error(`Invalid route component: ${comp}`);
        }
        await ((_b = router.onAfterPageLoad) == null ? void 0 : _b.call(router, href));
        route.path = inBrowser ? pendingPath : withBase(pendingPath);
        route.component = markRaw(comp);
        route.data = true ? markRaw(__pageData) : readonly(__pageData);
        if (inBrowser) {
          nextTick(() => {
            let actualPathname = siteDataRef.value.base + __pageData.relativePath.replace(/(?:(^|\/)index)?\.md$/, "$1");
            if (!siteDataRef.value.cleanUrls && !actualPathname.endsWith("/")) {
              actualPathname += ".html";
            }
            if (actualPathname !== targetLoc.pathname) {
              targetLoc.pathname = actualPathname;
              href = actualPathname + targetLoc.search + targetLoc.hash;
              history.replaceState({}, "", href);
            }
            if (targetLoc.hash && !scrollPosition) {
              let target = null;
              try {
                target = document.getElementById(decodeURIComponent(targetLoc.hash).slice(1));
              } catch (e) {
                console.warn(e);
              }
              if (target) {
                scrollTo(target, targetLoc.hash);
                return;
              }
            }
            window.scrollTo(0, scrollPosition);
          });
        }
      }
    } catch (err) {
      if (!/fetch|Page not found/.test(err.message) && !/^\/404(\.html|\/)?$/.test(href)) {
        console.error(err);
      }
      if (!isRetry) {
        try {
          const res = await fetch(siteDataRef.value.base + "hashmap.json");
          window.__VP_HASH_MAP__ = await res.json();
          await loadPage(href, scrollPosition, true);
          return;
        } catch (e) {
        }
      }
      if (latestPendingPath === pendingPath) {
        latestPendingPath = null;
        route.path = inBrowser ? pendingPath : withBase(pendingPath);
        route.component = fallbackComponent ? markRaw(fallbackComponent) : null;
        const relativePath = inBrowser ? pendingPath.replace(/(^|\/)$/, "$1index").replace(/(\.html)?$/, ".md").replace(/^\//, "") : "404.md";
        route.data = { ...notFoundPageData, relativePath };
      }
    }
  }
  if (inBrowser) {
    if (history.state === null) {
      history.replaceState({}, "");
    }
    window.addEventListener("click", (e) => {
      if (e.defaultPrevented || !(e.target instanceof Element) || e.target.closest("button") || // temporary fix for docsearch action buttons
      e.button !== 0 || e.ctrlKey || e.shiftKey || e.altKey || e.metaKey)
        return;
      const link2 = e.target.closest("a");
      if (!link2 || link2.closest(".vp-raw") || link2.hasAttribute("download") || link2.hasAttribute("target"))
        return;
      const linkHref = link2.getAttribute("href") ?? (link2 instanceof SVGAElement ? link2.getAttribute("xlink:href") : null);
      if (linkHref == null)
        return;
      const { href, origin, pathname, hash, search } = new URL(linkHref, link2.baseURI);
      const currentUrl = new URL(location.href);
      if (origin === currentUrl.origin && treatAsHtml(pathname)) {
        e.preventDefault();
        if (pathname === currentUrl.pathname && search === currentUrl.search) {
          if (hash !== currentUrl.hash) {
            history.pushState({}, "", href);
            window.dispatchEvent(new HashChangeEvent("hashchange", {
              oldURL: currentUrl.href,
              newURL: href
            }));
          }
          if (hash) {
            scrollTo(link2, hash, link2.classList.contains("header-anchor"));
          } else {
            window.scrollTo(0, 0);
          }
        } else {
          go(href);
        }
      }
    }, { capture: true });
    window.addEventListener("popstate", async (e) => {
      var _a;
      if (e.state === null)
        return;
      const href = normalizeHref(location.href);
      await loadPage(href, e.state && e.state.scrollPosition || 0);
      await ((_a = router.onAfterRouteChange ?? router.onAfterRouteChanged) == null ? void 0 : _a(href));
    });
    window.addEventListener("hashchange", (e) => {
      e.preventDefault();
    });
  }
  return router;
}
function useRouter() {
  const router = inject(RouterSymbol);
  if (!router) {
    throw new Error("useRouter() is called without provider.");
  }
  return router;
}
function useRoute() {
  return useRouter().route;
}
function scrollTo(el, hash, smooth = false) {
  let target = null;
  try {
    target = el.classList.contains("header-anchor") ? el : document.getElementById(decodeURIComponent(hash).slice(1));
  } catch (e) {
    console.warn(e);
  }
  if (target) {
    let scrollToTarget = function() {
      if (!smooth || Math.abs(targetTop - window.scrollY) > window.innerHeight)
        window.scrollTo(0, targetTop);
      else
        window.scrollTo({ left: 0, top: targetTop, behavior: "smooth" });
    };
    const targetPadding = parseInt(window.getComputedStyle(target).paddingTop, 10);
    const targetTop = window.scrollY + target.getBoundingClientRect().top - getScrollOffset() + targetPadding;
    requestAnimationFrame(scrollToTarget);
  }
}
function normalizeHref(href) {
  const url = new URL(href, fakeHost);
  url.pathname = url.pathname.replace(/(^|\/)index(\.html)?$/, "$1");
  if (siteDataRef.value.cleanUrls)
    url.pathname = url.pathname.replace(/\.html$/, "");
  else if (!url.pathname.endsWith("/") && !url.pathname.endsWith(".html"))
    url.pathname += ".html";
  return url.pathname + url.search + url.hash;
}
const runCbs = () => contentUpdatedCallbacks.forEach((fn) => fn());
const Content = defineComponent({
  name: "VitePressContent",
  props: {
    as: { type: [Object, String], default: "div" }
  },
  setup(props) {
    const route = useRoute();
    const { frontmatter, site } = useData$1();
    watch(frontmatter, runCbs, { deep: true, flush: "post" });
    return () => h(props.as, site.value.contentProps ?? { style: { position: "relative" } }, [
      route.component ? h(route.component, {
        onVnodeMounted: runCbs,
        onVnodeUpdated: runCbs,
        onVnodeUnmounted: runCbs
      }) : "404 Page Not Found"
    ]);
  }
});
const _sfc_main$16 = /* @__PURE__ */ defineComponent({
  __name: "VPBackdrop",
  __ssrInlineRender: true,
  props: {
    show: { type: Boolean }
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      if (__props.show) {
        _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPBackdrop" }, _attrs))} data-v-5c9d710e></div>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$16 = _sfc_main$16.setup;
_sfc_main$16.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPBackdrop.vue");
  return _sfc_setup$16 ? _sfc_setup$16(props, ctx) : void 0;
};
const VPBackdrop = /* @__PURE__ */ _export_sfc(_sfc_main$16, [["__scopeId", "data-v-5c9d710e"]]);
const useData = useData$1;
function throttleAndDebounce(fn, delay) {
  let timeoutId;
  let called = false;
  return () => {
    if (timeoutId)
      clearTimeout(timeoutId);
    if (!called) {
      fn();
      (called = true) && setTimeout(() => called = false, delay);
    } else
      timeoutId = setTimeout(fn, delay);
  };
}
function ensureStartingSlash(path) {
  return path.startsWith("/") ? path : `/${path}`;
}
function normalizeLink$1(url) {
  const { pathname, search, hash, protocol } = new URL(url, "http://a.com");
  if (isExternal(url) || url.startsWith("#") || !protocol.startsWith("http") || !treatAsHtml(pathname))
    return url;
  const { site } = useData();
  const normalizedPath = pathname.endsWith("/") || pathname.endsWith(".html") ? url : url.replace(/(?:(^\.+)\/)?.*$/, `$1${pathname.replace(/(\.md)?$/, site.value.cleanUrls ? "" : ".html")}${search}${hash}`);
  return withBase(normalizedPath);
}
function useLangs({ correspondingLink = false } = {}) {
  const { site, localeIndex, page, theme: theme2, hash } = useData();
  const currentLang = computed(() => {
    var _a, _b;
    return {
      label: (_a = site.value.locales[localeIndex.value]) == null ? void 0 : _a.label,
      link: ((_b = site.value.locales[localeIndex.value]) == null ? void 0 : _b.link) || (localeIndex.value === "root" ? "/" : `/${localeIndex.value}/`)
    };
  });
  const localeLinks = computed(() => Object.entries(site.value.locales).flatMap(([key, value]) => currentLang.value.label === value.label ? [] : {
    text: value.label,
    link: normalizeLink(value.link || (key === "root" ? "/" : `/${key}/`), theme2.value.i18nRouting !== false && correspondingLink, page.value.relativePath.slice(currentLang.value.link.length - 1), !site.value.cleanUrls) + hash.value
  }));
  return { localeLinks, currentLang };
}
function normalizeLink(link2, addPath, path, addExt) {
  return addPath ? link2.replace(/\/$/, "") + ensureStartingSlash(path.replace(/(^|\/)index\.md$/, "$1").replace(/\.md$/, addExt ? ".html" : "")) : link2;
}
const _sfc_main$15 = /* @__PURE__ */ defineComponent({
  __name: "NotFound",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    const { currentLang } = useLangs();
    return (_ctx, _push, _parent, _attrs) => {
      var _a, _b, _c, _d, _e;
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "NotFound" }, _attrs))} data-v-3a76262f><p class="code" data-v-3a76262f>${ssrInterpolate(((_a = unref(theme2).notFound) == null ? void 0 : _a.code) ?? "404")}</p><h1 class="title" data-v-3a76262f>${ssrInterpolate(((_b = unref(theme2).notFound) == null ? void 0 : _b.title) ?? "PAGE NOT FOUND")}</h1><div class="divider" data-v-3a76262f></div><blockquote class="quote" data-v-3a76262f>${ssrInterpolate(((_c = unref(theme2).notFound) == null ? void 0 : _c.quote) ?? "But if you don't change your direction, and if you keep looking, you may end up where you are heading.")}</blockquote><div class="action" data-v-3a76262f><a class="link"${ssrRenderAttr("href", unref(withBase)(unref(currentLang).link))}${ssrRenderAttr("aria-label", ((_d = unref(theme2).notFound) == null ? void 0 : _d.linkLabel) ?? "go to home")} data-v-3a76262f>${ssrInterpolate(((_e = unref(theme2).notFound) == null ? void 0 : _e.linkText) ?? "Take me home")}</a></div></div>`);
    };
  }
});
const _sfc_setup$15 = _sfc_main$15.setup;
_sfc_main$15.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/NotFound.vue");
  return _sfc_setup$15 ? _sfc_setup$15(props, ctx) : void 0;
};
const NotFound = /* @__PURE__ */ _export_sfc(_sfc_main$15, [["__scopeId", "data-v-3a76262f"]]);
function getSidebar(_sidebar, path) {
  if (Array.isArray(_sidebar))
    return addBase(_sidebar);
  if (_sidebar == null)
    return [];
  path = ensureStartingSlash(path);
  const dir = Object.keys(_sidebar).sort((a, b) => {
    return b.split("/").length - a.split("/").length;
  }).find((dir2) => {
    return path.startsWith(ensureStartingSlash(dir2));
  });
  const sidebar = dir ? _sidebar[dir] : [];
  return Array.isArray(sidebar) ? addBase(sidebar) : addBase(sidebar.items, sidebar.base);
}
function getSidebarGroups(sidebar) {
  const groups = [];
  let lastGroupIndex = 0;
  for (const index in sidebar) {
    const item = sidebar[index];
    if (item.items) {
      lastGroupIndex = groups.push(item);
      continue;
    }
    if (!groups[lastGroupIndex]) {
      groups.push({ items: [] });
    }
    groups[lastGroupIndex].items.push(item);
  }
  return groups;
}
function getFlatSideBarLinks(sidebar) {
  const links = [];
  function recursivelyExtractLinks(items) {
    for (const item of items) {
      if (item.text && item.link) {
        links.push({
          text: item.text,
          link: item.link,
          docFooterText: item.docFooterText
        });
      }
      if (item.items) {
        recursivelyExtractLinks(item.items);
      }
    }
  }
  recursivelyExtractLinks(sidebar);
  return links;
}
function hasActiveLink(path, items) {
  if (Array.isArray(items)) {
    return items.some((item) => hasActiveLink(path, item));
  }
  return isActive(path, items.link) ? true : items.items ? hasActiveLink(path, items.items) : false;
}
function addBase(items, _base) {
  return [...items].map((_item) => {
    const item = { ..._item };
    const base = item.base || _base;
    if (base && item.link)
      item.link = base + item.link;
    if (item.items)
      item.items = addBase(item.items, base);
    return item;
  });
}
function useSidebar() {
  const { frontmatter, page, theme: theme2 } = useData();
  const is960 = useMediaQuery("(min-width: 960px)");
  const isOpen = ref(false);
  const _sidebar = computed(() => {
    const sidebarConfig = theme2.value.sidebar;
    const relativePath = page.value.relativePath;
    return sidebarConfig ? getSidebar(sidebarConfig, relativePath) : [];
  });
  const sidebar = ref(_sidebar.value);
  watch(_sidebar, (next, prev) => {
    if (JSON.stringify(next) !== JSON.stringify(prev))
      sidebar.value = _sidebar.value;
  });
  const hasSidebar = computed(() => {
    return frontmatter.value.sidebar !== false && sidebar.value.length > 0 && frontmatter.value.layout !== "home";
  });
  const leftAside = computed(() => {
    if (hasAside)
      return frontmatter.value.aside == null ? theme2.value.aside === "left" : frontmatter.value.aside === "left";
    return false;
  });
  const hasAside = computed(() => {
    if (frontmatter.value.layout === "home")
      return false;
    if (frontmatter.value.aside != null)
      return !!frontmatter.value.aside;
    return theme2.value.aside !== false;
  });
  const isSidebarEnabled = computed(() => hasSidebar.value && is960.value);
  const sidebarGroups = computed(() => {
    return hasSidebar.value ? getSidebarGroups(sidebar.value) : [];
  });
  function open() {
    isOpen.value = true;
  }
  function close() {
    isOpen.value = false;
  }
  function toggle() {
    isOpen.value ? close() : open();
  }
  return {
    isOpen,
    sidebar,
    sidebarGroups,
    hasSidebar,
    hasAside,
    leftAside,
    isSidebarEnabled,
    open,
    close,
    toggle
  };
}
function useCloseSidebarOnEscape(isOpen, close) {
  let triggerElement;
  watchEffect(() => {
    triggerElement = isOpen.value ? document.activeElement : void 0;
  });
  onMounted(() => {
    window.addEventListener("keyup", onEscape);
  });
  onUnmounted(() => {
    window.removeEventListener("keyup", onEscape);
  });
  function onEscape(e) {
    if (e.key === "Escape" && isOpen.value) {
      close();
      triggerElement == null ? void 0 : triggerElement.focus();
    }
  }
}
function useSidebarControl(item) {
  const { page, hash } = useData();
  const collapsed = ref(false);
  const collapsible = computed(() => {
    return item.value.collapsed != null;
  });
  const isLink = computed(() => {
    return !!item.value.link;
  });
  const isActiveLink = ref(false);
  const updateIsActiveLink = () => {
    isActiveLink.value = isActive(page.value.relativePath, item.value.link);
  };
  watch([page, item, hash], updateIsActiveLink);
  onMounted(updateIsActiveLink);
  const hasActiveLink$1 = computed(() => {
    if (isActiveLink.value) {
      return true;
    }
    return item.value.items ? hasActiveLink(page.value.relativePath, item.value.items) : false;
  });
  const hasChildren = computed(() => {
    return !!(item.value.items && item.value.items.length);
  });
  watchEffect(() => {
    collapsed.value = !!(collapsible.value && item.value.collapsed);
  });
  watchPostEffect(() => {
    (isActiveLink.value || hasActiveLink$1.value) && (collapsed.value = false);
  });
  function toggle() {
    if (collapsible.value) {
      collapsed.value = !collapsed.value;
    }
  }
  return {
    collapsed,
    collapsible,
    isLink,
    isActiveLink,
    hasActiveLink: hasActiveLink$1,
    hasChildren,
    toggle
  };
}
function useAside() {
  const { hasSidebar } = useSidebar();
  const is960 = useMediaQuery("(min-width: 960px)");
  const is1280 = useMediaQuery("(min-width: 1280px)");
  const isAsideEnabled = computed(() => {
    if (!is1280.value && !is960.value) {
      return false;
    }
    return hasSidebar.value ? is1280.value : is960.value;
  });
  return {
    isAsideEnabled
  };
}
const ignoreRE = /\b(?:VPBadge|header-anchor|footnote-ref|ignore-header)\b/;
const resolvedHeaders = [];
function resolveTitle(theme2) {
  return typeof theme2.outline === "object" && !Array.isArray(theme2.outline) && theme2.outline.label || theme2.outlineTitle || "On this page";
}
function getHeaders(range) {
  const headers = [
    ...document.querySelectorAll(".VPDoc :where(h1,h2,h3,h4,h5,h6)")
  ].filter((el) => el.id && el.hasChildNodes()).map((el) => {
    const level = Number(el.tagName[1]);
    return {
      element: el,
      title: serializeHeader(el),
      link: "#" + el.id,
      level
    };
  });
  return resolveHeaders(headers, range);
}
function serializeHeader(h2) {
  let ret = "";
  for (const node of h2.childNodes) {
    if (node.nodeType === 1) {
      if (ignoreRE.test(node.className))
        continue;
      ret += node.textContent;
    } else if (node.nodeType === 3) {
      ret += node.textContent;
    }
  }
  return ret.trim();
}
function resolveHeaders(headers, range) {
  if (range === false) {
    return [];
  }
  const levelsRange = (typeof range === "object" && !Array.isArray(range) ? range.level : range) || 2;
  const [high, low] = typeof levelsRange === "number" ? [levelsRange, levelsRange] : levelsRange === "deep" ? [2, 6] : levelsRange;
  return buildTree(headers, high, low);
}
function useActiveAnchor(container, marker) {
  const { isAsideEnabled } = useAside();
  const onScroll = throttleAndDebounce(setActiveLink, 100);
  let prevActiveLink = null;
  onMounted(() => {
    requestAnimationFrame(setActiveLink);
    window.addEventListener("scroll", onScroll);
  });
  onUpdated(() => {
    activateLink(location.hash);
  });
  onUnmounted(() => {
    window.removeEventListener("scroll", onScroll);
  });
  function setActiveLink() {
    if (!isAsideEnabled.value) {
      return;
    }
    const scrollY = window.scrollY;
    const innerHeight = window.innerHeight;
    const offsetHeight = document.body.offsetHeight;
    const isBottom = Math.abs(scrollY + innerHeight - offsetHeight) < 1;
    const headers = resolvedHeaders.map(({ element, link: link2 }) => ({
      link: link2,
      top: getAbsoluteTop(element)
    })).filter(({ top }) => !Number.isNaN(top)).sort((a, b) => a.top - b.top);
    if (!headers.length) {
      activateLink(null);
      return;
    }
    if (scrollY < 1) {
      activateLink(null);
      return;
    }
    if (isBottom) {
      activateLink(headers[headers.length - 1].link);
      return;
    }
    let activeLink = null;
    for (const { link: link2, top } of headers) {
      if (top > scrollY + getScrollOffset() + 4) {
        break;
      }
      activeLink = link2;
    }
    activateLink(activeLink);
  }
  function activateLink(hash) {
    if (prevActiveLink) {
      prevActiveLink.classList.remove("active");
    }
    if (hash == null) {
      prevActiveLink = null;
    } else {
      prevActiveLink = container.value.querySelector(`a[href="${decodeURIComponent(hash)}"]`);
    }
    const activeLink = prevActiveLink;
    if (activeLink) {
      activeLink.classList.add("active");
      marker.value.style.top = activeLink.offsetTop + 39 + "px";
      marker.value.style.opacity = "1";
    } else {
      marker.value.style.top = "33px";
      marker.value.style.opacity = "0";
    }
  }
}
function getAbsoluteTop(element) {
  let offsetTop = 0;
  while (element !== document.body) {
    if (element === null) {
      return NaN;
    }
    offsetTop += element.offsetTop;
    element = element.offsetParent;
  }
  return offsetTop;
}
function buildTree(data, min, max) {
  resolvedHeaders.length = 0;
  const result = [];
  const stack = [];
  data.forEach((item) => {
    const node = { ...item, children: [] };
    let parent = stack[stack.length - 1];
    while (parent && parent.level >= node.level) {
      stack.pop();
      parent = stack[stack.length - 1];
    }
    if (node.element.classList.contains("ignore-header") || parent && "shouldIgnore" in parent) {
      stack.push({ level: node.level, shouldIgnore: true });
      return;
    }
    if (node.level > max || node.level < min)
      return;
    resolvedHeaders.push({ element: node.element, link: node.link });
    if (parent)
      parent.children.push(node);
    else
      result.push(node);
    stack.push(node);
  });
  return result;
}
const _sfc_main$14 = /* @__PURE__ */ defineComponent({
  __name: "VPDocOutlineItem",
  __ssrInlineRender: true,
  props: {
    headers: {},
    root: { type: Boolean }
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      const _component_VPDocOutlineItem = resolveComponent("VPDocOutlineItem", true);
      _push(`<ul${ssrRenderAttrs(mergeProps({
        class: ["VPDocOutlineItem", __props.root ? "root" : "nested"]
      }, _attrs))} data-v-3f640748><!--[-->`);
      ssrRenderList(__props.headers, ({ children, link: link2, title }) => {
        _push(`<li data-v-3f640748><a class="outline-link"${ssrRenderAttr("href", link2)}${ssrRenderAttr("title", title)} data-v-3f640748>${ssrInterpolate(title)}</a>`);
        if (children == null ? void 0 : children.length) {
          _push(ssrRenderComponent(_component_VPDocOutlineItem, { headers: children }, null, _parent));
        } else {
          _push(`<!---->`);
        }
        _push(`</li>`);
      });
      _push(`<!--]--></ul>`);
    };
  }
});
const _sfc_setup$14 = _sfc_main$14.setup;
_sfc_main$14.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPDocOutlineItem.vue");
  return _sfc_setup$14 ? _sfc_setup$14(props, ctx) : void 0;
};
const VPDocOutlineItem = /* @__PURE__ */ _export_sfc(_sfc_main$14, [["__scopeId", "data-v-3f640748"]]);
const _sfc_main$13 = /* @__PURE__ */ defineComponent({
  __name: "VPDocAsideOutline",
  __ssrInlineRender: true,
  setup(__props) {
    const { frontmatter, theme: theme2 } = useData();
    const headers = shallowRef([]);
    onContentUpdated(() => {
      headers.value = getHeaders(frontmatter.value.outline ?? theme2.value.outline);
    });
    const container = ref();
    const marker = ref();
    useActiveAnchor(container, marker);
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<nav${ssrRenderAttrs(mergeProps({
        "aria-labelledby": "doc-outline-aria-label",
        class: ["VPDocAsideOutline", { "has-outline": headers.value.length > 0 }],
        ref_key: "container",
        ref: container
      }, _attrs))} data-v-146b846f><div class="content" data-v-146b846f><div class="outline-marker" data-v-146b846f></div><div aria-level="2" class="outline-title" id="doc-outline-aria-label" role="heading" data-v-146b846f>${ssrInterpolate(unref(resolveTitle)(unref(theme2)))}</div>`);
      _push(ssrRenderComponent(VPDocOutlineItem, {
        headers: headers.value,
        root: true
      }, null, _parent));
      _push(`</div></nav>`);
    };
  }
});
const _sfc_setup$13 = _sfc_main$13.setup;
_sfc_main$13.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPDocAsideOutline.vue");
  return _sfc_setup$13 ? _sfc_setup$13(props, ctx) : void 0;
};
const VPDocAsideOutline = /* @__PURE__ */ _export_sfc(_sfc_main$13, [["__scopeId", "data-v-146b846f"]]);
const _sfc_main$12 = /* @__PURE__ */ defineComponent({
  __name: "VPDocAsideCarbonAds",
  __ssrInlineRender: true,
  props: {
    carbonAds: {}
  },
  setup(__props) {
    const VPCarbonAds = () => null;
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPDocAsideCarbonAds" }, _attrs))}>`);
      _push(ssrRenderComponent(unref(VPCarbonAds), { "carbon-ads": __props.carbonAds }, null, _parent));
      _push(`</div>`);
    };
  }
});
const _sfc_setup$12 = _sfc_main$12.setup;
_sfc_main$12.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPDocAsideCarbonAds.vue");
  return _sfc_setup$12 ? _sfc_setup$12(props, ctx) : void 0;
};
const _sfc_main$11 = /* @__PURE__ */ defineComponent({
  __name: "VPDocAside",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPDocAside" }, _attrs))} data-v-8d6b64b7>`);
      ssrRenderSlot(_ctx.$slots, "aside-top", {}, null, _push, _parent);
      ssrRenderSlot(_ctx.$slots, "aside-outline-before", {}, null, _push, _parent);
      _push(ssrRenderComponent(VPDocAsideOutline, null, null, _parent));
      ssrRenderSlot(_ctx.$slots, "aside-outline-after", {}, null, _push, _parent);
      _push(`<div class="spacer" data-v-8d6b64b7></div>`);
      ssrRenderSlot(_ctx.$slots, "aside-ads-before", {}, null, _push, _parent);
      if (unref(theme2).carbonAds) {
        _push(ssrRenderComponent(_sfc_main$12, {
          "carbon-ads": unref(theme2).carbonAds
        }, null, _parent));
      } else {
        _push(`<!---->`);
      }
      ssrRenderSlot(_ctx.$slots, "aside-ads-after", {}, null, _push, _parent);
      ssrRenderSlot(_ctx.$slots, "aside-bottom", {}, null, _push, _parent);
      _push(`</div>`);
    };
  }
});
const _sfc_setup$11 = _sfc_main$11.setup;
_sfc_main$11.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPDocAside.vue");
  return _sfc_setup$11 ? _sfc_setup$11(props, ctx) : void 0;
};
const VPDocAside = /* @__PURE__ */ _export_sfc(_sfc_main$11, [["__scopeId", "data-v-8d6b64b7"]]);
function useEditLink() {
  const { theme: theme2, page } = useData();
  return computed(() => {
    const { text = "Edit this page", pattern = "" } = theme2.value.editLink || {};
    let url;
    if (typeof pattern === "function") {
      url = pattern(page.value);
    } else {
      url = pattern.replace(/:path/g, page.value.filePath);
    }
    return { url, text };
  });
}
function usePrevNext() {
  const { page, theme: theme2, frontmatter } = useData();
  return computed(() => {
    var _a, _b, _c, _d, _e, _f, _g, _h;
    const sidebar = getSidebar(theme2.value.sidebar, page.value.relativePath);
    const links = getFlatSideBarLinks(sidebar);
    const candidates = uniqBy(links, (link2) => link2.link.replace(/[?#].*$/, ""));
    const index = candidates.findIndex((link2) => {
      return isActive(page.value.relativePath, link2.link);
    });
    const hidePrev = ((_a = theme2.value.docFooter) == null ? void 0 : _a.prev) === false && !frontmatter.value.prev || frontmatter.value.prev === false;
    const hideNext = ((_b = theme2.value.docFooter) == null ? void 0 : _b.next) === false && !frontmatter.value.next || frontmatter.value.next === false;
    return {
      prev: hidePrev ? void 0 : {
        text: (typeof frontmatter.value.prev === "string" ? frontmatter.value.prev : typeof frontmatter.value.prev === "object" ? frontmatter.value.prev.text : void 0) ?? ((_c = candidates[index - 1]) == null ? void 0 : _c.docFooterText) ?? ((_d = candidates[index - 1]) == null ? void 0 : _d.text),
        link: (typeof frontmatter.value.prev === "object" ? frontmatter.value.prev.link : void 0) ?? ((_e = candidates[index - 1]) == null ? void 0 : _e.link)
      },
      next: hideNext ? void 0 : {
        text: (typeof frontmatter.value.next === "string" ? frontmatter.value.next : typeof frontmatter.value.next === "object" ? frontmatter.value.next.text : void 0) ?? ((_f = candidates[index + 1]) == null ? void 0 : _f.docFooterText) ?? ((_g = candidates[index + 1]) == null ? void 0 : _g.text),
        link: (typeof frontmatter.value.next === "object" ? frontmatter.value.next.link : void 0) ?? ((_h = candidates[index + 1]) == null ? void 0 : _h.link)
      }
    };
  });
}
function uniqBy(array, keyFn) {
  const seen = /* @__PURE__ */ new Set();
  return array.filter((item) => {
    const k = keyFn(item);
    return seen.has(k) ? false : seen.add(k);
  });
}
const _sfc_main$10 = /* @__PURE__ */ defineComponent({
  __name: "VPLink",
  __ssrInlineRender: true,
  props: {
    tag: {},
    href: {},
    noIcon: { type: Boolean },
    target: {},
    rel: {}
  },
  setup(__props) {
    const props = __props;
    const tag = computed(() => props.tag ?? (props.href ? "a" : "span"));
    const isExternal2 = computed(
      () => props.href && EXTERNAL_URL_RE.test(props.href) || props.target === "_blank"
    );
    return (_ctx, _push, _parent, _attrs) => {
      ssrRenderVNode(_push, createVNode(resolveDynamicComponent(tag.value), mergeProps({
        class: ["VPLink", {
          link: __props.href,
          "vp-external-link-icon": isExternal2.value,
          "no-icon": __props.noIcon
        }],
        href: __props.href ? unref(normalizeLink$1)(__props.href) : void 0,
        target: __props.target ?? (isExternal2.value ? "_blank" : void 0),
        rel: __props.rel ?? (isExternal2.value ? "noreferrer" : void 0)
      }, _attrs), {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "default", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "default")
            ];
          }
        }),
        _: 3
      }), _parent);
    };
  }
});
const _sfc_setup$10 = _sfc_main$10.setup;
_sfc_main$10.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPLink.vue");
  return _sfc_setup$10 ? _sfc_setup$10(props, ctx) : void 0;
};
const _sfc_main$$ = /* @__PURE__ */ defineComponent({
  __name: "VPDocFooterLastUpdated",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2, page, lang } = useData();
    const date = computed(
      () => new Date(page.value.lastUpdated)
    );
    const isoDatetime = computed(() => date.value.toISOString());
    const datetime = ref("");
    onMounted(() => {
      watchEffect(() => {
        var _a, _b, _c;
        datetime.value = new Intl.DateTimeFormat(
          ((_b = (_a = theme2.value.lastUpdated) == null ? void 0 : _a.formatOptions) == null ? void 0 : _b.forceLocale) ? lang.value : void 0,
          ((_c = theme2.value.lastUpdated) == null ? void 0 : _c.formatOptions) ?? {
            dateStyle: "short",
            timeStyle: "short"
          }
        ).format(date.value);
      });
    });
    return (_ctx, _push, _parent, _attrs) => {
      var _a;
      _push(`<p${ssrRenderAttrs(mergeProps({ class: "VPLastUpdated" }, _attrs))} data-v-1c642466>${ssrInterpolate(((_a = unref(theme2).lastUpdated) == null ? void 0 : _a.text) || unref(theme2).lastUpdatedText || "Last updated")}: <time${ssrRenderAttr("datetime", isoDatetime.value)} data-v-1c642466>${ssrInterpolate(datetime.value)}</time></p>`);
    };
  }
});
const _sfc_setup$$ = _sfc_main$$.setup;
_sfc_main$$.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPDocFooterLastUpdated.vue");
  return _sfc_setup$$ ? _sfc_setup$$(props, ctx) : void 0;
};
const VPDocFooterLastUpdated = /* @__PURE__ */ _export_sfc(_sfc_main$$, [["__scopeId", "data-v-1c642466"]]);
const _sfc_main$_ = /* @__PURE__ */ defineComponent({
  __name: "VPDocFooter",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2, page, frontmatter } = useData();
    const editLink = useEditLink();
    const control = usePrevNext();
    const hasEditLink = computed(
      () => theme2.value.editLink && frontmatter.value.editLink !== false
    );
    const hasLastUpdated = computed(() => page.value.lastUpdated);
    const showFooter = computed(
      () => hasEditLink.value || hasLastUpdated.value || control.value.prev || control.value.next
    );
    return (_ctx, _push, _parent, _attrs) => {
      var _a, _b, _c, _d;
      if (showFooter.value) {
        _push(`<footer${ssrRenderAttrs(mergeProps({ class: "VPDocFooter" }, _attrs))} data-v-3fcdfe84>`);
        ssrRenderSlot(_ctx.$slots, "doc-footer-before", {}, null, _push, _parent);
        if (hasEditLink.value || hasLastUpdated.value) {
          _push(`<div class="edit-info" data-v-3fcdfe84>`);
          if (hasEditLink.value) {
            _push(`<div class="edit-link" data-v-3fcdfe84>`);
            _push(ssrRenderComponent(_sfc_main$10, {
              class: "edit-link-button",
              href: unref(editLink).url,
              "no-icon": true
            }, {
              default: withCtx((_, _push2, _parent2, _scopeId) => {
                if (_push2) {
                  _push2(`<span class="vpi-square-pen edit-link-icon" data-v-3fcdfe84${_scopeId}></span> ${ssrInterpolate(unref(editLink).text)}`);
                } else {
                  return [
                    createVNode("span", { class: "vpi-square-pen edit-link-icon" }),
                    createTextVNode(" " + toDisplayString(unref(editLink).text), 1)
                  ];
                }
              }),
              _: 1
            }, _parent));
            _push(`</div>`);
          } else {
            _push(`<!---->`);
          }
          if (hasLastUpdated.value) {
            _push(`<div class="last-updated" data-v-3fcdfe84>`);
            _push(ssrRenderComponent(VPDocFooterLastUpdated, null, null, _parent));
            _push(`</div>`);
          } else {
            _push(`<!---->`);
          }
          _push(`</div>`);
        } else {
          _push(`<!---->`);
        }
        if (((_a = unref(control).prev) == null ? void 0 : _a.link) || ((_b = unref(control).next) == null ? void 0 : _b.link)) {
          _push(`<nav class="prev-next" aria-labelledby="doc-footer-aria-label" data-v-3fcdfe84><span class="visually-hidden" id="doc-footer-aria-label" data-v-3fcdfe84>Pager</span><div class="pager" data-v-3fcdfe84>`);
          if ((_c = unref(control).prev) == null ? void 0 : _c.link) {
            _push(ssrRenderComponent(_sfc_main$10, {
              class: "pager-link prev",
              href: unref(control).prev.link
            }, {
              default: withCtx((_, _push2, _parent2, _scopeId) => {
                var _a2, _b2;
                if (_push2) {
                  _push2(`<span class="desc" data-v-3fcdfe84${_scopeId}>${(((_a2 = unref(theme2).docFooter) == null ? void 0 : _a2.prev) || "Previous page") ?? ""}</span><span class="title" data-v-3fcdfe84${_scopeId}>${unref(control).prev.text ?? ""}</span>`);
                } else {
                  return [
                    createVNode("span", {
                      class: "desc",
                      innerHTML: ((_b2 = unref(theme2).docFooter) == null ? void 0 : _b2.prev) || "Previous page"
                    }, null, 8, ["innerHTML"]),
                    createVNode("span", {
                      class: "title",
                      innerHTML: unref(control).prev.text
                    }, null, 8, ["innerHTML"])
                  ];
                }
              }),
              _: 1
            }, _parent));
          } else {
            _push(`<!---->`);
          }
          _push(`</div><div class="pager" data-v-3fcdfe84>`);
          if ((_d = unref(control).next) == null ? void 0 : _d.link) {
            _push(ssrRenderComponent(_sfc_main$10, {
              class: "pager-link next",
              href: unref(control).next.link
            }, {
              default: withCtx((_, _push2, _parent2, _scopeId) => {
                var _a2, _b2;
                if (_push2) {
                  _push2(`<span class="desc" data-v-3fcdfe84${_scopeId}>${(((_a2 = unref(theme2).docFooter) == null ? void 0 : _a2.next) || "Next page") ?? ""}</span><span class="title" data-v-3fcdfe84${_scopeId}>${unref(control).next.text ?? ""}</span>`);
                } else {
                  return [
                    createVNode("span", {
                      class: "desc",
                      innerHTML: ((_b2 = unref(theme2).docFooter) == null ? void 0 : _b2.next) || "Next page"
                    }, null, 8, ["innerHTML"]),
                    createVNode("span", {
                      class: "title",
                      innerHTML: unref(control).next.text
                    }, null, 8, ["innerHTML"])
                  ];
                }
              }),
              _: 1
            }, _parent));
          } else {
            _push(`<!---->`);
          }
          _push(`</div></nav>`);
        } else {
          _push(`<!---->`);
        }
        _push(`</footer>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$_ = _sfc_main$_.setup;
_sfc_main$_.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPDocFooter.vue");
  return _sfc_setup$_ ? _sfc_setup$_(props, ctx) : void 0;
};
const VPDocFooter = /* @__PURE__ */ _export_sfc(_sfc_main$_, [["__scopeId", "data-v-3fcdfe84"]]);
const _sfc_main$Z = /* @__PURE__ */ defineComponent({
  __name: "VPDoc",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    const route = useRoute();
    const { hasSidebar, hasAside, leftAside } = useSidebar();
    const pageName = computed(
      () => route.path.replace(/[./]+/g, "_").replace(/_html$/, "")
    );
    return (_ctx, _push, _parent, _attrs) => {
      const _component_Content = resolveComponent("Content");
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPDoc", { "has-sidebar": unref(hasSidebar), "has-aside": unref(hasAside) }]
      }, _attrs))} data-v-95ef315c>`);
      ssrRenderSlot(_ctx.$slots, "doc-top", {}, null, _push, _parent);
      _push(`<div class="container" data-v-95ef315c>`);
      if (unref(hasAside)) {
        _push(`<div class="${ssrRenderClass([{ "left-aside": unref(leftAside) }, "aside"])}" data-v-95ef315c><div class="aside-curtain" data-v-95ef315c></div><div class="aside-container" data-v-95ef315c><div class="aside-content" data-v-95ef315c>`);
        _push(ssrRenderComponent(VPDocAside, null, {
          "aside-top": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-top", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-top", {}, void 0, true)
              ];
            }
          }),
          "aside-bottom": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-bottom", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-bottom", {}, void 0, true)
              ];
            }
          }),
          "aside-outline-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-outline-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-outline-before", {}, void 0, true)
              ];
            }
          }),
          "aside-outline-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-outline-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-outline-after", {}, void 0, true)
              ];
            }
          }),
          "aside-ads-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-ads-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-ads-before", {}, void 0, true)
              ];
            }
          }),
          "aside-ads-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-ads-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-ads-after", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
        _push(`</div></div></div>`);
      } else {
        _push(`<!---->`);
      }
      _push(`<div class="content" data-v-95ef315c><div class="content-container" data-v-95ef315c>`);
      ssrRenderSlot(_ctx.$slots, "doc-before", {}, null, _push, _parent);
      _push(`<main class="main" data-v-95ef315c>`);
      _push(ssrRenderComponent(_component_Content, {
        class: ["vp-doc", [
          pageName.value,
          unref(theme2).externalLinkIcon && "external-link-icon-enabled"
        ]]
      }, null, _parent));
      _push(`</main>`);
      _push(ssrRenderComponent(VPDocFooter, null, {
        "doc-footer-before": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "doc-footer-before", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "doc-footer-before", {}, void 0, true)
            ];
          }
        }),
        _: 3
      }, _parent));
      ssrRenderSlot(_ctx.$slots, "doc-after", {}, null, _push, _parent);
      _push(`</div></div></div>`);
      ssrRenderSlot(_ctx.$slots, "doc-bottom", {}, null, _push, _parent);
      _push(`</div>`);
    };
  }
});
const _sfc_setup$Z = _sfc_main$Z.setup;
_sfc_main$Z.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPDoc.vue");
  return _sfc_setup$Z ? _sfc_setup$Z(props, ctx) : void 0;
};
const VPDoc = /* @__PURE__ */ _export_sfc(_sfc_main$Z, [["__scopeId", "data-v-95ef315c"]]);
const _sfc_main$Y = /* @__PURE__ */ defineComponent({
  __name: "VPButton",
  __ssrInlineRender: true,
  props: {
    tag: {},
    size: { default: "medium" },
    theme: { default: "brand" },
    text: {},
    href: {},
    target: {},
    rel: {}
  },
  setup(__props) {
    const props = __props;
    const isExternal2 = computed(
      () => props.href && EXTERNAL_URL_RE.test(props.href)
    );
    const component = computed(() => {
      return props.tag || (props.href ? "a" : "button");
    });
    return (_ctx, _push, _parent, _attrs) => {
      ssrRenderVNode(_push, createVNode(resolveDynamicComponent(component.value), mergeProps({
        class: ["VPButton", [__props.size, __props.theme]],
        href: __props.href ? unref(normalizeLink$1)(__props.href) : void 0,
        target: props.target ?? (isExternal2.value ? "_blank" : void 0),
        rel: props.rel ?? (isExternal2.value ? "noreferrer" : void 0)
      }, _attrs), {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            _push2(`${ssrInterpolate(__props.text)}`);
          } else {
            return [
              createTextVNode(toDisplayString(__props.text), 1)
            ];
          }
        }),
        _: 1
      }), _parent);
    };
  }
});
const _sfc_setup$Y = _sfc_main$Y.setup;
_sfc_main$Y.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPButton.vue");
  return _sfc_setup$Y ? _sfc_setup$Y(props, ctx) : void 0;
};
const VPButton = /* @__PURE__ */ _export_sfc(_sfc_main$Y, [["__scopeId", "data-v-ca054165"]]);
const _sfc_main$X = /* @__PURE__ */ defineComponent({
  ...{ inheritAttrs: false },
  __name: "VPImage",
  __ssrInlineRender: true,
  props: {
    image: {},
    alt: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      const _component_VPImage = resolveComponent("VPImage", true);
      if (__props.image) {
        _push(`<!--[-->`);
        if (typeof __props.image === "string" || "src" in __props.image) {
          _push(`<img${ssrRenderAttrs(mergeProps({ class: "VPImage" }, typeof __props.image === "string" ? _ctx.$attrs : { ...__props.image, ..._ctx.$attrs }, {
            src: unref(withBase)(typeof __props.image === "string" ? __props.image : __props.image.src),
            alt: __props.alt ?? (typeof __props.image === "string" ? "" : __props.image.alt || "")
          }))} data-v-0b5866fd>`);
        } else {
          _push(`<!--[-->`);
          _push(ssrRenderComponent(_component_VPImage, mergeProps({
            class: "dark",
            image: __props.image.dark,
            alt: __props.image.alt
          }, _ctx.$attrs), null, _parent));
          _push(ssrRenderComponent(_component_VPImage, mergeProps({
            class: "light",
            image: __props.image.light,
            alt: __props.image.alt
          }, _ctx.$attrs), null, _parent));
          _push(`<!--]-->`);
        }
        _push(`<!--]-->`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$X = _sfc_main$X.setup;
_sfc_main$X.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPImage.vue");
  return _sfc_setup$X ? _sfc_setup$X(props, ctx) : void 0;
};
const VPImage = /* @__PURE__ */ _export_sfc(_sfc_main$X, [["__scopeId", "data-v-0b5866fd"]]);
const _sfc_main$W = /* @__PURE__ */ defineComponent({
  __name: "VPHero",
  __ssrInlineRender: true,
  props: {
    name: {},
    text: {},
    tagline: {},
    image: {},
    actions: {}
  },
  setup(__props) {
    const heroImageSlotExists = inject("hero-image-slot-exists");
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPHero", { "has-image": __props.image || unref(heroImageSlotExists) }]
      }, _attrs))} data-v-1ae02c0e><div class="container" data-v-1ae02c0e><div class="main" data-v-1ae02c0e>`);
      ssrRenderSlot(_ctx.$slots, "home-hero-info-before", {}, null, _push, _parent);
      ssrRenderSlot(_ctx.$slots, "home-hero-info", {}, () => {
        _push(`<h1 class="heading" data-v-1ae02c0e>`);
        if (__props.name) {
          _push(`<span class="name clip" data-v-1ae02c0e>${__props.name ?? ""}</span>`);
        } else {
          _push(`<!---->`);
        }
        if (__props.text) {
          _push(`<span class="text" data-v-1ae02c0e>${__props.text ?? ""}</span>`);
        } else {
          _push(`<!---->`);
        }
        _push(`</h1>`);
        if (__props.tagline) {
          _push(`<p class="tagline" data-v-1ae02c0e>${__props.tagline ?? ""}</p>`);
        } else {
          _push(`<!---->`);
        }
      }, _push, _parent);
      ssrRenderSlot(_ctx.$slots, "home-hero-info-after", {}, null, _push, _parent);
      if (__props.actions) {
        _push(`<div class="actions" data-v-1ae02c0e><!--[-->`);
        ssrRenderList(__props.actions, (action) => {
          _push(`<div class="action" data-v-1ae02c0e>`);
          _push(ssrRenderComponent(VPButton, {
            tag: "a",
            size: "medium",
            theme: action.theme,
            text: action.text,
            href: action.link,
            target: action.target,
            rel: action.rel
          }, null, _parent));
          _push(`</div>`);
        });
        _push(`<!--]--></div>`);
      } else {
        _push(`<!---->`);
      }
      ssrRenderSlot(_ctx.$slots, "home-hero-actions-after", {}, null, _push, _parent);
      _push(`</div>`);
      if (__props.image || unref(heroImageSlotExists)) {
        _push(`<div class="image" data-v-1ae02c0e><div class="image-container" data-v-1ae02c0e><div class="image-bg" data-v-1ae02c0e></div>`);
        ssrRenderSlot(_ctx.$slots, "home-hero-image", {}, () => {
          if (__props.image) {
            _push(ssrRenderComponent(VPImage, {
              class: "image-src",
              image: __props.image
            }, null, _parent));
          } else {
            _push(`<!---->`);
          }
        }, _push, _parent);
        _push(`</div></div>`);
      } else {
        _push(`<!---->`);
      }
      _push(`</div></div>`);
    };
  }
});
const _sfc_setup$W = _sfc_main$W.setup;
_sfc_main$W.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPHero.vue");
  return _sfc_setup$W ? _sfc_setup$W(props, ctx) : void 0;
};
const VPHero = /* @__PURE__ */ _export_sfc(_sfc_main$W, [["__scopeId", "data-v-1ae02c0e"]]);
const _sfc_main$V = /* @__PURE__ */ defineComponent({
  __name: "VPHomeHero",
  __ssrInlineRender: true,
  setup(__props) {
    const { frontmatter: fm } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(fm).hero) {
        _push(ssrRenderComponent(VPHero, mergeProps({
          class: "VPHomeHero",
          name: unref(fm).hero.name,
          text: unref(fm).hero.text,
          tagline: unref(fm).hero.tagline,
          image: unref(fm).hero.image,
          actions: unref(fm).hero.actions
        }, _attrs), {
          "home-hero-info-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info-before")
              ];
            }
          }),
          "home-hero-info": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info")
              ];
            }
          }),
          "home-hero-info-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info-after")
              ];
            }
          }),
          "home-hero-actions-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-actions-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-actions-after")
              ];
            }
          }),
          "home-hero-image": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-image", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-image")
              ];
            }
          }),
          _: 3
        }, _parent));
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$V = _sfc_main$V.setup;
_sfc_main$V.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPHomeHero.vue");
  return _sfc_setup$V ? _sfc_setup$V(props, ctx) : void 0;
};
const _sfc_main$U = /* @__PURE__ */ defineComponent({
  __name: "VPFeature",
  __ssrInlineRender: true,
  props: {
    icon: {},
    title: {},
    details: {},
    link: {},
    linkText: {},
    rel: {},
    target: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(ssrRenderComponent(_sfc_main$10, mergeProps({
        class: "VPFeature",
        href: __props.link,
        rel: __props.rel,
        target: __props.target,
        "no-icon": true,
        tag: __props.link ? "a" : "div"
      }, _attrs), {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            _push2(`<article class="box" data-v-f62498e9${_scopeId}>`);
            if (typeof __props.icon === "object" && __props.icon.wrap) {
              _push2(`<div class="icon" data-v-f62498e9${_scopeId}>`);
              _push2(ssrRenderComponent(VPImage, {
                image: __props.icon,
                alt: __props.icon.alt,
                height: __props.icon.height || 48,
                width: __props.icon.width || 48
              }, null, _parent2, _scopeId));
              _push2(`</div>`);
            } else if (typeof __props.icon === "object") {
              _push2(ssrRenderComponent(VPImage, {
                image: __props.icon,
                alt: __props.icon.alt,
                height: __props.icon.height || 48,
                width: __props.icon.width || 48
              }, null, _parent2, _scopeId));
            } else if (__props.icon) {
              _push2(`<div class="icon" data-v-f62498e9${_scopeId}>${__props.icon ?? ""}</div>`);
            } else {
              _push2(`<!---->`);
            }
            _push2(`<h2 class="title" data-v-f62498e9${_scopeId}>${__props.title ?? ""}</h2>`);
            if (__props.details) {
              _push2(`<p class="details" data-v-f62498e9${_scopeId}>${__props.details ?? ""}</p>`);
            } else {
              _push2(`<!---->`);
            }
            if (__props.linkText) {
              _push2(`<div class="link-text" data-v-f62498e9${_scopeId}><p class="link-text-value" data-v-f62498e9${_scopeId}>${ssrInterpolate(__props.linkText)} <span class="vpi-arrow-right link-text-icon" data-v-f62498e9${_scopeId}></span></p></div>`);
            } else {
              _push2(`<!---->`);
            }
            _push2(`</article>`);
          } else {
            return [
              createVNode("article", { class: "box" }, [
                typeof __props.icon === "object" && __props.icon.wrap ? (openBlock(), createBlock("div", {
                  key: 0,
                  class: "icon"
                }, [
                  createVNode(VPImage, {
                    image: __props.icon,
                    alt: __props.icon.alt,
                    height: __props.icon.height || 48,
                    width: __props.icon.width || 48
                  }, null, 8, ["image", "alt", "height", "width"])
                ])) : typeof __props.icon === "object" ? (openBlock(), createBlock(VPImage, {
                  key: 1,
                  image: __props.icon,
                  alt: __props.icon.alt,
                  height: __props.icon.height || 48,
                  width: __props.icon.width || 48
                }, null, 8, ["image", "alt", "height", "width"])) : __props.icon ? (openBlock(), createBlock("div", {
                  key: 2,
                  class: "icon",
                  innerHTML: __props.icon
                }, null, 8, ["innerHTML"])) : createCommentVNode("", true),
                createVNode("h2", {
                  class: "title",
                  innerHTML: __props.title
                }, null, 8, ["innerHTML"]),
                __props.details ? (openBlock(), createBlock("p", {
                  key: 3,
                  class: "details",
                  innerHTML: __props.details
                }, null, 8, ["innerHTML"])) : createCommentVNode("", true),
                __props.linkText ? (openBlock(), createBlock("div", {
                  key: 4,
                  class: "link-text"
                }, [
                  createVNode("p", { class: "link-text-value" }, [
                    createTextVNode(toDisplayString(__props.linkText) + " ", 1),
                    createVNode("span", { class: "vpi-arrow-right link-text-icon" })
                  ])
                ])) : createCommentVNode("", true)
              ])
            ];
          }
        }),
        _: 1
      }, _parent));
    };
  }
});
const _sfc_setup$U = _sfc_main$U.setup;
_sfc_main$U.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPFeature.vue");
  return _sfc_setup$U ? _sfc_setup$U(props, ctx) : void 0;
};
const VPFeature = /* @__PURE__ */ _export_sfc(_sfc_main$U, [["__scopeId", "data-v-f62498e9"]]);
const _sfc_main$T = /* @__PURE__ */ defineComponent({
  __name: "VPFeatures",
  __ssrInlineRender: true,
  props: {
    features: {}
  },
  setup(__props) {
    const props = __props;
    const grid = computed(() => {
      const length = props.features.length;
      if (!length) {
        return;
      } else if (length === 2) {
        return "grid-2";
      } else if (length === 3) {
        return "grid-3";
      } else if (length % 3 === 0) {
        return "grid-6";
      } else if (length > 3) {
        return "grid-4";
      }
    });
    return (_ctx, _push, _parent, _attrs) => {
      if (__props.features) {
        _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPFeatures" }, _attrs))} data-v-a34caa2b><div class="container" data-v-a34caa2b><div class="items" data-v-a34caa2b><!--[-->`);
        ssrRenderList(__props.features, (feature) => {
          _push(`<div class="${ssrRenderClass([[grid.value], "item"])}" data-v-a34caa2b>`);
          _push(ssrRenderComponent(VPFeature, {
            icon: feature.icon,
            title: feature.title,
            details: feature.details,
            link: feature.link,
            "link-text": feature.linkText,
            rel: feature.rel,
            target: feature.target
          }, null, _parent));
          _push(`</div>`);
        });
        _push(`<!--]--></div></div></div>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$T = _sfc_main$T.setup;
_sfc_main$T.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPFeatures.vue");
  return _sfc_setup$T ? _sfc_setup$T(props, ctx) : void 0;
};
const VPFeatures = /* @__PURE__ */ _export_sfc(_sfc_main$T, [["__scopeId", "data-v-a34caa2b"]]);
const _sfc_main$S = /* @__PURE__ */ defineComponent({
  __name: "VPHomeFeatures",
  __ssrInlineRender: true,
  setup(__props) {
    const { frontmatter: fm } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(fm).features) {
        _push(ssrRenderComponent(VPFeatures, mergeProps({
          class: "VPHomeFeatures",
          features: unref(fm).features
        }, _attrs), null, _parent));
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$S = _sfc_main$S.setup;
_sfc_main$S.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPHomeFeatures.vue");
  return _sfc_setup$S ? _sfc_setup$S(props, ctx) : void 0;
};
const _sfc_main$R = /* @__PURE__ */ defineComponent({
  __name: "VPHomeContent",
  __ssrInlineRender: true,
  setup(__props) {
    const { width: vw } = useWindowSize({
      initialWidth: 0,
      includeScrollbar: false
    });
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: "vp-doc container",
        style: unref(vw) ? { "--vp-offset": `calc(50% - ${unref(vw) / 2}px)` } : {}
      }, _attrs))} data-v-6f245bea>`);
      ssrRenderSlot(_ctx.$slots, "default", {}, null, _push, _parent);
      _push(`</div>`);
    };
  }
});
const _sfc_setup$R = _sfc_main$R.setup;
_sfc_main$R.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPHomeContent.vue");
  return _sfc_setup$R ? _sfc_setup$R(props, ctx) : void 0;
};
const VPHomeContent = /* @__PURE__ */ _export_sfc(_sfc_main$R, [["__scopeId", "data-v-6f245bea"]]);
const _sfc_main$Q = /* @__PURE__ */ defineComponent({
  __name: "VPHome",
  __ssrInlineRender: true,
  setup(__props) {
    const { frontmatter, theme: theme2 } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      const _component_Content = resolveComponent("Content");
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPHome", {
          "external-link-icon-enabled": unref(theme2).externalLinkIcon
        }]
      }, _attrs))} data-v-a9bcf33c>`);
      ssrRenderSlot(_ctx.$slots, "home-hero-before", {}, null, _push, _parent);
      _push(ssrRenderComponent(_sfc_main$V, null, {
        "home-hero-info-before": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "home-hero-info-before", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "home-hero-info-before", {}, void 0, true)
            ];
          }
        }),
        "home-hero-info": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "home-hero-info", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "home-hero-info", {}, void 0, true)
            ];
          }
        }),
        "home-hero-info-after": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "home-hero-info-after", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "home-hero-info-after", {}, void 0, true)
            ];
          }
        }),
        "home-hero-actions-after": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "home-hero-actions-after", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "home-hero-actions-after", {}, void 0, true)
            ];
          }
        }),
        "home-hero-image": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "home-hero-image", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "home-hero-image", {}, void 0, true)
            ];
          }
        }),
        _: 3
      }, _parent));
      ssrRenderSlot(_ctx.$slots, "home-hero-after", {}, null, _push, _parent);
      ssrRenderSlot(_ctx.$slots, "home-features-before", {}, null, _push, _parent);
      _push(ssrRenderComponent(_sfc_main$S, null, null, _parent));
      ssrRenderSlot(_ctx.$slots, "home-features-after", {}, null, _push, _parent);
      if (unref(frontmatter).markdownStyles !== false) {
        _push(ssrRenderComponent(VPHomeContent, null, {
          default: withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              _push2(ssrRenderComponent(_component_Content, null, null, _parent2, _scopeId));
            } else {
              return [
                createVNode(_component_Content)
              ];
            }
          }),
          _: 1
        }, _parent));
      } else {
        _push(ssrRenderComponent(_component_Content, null, null, _parent));
      }
      _push(`</div>`);
    };
  }
});
const _sfc_setup$Q = _sfc_main$Q.setup;
_sfc_main$Q.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPHome.vue");
  return _sfc_setup$Q ? _sfc_setup$Q(props, ctx) : void 0;
};
const VPHome = /* @__PURE__ */ _export_sfc(_sfc_main$Q, [["__scopeId", "data-v-a9bcf33c"]]);
const _sfc_main$P = {};
function _sfc_ssrRender$1(_ctx, _push, _parent, _attrs) {
  const _component_Content = resolveComponent("Content");
  _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPPage" }, _attrs))}>`);
  ssrRenderSlot(_ctx.$slots, "page-top", {}, null, _push, _parent);
  _push(ssrRenderComponent(_component_Content, null, null, _parent));
  ssrRenderSlot(_ctx.$slots, "page-bottom", {}, null, _push, _parent);
  _push(`</div>`);
}
const _sfc_setup$P = _sfc_main$P.setup;
_sfc_main$P.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPPage.vue");
  return _sfc_setup$P ? _sfc_setup$P(props, ctx) : void 0;
};
const VPPage = /* @__PURE__ */ _export_sfc(_sfc_main$P, [["ssrRender", _sfc_ssrRender$1]]);
const _sfc_main$O = /* @__PURE__ */ defineComponent({
  __name: "VPContent",
  __ssrInlineRender: true,
  setup(__props) {
    const { page, frontmatter } = useData();
    const { hasSidebar } = useSidebar();
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPContent", {
          "has-sidebar": unref(hasSidebar),
          "is-home": unref(frontmatter).layout === "home"
        }],
        id: "VPContent"
      }, _attrs))} data-v-92a7e9e1>`);
      if (unref(page).isNotFound) {
        ssrRenderSlot(_ctx.$slots, "not-found", {}, () => {
          _push(ssrRenderComponent(NotFound, null, null, _parent));
        }, _push, _parent);
      } else if (unref(frontmatter).layout === "page") {
        _push(ssrRenderComponent(VPPage, null, {
          "page-top": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "page-top", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "page-top", {}, void 0, true)
              ];
            }
          }),
          "page-bottom": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "page-bottom", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "page-bottom", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
      } else if (unref(frontmatter).layout === "home") {
        _push(ssrRenderComponent(VPHome, null, {
          "home-hero-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-before", {}, void 0, true)
              ];
            }
          }),
          "home-hero-info-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info-before", {}, void 0, true)
              ];
            }
          }),
          "home-hero-info": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info", {}, void 0, true)
              ];
            }
          }),
          "home-hero-info-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info-after", {}, void 0, true)
              ];
            }
          }),
          "home-hero-actions-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-actions-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-actions-after", {}, void 0, true)
              ];
            }
          }),
          "home-hero-image": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-image", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-image", {}, void 0, true)
              ];
            }
          }),
          "home-hero-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-after", {}, void 0, true)
              ];
            }
          }),
          "home-features-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-features-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-features-before", {}, void 0, true)
              ];
            }
          }),
          "home-features-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-features-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-features-after", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
      } else if (unref(frontmatter).layout && unref(frontmatter).layout !== "doc") {
        ssrRenderVNode(_push, createVNode(resolveDynamicComponent(unref(frontmatter).layout), null, null), _parent);
      } else {
        _push(ssrRenderComponent(VPDoc, null, {
          "doc-top": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-top", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-top", {}, void 0, true)
              ];
            }
          }),
          "doc-bottom": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-bottom", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-bottom", {}, void 0, true)
              ];
            }
          }),
          "doc-footer-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-footer-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-footer-before", {}, void 0, true)
              ];
            }
          }),
          "doc-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-before", {}, void 0, true)
              ];
            }
          }),
          "doc-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-after", {}, void 0, true)
              ];
            }
          }),
          "aside-top": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-top", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-top", {}, void 0, true)
              ];
            }
          }),
          "aside-outline-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-outline-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-outline-before", {}, void 0, true)
              ];
            }
          }),
          "aside-outline-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-outline-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-outline-after", {}, void 0, true)
              ];
            }
          }),
          "aside-ads-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-ads-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-ads-before", {}, void 0, true)
              ];
            }
          }),
          "aside-ads-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-ads-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-ads-after", {}, void 0, true)
              ];
            }
          }),
          "aside-bottom": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-bottom", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-bottom", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
      }
      _push(`</div>`);
    };
  }
});
const _sfc_setup$O = _sfc_main$O.setup;
_sfc_main$O.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPContent.vue");
  return _sfc_setup$O ? _sfc_setup$O(props, ctx) : void 0;
};
const VPContent = /* @__PURE__ */ _export_sfc(_sfc_main$O, [["__scopeId", "data-v-92a7e9e1"]]);
const _sfc_main$N = /* @__PURE__ */ defineComponent({
  __name: "VPFooter",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2, frontmatter } = useData();
    const { hasSidebar } = useSidebar();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(theme2).footer && unref(frontmatter).footer !== false) {
        _push(`<footer${ssrRenderAttrs(mergeProps({
          class: ["VPFooter", { "has-sidebar": unref(hasSidebar) }]
        }, _attrs))} data-v-38c0173a><div class="container" data-v-38c0173a>`);
        if (unref(theme2).footer.message) {
          _push(`<p class="message" data-v-38c0173a>${unref(theme2).footer.message ?? ""}</p>`);
        } else {
          _push(`<!---->`);
        }
        if (unref(theme2).footer.copyright) {
          _push(`<p class="copyright" data-v-38c0173a>${unref(theme2).footer.copyright ?? ""}</p>`);
        } else {
          _push(`<!---->`);
        }
        _push(`</div></footer>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$N = _sfc_main$N.setup;
_sfc_main$N.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPFooter.vue");
  return _sfc_setup$N ? _sfc_setup$N(props, ctx) : void 0;
};
const VPFooter = /* @__PURE__ */ _export_sfc(_sfc_main$N, [["__scopeId", "data-v-38c0173a"]]);
function useLocalNav() {
  const { theme: theme2, frontmatter } = useData();
  const headers = shallowRef([]);
  const hasLocalNav = computed(() => {
    return headers.value.length > 0;
  });
  onContentUpdated(() => {
    headers.value = getHeaders(frontmatter.value.outline ?? theme2.value.outline);
  });
  return {
    headers,
    hasLocalNav
  };
}
const _sfc_main$M = /* @__PURE__ */ defineComponent({
  __name: "VPLocalNavOutlineDropdown",
  __ssrInlineRender: true,
  props: {
    headers: {},
    navHeight: {}
  },
  setup(__props) {
    const { theme: theme2 } = useData();
    const open = ref(false);
    const vh = ref(0);
    const main = ref();
    ref();
    function closeOnClickOutside(e) {
      var _a;
      if (!((_a = main.value) == null ? void 0 : _a.contains(e.target))) {
        open.value = false;
      }
    }
    watch(open, (value) => {
      if (value) {
        document.addEventListener("click", closeOnClickOutside);
        return;
      }
      document.removeEventListener("click", closeOnClickOutside);
    });
    onKeyStroke("Escape", () => {
      open.value = false;
    });
    onContentUpdated(() => {
      open.value = false;
    });
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: "VPLocalNavOutlineDropdown",
        style: { "--vp-vh": vh.value + "px" },
        ref_key: "main",
        ref: main
      }, _attrs))} data-v-d9145a75>`);
      if (__props.headers.length > 0) {
        _push(`<button class="${ssrRenderClass({ open: open.value })}" data-v-d9145a75><span class="menu-text" data-v-d9145a75>${ssrInterpolate(unref(resolveTitle)(unref(theme2)))}</span><span class="vpi-chevron-right icon" data-v-d9145a75></span></button>`);
      } else {
        _push(`<button data-v-d9145a75>${ssrInterpolate(unref(theme2).returnToTopLabel || "Return to top")}</button>`);
      }
      if (open.value) {
        _push(`<div class="items" data-v-d9145a75><div class="header" data-v-d9145a75><a class="top-link" href="#" data-v-d9145a75>${ssrInterpolate(unref(theme2).returnToTopLabel || "Return to top")}</a></div><div class="outline" data-v-d9145a75>`);
        _push(ssrRenderComponent(VPDocOutlineItem, { headers: __props.headers }, null, _parent));
        _push(`</div></div>`);
      } else {
        _push(`<!---->`);
      }
      _push(`</div>`);
    };
  }
});
const _sfc_setup$M = _sfc_main$M.setup;
_sfc_main$M.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPLocalNavOutlineDropdown.vue");
  return _sfc_setup$M ? _sfc_setup$M(props, ctx) : void 0;
};
const VPLocalNavOutlineDropdown = /* @__PURE__ */ _export_sfc(_sfc_main$M, [["__scopeId", "data-v-d9145a75"]]);
const _sfc_main$L = /* @__PURE__ */ defineComponent({
  __name: "VPLocalNav",
  __ssrInlineRender: true,
  props: {
    open: { type: Boolean }
  },
  emits: ["open-menu"],
  setup(__props) {
    const { theme: theme2, frontmatter } = useData();
    const { hasSidebar } = useSidebar();
    const { headers } = useLocalNav();
    const { y } = useWindowScroll();
    const navHeight = ref(0);
    onMounted(() => {
      navHeight.value = parseInt(
        getComputedStyle(document.documentElement).getPropertyValue(
          "--vp-nav-height"
        )
      );
    });
    onContentUpdated(() => {
      headers.value = getHeaders(frontmatter.value.outline ?? theme2.value.outline);
    });
    const empty = computed(() => {
      return headers.value.length === 0;
    });
    const emptyAndNoSidebar = computed(() => {
      return empty.value && !hasSidebar.value;
    });
    const classes = computed(() => {
      return {
        VPLocalNav: true,
        "has-sidebar": hasSidebar.value,
        empty: empty.value,
        fixed: emptyAndNoSidebar.value
      };
    });
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(frontmatter).layout !== "home" && (!emptyAndNoSidebar.value || unref(y) >= navHeight.value)) {
        _push(`<div${ssrRenderAttrs(mergeProps({ class: classes.value }, _attrs))} data-v-bf5a9b89><div class="container" data-v-bf5a9b89>`);
        if (unref(hasSidebar)) {
          _push(`<button class="menu"${ssrRenderAttr("aria-expanded", __props.open)} aria-controls="VPSidebarNav" data-v-bf5a9b89><span class="vpi-align-left menu-icon" data-v-bf5a9b89></span><span class="menu-text" data-v-bf5a9b89>${ssrInterpolate(unref(theme2).sidebarMenuLabel || "Menu")}</span></button>`);
        } else {
          _push(`<!---->`);
        }
        _push(ssrRenderComponent(VPLocalNavOutlineDropdown, {
          headers: unref(headers),
          navHeight: navHeight.value
        }, null, _parent));
        _push(`</div></div>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$L = _sfc_main$L.setup;
_sfc_main$L.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPLocalNav.vue");
  return _sfc_setup$L ? _sfc_setup$L(props, ctx) : void 0;
};
const VPLocalNav = /* @__PURE__ */ _export_sfc(_sfc_main$L, [["__scopeId", "data-v-bf5a9b89"]]);
function useNav() {
  const isScreenOpen = ref(false);
  function openScreen() {
    isScreenOpen.value = true;
    window.addEventListener("resize", closeScreenOnTabletWindow);
  }
  function closeScreen() {
    isScreenOpen.value = false;
    window.removeEventListener("resize", closeScreenOnTabletWindow);
  }
  function toggleScreen() {
    isScreenOpen.value ? closeScreen() : openScreen();
  }
  function closeScreenOnTabletWindow() {
    window.outerWidth >= 768 && closeScreen();
  }
  const route = useRoute();
  watch(() => route.path, closeScreen);
  return {
    isScreenOpen,
    openScreen,
    closeScreen,
    toggleScreen
  };
}
const _sfc_main$K = {};
function _sfc_ssrRender(_ctx, _push, _parent, _attrs) {
  _push(`<button${ssrRenderAttrs(mergeProps({
    class: "VPSwitch",
    type: "button",
    role: "switch"
  }, _attrs))} data-v-1c08200f><span class="check" data-v-1c08200f>`);
  if (_ctx.$slots.default) {
    _push(`<span class="icon" data-v-1c08200f>`);
    ssrRenderSlot(_ctx.$slots, "default", {}, null, _push, _parent);
    _push(`</span>`);
  } else {
    _push(`<!---->`);
  }
  _push(`</span></button>`);
}
const _sfc_setup$K = _sfc_main$K.setup;
_sfc_main$K.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSwitch.vue");
  return _sfc_setup$K ? _sfc_setup$K(props, ctx) : void 0;
};
const VPSwitch = /* @__PURE__ */ _export_sfc(_sfc_main$K, [["ssrRender", _sfc_ssrRender], ["__scopeId", "data-v-1c08200f"]]);
const _sfc_main$J = /* @__PURE__ */ defineComponent({
  __name: "VPSwitchAppearance",
  __ssrInlineRender: true,
  setup(__props) {
    const { isDark, theme: theme2 } = useData();
    const toggleAppearance = inject("toggle-appearance", () => {
      isDark.value = !isDark.value;
    });
    const switchTitle = ref("");
    watchPostEffect(() => {
      switchTitle.value = isDark.value ? theme2.value.lightModeSwitchTitle || "Switch to light theme" : theme2.value.darkModeSwitchTitle || "Switch to dark theme";
    });
    return (_ctx, _push, _parent, _attrs) => {
      _push(ssrRenderComponent(VPSwitch, mergeProps({
        title: switchTitle.value,
        class: "VPSwitchAppearance",
        "aria-checked": unref(isDark),
        onClick: unref(toggleAppearance)
      }, _attrs), {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            _push2(`<span class="vpi-sun sun" data-v-8056d3fb${_scopeId}></span><span class="vpi-moon moon" data-v-8056d3fb${_scopeId}></span>`);
          } else {
            return [
              createVNode("span", { class: "vpi-sun sun" }),
              createVNode("span", { class: "vpi-moon moon" })
            ];
          }
        }),
        _: 1
      }, _parent));
    };
  }
});
const _sfc_setup$J = _sfc_main$J.setup;
_sfc_main$J.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSwitchAppearance.vue");
  return _sfc_setup$J ? _sfc_setup$J(props, ctx) : void 0;
};
const VPSwitchAppearance = /* @__PURE__ */ _export_sfc(_sfc_main$J, [["__scopeId", "data-v-8056d3fb"]]);
const _sfc_main$I = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarAppearance",
  __ssrInlineRender: true,
  setup(__props) {
    const { site } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(site).appearance && unref(site).appearance !== "force-dark" && unref(site).appearance !== "force-auto") {
        _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPNavBarAppearance" }, _attrs))} data-v-892e05f5>`);
        _push(ssrRenderComponent(VPSwitchAppearance, null, null, _parent));
        _push(`</div>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$I = _sfc_main$I.setup;
_sfc_main$I.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarAppearance.vue");
  return _sfc_setup$I ? _sfc_setup$I(props, ctx) : void 0;
};
const VPNavBarAppearance = /* @__PURE__ */ _export_sfc(_sfc_main$I, [["__scopeId", "data-v-892e05f5"]]);
const focusedElement = ref();
let active = false;
let listeners = 0;
function useFlyout(options) {
  const focus = ref(false);
  if (inBrowser) {
    !active && activateFocusTracking();
    listeners++;
    const unwatch = watch(focusedElement, (el) => {
      var _a, _b, _c;
      if (el === options.el.value || ((_a = options.el.value) == null ? void 0 : _a.contains(el))) {
        focus.value = true;
        (_b = options.onFocus) == null ? void 0 : _b.call(options);
      } else {
        focus.value = false;
        (_c = options.onBlur) == null ? void 0 : _c.call(options);
      }
    });
    onUnmounted(() => {
      unwatch();
      listeners--;
      if (!listeners) {
        deactivateFocusTracking();
      }
    });
  }
  return readonly(focus);
}
function activateFocusTracking() {
  document.addEventListener("focusin", handleFocusIn);
  active = true;
  focusedElement.value = document.activeElement;
}
function deactivateFocusTracking() {
  document.removeEventListener("focusin", handleFocusIn);
}
function handleFocusIn() {
  focusedElement.value = document.activeElement;
}
const _sfc_main$H = /* @__PURE__ */ defineComponent({
  __name: "VPMenuLink",
  __ssrInlineRender: true,
  props: {
    item: {}
  },
  setup(__props) {
    const { page } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPMenuLink" }, _attrs))} data-v-5866e4ac>`);
      _push(ssrRenderComponent(_sfc_main$10, {
        class: {
          active: unref(isActive)(
            unref(page).relativePath,
            __props.item.activeMatch || __props.item.link,
            !!__props.item.activeMatch
          )
        },
        href: __props.item.link,
        target: __props.item.target,
        rel: __props.item.rel,
        "no-icon": __props.item.noIcon
      }, {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            _push2(`<span data-v-5866e4ac${_scopeId}>${__props.item.text ?? ""}</span>`);
          } else {
            return [
              createVNode("span", {
                innerHTML: __props.item.text
              }, null, 8, ["innerHTML"])
            ];
          }
        }),
        _: 1
      }, _parent));
      _push(`</div>`);
    };
  }
});
const _sfc_setup$H = _sfc_main$H.setup;
_sfc_main$H.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPMenuLink.vue");
  return _sfc_setup$H ? _sfc_setup$H(props, ctx) : void 0;
};
const VPMenuLink = /* @__PURE__ */ _export_sfc(_sfc_main$H, [["__scopeId", "data-v-5866e4ac"]]);
const _sfc_main$G = /* @__PURE__ */ defineComponent({
  __name: "VPMenuGroup",
  __ssrInlineRender: true,
  props: {
    text: {},
    items: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPMenuGroup" }, _attrs))} data-v-99b988cf>`);
      if (__props.text) {
        _push(`<p class="title" data-v-99b988cf>${ssrInterpolate(__props.text)}</p>`);
      } else {
        _push(`<!---->`);
      }
      _push(`<!--[-->`);
      ssrRenderList(__props.items, (item) => {
        _push(`<!--[-->`);
        if ("link" in item) {
          _push(ssrRenderComponent(VPMenuLink, { item }, null, _parent));
        } else {
          _push(`<!---->`);
        }
        _push(`<!--]-->`);
      });
      _push(`<!--]--></div>`);
    };
  }
});
const _sfc_setup$G = _sfc_main$G.setup;
_sfc_main$G.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPMenuGroup.vue");
  return _sfc_setup$G ? _sfc_setup$G(props, ctx) : void 0;
};
const VPMenuGroup = /* @__PURE__ */ _export_sfc(_sfc_main$G, [["__scopeId", "data-v-99b988cf"]]);
const _sfc_main$F = /* @__PURE__ */ defineComponent({
  __name: "VPMenu",
  __ssrInlineRender: true,
  props: {
    items: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPMenu" }, _attrs))} data-v-2d538e96>`);
      if (__props.items) {
        _push(`<div class="items" data-v-2d538e96><!--[-->`);
        ssrRenderList(__props.items, (item) => {
          _push(`<!--[-->`);
          if ("link" in item) {
            _push(ssrRenderComponent(VPMenuLink, { item }, null, _parent));
          } else if ("component" in item) {
            ssrRenderVNode(_push, createVNode(resolveDynamicComponent(item.component), mergeProps({ ref_for: true }, item.props), null), _parent);
          } else {
            _push(ssrRenderComponent(VPMenuGroup, {
              text: item.text,
              items: item.items
            }, null, _parent));
          }
          _push(`<!--]-->`);
        });
        _push(`<!--]--></div>`);
      } else {
        _push(`<!---->`);
      }
      ssrRenderSlot(_ctx.$slots, "default", {}, null, _push, _parent);
      _push(`</div>`);
    };
  }
});
const _sfc_setup$F = _sfc_main$F.setup;
_sfc_main$F.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPMenu.vue");
  return _sfc_setup$F ? _sfc_setup$F(props, ctx) : void 0;
};
const VPMenu = /* @__PURE__ */ _export_sfc(_sfc_main$F, [["__scopeId", "data-v-2d538e96"]]);
const _sfc_main$E = /* @__PURE__ */ defineComponent({
  __name: "VPFlyout",
  __ssrInlineRender: true,
  props: {
    icon: {},
    button: {},
    label: {},
    items: {}
  },
  setup(__props) {
    const open = ref(false);
    const el = ref();
    useFlyout({ el, onBlur });
    function onBlur() {
      open.value = false;
    }
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: "VPFlyout",
        ref_key: "el",
        ref: el
      }, _attrs))} data-v-f7ec1ead><button type="button" class="button" aria-haspopup="true"${ssrRenderAttr("aria-expanded", open.value)}${ssrRenderAttr("aria-label", __props.label)} data-v-f7ec1ead>`);
      if (__props.button || __props.icon) {
        _push(`<span class="text" data-v-f7ec1ead>`);
        if (__props.icon) {
          _push(`<span class="${ssrRenderClass([__props.icon, "option-icon"])}" data-v-f7ec1ead></span>`);
        } else {
          _push(`<!---->`);
        }
        if (__props.button) {
          _push(`<span data-v-f7ec1ead>${__props.button ?? ""}</span>`);
        } else {
          _push(`<!---->`);
        }
        _push(`<span class="vpi-chevron-down text-icon" data-v-f7ec1ead></span></span>`);
      } else {
        _push(`<span class="vpi-more-horizontal icon" data-v-f7ec1ead></span>`);
      }
      _push(`</button><div class="menu" data-v-f7ec1ead>`);
      _push(ssrRenderComponent(VPMenu, { items: __props.items }, {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "default", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "default", {}, void 0, true)
            ];
          }
        }),
        _: 3
      }, _parent));
      _push(`</div></div>`);
    };
  }
});
const _sfc_setup$E = _sfc_main$E.setup;
_sfc_main$E.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPFlyout.vue");
  return _sfc_setup$E ? _sfc_setup$E(props, ctx) : void 0;
};
const VPFlyout = /* @__PURE__ */ _export_sfc(_sfc_main$E, [["__scopeId", "data-v-f7ec1ead"]]);
const _sfc_main$D = /* @__PURE__ */ defineComponent({
  __name: "VPSocialLink",
  __ssrInlineRender: true,
  props: {
    icon: {},
    link: {},
    ariaLabel: {}
  },
  setup(__props) {
    var _a;
    const props = __props;
    const el = ref();
    onMounted(async () => {
      var _a2;
      await nextTick();
      const span = (_a2 = el.value) == null ? void 0 : _a2.children[0];
      if (span instanceof HTMLElement && span.className.startsWith("vpi-social-") && (getComputedStyle(span).maskImage || getComputedStyle(span).webkitMaskImage) === "none") {
        span.style.setProperty(
          "--icon",
          `url('https://api.iconify.design/simple-icons/${props.icon}.svg')`
        );
      }
    });
    const svg = computed(() => {
      if (typeof props.icon === "object") return props.icon.svg;
      return `<span class="vpi-social-${props.icon}"></span>`;
    });
    {
      typeof props.icon === "string" && ((_a = useSSRContext()) == null ? void 0 : _a.vpSocialIcons.add(props.icon));
    }
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<a${ssrRenderAttrs(mergeProps({
        ref_key: "el",
        ref: el,
        class: "VPSocialLink no-icon",
        href: __props.link,
        "aria-label": __props.ariaLabel ?? (typeof __props.icon === "string" ? __props.icon : ""),
        target: "_blank",
        rel: "noopener"
      }, _attrs))} data-v-7b0d4a53>${svg.value ?? ""}</a>`);
    };
  }
});
const _sfc_setup$D = _sfc_main$D.setup;
_sfc_main$D.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSocialLink.vue");
  return _sfc_setup$D ? _sfc_setup$D(props, ctx) : void 0;
};
const VPSocialLink = /* @__PURE__ */ _export_sfc(_sfc_main$D, [["__scopeId", "data-v-7b0d4a53"]]);
const _sfc_main$C = /* @__PURE__ */ defineComponent({
  __name: "VPSocialLinks",
  __ssrInlineRender: true,
  props: {
    links: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPSocialLinks" }, _attrs))} data-v-27f822a8><!--[-->`);
      ssrRenderList(__props.links, ({ link: link2, icon, ariaLabel }) => {
        _push(ssrRenderComponent(VPSocialLink, {
          key: link2,
          icon,
          link: link2,
          ariaLabel
        }, null, _parent));
      });
      _push(`<!--]--></div>`);
    };
  }
});
const _sfc_setup$C = _sfc_main$C.setup;
_sfc_main$C.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSocialLinks.vue");
  return _sfc_setup$C ? _sfc_setup$C(props, ctx) : void 0;
};
const VPSocialLinks = /* @__PURE__ */ _export_sfc(_sfc_main$C, [["__scopeId", "data-v-27f822a8"]]);
const _sfc_main$B = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarExtra",
  __ssrInlineRender: true,
  setup(__props) {
    const { site, theme: theme2 } = useData();
    const { localeLinks, currentLang } = useLangs({ correspondingLink: true });
    const hasExtraContent = computed(
      () => localeLinks.value.length && currentLang.value.label || site.value.appearance || theme2.value.socialLinks
    );
    return (_ctx, _push, _parent, _attrs) => {
      if (hasExtraContent.value) {
        _push(ssrRenderComponent(VPFlyout, mergeProps({
          class: "VPNavBarExtra",
          label: "extra navigation"
        }, _attrs), {
          default: withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              if (unref(localeLinks).length && unref(currentLang).label) {
                _push2(`<div class="group translations" data-v-a22404e7${_scopeId}><p class="trans-title" data-v-a22404e7${_scopeId}>${ssrInterpolate(unref(currentLang).label)}</p><!--[-->`);
                ssrRenderList(unref(localeLinks), (locale) => {
                  _push2(ssrRenderComponent(VPMenuLink, { item: locale }, null, _parent2, _scopeId));
                });
                _push2(`<!--]--></div>`);
              } else {
                _push2(`<!---->`);
              }
              if (unref(site).appearance && unref(site).appearance !== "force-dark" && unref(site).appearance !== "force-auto") {
                _push2(`<div class="group" data-v-a22404e7${_scopeId}><div class="item appearance" data-v-a22404e7${_scopeId}><p class="label" data-v-a22404e7${_scopeId}>${ssrInterpolate(unref(theme2).darkModeSwitchLabel || "Appearance")}</p><div class="appearance-action" data-v-a22404e7${_scopeId}>`);
                _push2(ssrRenderComponent(VPSwitchAppearance, null, null, _parent2, _scopeId));
                _push2(`</div></div></div>`);
              } else {
                _push2(`<!---->`);
              }
              if (unref(theme2).socialLinks) {
                _push2(`<div class="group" data-v-a22404e7${_scopeId}><div class="item social-links" data-v-a22404e7${_scopeId}>`);
                _push2(ssrRenderComponent(VPSocialLinks, {
                  class: "social-links-list",
                  links: unref(theme2).socialLinks
                }, null, _parent2, _scopeId));
                _push2(`</div></div>`);
              } else {
                _push2(`<!---->`);
              }
            } else {
              return [
                unref(localeLinks).length && unref(currentLang).label ? (openBlock(), createBlock("div", {
                  key: 0,
                  class: "group translations"
                }, [
                  createVNode("p", { class: "trans-title" }, toDisplayString(unref(currentLang).label), 1),
                  (openBlock(true), createBlock(Fragment, null, renderList(unref(localeLinks), (locale) => {
                    return openBlock(), createBlock(VPMenuLink, {
                      key: locale.link,
                      item: locale
                    }, null, 8, ["item"]);
                  }), 128))
                ])) : createCommentVNode("", true),
                unref(site).appearance && unref(site).appearance !== "force-dark" && unref(site).appearance !== "force-auto" ? (openBlock(), createBlock("div", {
                  key: 1,
                  class: "group"
                }, [
                  createVNode("div", { class: "item appearance" }, [
                    createVNode("p", { class: "label" }, toDisplayString(unref(theme2).darkModeSwitchLabel || "Appearance"), 1),
                    createVNode("div", { class: "appearance-action" }, [
                      createVNode(VPSwitchAppearance)
                    ])
                  ])
                ])) : createCommentVNode("", true),
                unref(theme2).socialLinks ? (openBlock(), createBlock("div", {
                  key: 2,
                  class: "group"
                }, [
                  createVNode("div", { class: "item social-links" }, [
                    createVNode(VPSocialLinks, {
                      class: "social-links-list",
                      links: unref(theme2).socialLinks
                    }, null, 8, ["links"])
                  ])
                ])) : createCommentVNode("", true)
              ];
            }
          }),
          _: 1
        }, _parent));
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$B = _sfc_main$B.setup;
_sfc_main$B.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarExtra.vue");
  return _sfc_setup$B ? _sfc_setup$B(props, ctx) : void 0;
};
const VPNavBarExtra = /* @__PURE__ */ _export_sfc(_sfc_main$B, [["__scopeId", "data-v-a22404e7"]]);
const _sfc_main$A = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarHamburger",
  __ssrInlineRender: true,
  props: {
    active: { type: Boolean }
  },
  emits: ["click"],
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<button${ssrRenderAttrs(mergeProps({
        type: "button",
        class: ["VPNavBarHamburger", { active: __props.active }],
        "aria-label": "mobile navigation",
        "aria-expanded": __props.active,
        "aria-controls": "VPNavScreen"
      }, _attrs))} data-v-ea8726fd><span class="container" data-v-ea8726fd><span class="top" data-v-ea8726fd></span><span class="middle" data-v-ea8726fd></span><span class="bottom" data-v-ea8726fd></span></span></button>`);
    };
  }
});
const _sfc_setup$A = _sfc_main$A.setup;
_sfc_main$A.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarHamburger.vue");
  return _sfc_setup$A ? _sfc_setup$A(props, ctx) : void 0;
};
const VPNavBarHamburger = /* @__PURE__ */ _export_sfc(_sfc_main$A, [["__scopeId", "data-v-ea8726fd"]]);
const _sfc_main$z = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarMenuLink",
  __ssrInlineRender: true,
  props: {
    item: {}
  },
  setup(__props) {
    const { page } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      _push(ssrRenderComponent(_sfc_main$10, mergeProps({
        class: {
          VPNavBarMenuLink: true,
          active: unref(isActive)(
            unref(page).relativePath,
            __props.item.activeMatch || __props.item.link,
            !!__props.item.activeMatch
          )
        },
        href: __props.item.link,
        target: __props.item.target,
        rel: __props.item.rel,
        "no-icon": __props.item.noIcon,
        tabindex: "0"
      }, _attrs), {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            _push2(`<span data-v-1dbdc38d${_scopeId}>${__props.item.text ?? ""}</span>`);
          } else {
            return [
              createVNode("span", {
                innerHTML: __props.item.text
              }, null, 8, ["innerHTML"])
            ];
          }
        }),
        _: 1
      }, _parent));
    };
  }
});
const _sfc_setup$z = _sfc_main$z.setup;
_sfc_main$z.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarMenuLink.vue");
  return _sfc_setup$z ? _sfc_setup$z(props, ctx) : void 0;
};
const VPNavBarMenuLink = /* @__PURE__ */ _export_sfc(_sfc_main$z, [["__scopeId", "data-v-1dbdc38d"]]);
const _sfc_main$y = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarMenuGroup",
  __ssrInlineRender: true,
  props: {
    item: {}
  },
  setup(__props) {
    const props = __props;
    const { page } = useData();
    const isChildActive = (navItem) => {
      if ("component" in navItem) return false;
      if ("link" in navItem) {
        return isActive(
          page.value.relativePath,
          navItem.link,
          !!props.item.activeMatch
        );
      }
      return navItem.items.some(isChildActive);
    };
    const childrenActive = computed(() => isChildActive(props.item));
    return (_ctx, _push, _parent, _attrs) => {
      _push(ssrRenderComponent(VPFlyout, mergeProps({
        class: {
          VPNavBarMenuGroup: true,
          active: unref(isActive)(unref(page).relativePath, __props.item.activeMatch, !!__props.item.activeMatch) || childrenActive.value
        },
        button: __props.item.text,
        items: __props.item.items
      }, _attrs), null, _parent));
    };
  }
});
const _sfc_setup$y = _sfc_main$y.setup;
_sfc_main$y.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarMenuGroup.vue");
  return _sfc_setup$y ? _sfc_setup$y(props, ctx) : void 0;
};
const _sfc_main$x = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarMenu",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(theme2).nav) {
        _push(`<nav${ssrRenderAttrs(mergeProps({
          "aria-labelledby": "main-nav-aria-label",
          class: "VPNavBarMenu"
        }, _attrs))} data-v-b649f36a><span id="main-nav-aria-label" class="visually-hidden" data-v-b649f36a> Main Navigation </span><!--[-->`);
        ssrRenderList(unref(theme2).nav, (item) => {
          _push(`<!--[-->`);
          if ("link" in item) {
            _push(ssrRenderComponent(VPNavBarMenuLink, { item }, null, _parent));
          } else if ("component" in item) {
            ssrRenderVNode(_push, createVNode(resolveDynamicComponent(item.component), mergeProps({ ref_for: true }, item.props), null), _parent);
          } else {
            _push(ssrRenderComponent(_sfc_main$y, { item }, null, _parent));
          }
          _push(`<!--]-->`);
        });
        _push(`<!--]--></nav>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$x = _sfc_main$x.setup;
_sfc_main$x.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarMenu.vue");
  return _sfc_setup$x ? _sfc_setup$x(props, ctx) : void 0;
};
const VPNavBarMenu = /* @__PURE__ */ _export_sfc(_sfc_main$x, [["__scopeId", "data-v-b649f36a"]]);
function createSearchTranslate(defaultTranslations) {
  const { localeIndex, theme: theme2 } = useData();
  function translate(key) {
    var _a, _b, _c;
    const keyPath = key.split(".");
    const themeObject = (_a = theme2.value.search) == null ? void 0 : _a.options;
    const isObject2 = themeObject && typeof themeObject === "object";
    const locales = isObject2 && ((_c = (_b = themeObject.locales) == null ? void 0 : _b[localeIndex.value]) == null ? void 0 : _c.translations) || null;
    const translations = isObject2 && themeObject.translations || null;
    let localeResult = locales;
    let translationResult = translations;
    let defaultResult = defaultTranslations;
    const lastKey = keyPath.pop();
    for (const k of keyPath) {
      let fallbackResult = null;
      const foundInFallback = defaultResult == null ? void 0 : defaultResult[k];
      if (foundInFallback) {
        fallbackResult = defaultResult = foundInFallback;
      }
      const foundInTranslation = translationResult == null ? void 0 : translationResult[k];
      if (foundInTranslation) {
        fallbackResult = translationResult = foundInTranslation;
      }
      const foundInLocale = localeResult == null ? void 0 : localeResult[k];
      if (foundInLocale) {
        fallbackResult = localeResult = foundInLocale;
      }
      if (!foundInFallback) {
        defaultResult = fallbackResult;
      }
      if (!foundInTranslation) {
        translationResult = fallbackResult;
      }
      if (!foundInLocale) {
        localeResult = fallbackResult;
      }
    }
    return (localeResult == null ? void 0 : localeResult[lastKey]) ?? (translationResult == null ? void 0 : translationResult[lastKey]) ?? (defaultResult == null ? void 0 : defaultResult[lastKey]) ?? "";
  }
  return translate;
}
const _sfc_main$w = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarSearchButton",
  __ssrInlineRender: true,
  setup(__props) {
    const defaultTranslations = {
      button: {
        buttonText: "Search",
        buttonAriaLabel: "Search"
      }
    };
    const translate = createSearchTranslate(defaultTranslations);
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<button${ssrRenderAttrs(mergeProps({
        type: "button",
        class: "DocSearch DocSearch-Button",
        "aria-label": unref(translate)("button.buttonAriaLabel")
      }, _attrs))}><span class="DocSearch-Button-Container"><span class="vp-icon DocSearch-Search-Icon"></span><span class="DocSearch-Button-Placeholder">${ssrInterpolate(unref(translate)("button.buttonText"))}</span></span><span class="DocSearch-Button-Keys"><kbd class="DocSearch-Button-Key"></kbd><kbd class="DocSearch-Button-Key">K</kbd></span></button>`);
    };
  }
});
const _sfc_setup$w = _sfc_main$w.setup;
_sfc_main$w.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarSearchButton.vue");
  return _sfc_setup$w ? _sfc_setup$w(props, ctx) : void 0;
};
const _sfc_main$v = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarSearch",
  __ssrInlineRender: true,
  setup(__props) {
    const VPLocalSearchBox = defineAsyncComponent(() => import("./VPLocalSearchBox.XxTBPMv-.js"));
    const VPAlgoliaSearchBox = () => null;
    const { theme: theme2 } = useData();
    const loaded = ref(false);
    const actuallyLoaded = ref(false);
    onMounted(() => {
      {
        return;
      }
    });
    function load() {
      if (!loaded.value) {
        loaded.value = true;
        setTimeout(poll, 16);
      }
    }
    function poll() {
      const e = new Event("keydown");
      e.key = "k";
      e.metaKey = true;
      window.dispatchEvent(e);
      setTimeout(() => {
        if (!document.querySelector(".DocSearch-Modal")) {
          poll();
        }
      }, 16);
    }
    function isEditingContent(event) {
      const element = event.target;
      const tagName = element.tagName;
      return element.isContentEditable || tagName === "INPUT" || tagName === "SELECT" || tagName === "TEXTAREA";
    }
    const showSearch = ref(false);
    {
      onKeyStroke("k", (event) => {
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          showSearch.value = true;
        }
      });
      onKeyStroke("/", (event) => {
        if (!isEditingContent(event)) {
          event.preventDefault();
          showSearch.value = true;
        }
      });
    }
    const provider = "local";
    return (_ctx, _push, _parent, _attrs) => {
      var _a;
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPNavBarSearch" }, _attrs))}>`);
      if (unref(provider) === "local") {
        _push(`<!--[-->`);
        if (showSearch.value) {
          _push(ssrRenderComponent(unref(VPLocalSearchBox), {
            onClose: ($event) => showSearch.value = false
          }, null, _parent));
        } else {
          _push(`<!---->`);
        }
        _push(`<div id="local-search">`);
        _push(ssrRenderComponent(_sfc_main$w, {
          onClick: ($event) => showSearch.value = true
        }, null, _parent));
        _push(`</div><!--]-->`);
      } else if (unref(provider) === "algolia") {
        _push(`<!--[-->`);
        if (loaded.value) {
          _push(ssrRenderComponent(unref(VPAlgoliaSearchBox), {
            algolia: ((_a = unref(theme2).search) == null ? void 0 : _a.options) ?? unref(theme2).algolia,
            onVnodeBeforeMount: ($event) => actuallyLoaded.value = true
          }, null, _parent));
        } else {
          _push(`<!---->`);
        }
        if (!actuallyLoaded.value) {
          _push(`<div id="docsearch">`);
          _push(ssrRenderComponent(_sfc_main$w, { onClick: load }, null, _parent));
          _push(`</div>`);
        } else {
          _push(`<!---->`);
        }
        _push(`<!--]-->`);
      } else {
        _push(`<!---->`);
      }
      _push(`</div>`);
    };
  }
});
const _sfc_setup$v = _sfc_main$v.setup;
_sfc_main$v.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarSearch.vue");
  return _sfc_setup$v ? _sfc_setup$v(props, ctx) : void 0;
};
const _sfc_main$u = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarSocialLinks",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(theme2).socialLinks) {
        _push(ssrRenderComponent(VPSocialLinks, mergeProps({
          class: "VPNavBarSocialLinks",
          links: unref(theme2).socialLinks
        }, _attrs), null, _parent));
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$u = _sfc_main$u.setup;
_sfc_main$u.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarSocialLinks.vue");
  return _sfc_setup$u ? _sfc_setup$u(props, ctx) : void 0;
};
const VPNavBarSocialLinks = /* @__PURE__ */ _export_sfc(_sfc_main$u, [["__scopeId", "data-v-073d3f6b"]]);
const _sfc_main$t = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarTitle",
  __ssrInlineRender: true,
  setup(__props) {
    const { site, theme: theme2 } = useData();
    const { hasSidebar } = useSidebar();
    const { currentLang } = useLangs();
    const link2 = computed(
      () => {
        var _a;
        return typeof theme2.value.logoLink === "string" ? theme2.value.logoLink : (_a = theme2.value.logoLink) == null ? void 0 : _a.link;
      }
    );
    const rel = computed(
      () => {
        var _a;
        return typeof theme2.value.logoLink === "string" ? void 0 : (_a = theme2.value.logoLink) == null ? void 0 : _a.rel;
      }
    );
    const target = computed(
      () => {
        var _a;
        return typeof theme2.value.logoLink === "string" ? void 0 : (_a = theme2.value.logoLink) == null ? void 0 : _a.target;
      }
    );
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPNavBarTitle", { "has-sidebar": unref(hasSidebar) }]
      }, _attrs))} data-v-50b3942e><a class="title"${ssrRenderAttr("href", link2.value ?? unref(normalizeLink$1)(unref(currentLang).link))}${ssrRenderAttr("rel", rel.value)}${ssrRenderAttr("target", target.value)} data-v-50b3942e>`);
      ssrRenderSlot(_ctx.$slots, "nav-bar-title-before", {}, null, _push, _parent);
      if (unref(theme2).logo) {
        _push(ssrRenderComponent(VPImage, {
          class: "logo",
          image: unref(theme2).logo
        }, null, _parent));
      } else {
        _push(`<!---->`);
      }
      if (unref(theme2).siteTitle) {
        _push(`<span data-v-50b3942e>${unref(theme2).siteTitle ?? ""}</span>`);
      } else if (unref(theme2).siteTitle === void 0) {
        _push(`<span data-v-50b3942e>${ssrInterpolate(unref(site).title)}</span>`);
      } else {
        _push(`<!---->`);
      }
      ssrRenderSlot(_ctx.$slots, "nav-bar-title-after", {}, null, _push, _parent);
      _push(`</a></div>`);
    };
  }
});
const _sfc_setup$t = _sfc_main$t.setup;
_sfc_main$t.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarTitle.vue");
  return _sfc_setup$t ? _sfc_setup$t(props, ctx) : void 0;
};
const VPNavBarTitle = /* @__PURE__ */ _export_sfc(_sfc_main$t, [["__scopeId", "data-v-50b3942e"]]);
const _sfc_main$s = /* @__PURE__ */ defineComponent({
  __name: "VPNavBarTranslations",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    const { localeLinks, currentLang } = useLangs({ correspondingLink: true });
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(localeLinks).length && unref(currentLang).label) {
        _push(ssrRenderComponent(VPFlyout, mergeProps({
          class: "VPNavBarTranslations",
          icon: "vpi-languages",
          label: unref(theme2).langMenuLabel || "Change language"
        }, _attrs), {
          default: withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              _push2(`<div class="items" data-v-3eb1bcd9${_scopeId}><p class="title" data-v-3eb1bcd9${_scopeId}>${ssrInterpolate(unref(currentLang).label)}</p><!--[-->`);
              ssrRenderList(unref(localeLinks), (locale) => {
                _push2(ssrRenderComponent(VPMenuLink, { item: locale }, null, _parent2, _scopeId));
              });
              _push2(`<!--]--></div>`);
            } else {
              return [
                createVNode("div", { class: "items" }, [
                  createVNode("p", { class: "title" }, toDisplayString(unref(currentLang).label), 1),
                  (openBlock(true), createBlock(Fragment, null, renderList(unref(localeLinks), (locale) => {
                    return openBlock(), createBlock(VPMenuLink, {
                      key: locale.link,
                      item: locale
                    }, null, 8, ["item"]);
                  }), 128))
                ])
              ];
            }
          }),
          _: 1
        }, _parent));
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$s = _sfc_main$s.setup;
_sfc_main$s.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBarTranslations.vue");
  return _sfc_setup$s ? _sfc_setup$s(props, ctx) : void 0;
};
const VPNavBarTranslations = /* @__PURE__ */ _export_sfc(_sfc_main$s, [["__scopeId", "data-v-3eb1bcd9"]]);
const _sfc_main$r = /* @__PURE__ */ defineComponent({
  __name: "VPNavBar",
  __ssrInlineRender: true,
  props: {
    isScreenOpen: { type: Boolean }
  },
  emits: ["toggle-screen"],
  setup(__props) {
    const props = __props;
    const { y } = useWindowScroll();
    const { hasSidebar } = useSidebar();
    const { frontmatter } = useData();
    const classes = ref({});
    watchPostEffect(() => {
      classes.value = {
        "has-sidebar": hasSidebar.value,
        "home": frontmatter.value.layout === "home",
        "top": y.value === 0,
        "screen-open": props.isScreenOpen
      };
    });
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPNavBar", classes.value]
      }, _attrs))} data-v-3e400d42><div class="wrapper" data-v-3e400d42><div class="container" data-v-3e400d42><div class="title" data-v-3e400d42>`);
      _push(ssrRenderComponent(VPNavBarTitle, null, {
        "nav-bar-title-before": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "nav-bar-title-before", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "nav-bar-title-before", {}, void 0, true)
            ];
          }
        }),
        "nav-bar-title-after": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            ssrRenderSlot(_ctx.$slots, "nav-bar-title-after", {}, null, _push2, _parent2, _scopeId);
          } else {
            return [
              renderSlot(_ctx.$slots, "nav-bar-title-after", {}, void 0, true)
            ];
          }
        }),
        _: 3
      }, _parent));
      _push(`</div><div class="content" data-v-3e400d42><div class="content-body" data-v-3e400d42>`);
      ssrRenderSlot(_ctx.$slots, "nav-bar-content-before", {}, null, _push, _parent);
      _push(ssrRenderComponent(_sfc_main$v, { class: "search" }, null, _parent));
      _push(ssrRenderComponent(VPNavBarMenu, { class: "menu" }, null, _parent));
      _push(ssrRenderComponent(VPNavBarTranslations, { class: "translations" }, null, _parent));
      _push(ssrRenderComponent(VPNavBarAppearance, { class: "appearance" }, null, _parent));
      _push(ssrRenderComponent(VPNavBarSocialLinks, { class: "social-links" }, null, _parent));
      _push(ssrRenderComponent(VPNavBarExtra, { class: "extra" }, null, _parent));
      ssrRenderSlot(_ctx.$slots, "nav-bar-content-after", {}, null, _push, _parent);
      _push(ssrRenderComponent(VPNavBarHamburger, {
        class: "hamburger",
        active: __props.isScreenOpen,
        onClick: ($event) => _ctx.$emit("toggle-screen")
      }, null, _parent));
      _push(`</div></div></div></div><div class="divider" data-v-3e400d42><div class="divider-line" data-v-3e400d42></div></div></div>`);
    };
  }
});
const _sfc_setup$r = _sfc_main$r.setup;
_sfc_main$r.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavBar.vue");
  return _sfc_setup$r ? _sfc_setup$r(props, ctx) : void 0;
};
const VPNavBar = /* @__PURE__ */ _export_sfc(_sfc_main$r, [["__scopeId", "data-v-3e400d42"]]);
const _sfc_main$q = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreenAppearance",
  __ssrInlineRender: true,
  setup(__props) {
    const { site, theme: theme2 } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(site).appearance && unref(site).appearance !== "force-dark" && unref(site).appearance !== "force-auto") {
        _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPNavScreenAppearance" }, _attrs))} data-v-148e0e46><p class="text" data-v-148e0e46>${ssrInterpolate(unref(theme2).darkModeSwitchLabel || "Appearance")}</p>`);
        _push(ssrRenderComponent(VPSwitchAppearance, null, null, _parent));
        _push(`</div>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$q = _sfc_main$q.setup;
_sfc_main$q.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreenAppearance.vue");
  return _sfc_setup$q ? _sfc_setup$q(props, ctx) : void 0;
};
const VPNavScreenAppearance = /* @__PURE__ */ _export_sfc(_sfc_main$q, [["__scopeId", "data-v-148e0e46"]]);
const _sfc_main$p = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreenMenuLink",
  __ssrInlineRender: true,
  props: {
    item: {}
  },
  setup(__props) {
    const closeScreen = inject("close-screen");
    return (_ctx, _push, _parent, _attrs) => {
      _push(ssrRenderComponent(_sfc_main$10, mergeProps({
        class: "VPNavScreenMenuLink",
        href: __props.item.link,
        target: __props.item.target,
        rel: __props.item.rel,
        "no-icon": __props.item.noIcon,
        onClick: unref(closeScreen)
      }, _attrs), {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            _push2(`<span data-v-a773ca40${_scopeId}>${__props.item.text ?? ""}</span>`);
          } else {
            return [
              createVNode("span", {
                innerHTML: __props.item.text
              }, null, 8, ["innerHTML"])
            ];
          }
        }),
        _: 1
      }, _parent));
    };
  }
});
const _sfc_setup$p = _sfc_main$p.setup;
_sfc_main$p.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreenMenuLink.vue");
  return _sfc_setup$p ? _sfc_setup$p(props, ctx) : void 0;
};
const VPNavScreenMenuLink = /* @__PURE__ */ _export_sfc(_sfc_main$p, [["__scopeId", "data-v-a773ca40"]]);
const _sfc_main$o = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreenMenuGroupLink",
  __ssrInlineRender: true,
  props: {
    item: {}
  },
  setup(__props) {
    const closeScreen = inject("close-screen");
    return (_ctx, _push, _parent, _attrs) => {
      _push(ssrRenderComponent(_sfc_main$10, mergeProps({
        class: "VPNavScreenMenuGroupLink",
        href: __props.item.link,
        target: __props.item.target,
        rel: __props.item.rel,
        "no-icon": __props.item.noIcon,
        onClick: unref(closeScreen)
      }, _attrs), {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            _push2(`<span data-v-5490491d${_scopeId}>${__props.item.text ?? ""}</span>`);
          } else {
            return [
              createVNode("span", {
                innerHTML: __props.item.text
              }, null, 8, ["innerHTML"])
            ];
          }
        }),
        _: 1
      }, _parent));
    };
  }
});
const _sfc_setup$o = _sfc_main$o.setup;
_sfc_main$o.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreenMenuGroupLink.vue");
  return _sfc_setup$o ? _sfc_setup$o(props, ctx) : void 0;
};
const VPNavScreenMenuGroupLink = /* @__PURE__ */ _export_sfc(_sfc_main$o, [["__scopeId", "data-v-5490491d"]]);
const _sfc_main$n = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreenMenuGroupSection",
  __ssrInlineRender: true,
  props: {
    text: {},
    items: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPNavScreenMenuGroupSection" }, _attrs))} data-v-f46d0441>`);
      if (__props.text) {
        _push(`<p class="title" data-v-f46d0441>${ssrInterpolate(__props.text)}</p>`);
      } else {
        _push(`<!---->`);
      }
      _push(`<!--[-->`);
      ssrRenderList(__props.items, (item) => {
        _push(ssrRenderComponent(VPNavScreenMenuGroupLink, {
          key: item.text,
          item
        }, null, _parent));
      });
      _push(`<!--]--></div>`);
    };
  }
});
const _sfc_setup$n = _sfc_main$n.setup;
_sfc_main$n.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreenMenuGroupSection.vue");
  return _sfc_setup$n ? _sfc_setup$n(props, ctx) : void 0;
};
const VPNavScreenMenuGroupSection = /* @__PURE__ */ _export_sfc(_sfc_main$n, [["__scopeId", "data-v-f46d0441"]]);
const _sfc_main$m = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreenMenuGroup",
  __ssrInlineRender: true,
  props: {
    text: {},
    items: {}
  },
  setup(__props) {
    const props = __props;
    const isOpen = ref(false);
    const groupId = computed(
      () => `NavScreenGroup-${props.text.replace(" ", "-").toLowerCase()}`
    );
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPNavScreenMenuGroup", { open: isOpen.value }]
      }, _attrs))} data-v-6eaa307b><button class="button"${ssrRenderAttr("aria-controls", groupId.value)}${ssrRenderAttr("aria-expanded", isOpen.value)} data-v-6eaa307b><span class="button-text" data-v-6eaa307b>${__props.text ?? ""}</span><span class="vpi-plus button-icon" data-v-6eaa307b></span></button><div${ssrRenderAttr("id", groupId.value)} class="items" data-v-6eaa307b><!--[-->`);
      ssrRenderList(__props.items, (item) => {
        _push(`<!--[-->`);
        if ("link" in item) {
          _push(`<div class="item" data-v-6eaa307b>`);
          _push(ssrRenderComponent(VPNavScreenMenuGroupLink, { item }, null, _parent));
          _push(`</div>`);
        } else if ("component" in item) {
          _push(`<div class="item" data-v-6eaa307b>`);
          ssrRenderVNode(_push, createVNode(resolveDynamicComponent(item.component), mergeProps({ ref_for: true }, item.props, { "screen-menu": "" }), null), _parent);
          _push(`</div>`);
        } else {
          _push(`<div class="group" data-v-6eaa307b>`);
          _push(ssrRenderComponent(VPNavScreenMenuGroupSection, {
            text: item.text,
            items: item.items
          }, null, _parent));
          _push(`</div>`);
        }
        _push(`<!--]-->`);
      });
      _push(`<!--]--></div></div>`);
    };
  }
});
const _sfc_setup$m = _sfc_main$m.setup;
_sfc_main$m.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreenMenuGroup.vue");
  return _sfc_setup$m ? _sfc_setup$m(props, ctx) : void 0;
};
const VPNavScreenMenuGroup = /* @__PURE__ */ _export_sfc(_sfc_main$m, [["__scopeId", "data-v-6eaa307b"]]);
const _sfc_main$l = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreenMenu",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(theme2).nav) {
        _push(`<nav${ssrRenderAttrs(mergeProps({ class: "VPNavScreenMenu" }, _attrs))}><!--[-->`);
        ssrRenderList(unref(theme2).nav, (item) => {
          _push(`<!--[-->`);
          if ("link" in item) {
            _push(ssrRenderComponent(VPNavScreenMenuLink, { item }, null, _parent));
          } else if ("component" in item) {
            ssrRenderVNode(_push, createVNode(resolveDynamicComponent(item.component), mergeProps({ ref_for: true }, item.props, { "screen-menu": "" }), null), _parent);
          } else {
            _push(ssrRenderComponent(VPNavScreenMenuGroup, {
              text: item.text || "",
              items: item.items
            }, null, _parent));
          }
          _push(`<!--]-->`);
        });
        _push(`<!--]--></nav>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$l = _sfc_main$l.setup;
_sfc_main$l.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreenMenu.vue");
  return _sfc_setup$l ? _sfc_setup$l(props, ctx) : void 0;
};
const _sfc_main$k = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreenSocialLinks",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(theme2).socialLinks) {
        _push(ssrRenderComponent(VPSocialLinks, mergeProps({
          class: "VPNavScreenSocialLinks",
          links: unref(theme2).socialLinks
        }, _attrs), null, _parent));
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$k = _sfc_main$k.setup;
_sfc_main$k.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreenSocialLinks.vue");
  return _sfc_setup$k ? _sfc_setup$k(props, ctx) : void 0;
};
const _sfc_main$j = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreenTranslations",
  __ssrInlineRender: true,
  setup(__props) {
    const { localeLinks, currentLang } = useLangs({ correspondingLink: true });
    const isOpen = ref(false);
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(localeLinks).length && unref(currentLang).label) {
        _push(`<div${ssrRenderAttrs(mergeProps({
          class: ["VPNavScreenTranslations", { open: isOpen.value }]
        }, _attrs))} data-v-9bbe54a6><button class="title" data-v-9bbe54a6><span class="vpi-languages icon lang" data-v-9bbe54a6></span> ${ssrInterpolate(unref(currentLang).label)} <span class="vpi-chevron-down icon chevron" data-v-9bbe54a6></span></button><ul class="list" data-v-9bbe54a6><!--[-->`);
        ssrRenderList(unref(localeLinks), (locale) => {
          _push(`<li class="item" data-v-9bbe54a6>`);
          _push(ssrRenderComponent(_sfc_main$10, {
            class: "link",
            href: locale.link
          }, {
            default: withCtx((_, _push2, _parent2, _scopeId) => {
              if (_push2) {
                _push2(`${ssrInterpolate(locale.text)}`);
              } else {
                return [
                  createTextVNode(toDisplayString(locale.text), 1)
                ];
              }
            }),
            _: 2
          }, _parent));
          _push(`</li>`);
        });
        _push(`<!--]--></ul></div>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$j = _sfc_main$j.setup;
_sfc_main$j.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreenTranslations.vue");
  return _sfc_setup$j ? _sfc_setup$j(props, ctx) : void 0;
};
const VPNavScreenTranslations = /* @__PURE__ */ _export_sfc(_sfc_main$j, [["__scopeId", "data-v-9bbe54a6"]]);
const _sfc_main$i = /* @__PURE__ */ defineComponent({
  __name: "VPNavScreen",
  __ssrInlineRender: true,
  props: {
    open: { type: Boolean }
  },
  setup(__props) {
    const screen = ref(null);
    useScrollLock(inBrowser ? document.body : null);
    return (_ctx, _push, _parent, _attrs) => {
      if (__props.open) {
        _push(`<div${ssrRenderAttrs(mergeProps({
          class: "VPNavScreen",
          ref_key: "screen",
          ref: screen,
          id: "VPNavScreen"
        }, _attrs))} data-v-04dcb82f><div class="container" data-v-04dcb82f>`);
        ssrRenderSlot(_ctx.$slots, "nav-screen-content-before", {}, null, _push, _parent);
        _push(ssrRenderComponent(_sfc_main$l, { class: "menu" }, null, _parent));
        _push(ssrRenderComponent(VPNavScreenTranslations, { class: "translations" }, null, _parent));
        _push(ssrRenderComponent(VPNavScreenAppearance, { class: "appearance" }, null, _parent));
        _push(ssrRenderComponent(_sfc_main$k, { class: "social-links" }, null, _parent));
        ssrRenderSlot(_ctx.$slots, "nav-screen-content-after", {}, null, _push, _parent);
        _push(`</div></div>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$i = _sfc_main$i.setup;
_sfc_main$i.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNavScreen.vue");
  return _sfc_setup$i ? _sfc_setup$i(props, ctx) : void 0;
};
const VPNavScreen = /* @__PURE__ */ _export_sfc(_sfc_main$i, [["__scopeId", "data-v-04dcb82f"]]);
const _sfc_main$h = /* @__PURE__ */ defineComponent({
  __name: "VPNav",
  __ssrInlineRender: true,
  setup(__props) {
    const { isScreenOpen, closeScreen, toggleScreen } = useNav();
    const { frontmatter } = useData();
    const hasNavbar = computed(() => {
      return frontmatter.value.navbar !== false;
    });
    provide("close-screen", closeScreen);
    watchEffect(() => {
      if (inBrowser) {
        document.documentElement.classList.toggle("hide-nav", !hasNavbar.value);
      }
    });
    return (_ctx, _push, _parent, _attrs) => {
      if (hasNavbar.value) {
        _push(`<header${ssrRenderAttrs(mergeProps({ class: "VPNav" }, _attrs))} data-v-d216a525>`);
        _push(ssrRenderComponent(VPNavBar, {
          "is-screen-open": unref(isScreenOpen),
          onToggleScreen: unref(toggleScreen)
        }, {
          "nav-bar-title-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-bar-title-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-bar-title-before", {}, void 0, true)
              ];
            }
          }),
          "nav-bar-title-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-bar-title-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-bar-title-after", {}, void 0, true)
              ];
            }
          }),
          "nav-bar-content-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-bar-content-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-bar-content-before", {}, void 0, true)
              ];
            }
          }),
          "nav-bar-content-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-bar-content-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-bar-content-after", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
        _push(ssrRenderComponent(VPNavScreen, { open: unref(isScreenOpen) }, {
          "nav-screen-content-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-screen-content-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-screen-content-before", {}, void 0, true)
              ];
            }
          }),
          "nav-screen-content-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-screen-content-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-screen-content-after", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
        _push(`</header>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$h = _sfc_main$h.setup;
_sfc_main$h.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPNav.vue");
  return _sfc_setup$h ? _sfc_setup$h(props, ctx) : void 0;
};
const VPNav = /* @__PURE__ */ _export_sfc(_sfc_main$h, [["__scopeId", "data-v-d216a525"]]);
const _sfc_main$g = /* @__PURE__ */ defineComponent({
  __name: "VPSidebarItem",
  __ssrInlineRender: true,
  props: {
    item: {},
    depth: {}
  },
  setup(__props) {
    const props = __props;
    const {
      collapsed,
      collapsible,
      isLink,
      isActiveLink,
      hasActiveLink: hasActiveLink2,
      hasChildren,
      toggle
    } = useSidebarControl(computed(() => props.item));
    const sectionTag = computed(() => hasChildren.value ? "section" : `div`);
    const linkTag = computed(() => isLink.value ? "a" : "div");
    const textTag = computed(() => {
      return !hasChildren.value ? "p" : props.depth + 2 === 7 ? "p" : `h${props.depth + 2}`;
    });
    const itemRole = computed(() => isLink.value ? void 0 : "button");
    const classes = computed(() => [
      [`level-${props.depth}`],
      { collapsible: collapsible.value },
      { collapsed: collapsed.value },
      { "is-link": isLink.value },
      { "is-active": isActiveLink.value },
      { "has-active": hasActiveLink2.value }
    ]);
    function onItemInteraction(e) {
      if ("key" in e && e.key !== "Enter") {
        return;
      }
      !props.item.link && toggle();
    }
    function onCaretClick() {
      props.item.link && toggle();
    }
    return (_ctx, _push, _parent, _attrs) => {
      const _component_VPSidebarItem = resolveComponent("VPSidebarItem", true);
      ssrRenderVNode(_push, createVNode(resolveDynamicComponent(sectionTag.value), mergeProps({
        class: ["VPSidebarItem", classes.value]
      }, _attrs), {
        default: withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            if (__props.item.text) {
              _push2(`<div class="item"${ssrRenderAttr("role", itemRole.value)}${ssrRenderAttr("tabindex", __props.item.items && 0)} data-v-5ba4e650${_scopeId}><div class="indicator" data-v-5ba4e650${_scopeId}></div>`);
              if (__props.item.link) {
                _push2(ssrRenderComponent(_sfc_main$10, {
                  tag: linkTag.value,
                  class: "link",
                  href: __props.item.link,
                  rel: __props.item.rel,
                  target: __props.item.target
                }, {
                  default: withCtx((_2, _push3, _parent3, _scopeId2) => {
                    if (_push3) {
                      ssrRenderVNode(_push3, createVNode(resolveDynamicComponent(textTag.value), { class: "text" }, null), _parent3, _scopeId2);
                    } else {
                      return [
                        (openBlock(), createBlock(resolveDynamicComponent(textTag.value), {
                          class: "text",
                          innerHTML: __props.item.text
                        }, null, 8, ["innerHTML"]))
                      ];
                    }
                  }),
                  _: 1
                }, _parent2, _scopeId));
              } else {
                ssrRenderVNode(_push2, createVNode(resolveDynamicComponent(textTag.value), { class: "text" }, null), _parent2, _scopeId);
              }
              if (__props.item.collapsed != null && __props.item.items && __props.item.items.length) {
                _push2(`<div class="caret" role="button" aria-label="toggle section" tabindex="0" data-v-5ba4e650${_scopeId}><span class="vpi-chevron-right caret-icon" data-v-5ba4e650${_scopeId}></span></div>`);
              } else {
                _push2(`<!---->`);
              }
              _push2(`</div>`);
            } else {
              _push2(`<!---->`);
            }
            if (__props.item.items && __props.item.items.length) {
              _push2(`<div class="items" data-v-5ba4e650${_scopeId}>`);
              if (__props.depth < 5) {
                _push2(`<!--[-->`);
                ssrRenderList(__props.item.items, (i) => {
                  _push2(ssrRenderComponent(_component_VPSidebarItem, {
                    key: i.text,
                    item: i,
                    depth: __props.depth + 1
                  }, null, _parent2, _scopeId));
                });
                _push2(`<!--]-->`);
              } else {
                _push2(`<!---->`);
              }
              _push2(`</div>`);
            } else {
              _push2(`<!---->`);
            }
          } else {
            return [
              __props.item.text ? (openBlock(), createBlock("div", mergeProps({
                key: 0,
                class: "item",
                role: itemRole.value
              }, toHandlers(
                __props.item.items ? { click: onItemInteraction, keydown: onItemInteraction } : {},
                true
              ), {
                tabindex: __props.item.items && 0
              }), [
                createVNode("div", { class: "indicator" }),
                __props.item.link ? (openBlock(), createBlock(_sfc_main$10, {
                  key: 0,
                  tag: linkTag.value,
                  class: "link",
                  href: __props.item.link,
                  rel: __props.item.rel,
                  target: __props.item.target
                }, {
                  default: withCtx(() => [
                    (openBlock(), createBlock(resolveDynamicComponent(textTag.value), {
                      class: "text",
                      innerHTML: __props.item.text
                    }, null, 8, ["innerHTML"]))
                  ]),
                  _: 1
                }, 8, ["tag", "href", "rel", "target"])) : (openBlock(), createBlock(resolveDynamicComponent(textTag.value), {
                  key: 1,
                  class: "text",
                  innerHTML: __props.item.text
                }, null, 8, ["innerHTML"])),
                __props.item.collapsed != null && __props.item.items && __props.item.items.length ? (openBlock(), createBlock("div", {
                  key: 2,
                  class: "caret",
                  role: "button",
                  "aria-label": "toggle section",
                  onClick: onCaretClick,
                  onKeydown: withKeys(onCaretClick, ["enter"]),
                  tabindex: "0"
                }, [
                  createVNode("span", { class: "vpi-chevron-right caret-icon" })
                ], 32)) : createCommentVNode("", true)
              ], 16, ["role", "tabindex"])) : createCommentVNode("", true),
              __props.item.items && __props.item.items.length ? (openBlock(), createBlock("div", {
                key: 1,
                class: "items"
              }, [
                __props.depth < 5 ? (openBlock(true), createBlock(Fragment, { key: 0 }, renderList(__props.item.items, (i) => {
                  return openBlock(), createBlock(_component_VPSidebarItem, {
                    key: i.text,
                    item: i,
                    depth: __props.depth + 1
                  }, null, 8, ["item", "depth"]);
                }), 128)) : createCommentVNode("", true)
              ])) : createCommentVNode("", true)
            ];
          }
        }),
        _: 1
      }), _parent);
    };
  }
});
const _sfc_setup$g = _sfc_main$g.setup;
_sfc_main$g.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSidebarItem.vue");
  return _sfc_setup$g ? _sfc_setup$g(props, ctx) : void 0;
};
const VPSidebarItem = /* @__PURE__ */ _export_sfc(_sfc_main$g, [["__scopeId", "data-v-5ba4e650"]]);
const _sfc_main$f = /* @__PURE__ */ defineComponent({
  __name: "VPSidebarGroup",
  __ssrInlineRender: true,
  props: {
    items: {}
  },
  setup(__props) {
    const disableTransition = ref(true);
    let timer = null;
    onMounted(() => {
      timer = setTimeout(() => {
        timer = null;
        disableTransition.value = false;
      }, 300);
    });
    onBeforeUnmount(() => {
      if (timer != null) {
        clearTimeout(timer);
        timer = null;
      }
    });
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<!--[-->`);
      ssrRenderList(__props.items, (item) => {
        _push(`<div class="${ssrRenderClass([{ "no-transition": disableTransition.value }, "group"])}" data-v-d21c66e3>`);
        _push(ssrRenderComponent(VPSidebarItem, {
          item,
          depth: 0
        }, null, _parent));
        _push(`</div>`);
      });
      _push(`<!--]-->`);
    };
  }
});
const _sfc_setup$f = _sfc_main$f.setup;
_sfc_main$f.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSidebarGroup.vue");
  return _sfc_setup$f ? _sfc_setup$f(props, ctx) : void 0;
};
const VPSidebarGroup = /* @__PURE__ */ _export_sfc(_sfc_main$f, [["__scopeId", "data-v-d21c66e3"]]);
const _sfc_main$e = /* @__PURE__ */ defineComponent({
  __name: "VPSidebar",
  __ssrInlineRender: true,
  props: {
    open: { type: Boolean }
  },
  setup(__props) {
    const { sidebarGroups, hasSidebar } = useSidebar();
    const props = __props;
    const navEl = ref(null);
    const isLocked = useScrollLock(inBrowser ? document.body : null);
    watch(
      [props, navEl],
      () => {
        var _a;
        if (props.open) {
          isLocked.value = true;
          (_a = navEl.value) == null ? void 0 : _a.focus();
        } else isLocked.value = false;
      },
      { immediate: true, flush: "post" }
    );
    const key = ref(0);
    watch(
      sidebarGroups,
      () => {
        key.value += 1;
      },
      { deep: true }
    );
    return (_ctx, _push, _parent, _attrs) => {
      if (unref(hasSidebar)) {
        _push(`<aside${ssrRenderAttrs(mergeProps({
          class: ["VPSidebar", { open: __props.open }],
          ref_key: "navEl",
          ref: navEl
        }, _attrs))} data-v-ac3d6208><div class="curtain" data-v-ac3d6208></div><nav class="nav" id="VPSidebarNav" aria-labelledby="sidebar-aria-label" tabindex="-1" data-v-ac3d6208><span class="visually-hidden" id="sidebar-aria-label" data-v-ac3d6208> Sidebar Navigation </span>`);
        ssrRenderSlot(_ctx.$slots, "sidebar-nav-before", {}, null, _push, _parent);
        _push(ssrRenderComponent(VPSidebarGroup, {
          items: unref(sidebarGroups),
          key: key.value
        }, null, _parent));
        ssrRenderSlot(_ctx.$slots, "sidebar-nav-after", {}, null, _push, _parent);
        _push(`</nav></aside>`);
      } else {
        _push(`<!---->`);
      }
    };
  }
});
const _sfc_setup$e = _sfc_main$e.setup;
_sfc_main$e.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSidebar.vue");
  return _sfc_setup$e ? _sfc_setup$e(props, ctx) : void 0;
};
const VPSidebar = /* @__PURE__ */ _export_sfc(_sfc_main$e, [["__scopeId", "data-v-ac3d6208"]]);
const _sfc_main$d = /* @__PURE__ */ defineComponent({
  __name: "VPSkipLink",
  __ssrInlineRender: true,
  setup(__props) {
    const { theme: theme2 } = useData();
    const route = useRoute();
    const backToTop = ref();
    watch(() => route.path, () => backToTop.value.focus());
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<!--[--><span tabindex="-1" data-v-3ed116ef></span><a href="#VPContent" class="VPSkipLink visually-hidden" data-v-3ed116ef>${ssrInterpolate(unref(theme2).skipToContentLabel || "Skip to content")}</a><!--]-->`);
    };
  }
});
const _sfc_setup$d = _sfc_main$d.setup;
_sfc_main$d.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSkipLink.vue");
  return _sfc_setup$d ? _sfc_setup$d(props, ctx) : void 0;
};
const VPSkipLink = /* @__PURE__ */ _export_sfc(_sfc_main$d, [["__scopeId", "data-v-3ed116ef"]]);
const _sfc_main$c = /* @__PURE__ */ defineComponent({
  __name: "Layout",
  __ssrInlineRender: true,
  setup(__props) {
    const {
      isOpen: isSidebarOpen,
      open: openSidebar,
      close: closeSidebar
    } = useSidebar();
    const route = useRoute();
    watch(() => route.path, closeSidebar);
    useCloseSidebarOnEscape(isSidebarOpen, closeSidebar);
    const { frontmatter } = useData();
    const slots = useSlots();
    const heroImageSlotExists = computed(() => !!slots["home-hero-image"]);
    provide("hero-image-slot-exists", heroImageSlotExists);
    return (_ctx, _push, _parent, _attrs) => {
      const _component_Content = resolveComponent("Content");
      if (unref(frontmatter).layout !== false) {
        _push(`<div${ssrRenderAttrs(mergeProps({
          class: ["Layout", unref(frontmatter).pageClass]
        }, _attrs))} data-v-1429686b>`);
        ssrRenderSlot(_ctx.$slots, "layout-top", {}, null, _push, _parent);
        _push(ssrRenderComponent(VPSkipLink, null, null, _parent));
        _push(ssrRenderComponent(VPBackdrop, {
          class: "backdrop",
          show: unref(isSidebarOpen),
          onClick: unref(closeSidebar)
        }, null, _parent));
        _push(ssrRenderComponent(VPNav, null, {
          "nav-bar-title-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-bar-title-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-bar-title-before", {}, void 0, true)
              ];
            }
          }),
          "nav-bar-title-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-bar-title-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-bar-title-after", {}, void 0, true)
              ];
            }
          }),
          "nav-bar-content-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-bar-content-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-bar-content-before", {}, void 0, true)
              ];
            }
          }),
          "nav-bar-content-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-bar-content-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-bar-content-after", {}, void 0, true)
              ];
            }
          }),
          "nav-screen-content-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-screen-content-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-screen-content-before", {}, void 0, true)
              ];
            }
          }),
          "nav-screen-content-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "nav-screen-content-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "nav-screen-content-after", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
        _push(ssrRenderComponent(VPLocalNav, {
          open: unref(isSidebarOpen),
          onOpenMenu: unref(openSidebar)
        }, null, _parent));
        _push(ssrRenderComponent(VPSidebar, { open: unref(isSidebarOpen) }, {
          "sidebar-nav-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "sidebar-nav-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "sidebar-nav-before", {}, void 0, true)
              ];
            }
          }),
          "sidebar-nav-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "sidebar-nav-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "sidebar-nav-after", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
        _push(ssrRenderComponent(VPContent, null, {
          "page-top": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "page-top", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "page-top", {}, void 0, true)
              ];
            }
          }),
          "page-bottom": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "page-bottom", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "page-bottom", {}, void 0, true)
              ];
            }
          }),
          "not-found": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "not-found", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "not-found", {}, void 0, true)
              ];
            }
          }),
          "home-hero-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-before", {}, void 0, true)
              ];
            }
          }),
          "home-hero-info-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info-before", {}, void 0, true)
              ];
            }
          }),
          "home-hero-info": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info", {}, void 0, true)
              ];
            }
          }),
          "home-hero-info-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-info-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-info-after", {}, void 0, true)
              ];
            }
          }),
          "home-hero-actions-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-actions-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-actions-after", {}, void 0, true)
              ];
            }
          }),
          "home-hero-image": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-image", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-image", {}, void 0, true)
              ];
            }
          }),
          "home-hero-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-hero-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-hero-after", {}, void 0, true)
              ];
            }
          }),
          "home-features-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-features-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-features-before", {}, void 0, true)
              ];
            }
          }),
          "home-features-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "home-features-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "home-features-after", {}, void 0, true)
              ];
            }
          }),
          "doc-footer-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-footer-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-footer-before", {}, void 0, true)
              ];
            }
          }),
          "doc-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-before", {}, void 0, true)
              ];
            }
          }),
          "doc-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-after", {}, void 0, true)
              ];
            }
          }),
          "doc-top": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-top", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-top", {}, void 0, true)
              ];
            }
          }),
          "doc-bottom": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "doc-bottom", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "doc-bottom", {}, void 0, true)
              ];
            }
          }),
          "aside-top": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-top", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-top", {}, void 0, true)
              ];
            }
          }),
          "aside-bottom": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-bottom", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-bottom", {}, void 0, true)
              ];
            }
          }),
          "aside-outline-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-outline-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-outline-before", {}, void 0, true)
              ];
            }
          }),
          "aside-outline-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-outline-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-outline-after", {}, void 0, true)
              ];
            }
          }),
          "aside-ads-before": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-ads-before", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-ads-before", {}, void 0, true)
              ];
            }
          }),
          "aside-ads-after": withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              ssrRenderSlot(_ctx.$slots, "aside-ads-after", {}, null, _push2, _parent2, _scopeId);
            } else {
              return [
                renderSlot(_ctx.$slots, "aside-ads-after", {}, void 0, true)
              ];
            }
          }),
          _: 3
        }, _parent));
        _push(ssrRenderComponent(VPFooter, null, null, _parent));
        ssrRenderSlot(_ctx.$slots, "layout-bottom", {}, null, _push, _parent);
        _push(`</div>`);
      } else {
        _push(ssrRenderComponent(_component_Content, _attrs, null, _parent));
      }
    };
  }
});
const _sfc_setup$c = _sfc_main$c.setup;
_sfc_main$c.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/Layout.vue");
  return _sfc_setup$c ? _sfc_setup$c(props, ctx) : void 0;
};
const Layout = /* @__PURE__ */ _export_sfc(_sfc_main$c, [["__scopeId", "data-v-1429686b"]]);
const GridSettings = {
  xmini: [[0, 2]],
  mini: [],
  small: [
    [920, 6],
    [768, 5],
    [640, 4],
    [480, 3],
    [0, 2]
  ],
  medium: [
    [960, 5],
    [832, 4],
    [640, 3],
    [480, 2]
  ],
  big: [
    [832, 3],
    [640, 2]
  ]
};
function useSponsorsGrid({ el, size = "medium" }) {
  const onResize = throttleAndDebounce(manage, 100);
  onMounted(() => {
    manage();
    window.addEventListener("resize", onResize);
  });
  onUnmounted(() => {
    window.removeEventListener("resize", onResize);
  });
  function manage() {
    adjustSlots(el.value, size);
  }
}
function adjustSlots(el, size) {
  const tsize = el.children.length;
  const asize = el.querySelectorAll(".vp-sponsor-grid-item:not(.empty)").length;
  const grid = setGrid(el, size, asize);
  manageSlots(el, grid, tsize, asize);
}
function setGrid(el, size, items) {
  const settings = GridSettings[size];
  const screen = window.innerWidth;
  let grid = 1;
  settings.some(([breakpoint, value]) => {
    if (screen >= breakpoint) {
      grid = items < value ? items : value;
      return true;
    }
  });
  setGridData(el, grid);
  return grid;
}
function setGridData(el, value) {
  el.dataset.vpGrid = String(value);
}
function manageSlots(el, grid, tsize, asize) {
  const diff = tsize - asize;
  const rem = asize % grid;
  const drem = rem === 0 ? rem : grid - rem;
  neutralizeSlots(el, drem - diff);
}
function neutralizeSlots(el, count) {
  if (count === 0) {
    return;
  }
  count > 0 ? addSlots(el, count) : removeSlots(el, count * -1);
}
function addSlots(el, count) {
  for (let i = 0; i < count; i++) {
    const slot = document.createElement("div");
    slot.classList.add("vp-sponsor-grid-item", "empty");
    el.append(slot);
  }
}
function removeSlots(el, count) {
  for (let i = 0; i < count; i++) {
    el.removeChild(el.lastElementChild);
  }
}
const _sfc_main$b = /* @__PURE__ */ defineComponent({
  __name: "VPSponsorsGrid",
  __ssrInlineRender: true,
  props: {
    size: { default: "medium" },
    data: {}
  },
  setup(__props) {
    const props = __props;
    const el = ref(null);
    useSponsorsGrid({ el, size: props.size });
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPSponsorsGrid vp-sponsor-grid", [__props.size]],
        ref_key: "el",
        ref: el
      }, _attrs))}><!--[-->`);
      ssrRenderList(__props.data, (sponsor) => {
        _push(`<div class="vp-sponsor-grid-item"><a class="vp-sponsor-grid-link"${ssrRenderAttr("href", sponsor.url)} target="_blank" rel="sponsored noopener"><article class="vp-sponsor-grid-box"><h4 class="visually-hidden">${ssrInterpolate(sponsor.name)}</h4><img class="vp-sponsor-grid-image"${ssrRenderAttr("src", sponsor.img)}${ssrRenderAttr("alt", sponsor.name)}></article></a></div>`);
      });
      _push(`<!--]--></div>`);
    };
  }
});
const _sfc_setup$b = _sfc_main$b.setup;
_sfc_main$b.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSponsorsGrid.vue");
  return _sfc_setup$b ? _sfc_setup$b(props, ctx) : void 0;
};
const _sfc_main$a = /* @__PURE__ */ defineComponent({
  __name: "VPSponsors",
  __ssrInlineRender: true,
  props: {
    mode: { default: "normal" },
    tier: {},
    size: {},
    data: {}
  },
  setup(__props) {
    const props = __props;
    const sponsors = computed(() => {
      const isSponsors = props.data.some((s) => {
        return "items" in s;
      });
      if (isSponsors) {
        return props.data;
      }
      return [
        { tier: props.tier, size: props.size, items: props.data }
      ];
    });
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPSponsors vp-sponsor", [__props.mode]]
      }, _attrs))}><!--[-->`);
      ssrRenderList(sponsors.value, (sponsor, index) => {
        _push(`<section class="vp-sponsor-section">`);
        if (sponsor.tier) {
          _push(`<h3 class="vp-sponsor-tier">${ssrInterpolate(sponsor.tier)}</h3>`);
        } else {
          _push(`<!---->`);
        }
        _push(ssrRenderComponent(_sfc_main$b, {
          size: sponsor.size,
          data: sponsor.items
        }, null, _parent));
        _push(`</section>`);
      });
      _push(`<!--]--></div>`);
    };
  }
});
const _sfc_setup$a = _sfc_main$a.setup;
_sfc_main$a.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPSponsors.vue");
  return _sfc_setup$a ? _sfc_setup$a(props, ctx) : void 0;
};
const _sfc_main$9 = /* @__PURE__ */ defineComponent({
  __name: "VPDocAsideSponsors",
  __ssrInlineRender: true,
  props: {
    tier: {},
    size: {},
    data: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "VPDocAsideSponsors" }, _attrs))}>`);
      _push(ssrRenderComponent(_sfc_main$a, {
        mode: "aside",
        tier: __props.tier,
        size: __props.size,
        data: __props.data
      }, null, _parent));
      _push(`</div>`);
    };
  }
});
const _sfc_setup$9 = _sfc_main$9.setup;
_sfc_main$9.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPDocAsideSponsors.vue");
  return _sfc_setup$9 ? _sfc_setup$9(props, ctx) : void 0;
};
const _sfc_main$8 = /* @__PURE__ */ defineComponent({
  __name: "VPHomeSponsors",
  __ssrInlineRender: true,
  props: {
    message: {},
    actionText: { default: "Become a sponsor" },
    actionLink: {},
    data: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<section${ssrRenderAttrs(mergeProps({ class: "VPHomeSponsors" }, _attrs))} data-v-d34155cb><div class="container" data-v-d34155cb><div class="header" data-v-d34155cb><div class="love" data-v-d34155cb><span class="vpi-heart icon" data-v-d34155cb></span></div>`);
      if (__props.message) {
        _push(`<h2 class="message" data-v-d34155cb>${ssrInterpolate(__props.message)}</h2>`);
      } else {
        _push(`<!---->`);
      }
      _push(`</div><div class="sponsors" data-v-d34155cb>`);
      _push(ssrRenderComponent(_sfc_main$a, { data: __props.data }, null, _parent));
      _push(`</div>`);
      if (__props.actionLink) {
        _push(`<div class="action" data-v-d34155cb>`);
        _push(ssrRenderComponent(VPButton, {
          theme: "sponsor",
          text: __props.actionText,
          href: __props.actionLink
        }, null, _parent));
        _push(`</div>`);
      } else {
        _push(`<!---->`);
      }
      _push(`</div></section>`);
    };
  }
});
const _sfc_setup$8 = _sfc_main$8.setup;
_sfc_main$8.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPHomeSponsors.vue");
  return _sfc_setup$8 ? _sfc_setup$8(props, ctx) : void 0;
};
const _sfc_main$7 = /* @__PURE__ */ defineComponent({
  __name: "VPTeamMembersItem",
  __ssrInlineRender: true,
  props: {
    size: { default: "medium" },
    member: {}
  },
  setup(__props) {
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<article${ssrRenderAttrs(mergeProps({
        class: ["VPTeamMembersItem", [__props.size]]
      }, _attrs))} data-v-70eb5086><div class="profile" data-v-70eb5086><figure class="avatar" data-v-70eb5086><img class="avatar-img"${ssrRenderAttr("src", __props.member.avatar)}${ssrRenderAttr("alt", __props.member.name)} data-v-70eb5086></figure><div class="data" data-v-70eb5086><h1 class="name" data-v-70eb5086>${ssrInterpolate(__props.member.name)}</h1>`);
      if (__props.member.title || __props.member.org) {
        _push(`<p class="affiliation" data-v-70eb5086>`);
        if (__props.member.title) {
          _push(`<span class="title" data-v-70eb5086>${ssrInterpolate(__props.member.title)}</span>`);
        } else {
          _push(`<!---->`);
        }
        if (__props.member.title && __props.member.org) {
          _push(`<span class="at" data-v-70eb5086> @ </span>`);
        } else {
          _push(`<!---->`);
        }
        if (__props.member.org) {
          _push(ssrRenderComponent(_sfc_main$10, {
            class: ["org", { link: __props.member.orgLink }],
            href: __props.member.orgLink,
            "no-icon": ""
          }, {
            default: withCtx((_, _push2, _parent2, _scopeId) => {
              if (_push2) {
                _push2(`${ssrInterpolate(__props.member.org)}`);
              } else {
                return [
                  createTextVNode(toDisplayString(__props.member.org), 1)
                ];
              }
            }),
            _: 1
          }, _parent));
        } else {
          _push(`<!---->`);
        }
        _push(`</p>`);
      } else {
        _push(`<!---->`);
      }
      if (__props.member.desc) {
        _push(`<p class="desc" data-v-70eb5086>${__props.member.desc ?? ""}</p>`);
      } else {
        _push(`<!---->`);
      }
      if (__props.member.links) {
        _push(`<div class="links" data-v-70eb5086>`);
        _push(ssrRenderComponent(VPSocialLinks, {
          links: __props.member.links
        }, null, _parent));
        _push(`</div>`);
      } else {
        _push(`<!---->`);
      }
      _push(`</div></div>`);
      if (__props.member.sponsor) {
        _push(`<div class="sp" data-v-70eb5086>`);
        _push(ssrRenderComponent(_sfc_main$10, {
          class: "sp-link",
          href: __props.member.sponsor,
          "no-icon": ""
        }, {
          default: withCtx((_, _push2, _parent2, _scopeId) => {
            if (_push2) {
              _push2(`<span class="vpi-heart sp-icon" data-v-70eb5086${_scopeId}></span> ${ssrInterpolate(__props.member.actionText || "Sponsor")}`);
            } else {
              return [
                createVNode("span", { class: "vpi-heart sp-icon" }),
                createTextVNode(" " + toDisplayString(__props.member.actionText || "Sponsor"), 1)
              ];
            }
          }),
          _: 1
        }, _parent));
        _push(`</div>`);
      } else {
        _push(`<!---->`);
      }
      _push(`</article>`);
    };
  }
});
const _sfc_setup$7 = _sfc_main$7.setup;
_sfc_main$7.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPTeamMembersItem.vue");
  return _sfc_setup$7 ? _sfc_setup$7(props, ctx) : void 0;
};
const VPTeamMembersItem = /* @__PURE__ */ _export_sfc(_sfc_main$7, [["__scopeId", "data-v-70eb5086"]]);
const _sfc_main$6 = /* @__PURE__ */ defineComponent({
  __name: "VPTeamMembers",
  __ssrInlineRender: true,
  props: {
    size: { default: "medium" },
    members: {}
  },
  setup(__props) {
    const props = __props;
    const classes = computed(() => [props.size, `count-${props.members.length}`]);
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: ["VPTeamMembers", classes.value]
      }, _attrs))} data-v-2fe20c75><div class="container" data-v-2fe20c75><!--[-->`);
      ssrRenderList(__props.members, (member) => {
        _push(`<div class="item" data-v-2fe20c75>`);
        _push(ssrRenderComponent(VPTeamMembersItem, {
          size: __props.size,
          member
        }, null, _parent));
        _push(`</div>`);
      });
      _push(`<!--]--></div></div>`);
    };
  }
});
const _sfc_setup$6 = _sfc_main$6.setup;
_sfc_main$6.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPTeamMembers.vue");
  return _sfc_setup$6 ? _sfc_setup$6(props, ctx) : void 0;
};
const _sfc_main$5 = {};
const _sfc_setup$5 = _sfc_main$5.setup;
_sfc_main$5.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPTeamPage.vue");
  return _sfc_setup$5 ? _sfc_setup$5(props, ctx) : void 0;
};
const _sfc_main$4 = {};
const _sfc_setup$4 = _sfc_main$4.setup;
_sfc_main$4.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPTeamPageSection.vue");
  return _sfc_setup$4 ? _sfc_setup$4(props, ctx) : void 0;
};
const _sfc_main$3 = {};
const _sfc_setup$3 = _sfc_main$3.setup;
_sfc_main$3.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add("../node_modules/.pnpm/vitepress@1.6.4_@algolia+cl_51491da4354ef93c5c1920dc2a9c4233/node_modules/vitepress/dist/client/theme-default/components/VPTeamPageTitle.vue");
  return _sfc_setup$3 ? _sfc_setup$3(props, ctx) : void 0;
};
const theme = {
  Layout,
  enhanceApp: ({ app }) => {
    app.component("Badge", _sfc_main$17);
  }
};
const _sfc_main$2 = /* @__PURE__ */ defineComponent({
  __name: "HomeHeroVisual",
  __ssrInlineRender: true,
  props: {
    locale: {}
  },
  setup(__props) {
    const props = __props;
    const isZh = computed(() => props.locale === "zh-CN");
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({
        class: "home-console",
        "aria-label": isZh.value ? "PI-Desktop 系统边界预览" : "PI-Desktop system boundary preview"
      }, _attrs))}><div class="home-console__topbar"><span class="home-console__traffic"><i></i><i></i><i></i></span><span>${ssrInterpolate(isZh.value ? "system / overview" : "system / overview")}</span><span class="home-console__status">${ssrInterpolate(isZh.value ? "LOCAL" : "LOCAL")}</span></div><div class="home-console__body"><div class="home-console__rail"><span class="home-console__rail-mark">P</span><span></span><span></span><span></span></div><div class="home-console__canvas"><div class="home-console__caption">${ssrInterpolate(isZh.value ? "RUNTIME TOPOLOGY" : "RUNTIME TOPOLOGY")}</div><div class="home-console__graph"><div class="home-console__node home-console__node--accent"><b>01</b><strong>Renderer</strong><small>presentation</small></div><span class="home-console__line home-console__line--one"></span><div class="home-console__node"><b>02</b><strong>Electron</strong><small>orchestration</small></div><span class="home-console__line home-console__line--two"></span><div class="home-console__node"><b>03</b><strong>Rust host</strong><small>authority</small></div><span class="home-console__line home-console__line--three"></span><div class="home-console__node home-console__node--state"><b>04</b><strong>SQLite</strong><small>durable state</small></div></div><div class="home-console__footer"><span>protocol v9</span><span>schema v11</span><span>inspectable by default</span></div></div></div></div>`);
    };
  }
});
const _sfc_setup$2 = _sfc_main$2.setup;
_sfc_main$2.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add(".vitepress/theme/components/HomeHeroVisual.vue");
  return _sfc_setup$2 ? _sfc_setup$2(props, ctx) : void 0;
};
const _sfc_main$1 = /* @__PURE__ */ defineComponent({
  __name: "HomeModules",
  __ssrInlineRender: true,
  props: {
    locale: {}
  },
  setup(__props) {
    const props = __props;
    const isZh = computed(() => props.locale === "zh-CN");
    const copy = computed(() => isZh.value ? {
      eyebrow: "PI-DESKTOP / DOCUMENTATION SYSTEM",
      title: "从意图进入，从边界深入。",
      intro: "这不是一份按文件堆叠的目录，而是一张能帮助你定位、理解、修改和验证系统的地图。",
      journeyLabel: "按意图阅读",
      journeyTitle: "先回答你正在解决的问题。",
      journeys: [
        { index: "01", label: "ORIENT", title: "建立产品模型", text: "先知道当前交付了什么、明确不做什么。", link: "/zh-CN/guide/" },
        { index: "02", label: "TRACE", title: "追踪一次请求", text: "从 Renderer 走到 Electron、pi、Rust host 和 SQLite。", link: "/zh-CN/spec/02-architecture/01-architecture" },
        { index: "03", label: "EXTEND", title: "开发一个插件", text: "从包格式、API、权限到生命周期逐步展开。", link: "/zh-CN/plugin-development" },
        { index: "04", label: "SHIP", title: "验证一次变化", text: "把规格、实现、E2E 场景和交付记录连起来。", link: "/zh-CN/spec/06-delivery/README" }
      ],
      mapLabel: "系统地图",
      mapTitle: "四层边界，保持一条阅读主线。",
      layers: [
        { name: "Shell", detail: "Electron", text: "桌面窗口、preload IPC、生命周期和平台能力。" },
        { name: "Agent", detail: "pi sidecar", text: "代理循环、工具调用、上下文与 provider-facing model work。" },
        { name: "Host", detail: "Rust core", text: "权限、进程、文件系统、RPC 和可取消的运行时边界。" },
        { name: "State", detail: "SQLite", text: "会话、配置、检查点、插件状态和持久化所有权。" }
      ],
      shelfLabel: "参考书架",
      shelfTitle: "按主题深入，而不是在文件树里迷路。",
      shelves: [
        { title: "产品", text: "范围、操作模式和非目标。", link: "/zh-CN/spec/01-product/README" },
        { title: "架构", text: "进程、数据和工程结构。", link: "/zh-CN/spec/02-architecture/README" },
        { title: "运行时", text: "协议、工具、host 和模型。", link: "/zh-CN/spec/03-runtime/README" },
        { title: "体验", text: "信息架构、国际化和交互。", link: "/zh-CN/spec/04-ux/README" },
        { title: "安全", text: "权限、路径和隔离边界。", link: "/zh-CN/spec/05-security/README" },
        { title: "交付", text: "验收、E2E 和发布。", link: "/zh-CN/spec/06-delivery/README" },
        { title: "插件", text: "扩展包、API 和生命周期。", link: "/zh-CN/spec/07-plugins/README" },
        { title: "决策", text: "ADR、决策日志和开放问题。", link: "/zh-CN/spec/08-meta/README" }
      ],
      snapshotLabel: "实现快照",
      snapshotTitle: "当前文档描述的版本线。",
      snapshot: [["应用版本线", "0.5.8"], ["Host wire protocol", "v9"], ["Storage schema", "v11"], ["文档源语言", "EN / 中文"]],
      footerTitle: "准备好进入代码库了吗？",
      footerText: "从快速开始建立上下文，或者直接打开规格索引。",
      footerPrimary: "开始阅读",
      footerSecondary: "查看规格索引",
      footerPrimaryLink: "/zh-CN/guide/",
      footerSecondaryLink: "/zh-CN/spec/README"
    } : {
      eyebrow: "PI-DESKTOP / DOCUMENTATION SYSTEM",
      title: "Enter by intent. Go deeper by boundary.",
      intro: "This is not a file dump. It is a map for locating, understanding, changing, and validating the system.",
      journeyLabel: "Read by intent",
      journeyTitle: "Start with the question you are solving.",
      journeys: [
        { index: "01", label: "ORIENT", title: "Build the product model", text: "Know what is shipped and what is deliberately out of scope.", link: "/guide/" },
        { index: "02", label: "TRACE", title: "Trace one request", text: "Follow the path through Renderer, Electron, pi, Rust host, and SQLite.", link: "/spec/02-architecture/01-architecture" },
        { index: "03", label: "EXTEND", title: "Build a plugin", text: "Move from package format to API, permissions, and lifecycle.", link: "/plugin-development" },
        { index: "04", label: "SHIP", title: "Validate a change", text: "Connect the spec, implementation, E2E scenario, and delivery record.", link: "/spec/06-delivery/README" }
      ],
      mapLabel: "System map",
      mapTitle: "Four boundaries. One readable thread.",
      layers: [
        { name: "Shell", detail: "Electron", text: "Windows, preload IPC, lifecycle, and platform capabilities." },
        { name: "Agent", detail: "pi sidecar", text: "Agent loop, tools, context, and provider-facing model work." },
        { name: "Host", detail: "Rust core", text: "Permissions, processes, filesystem, RPC, and cancellation." },
        { name: "State", detail: "SQLite", text: "Sessions, configuration, checkpoints, plugins, and ownership." }
      ],
      shelfLabel: "Reference shelf",
      shelfTitle: "Go deep by topic, not by getting lost in the file tree.",
      shelves: [
        { title: "Product", text: "Scope, modes, and non-goals.", link: "/spec/01-product/README" },
        { title: "Architecture", text: "Processes, data, and structure.", link: "/spec/02-architecture/README" },
        { title: "Runtime", text: "Protocols, tools, host, and models.", link: "/spec/03-runtime/README" },
        { title: "Experience", text: "IA, internationalization, and interaction.", link: "/spec/04-ux/README" },
        { title: "Security", text: "Permissions, paths, and isolation.", link: "/spec/05-security/README" },
        { title: "Delivery", text: "Acceptance, E2E, and release.", link: "/spec/06-delivery/README" },
        { title: "Plugins", text: "Packages, APIs, and lifecycle.", link: "/spec/07-plugins/README" },
        { title: "Decisions", text: "ADRs, decisions, and open questions.", link: "/spec/08-meta/README" }
      ],
      snapshotLabel: "Implementation snapshot",
      snapshotTitle: "The line this documentation describes.",
      snapshot: [["Application line", "0.5.8"], ["Host wire protocol", "v9"], ["Storage schema", "v11"], ["Source languages", "EN / 中文"]],
      footerTitle: "Ready to enter the codebase?",
      footerText: "Build context with the guide, or open the full specification index.",
      footerPrimary: "Start reading",
      footerSecondary: "Open spec index",
      footerPrimaryLink: "/guide/",
      footerSecondaryLink: "/spec/README"
    });
    return (_ctx, _push, _parent, _attrs) => {
      _push(`<div${ssrRenderAttrs(mergeProps({ class: "home-modules" }, _attrs))}><section class="home-block home-block--journeys" aria-labelledby="home-journeys-title"><div class="home-block__heading"><div><p class="home-kicker">${ssrInterpolate(copy.value.journeyLabel)}</p><h2 id="home-journeys-title">${ssrInterpolate(copy.value.journeyTitle)}</h2></div><p class="home-block__intro">${ssrInterpolate(copy.value.intro)}</p></div><div class="home-journeys"><!--[-->`);
      ssrRenderList(copy.value.journeys, (journey) => {
        _push(`<a class="home-journey"${ssrRenderAttr("href", journey.link)}><span class="home-journey__index">${ssrInterpolate(journey.index)}</span><span class="home-journey__content"><span class="home-journey__label">${ssrInterpolate(journey.label)}</span><strong>${ssrInterpolate(journey.title)}</strong><span>${ssrInterpolate(journey.text)}</span></span><span class="home-link-arrow" aria-hidden="true">↗</span></a>`);
      });
      _push(`<!--]--></div></section><section class="home-block home-block--map" aria-labelledby="home-map-title"><div class="home-block__heading home-block__heading--compact"><div><p class="home-kicker">${ssrInterpolate(copy.value.mapLabel)}</p><h2 id="home-map-title">${ssrInterpolate(copy.value.mapTitle)}</h2></div></div><div class="home-layers" role="list"><!--[-->`);
      ssrRenderList(copy.value.layers, (layer, index) => {
        _push(`<div class="home-layer" role="listitem"><span class="home-layer__number">0${ssrInterpolate(index + 1)}</span><span class="home-layer__name">${ssrInterpolate(layer.name)}</span><span class="home-layer__detail">${ssrInterpolate(layer.detail)}</span><span class="home-layer__text">${ssrInterpolate(layer.text)}</span></div>`);
      });
      _push(`<!--]--></div></section><section class="home-block home-block--shelf" aria-labelledby="home-shelf-title"><div class="home-block__heading"><div><p class="home-kicker">${ssrInterpolate(copy.value.shelfLabel)}</p><h2 id="home-shelf-title">${ssrInterpolate(copy.value.shelfTitle)}</h2></div></div><div class="home-shelf"><!--[-->`);
      ssrRenderList(copy.value.shelves, (shelf, index) => {
        _push(`<a class="home-shelf__item"${ssrRenderAttr("href", shelf.link)}><span class="home-shelf__number">${ssrInterpolate(String(index + 1).padStart(2, "0"))}</span><strong>${ssrInterpolate(shelf.title)}</strong><span>${ssrInterpolate(shelf.text)}</span><span class="home-link-arrow" aria-hidden="true">→</span></a>`);
      });
      _push(`<!--]--></div></section><section class="home-snapshot" aria-labelledby="home-snapshot-title"><div><p class="home-kicker">${ssrInterpolate(copy.value.snapshotLabel)}</p><h2 id="home-snapshot-title">${ssrInterpolate(copy.value.snapshotTitle)}</h2></div><div class="home-snapshot__grid" role="list"><!--[-->`);
      ssrRenderList(copy.value.snapshot, (item) => {
        _push(`<div role="listitem"><span>${ssrInterpolate(item[0])}</span><strong>${ssrInterpolate(item[1])}</strong></div>`);
      });
      _push(`<!--]--></div></section><section class="home-cta" aria-labelledby="home-cta-title"><div><p class="home-kicker">${ssrInterpolate(copy.value.eyebrow)}</p><h2 id="home-cta-title">${ssrInterpolate(copy.value.footerTitle)}</h2><p>${ssrInterpolate(copy.value.footerText)}</p></div><div class="home-cta__actions"><a class="home-button home-button--primary"${ssrRenderAttr("href", copy.value.footerPrimaryLink)}>${ssrInterpolate(copy.value.footerPrimary)}</a><a class="home-button home-button--secondary"${ssrRenderAttr("href", copy.value.footerSecondaryLink)}>${ssrInterpolate(copy.value.footerSecondary)}</a></div></section></div>`);
    };
  }
});
const _sfc_setup$1 = _sfc_main$1.setup;
_sfc_main$1.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add(".vitepress/theme/components/HomeModules.vue");
  return _sfc_setup$1 ? _sfc_setup$1(props, ctx) : void 0;
};
const _sfc_main = /* @__PURE__ */ defineComponent({
  __name: "Layout",
  __ssrInlineRender: true,
  setup(__props) {
    const { Layout: Layout2 } = theme;
    const { frontmatter, localeIndex } = useData$1();
    return (_ctx, _push, _parent, _attrs) => {
      _push(ssrRenderComponent(unref(Layout2), _attrs, {
        "home-hero-image": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            _push2(ssrRenderComponent(_sfc_main$2, { locale: unref(localeIndex) }, null, _parent2, _scopeId));
          } else {
            return [
              createVNode(_sfc_main$2, { locale: unref(localeIndex) }, null, 8, ["locale"])
            ];
          }
        }),
        "home-features-after": withCtx((_, _push2, _parent2, _scopeId) => {
          if (_push2) {
            if (unref(frontmatter).layout === "home") {
              _push2(ssrRenderComponent(_sfc_main$1, { locale: unref(localeIndex) }, null, _parent2, _scopeId));
            } else {
              _push2(`<!---->`);
            }
          } else {
            return [
              unref(frontmatter).layout === "home" ? (openBlock(), createBlock(_sfc_main$1, {
                key: 0,
                locale: unref(localeIndex)
              }, null, 8, ["locale"])) : createCommentVNode("", true)
            ];
          }
        }),
        _: 1
      }, _parent));
    };
  }
});
const _sfc_setup = _sfc_main.setup;
_sfc_main.setup = (props, ctx) => {
  const ssrContext = useSSRContext();
  (ssrContext.modules || (ssrContext.modules = /* @__PURE__ */ new Set())).add(".vitepress/theme/Layout.vue");
  return _sfc_setup ? _sfc_setup(props, ctx) : void 0;
};
const RawTheme = {
  extends: theme,
  Layout: _sfc_main
};
const ClientOnly = defineComponent({
  setup(_, { slots }) {
    const show = ref(false);
    onMounted(() => {
      show.value = true;
    });
    return () => show.value && slots.default ? slots.default() : null;
  }
});
function useCodeGroups() {
  if (inBrowser) {
    window.addEventListener("click", (e) => {
      var _a;
      const el = e.target;
      if (el.matches(".vp-code-group input")) {
        const group = (_a = el.parentElement) == null ? void 0 : _a.parentElement;
        if (!group)
          return;
        const i = Array.from(group.querySelectorAll("input")).indexOf(el);
        if (i < 0)
          return;
        const blocks = group.querySelector(".blocks");
        if (!blocks)
          return;
        const current = Array.from(blocks.children).find((child) => child.classList.contains("active"));
        if (!current)
          return;
        const next = blocks.children[i];
        if (!next || current === next)
          return;
        current.classList.remove("active");
        next.classList.add("active");
        const label = group == null ? void 0 : group.querySelector(`label[for="${el.id}"]`);
        label == null ? void 0 : label.scrollIntoView({ block: "nearest" });
      }
    });
  }
}
function useCopyCode() {
  if (inBrowser) {
    const timeoutIdMap = /* @__PURE__ */ new WeakMap();
    window.addEventListener("click", (e) => {
      var _a;
      const el = e.target;
      if (el.matches('div[class*="language-"] > button.copy')) {
        const parent = el.parentElement;
        const sibling = (_a = el.nextElementSibling) == null ? void 0 : _a.nextElementSibling;
        if (!parent || !sibling) {
          return;
        }
        const isShell = /language-(shellscript|shell|bash|sh|zsh)/.test(parent.className);
        const ignoredNodes = [".vp-copy-ignore", ".diff.remove"];
        const clone = sibling.cloneNode(true);
        clone.querySelectorAll(ignoredNodes.join(",")).forEach((node) => node.remove());
        let text = clone.textContent || "";
        if (isShell) {
          text = text.replace(/^ *(\$|>) /gm, "").trim();
        }
        copyToClipboard(text).then(() => {
          el.classList.add("copied");
          clearTimeout(timeoutIdMap.get(el));
          const timeoutId = setTimeout(() => {
            el.classList.remove("copied");
            el.blur();
            timeoutIdMap.delete(el);
          }, 2e3);
          timeoutIdMap.set(el, timeoutId);
        });
      }
    });
  }
}
async function copyToClipboard(text) {
  try {
    return navigator.clipboard.writeText(text);
  } catch {
    const element = document.createElement("textarea");
    const previouslyFocusedElement = document.activeElement;
    element.value = text;
    element.setAttribute("readonly", "");
    element.style.contain = "strict";
    element.style.position = "absolute";
    element.style.left = "-9999px";
    element.style.fontSize = "12pt";
    const selection = document.getSelection();
    const originalRange = selection ? selection.rangeCount > 0 && selection.getRangeAt(0) : null;
    document.body.appendChild(element);
    element.select();
    element.selectionStart = 0;
    element.selectionEnd = text.length;
    document.execCommand("copy");
    document.body.removeChild(element);
    if (originalRange) {
      selection.removeAllRanges();
      selection.addRange(originalRange);
    }
    if (previouslyFocusedElement) {
      previouslyFocusedElement.focus();
    }
  }
}
function useUpdateHead(route, siteDataByRouteRef) {
  let isFirstUpdate = true;
  let managedHeadElements = [];
  const updateHeadTags = (newTags) => {
    if (isFirstUpdate) {
      isFirstUpdate = false;
      newTags.forEach((tag) => {
        const headEl = createHeadElement(tag);
        for (const el of document.head.children) {
          if (el.isEqualNode(headEl)) {
            managedHeadElements.push(el);
            return;
          }
        }
      });
      return;
    }
    const newElements = newTags.map(createHeadElement);
    managedHeadElements.forEach((oldEl, oldIndex) => {
      const matchedIndex = newElements.findIndex((newEl) => newEl == null ? void 0 : newEl.isEqualNode(oldEl ?? null));
      if (matchedIndex !== -1) {
        delete newElements[matchedIndex];
      } else {
        oldEl == null ? void 0 : oldEl.remove();
        delete managedHeadElements[oldIndex];
      }
    });
    newElements.forEach((el) => el && document.head.appendChild(el));
    managedHeadElements = [...managedHeadElements, ...newElements].filter(Boolean);
  };
  watchEffect(() => {
    const pageData = route.data;
    const siteData2 = siteDataByRouteRef.value;
    const pageDescription = pageData && pageData.description;
    const frontmatterHead = pageData && pageData.frontmatter.head || [];
    const title = createTitle(siteData2, pageData);
    if (title !== document.title) {
      document.title = title;
    }
    const description = pageDescription || siteData2.description;
    let metaDescriptionElement = document.querySelector(`meta[name=description]`);
    if (metaDescriptionElement) {
      if (metaDescriptionElement.getAttribute("content") !== description) {
        metaDescriptionElement.setAttribute("content", description);
      }
    } else {
      createHeadElement(["meta", { name: "description", content: description }]);
    }
    updateHeadTags(mergeHead(siteData2.head, filterOutHeadDescription(frontmatterHead)));
  });
}
function createHeadElement([tag, attrs, innerHTML]) {
  const el = document.createElement(tag);
  for (const key in attrs) {
    el.setAttribute(key, attrs[key]);
  }
  if (innerHTML) {
    el.innerHTML = innerHTML;
  }
  if (tag === "script" && attrs.async == null) {
    el.async = false;
  }
  return el;
}
function isMetaDescription(headConfig) {
  return headConfig[0] === "meta" && headConfig[1] && headConfig[1].name === "description";
}
function filterOutHeadDescription(head) {
  return head.filter((h2) => !isMetaDescription(h2));
}
const hasFetched = /* @__PURE__ */ new Set();
const createLink = () => document.createElement("link");
const viaDOM = (url) => {
  const link2 = createLink();
  link2.rel = `prefetch`;
  link2.href = url;
  document.head.appendChild(link2);
};
const viaXHR = (url) => {
  const req = new XMLHttpRequest();
  req.open("GET", url, req.withCredentials = true);
  req.send();
};
let link;
const doFetch = inBrowser && (link = createLink()) && link.relList && link.relList.supports && link.relList.supports("prefetch") ? viaDOM : viaXHR;
function usePrefetch() {
  if (!inBrowser) {
    return;
  }
  if (!window.IntersectionObserver) {
    return;
  }
  let conn;
  if ((conn = navigator.connection) && (conn.saveData || /2g/.test(conn.effectiveType))) {
    return;
  }
  const rIC = window.requestIdleCallback || setTimeout;
  let observer = null;
  const observeLinks = () => {
    if (observer) {
      observer.disconnect();
    }
    observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          const link2 = entry.target;
          observer.unobserve(link2);
          const { pathname } = link2;
          if (!hasFetched.has(pathname)) {
            hasFetched.add(pathname);
            const pageChunkPath = pathToFile(pathname);
            if (pageChunkPath)
              doFetch(pageChunkPath);
          }
        }
      });
    });
    rIC(() => {
      document.querySelectorAll("#app a").forEach((link2) => {
        const { hostname, pathname } = new URL(link2.href instanceof SVGAnimatedString ? link2.href.animVal : link2.href, link2.baseURI);
        const extMatch = pathname.match(/\.\w+$/);
        if (extMatch && extMatch[0] !== ".html") {
          return;
        }
        if (
          // only prefetch same tab navigation, since a new tab will load
          // the lean js chunk instead.
          link2.target !== "_blank" && // only prefetch inbound links
          hostname === location.hostname
        ) {
          if (pathname !== location.pathname) {
            observer.observe(link2);
          } else {
            hasFetched.add(pathname);
          }
        }
      });
    });
  };
  onMounted(observeLinks);
  const route = useRoute();
  watch(() => route.path, observeLinks);
  onUnmounted(() => {
    observer && observer.disconnect();
  });
}
function resolveThemeExtends(theme2) {
  if (theme2.extends) {
    const base = resolveThemeExtends(theme2.extends);
    return {
      ...base,
      ...theme2,
      async enhanceApp(ctx) {
        if (base.enhanceApp)
          await base.enhanceApp(ctx);
        if (theme2.enhanceApp)
          await theme2.enhanceApp(ctx);
      }
    };
  }
  return theme2;
}
const Theme = resolveThemeExtends(RawTheme);
const VitePressApp = defineComponent({
  name: "VitePressApp",
  setup() {
    const { site, lang, dir } = useData$1();
    onMounted(() => {
      watchEffect(() => {
        document.documentElement.lang = lang.value;
        document.documentElement.dir = dir.value;
      });
    });
    if (site.value.router.prefetchLinks) {
      usePrefetch();
    }
    useCopyCode();
    useCodeGroups();
    if (Theme.setup)
      Theme.setup();
    return () => h(Theme.Layout);
  }
});
async function createApp() {
  globalThis.__VITEPRESS__ = true;
  const router = newRouter();
  const app = newApp();
  app.provide(RouterSymbol, router);
  const data = initData(router.route);
  app.provide(dataSymbol, data);
  app.component("Content", Content);
  app.component("ClientOnly", ClientOnly);
  Object.defineProperties(app.config.globalProperties, {
    $frontmatter: {
      get() {
        return data.frontmatter.value;
      }
    },
    $params: {
      get() {
        return data.page.value.params;
      }
    }
  });
  if (Theme.enhanceApp) {
    await Theme.enhanceApp({
      app,
      router,
      siteData: siteDataRef
    });
  }
  return { app, router, data };
}
function newApp() {
  return createSSRApp(VitePressApp);
}
function newRouter() {
  let isInitialPageLoad = inBrowser;
  return createRouter((path) => {
    let pageFilePath = pathToFile(path);
    let pageModule = null;
    if (pageFilePath) {
      if (isInitialPageLoad) {
        pageFilePath = pageFilePath.replace(/\.js$/, ".lean.js");
      }
      if (false) ;
      else {
        pageModule = import(
          /*@vite-ignore*/
          pageFilePath
        );
      }
    }
    if (inBrowser) {
      isInitialPageLoad = false;
    }
    return pageModule;
  }, Theme.NotFound);
}
if (inBrowser) {
  createApp().then(({ app, router, data }) => {
    router.go().then(() => {
      useUpdateHead(router.route, data.site);
      app.mount("#app");
    });
  });
}
async function render(path) {
  const { app, router } = await createApp();
  await router.go(path);
  const ctx = { content: "", vpSocialIcons: /* @__PURE__ */ new Set() };
  ctx.content = await renderToString(app, ctx);
  return ctx;
}
export {
  tryOnScopeDispose as a,
  useData as b,
  computedAsync as c,
  useSessionStorage as d,
  useLocalStorage as e,
  useRouter as f,
  createSearchTranslate as g,
  useEventListener as h,
  useScrollLock as i,
  dataSymbol as j,
  inBrowser as k,
  escapeRegExp as l,
  notNullish as n,
  onKeyStroke as o,
  pathToFile as p,
  render,
  toArray as t,
  unrefElement as u,
  watchDebounced as w
};
