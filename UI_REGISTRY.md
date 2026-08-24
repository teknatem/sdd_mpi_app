# UI REGISTRY

> **GENERATED file - do not edit by hand.** Source of truth is the CSS + Rust code.
> Regenerate: `powershell -File tools/gen_ui_registry.ps1`
> Factual half of the UI standard (what exists). The normative half - what is
> allowed - lives in `memory-bank/architecture/ui-standard.md`.

## Summary

| Metric | Value |
|--------|-------|
| CSS files | 48 |
| CSS lines | 21513 |
| Distinct classes | 2189 |
| Block roots | 584 |
| Classes with no Rust reference | 373 |
| Inline `style=` in .rs | 4191 |
| Hardcoded hex outside themes | 380 |
| Raw px in spacing/size props | 1832 |
| Tokens defined | 327 |
| Tokens undefined with NO fallback (broken) | 0 |
| Tokens undefined but with a fallback (dormant) | 1 |
| Tokens set by Rust at runtime | 6 |
| Tokens used but undefined (Thaw runtime) | 57 |
| Allowlist entries | 588 |

## Block roots (584)

One row per top-level BEM block. `Used` counts the block's classes that appear
in Rust (literal or `format!` stem). `Status` is the allowlist verdict.

| Block | Layer | Files | Classes | Used | Status |
|-------|-------|-------|---------|------|--------|
| `a007-link-card` | core | static/themes/core/layout.css | 12 | 9 | allowed |
| `a007-mp-list` | core | static/themes/core/layout.css | 5 | 5 | allowed |
| `a007-quick-access` | core | static/themes/core/layout.css | 2 | 2 | allowed |
| `active` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `activity-item` | core | static/themes/core/components.css | 8 | 0 | allowed / dead |
| `ai-chat-menu` | feature | static/ai_chat_menu.css | 7 | 7 | allowed |
| `alert` | core | static/themes/core/components.css | 3 | 3 | allowed |
| `app-body` | core | static/themes/core/app-shell.css | 1 | 1 | allowed |
| `app-header` | core | static/themes/core/app-shell.css | 8 | 6 | allowed |
| `app-layout` | core | static/themes/core/app-shell.css | 1 | 1 | allowed |
| `app-main` | core | static/themes/core/app-shell.css | 1 | 1 | allowed |
| `app-panel` | core | static/themes/core/app-shell.css | 8 | 3 | allowed |
| `app-panel-activity` | core | static/themes/core/app-shell.css | 7 | 0 | allowed / dead |
| `app-sidebar` | core | static/themes/core/app-shell.css | 13 | 8 | allowed |
| `app-tabs` | core | static/themes/core/app-shell.css | 3 | 3 | allowed |
| `badge` | core,feature,page | static/pages/sys_style_guide.css<br>+3 more | 14 | 12 | allowed |
| `badge-auto` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `badge-custom` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `badge-group` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `bg-secondary` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `bg-surface` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `bi-indicator-action` | core | static/themes/core/components.css | 5 | 5 | allowed |
| `bi-indicator-dataspec` | core | static/themes/core/components.css | 21 | 20 | allowed |
| `bi-indicator-filter-list` | core | static/themes/core/components.css | 4 | 4 | allowed |
| `bi-indicator-general` | core | static/themes/core/components.css | 3 | 0 | allowed / dead |
| `bi-indicator-test-result` | core | static/themes/core/components.css | 8 | 8 | allowed |
| `bi-llm-panel` | core | static/themes/core/components.css | 16 | 15 | allowed |
| `bi-preview` | core | static/themes/core/components.css | 12 | 11 | allowed |
| `bi-style-option` | core | static/themes/core/components.css | 4 | 0 | allowed / dead |
| `bi-timeline` | core | static/themes/core/components.css | 6 | 5 | allowed |
| `bi-timeline-card` | core | static/themes/core/components.css | 10 | 10 | allowed |
| `bi-timeline-chart` | core | static/themes/core/components.css | 7 | 7 | allowed |
| `bi-timeline-indicator` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `bi-viewspec` | core | static/themes/core/components.css | 7 | 5 | allowed |
| `bi-viewspec-editor` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `btn` | feature | static/plugin-sdk.css | 3 | 2 | allowed |
| `button` | core | static/themes/core/components.css<br>static/themes/core/thaw-patches.css | 9 | 8 | allowed |
| `button-group` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `card` | core,feature | static/plugin-sdk.css<br>+2 more | 3 | 3 | allowed |
| `card-animated` | core,page | static/pages/sys_tickets.css<br>static/themes/core/components.css | 18 | 18 | allowed |
| `card-body` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `card-errors` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `card-header` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `card-meta` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `card-status` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `card-title` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `card-warnings` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `cell-truncate` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `chat-more` | page | static/pages/a018_chat_workspace.css | 1 | 1 | allowed |
| `chat-questions-bar` | page | static/pages/a018_chat_workspace.css | 6 | 6 | allowed |
| `chat-tree` | page | static/pages/a018_chat_workspace.css | 11 | 9 | allowed |
| `chat-typing` | core | static/themes/core/components.css | 4 | 4 | allowed |
| `chat-workspace` | page | static/pages/a018_chat_workspace.css | 16 | 15 | allowed |
| `chat-workspace-drawer` | page | static/pages/a018_chat_workspace.css | 1 | 1 | allowed |
| `checkbox-group` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `checkbox-list` | core | static/themes/core/utilities.css | 3 | 0 | allowed / dead |
| `cm-editor` | page | static/pages/plugins.css | 1 | 0 | allowed / dead |
| `cm-focused` | page | static/pages/plugins.css | 1 | 0 | allowed / dead |
| `cm-scroller` | page | static/pages/plugins.css | 1 | 0 | allowed / dead |
| `code-box` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `code-editor` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `col-block` | core | static/themes/core/navigator.css | 1 | 1 | allowed |
| `col-desc` | core | static/themes/core/navigator.css | 1 | 1 | allowed |
| `col-funnel` | page | static/pages/a037_wb_product_snapshot.css | 1 | 1 | allowed |
| `col-funnel-controls` | page | static/pages/a037_wb_product_snapshot.css | 1 | 1 | allowed |
| `col-mp` | core | static/themes/core/navigator.css | 1 | 0 | allowed / dead |
| `col-mp-single` | core | static/themes/core/navigator.css | 1 | 1 | allowed |
| `col-name` | core | static/themes/core/navigator.css | 1 | 1 | allowed |
| `col-num` | core | static/themes/core/navigator.css | 1 | 1 | allowed |
| `col-type` | core | static/themes/core/navigator.css | 1 | 1 | allowed |
| `condition-add-btn-thaw` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `condition-checkbox` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `condition-display-with-checkbox` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `condition-editor-modal` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `condition-tab` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `condition-text-btn-active` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `condition-text-btn-inactive` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `condition-text-btn-thaw` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `config-created` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `config-description` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `config-name` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `config-picker-wrapper` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `config-updated` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `contains-tab` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `css` | core | static/themes/core/base.css<br>static/themes/core/index.css | 1 | 1 | allowed |
| `custom-date-inputs` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `d401-entity-desc` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-entity-subtitle` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-entity-title` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-fields` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-header` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-left` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-panel` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 2 | 2 | allowed |
| `d401-right` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-root` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-split` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-subtitle` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-table-scroll` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-title` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 1 | 1 | allowed |
| `d401-tree` | core | static/themes/core/dashboards/d405_metadata_dashboard.css | 12 | 9 | allowed |
| `d406-actions` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-badge` | page | static/pages/d406_wb_sales_funnel.css | 5 | 5 | allowed |
| `d406-btn` | page | static/pages/d406_wb_sales_funnel.css | 2 | 2 | allowed |
| `d406-c` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-chart-head` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-chart-sub` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-chart-title` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-chart-wrap` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-date` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-drill` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-drill-summary` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-field` | page | static/pages/d406_wb_sales_funnel.css | 2 | 1 | allowed |
| `d406-funnel-conv` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-funnel-stage-label` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-funnel-svg` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-funnel-value` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-head` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-modal` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-modal-body` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-modal-head` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-modal-overlay` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-modal-title` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-money` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-n` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-na` | page | static/pages/d406_wb_sales_funnel.css | 1 | 0 | allowed / dead |
| `d406-name` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-note` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-page` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-pager` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-pager-ctrls` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-pager-info` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-pager-page` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-pager-size` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-row` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-shell` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-sort` | page | static/pages/d406_wb_sales_funnel.css | 2 | 2 | allowed |
| `d406-state` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-tabbar` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-table` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-table-wrap` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-th` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-title` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d406-toolbar` | page | static/pages/d406_wb_sales_funnel.css | 1 | 1 | allowed |
| `d407-badge` | page | static/pages/d407_llm_quality.css | 4 | 4 | allowed |
| `d407-btn` | page | static/pages/d407_llm_quality.css | 2 | 2 | allowed |
| `d407-chip` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-chips` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-empty` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-field` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-head` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-note` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-page` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-row` | page | static/pages/d407_llm_quality.css | 2 | 2 | allowed |
| `d407-section` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-shell` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-state` | page | static/pages/d407_llm_quality.css | 2 | 2 | allowed |
| `d407-subtitle` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-table` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-tablewrap` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-td` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-th` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-tile` | page | static/pages/d407_llm_quality.css | 4 | 4 | allowed |
| `d407-tiles` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-title` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-toolbar` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `d407-verdict` | page | static/pages/d407_llm_quality.css | 6 | 6 | allowed |
| `d407-verdicts` | page | static/pages/d407_llm_quality.css | 1 | 1 | allowed |
| `dashboard-content` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `dashboard-filter` | core | static/themes/core/components.css | 7 | 0 | allowed / dead |
| `dashboard-filters` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `dashboard-mp-controls` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dashboard-period-controls` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dashboard-viewer` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `data-matrix` | core | static/themes/core/components.css | 5 | 4 | allowed |
| `data-matrix-wrapper` | core | static/themes/core/components.css | 3 | 1 | allowed |
| `datasets` | page | static/pages/sys_datasets.css | 62 | 59 | allowed |
| `data-table` | core,feature | static/plugin-sdk.css<br>static/themes/core/components.css | 6 | 6 | allowed |
| `date-input` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `date-range-picker` | core | static/themes/core/components.css | 51 | 51 | allowed |
| `delete-condition-btn` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `delta-chip` | core | static/themes/core/viz.css | 6 | 6 | allowed |
| `detail-grid` | core,page | static/pages/sys_style_guide.css<br>static/themes/core/layout.css | 2 | 2 | allowed |
| `details-flags` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `details-grid` | core | static/themes/core/components.css | 3 | 2 | allowed |
| `details-section` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `details-status-badges` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `detail-tabs` | core | static/themes/core/components.css | 3 | 3 | allowed |
| `dmc-btn` | core | static/themes/core/layout.css | 2 | 2 | allowed |
| `dmc-error` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dmc-group` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dmc-section-label` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dmc-status` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `doc-filter` | core | static/themes/core/components.css | 4 | 4 | allowed |
| `doc-filters` | core | static/themes/core/components.css | 2 | 1 | allowed |
| `doc-list` | core | static/themes/core/components.css | 4 | 0 | allowed / dead |
| `dom-inspector-badge` | page | static/pages/dom_inspector.css | 5 | 5 | allowed |
| `dom-inspector-content` | page | static/pages/dom_inspector.css | 1 | 1 | allowed |
| `dom-inspector-placeholder` | page | static/pages/dom_inspector.css | 1 | 1 | allowed |
| `dom-inspector-report` | page | static/pages/dom_inspector.css | 1 | 1 | allowed |
| `dom-inspector-row` | page | static/pages/dom_inspector.css | 3 | 3 | allowed |
| `dom-inspector-summary` | page | static/pages/dom_inspector.css | 9 | 9 | allowed |
| `dom-inspector-tab-key` | page | static/pages/dom_inspector.css | 1 | 1 | allowed |
| `dom-inspector-table` | page | static/pages/dom_inspector.css | 1 | 1 | allowed |
| `dom-tree-node` | page | static/pages/dom_inspector.css | 20 | 13 | allowed |
| `dpc-date-input` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dpc-mode-tab` | core | static/themes/core/layout.css | 2 | 2 | allowed |
| `dpc-mode-tabs` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dpc-nav-btn` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dpc-period-display` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dpc-section-label` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dpc-sep` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `dpc-slot-badge` | core | static/themes/core/layout.css | 3 | 3 | allowed |
| `dpc-slot-group` | core | static/themes/core/layout.css | 3 | 3 | allowed |
| `drill-cell` | core | static/themes/core/components.css<br>static/themes/core/fields.css | 5 | 5 | allowed |
| `drilldown-drawer` | core | static/themes/core/components.css | 2 | 0 | allowed / dead |
| `drilldown-report` | core | static/themes/core/components.css<br>static/themes/core/fields.css | 9 | 9 | allowed |
| `drill-picker` | core | static/themes/core/components.css | 10 | 10 | allowed |
| `drill-sort-icon` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `drill-th` | core | static/themes/core/components.css<br>static/themes/core/fields.css | 7 | 6 | allowed |
| `drp-btn-icon` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `drp-icon-btn` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `drp-nav-buttons` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `dsc-dim-toggle` | core | static/themes/core/components.css | 2 | 0 | allowed / dead |
| `dv-drawer` | core | static/themes/core/components.css | 4 | 3 | allowed |
| `dv-picker-card` | core | static/themes/core/components.css | 8 | 8 | allowed |
| `dv-picker-grid` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `dv-status-banner` | core | static/themes/core/components.css | 4 | 4 | allowed |
| `empty-state` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `error-banner` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `error-item` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `error-state` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `excel-importer` | core | static/themes/core/components.css | 32 | 31 | allowed |
| `ext-api` | page | static/pages/sys_tasks.css | 27 | 27 | allowed |
| `favorite-action-svg` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `favorite-checkbox` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `favorite-color` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `favorite-color-row` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `favorite-comment` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `favorite-drawer` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `favorite-drawer-overlay` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `favorite-icon-button` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `favorite-list` | core | static/themes/core/components.css | 12 | 10 | allowed |
| `favorite-modal` | core | static/themes/core/components.css | 10 | 10 | allowed |
| `favorite-scope-svg` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `favorite-star-button` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `favorite-star-svg` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `field` | feature | static/plugin-sdk.css | 2 | 1 | allowed |
| `field-db-column` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `field-flag` | core | static/themes/core/components.css | 3 | 0 | allowed / dead |
| `field-id` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `field-label` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `field-name` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `field-row` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `field-type` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `field-value` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `field-value-mono` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `field-value-mono-sm` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `field-value-nowrap` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `filter-bar` | core | static/themes/core/components.css<br>static/themes/core/fields.css | 8 | 8 | allowed |
| `filter-grid` | core | static/themes/core/layout.css | 2 | 2 | allowed |
| `filter-group` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `filter-input` | core,feature | static/themes/core/layout.css<br>static/universal_dashboard.css | 1 | 1 | allowed |
| `filter-panel` | core | static/themes/core/layout.css | 9 | 9 | allowed |
| `filter-panel-content` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `filter-panel-header` | core | static/themes/core/layout.css | 4 | 4 | allowed |
| `filter-reg` | core | static/themes/core/components.css | 9 | 9 | allowed |
| `filters` | feature | static/plugin-sdk.css | 1 | 1 | allowed |
| `filter-select` | core,feature | static/themes/core/layout.css<br>static/universal_dashboard.css | 1 | 1 | allowed |
| `filters-row` | core | static/themes/core/layout.css | 1 | 0 | allowed / dead |
| `filter-tag` | core | static/themes/core/layout.css | 2 | 2 | allowed |
| `filter-tags` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `filter-value-select` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `fixed-checkbox-column` | core | static/themes/core/thaw-patches.css | 1 | 1 | allowed |
| `font-bold` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `font-medium` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `font-semibold` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `form` | core,page | static/pages/login.css<br>+3 more | 16 | 16 | allowed |
| `form-actions-center` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `form-control` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `form-grid` | core | static/themes/core/utilities.css | 2 | 1 | allowed |
| `form-group` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `form-section` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `form-section-group` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `function-select` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `gap-lg` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `gap-md` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `gap-sm` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `general-info` | core | static/themes/core/components.css | 2 | 1 | allowed |
| `gldim-breadcrumb` | core | static/themes/core/general_ledger_dimensions.css | 2 | 2 | allowed |
| `gl-dim-chip` | core | static/themes/core/components.css<br>+2 more | 11 | 11 | allowed |
| `gl-dim-chip-empty` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `gl-dim-chip-list` | core | static/themes/core/general_ledger_dimensions.css<br>static/themes/core/layout.css | 1 | 1 | allowed |
| `gldim-copy-btn` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gldim-desc` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gldim-details` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gldim-hero` | core | static/themes/core/general_ledger_dimensions.css | 5 | 5 | allowed |
| `gl-dim-item` | core | static/themes/core/layout.css | 3 | 3 | allowed |
| `gldim-kv` | core | static/themes/core/general_ledger_dimensions.css | 4 | 4 | allowed |
| `gldim-layer` | core | static/themes/core/general_ledger_dimensions.css | 5 | 5 | allowed |
| `gldim-layers` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gldim-page` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gldim-panel` | core | static/themes/core/general_ledger_dimensions.css | 3 | 3 | allowed |
| `gldim-section` | core | static/themes/core/general_ledger_dimensions.css | 3 | 3 | allowed |
| `gl-dim-section` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `gldim-split` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gldim-system-badge` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gldim-tree` | core | static/themes/core/general_ledger_dimensions.css | 7 | 6 | allowed |
| `gldim-turnover-row` | core | static/themes/core/general_ledger_dimensions.css | 4 | 4 | allowed |
| `gl-drilldown` | core | static/themes/core/layout.css | 14 | 14 | allowed |
| `gl-entity-badge` | core | static/themes/core/layout.css | 8 | 8 | allowed |
| `gl-filter-bar` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `gl-layer-badge` | core | static/themes/core/layout.css | 8 | 8 | allowed |
| `gl-link-btn` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `gl-matrix` | page | static/pages/general_ledger_matrix.css | 24 | 24 | allowed |
| `gl-td-cols` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gl-td-hero` | core | static/themes/core/general_ledger_dimensions.css | 4 | 4 | allowed |
| `gl-td-key` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gl-td-mono` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gl-td-page` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gl-td-row` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gl-td-section` | core | static/themes/core/general_ledger_dimensions.css | 3 | 3 | allowed |
| `gl-td-val` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `gl-turnovers-table` | core | static/themes/core/general_ledger_dimensions.css | 1 | 1 | allowed |
| `grouping-order-controls` | core | static/themes/core/components.css | 3 | 3 | allowed |
| `header-center` | core | static/themes/core/layout.css | 1 | 0 | allowed / dead |
| `header-left` | core | static/themes/core/layout.css | 1 | 0 | allowed / dead |
| `header-right` | core | static/themes/core/layout.css | 1 | 0 | allowed / dead |
| `help-text` | core,feature | static/condition_editor.css<br>static/themes/core/components.css | 2 | 2 | allowed |
| `hidden` | core,page | static/pages/knowledge_base.css<br>static/themes/core/utilities.css | 1 | 1 | allowed |
| `icon-cell-container` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `import-ops` | page | static/pages/import_ops.css | 7 | 7 | allowed |
| `indicator-dashboard` | core | static/themes/core/components.css | 2 | 0 | allowed / dead |
| `indicator-detail` | core | static/themes/core/components.css | 37 | 34 | allowed |
| `indicator-detail-modal` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `indicator-refresh` | core | static/themes/core/components.css | 16 | 16 | allowed |
| `indicator-set` | core | static/themes/core/components.css | 6 | 0 | allowed / dead |
| `ind-picker` | core | static/themes/core/components.css | 23 | 22 | allowed |
| `info-box` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `info-message` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `info-popover-label` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `info-popover-portal` | core | static/themes/core/components.css | 7 | 7 | allowed |
| `input` | feature | static/plugin-sdk.css | 1 | 1 | allowed |
| `input-action-btn` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `input-actions` | core | static/themes/core/components.css | 2 | 0 | allowed / dead |
| `input-with-actions` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `invalid` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `json-boolean` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `json-key` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `json-null` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `json-number` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `json-preview` | core | static/themes/core/components.css<br>static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `json-punctuation` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `json-string` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `json-viewer` | core | static/themes/core/components.css | 4 | 0 | allowed / dead |
| `kb-meta` | page | static/pages/knowledge_base.css | 7 | 7 | allowed |
| `kb-vocabulary` | page | static/pages/knowledge_base.css | 8 | 8 | allowed |
| `kb-workspace` | page | static/pages/knowledge_base.css | 1 | 1 | allowed |
| `list-container` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `list-summary-bar` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `llm-skills` | page | static/pages/llm_skills.css | 10 | 10 | allowed |
| `llm-skills-list` | page | static/pages/llm_skills.css | 1 | 1 | allowed |
| `llm-tools` | page | static/pages/llm_tools.css | 23 | 22 | allowed |
| `llm-tools-list` | page | static/pages/llm_tools.css | 1 | 1 | allowed |
| `loading` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `loading-overlay` | core | static/themes/core/components.css | 3 | 3 | allowed |
| `loading-placeholder` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `loading-spinner` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `loading-state` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `login` | page | static/pages/login.css | 17 | 15 | allowed |
| `ltree` | core | static/themes/core/components.css | 6 | 6 | allowed |
| `ltree-btn` | core | static/themes/core/components.css | 4 | 4 | allowed |
| `ltree-cat` | core | static/themes/core/components.css | 7 | 7 | allowed |
| `ltree-cat-wrap` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `ltree-item` | core | static/themes/core/components.css | 7 | 7 | allowed |
| `ltree-items` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `ltree-section-header` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `maintenance` | page | static/pages/maintenance.css | 8 | 8 | allowed |
| `maintenance-banner` | page | static/pages/maintenance.css | 1 | 1 | allowed |
| `maintenance-line` | page | static/pages/login.css<br>static/pages/maintenance.css | 4 | 4 | allowed |
| `meta-strip` | core | static/themes/core/components.css | 8 | 8 | allowed |
| `meter` | core | static/themes/core/viz.css | 8 | 8 | allowed |
| `modal` | core,page | static/pages/sys_datasets.css<br>static/themes/core/components.css | 3 | 2 | allowed |
| `modal-actions-top` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `modal-body` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `modal-content-wide` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `modal-footer` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `modal-header` | core | static/themes/core/components.css | 2 | 1 | allowed |
| `modal-header-actions` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `modal-overlay` | core | static/themes/core/components.css | 3 | 3 | allowed |
| `modal-title` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `monospace-textarea` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `month-selector` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `mp-ms` | core | static/themes/core/components.css | 10 | 0 | allowed / dead |
| `mp-picker` | core | static/themes/core/components.css | 15 | 0 | allowed / dead |
| `muted` | feature | static/plugin-sdk.css | 1 | 1 | allowed |
| `navigator` | core | static/themes/core/navigator.css | 12 | 11 | allowed |
| `navigator-mp` | core | static/themes/core/navigator.css | 40 | 26 | allowed |
| `nav-tooltip-portal` | core | static/themes/core/navigator.css | 1 | 1 | allowed |
| `noise-pattern` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `nomenclature-picker` | core | static/themes/core/layout.css | 20 | 19 | allowed |
| `nullability-tab` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `num` | feature | static/plugin-sdk.css | 1 | 0 | allowed / dead |
| `p-0-8` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `p-6-8` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `p903-filter-note` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `p903-filter-notes` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `p907-detail` | page | static/pages/p907_ym_payment_report.css | 1 | 1 | allowed |
| `p907-ref-repr` | page | static/pages/p907_ym_payment_report.css | 1 | 1 | allowed |
| `p907-ref-value` | page | static/pages/p907_ym_payment_report.css | 1 | 1 | allowed |
| `p907-sort-header` | page | static/pages/p907_ym_payment_report.css | 1 | 1 | allowed |
| `page` | core,page | static/pages/llm_skills.css<br>+7 more | 24 | 22 | allowed |
| `page-action-button` | core,page | static/pages/sys_raw_storage.css<br>static/themes/core/layout.css | 6 | 6 | allowed |
| `page-size-select` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `pagination-btn` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `pagination-controls` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `pagination-info` | core | static/themes/core/layout.css | 1 | 1 | allowed |
| `param-index` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `param-table` | core | static/themes/core/components.css | 19 | 19 | allowed |
| `param-table-wrapper` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `param-value` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `picker` | core | static/themes/core/components.css | 16 | 14 | allowed |
| `placeholder` | core | static/themes/core/utilities.css | 2 | 1 | allowed |
| `plugin-code-editor` | page | static/pages/plugins.css | 2 | 2 | allowed |
| `plugin-data-mode` | page | static/pages/plugins.css | 7 | 6 | allowed |
| `plugin-host` | page | static/pages/plugins.css | 80 | 64 | allowed |
| `plugins-alert` | page | static/pages/plugins.css | 3 | 3 | allowed |
| `plugins-btn` | page | static/pages/plugins.css | 4 | 4 | allowed |
| `plugins-code` | page | static/pages/plugins.css | 1 | 1 | allowed |
| `plugins-dot` | page | static/pages/plugins.css | 3 | 0 | allowed / dead |
| `plugins-empty` | page | static/pages/plugins.css | 4 | 4 | allowed |
| `plugins-import` | page | static/pages/plugins.css | 2 | 2 | allowed |
| `plugins-link` | page | static/pages/plugins.css | 2 | 1 | allowed |
| `plugins-page` | page | static/pages/plugins.css | 6 | 6 | allowed |
| `plugins-server-cell` | page | static/pages/plugins.css | 1 | 1 | allowed |
| `plugins-table` | page | static/pages/plugins.css | 4 | 4 | allowed |
| `plugins-table-wrap` | page | static/pages/plugins.css | 1 | 1 | allowed |
| `preset-buttons` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `proj-detail` | core | static/themes/core/components.css | 27 | 24 | allowed |
| `proj-list-source` | core | static/themes/core/components.css | 7 | 0 | allowed / dead |
| `range-tab` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `raw-json-content` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `raw-json-header` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `raw-storage` | page | static/pages/sys_raw_storage.css | 16 | 15 | allowed |
| `record-field` | core | static/themes/core/general_ledger_details.css | 4 | 4 | allowed |
| `record-grid` | core | static/themes/core/general_ledger_details.css | 1 | 1 | allowed |
| `resizable` | core | static/themes/core/components.css<br>static/themes/core/thaw-patches.css | 1 | 1 | allowed |
| `resizing-column` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `role-cell` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `role-cell-wrapper` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `role-select` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `roles-matrix` | page | static/pages/sys_roles_matrix.css | 16 | 16 | allowed |
| `row` | feature | static/plugin-sdk.css | 1 | 1 | allowed |
| `scheduled-task-details` | core | static/themes/core/components.css | 17 | 4 | allowed |
| `scheduled-task-list` | page | static/pages/sys_tasks.css | 1 | 1 | allowed |
| `schema-browser` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-browser-content` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-browser-header` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-browser-main` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-browser-side` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-chip` | core | static/themes/core/components.css | 2 | 0 | allowed / dead |
| `schema-fields-tab` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `schema-id` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-list-header` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-name-link` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `schema-picker` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `schema-picker-label` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `schema-picker-select` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-settings-tab` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `schema-sql-tab` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `schema-table-name` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `schema-test-result` | core | static/themes/core/components.css | 7 | 0 | allowed / dead |
| `schema-test-tab` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `search-box` | core | static/themes/core/components.css | 3 | 3 | allowed |
| `section-title` | core | static/themes/core/utilities.css | 2 | 2 | allowed |
| `settings-table-wrapper` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `sg-activity` | page | static/pages/sys_style_guide.css | 4 | 4 | allowed |
| `sg-focal` | page | static/pages/sys_style_guide.css | 1 | 1 | allowed |
| `sg-item` | page | static/pages/sys_style_guide.css | 8 | 8 | allowed |
| `sg-items` | page | static/pages/sys_style_guide.css | 1 | 1 | allowed |
| `sg-login-preview` | page | static/pages/sys_style_guide.css | 2 | 2 | allowed |
| `sg-mock` | page | static/pages/sys_style_guide.css | 1 | 1 | allowed |
| `sg-mode-card` | page | static/pages/sys_style_guide.css | 7 | 7 | allowed |
| `sg-page-sub` | page | static/pages/sys_style_guide.css | 1 | 1 | allowed |
| `sg-page-title` | page | static/pages/sys_style_guide.css | 1 | 1 | allowed |
| `sg-section` | page | static/pages/sys_style_guide.css | 3 | 3 | allowed |
| `sg-swatch` | page | static/pages/sys_style_guide.css | 2 | 2 | allowed |
| `sg-type-mono` | page | static/pages/sys_style_guide.css | 1 | 1 | allowed |
| `sg-type-sans` | page | static/pages/sys_style_guide.css | 1 | 0 | allowed / dead |
| `skills-matrix` | page | static/pages/llm_skills.css | 19 | 19 | allowed |
| `sort-icon` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sparkline` | core | static/themes/core/viz.css | 8 | 8 | allowed |
| `spec-list` | core | static/themes/core/components.css | 15 | 14 | allowed |
| `spinner` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `sql-code` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `sql-content` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-display` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `sql-function` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-header-actions` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `sql-identifier` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-identifier-special` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-keyword` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-param` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-params` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-params-section` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-placeholder` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-query` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-query-section` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-section-title` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-string` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `sql-viewer-container` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `stack` | feature | static/plugin-sdk.css | 1 | 1 | allowed |
| `stat` | feature | static/plugin-sdk.css | 5 | 0 | allowed / dead |
| `stat-card` | core,page | static/pages/sys_raw_storage.css<br>static/themes/core/components.css | 13 | 6 | allowed |
| `stats` | feature | static/plugin-sdk.css | 1 | 1 | allowed |
| `stat-tile` | core | static/themes/core/viz.css | 13 | 13 | allowed |
| `status` | feature | static/plugin-sdk.css | 3 | 1 | allowed |
| `status-invalid` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `status-valid` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `summary-box` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `summary-item` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `sys-metrics` | page | static/pages/sys_metrics.css | 26 | 26 | allowed |
| `sys-processes` | page | static/pages/sys_processes.css | 46 | 46 | allowed |
| `sys-ticket-details` | page | static/pages/sys_tickets.css | 39 | 32 | allowed |
| `sys-ticket-details-page` | page | static/pages/sys_tickets.css | 1 | 1 | allowed |
| `sys-tickets` | page | static/pages/sys_tickets.css | 3 | 3 | allowed |
| `tab-content` | feature | static/condition_editor.css | 1 | 1 | allowed |
| `tab-icon` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `table` | core,page | static/pages/quality_check_details.css<br>+5 more | 33 | 21 | allowed |
| `table-wrap` | feature | static/plugin-sdk.css | 1 | 1 | allowed |
| `table-wrapper` | core,page | static/pages/sys_task_type_registry.css<br>static/themes/core/layout.css | 1 | 1 | allowed |
| `tabs-art` | core | static/themes/core/layout.css | 8 | 7 | allowed |
| `task-filter` | page | static/pages/sys_tasks.css | 4 | 4 | allowed |
| `task-type-registry` | page | static/pages/sys_task_type_registry.css | 14 | 13 | allowed |
| `td-w-10p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `td-w-14p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `td-w-15p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `td-w-20p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `td-w-35p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `td-w-46p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `test-result` | core | static/themes/core/components.css | 2 | 2 | allowed |
| `text-error` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `text-link` | core | static/themes/core/components.css | 1 | 0 | allowed / dead |
| `text-muted` | core,feature,page | static/pages/sys_datasets.css<br>+3 more | 1 | 1 | allowed |
| `text-negative` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `text-right` | core | static/themes/core/thaw-patches.css | 1 | 1 | allowed |
| `text-success` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `text-warning` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `thaw-badge` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-button` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-card` | core,page | static/pages/sys_tickets.css<br>static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-config-provider` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-input` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-tab` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-table` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-table-cell` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-table-cell-layout` | core | static/themes/core/components.css<br>static/themes/core/thaw-patches.css | 2 | 0 | allowed / dead |
| `thaw-table-header` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-table-header-cell` | core | static/themes/core/thaw-patches.css | 2 | 0 | allowed / dead |
| `thaw-table-row` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `thaw-textarea` | core | static/themes/core/thaw-patches.css | 1 | 0 | allowed / dead |
| `theme-dropdown` | core | static/themes/core/components.css | 3 | 3 | allowed |
| `theme-select-wrapper` | core | static/themes/core/components.css | 1 | 1 | allowed |
| `th-w-10p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `th-w-14p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `th-w-15p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `th-w-20p` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `th-w-35p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `th-w-46p` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `toolbar` | feature | static/plugin-sdk.css | 1 | 0 | allowed / dead |
| `tool-call` | core | static/themes/core/components.css | 19 | 19 | allowed |
| `tool-trace` | core | static/themes/core/components.css | 13 | 13 | allowed |
| `totals-row` | core | static/themes/core/utilities.css | 1 | 0 | allowed / dead |
| `u505-match` | core | static/themes/core/layout.css | 28 | 28 | allowed |
| `universal-dashboard` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `u-tech-label` | core | static/themes/core/utilities.css | 1 | 1 | allowed |
| `valid` | feature | static/universal_dashboard.css | 1 | 1 | allowed |
| `validation-card` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `validation-empty` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `validation-panel` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `validation-summary` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `warning-box` | core | static/themes/core/utilities.css | 4 | 4 | allowed |
| `warning-item` | feature | static/universal_dashboard.css | 1 | 0 | allowed / dead |
| `width-40px` | core | static/themes/core/thaw-patches.css | 1 | 1 | allowed |
| `windows-list` | core | static/themes/core/app-shell.css | 9 | 8 | allowed |

## Blocks defined in more than one file (30)

A block owned by several files has no owner. Each of these is either one
concept that must collapse onto a single definition, or a name collision.

| Block | Files | Defined in |
|-------|-------|------------|
| `page` | 8 | static/pages/llm_skills.css<br>static/pages/quality_check_details.css<br>static/pages/sys_datasets.css<br>static/pages/sys_raw_storage.css<br>static/pages/sys_tickets.css<br>static/themes/core/components.css<br>static/themes/core/layout.css<br>static/themes/core/utilities.css |
| `table` | 6 | static/pages/quality_check_details.css<br>static/pages/sys_datasets.css<br>static/pages/sys_style_guide.css<br>static/themes/core/components.css<br>static/themes/core/layout.css<br>static/themes/core/thaw-patches.css |
| `text-muted` | 4 | static/pages/sys_datasets.css<br>static/themes/core/components.css<br>static/themes/core/utilities.css<br>static/universal_dashboard.css |
| `badge` | 4 | static/pages/sys_style_guide.css<br>static/plugin-sdk.css<br>static/themes/core/components.css<br>static/themes/core/layout.css |
| `form` | 4 | static/pages/login.css<br>static/pages/sys_tickets.css<br>static/themes/core/components.css<br>static/themes/core/thaw-patches.css |
| `gl-dim-chip` | 3 | static/themes/core/components.css<br>static/themes/core/general_ledger_dimensions.css<br>static/themes/core/layout.css |
| `card` | 3 | static/plugin-sdk.css<br>static/themes/core/components.css<br>static/themes/core/layout.css |
| `hidden` | 2 | static/pages/knowledge_base.css<br>static/themes/core/utilities.css |
| `drill-th` | 2 | static/themes/core/components.css<br>static/themes/core/fields.css |
| `button` | 2 | static/themes/core/components.css<br>static/themes/core/thaw-patches.css |
| `drill-cell` | 2 | static/themes/core/components.css<br>static/themes/core/fields.css |
| `filter-select` | 2 | static/themes/core/layout.css<br>static/universal_dashboard.css |
| `help-text` | 2 | static/condition_editor.css<br>static/themes/core/components.css |
| `table-wrapper` | 2 | static/pages/sys_task_type_registry.css<br>static/themes/core/layout.css |
| `filter-bar` | 2 | static/themes/core/components.css<br>static/themes/core/fields.css |
| `card-animated` | 2 | static/pages/sys_tickets.css<br>static/themes/core/components.css |
| `thaw-table-cell-layout` | 2 | static/themes/core/components.css<br>static/themes/core/thaw-patches.css |
| `detail-grid` | 2 | static/pages/sys_style_guide.css<br>static/themes/core/layout.css |
| `drilldown-report` | 2 | static/themes/core/components.css<br>static/themes/core/fields.css |
| `thaw-card` | 2 | static/pages/sys_tickets.css<br>static/themes/core/thaw-patches.css |
| `stat-card` | 2 | static/pages/sys_raw_storage.css<br>static/themes/core/components.css |
| `resizable` | 2 | static/themes/core/components.css<br>static/themes/core/thaw-patches.css |
| `css` | 2 | static/themes/core/base.css<br>static/themes/core/index.css |
| `json-preview` | 2 | static/themes/core/components.css<br>static/themes/core/utilities.css |
| `data-table` | 2 | static/plugin-sdk.css<br>static/themes/core/components.css |
| `maintenance-line` | 2 | static/pages/login.css<br>static/pages/maintenance.css |
| `gl-dim-chip-list` | 2 | static/themes/core/general_ledger_dimensions.css<br>static/themes/core/layout.css |
| `modal` | 2 | static/pages/sys_datasets.css<br>static/themes/core/components.css |
| `filter-input` | 2 | static/themes/core/layout.css<br>static/universal_dashboard.css |
| `page-action-button` | 2 | static/pages/sys_raw_storage.css<br>static/themes/core/layout.css |

## Tokens

### Used but never defined - ours (0)

None. 

### Undefined but always carries a fallback (1)

These resolve to their `var(--x, fallback)` default, so nothing renders broken -
but the intended value never arrives. Usually a dangling hook: the CSS expects
someone to set the variable and nobody does.

| Token | Referenced from |
|-------|-----------------|
| `--nav-i` | static/themes/core/navigator.css |

### Set by Rust at runtime (6)

Written onto the element from a signal (`style:--x=..`), so they are absent from
the CSS by design. Not bugs.

`--drill-cols`, `--from-x`, `--from-y`, `--mp-color`, `--spec-cat`, `--tabs-z`

### Thaw runtime tokens (57)

Injected by the Thaw component library at runtime - undefined in our CSS by design.

`--borderRadiusLarge`, `--borderRadiusMedium`, `--borderRadiusSmall`, `--colorBrandBackground`, `--colorBrandBackground2`, `--colorBrandBackgroundHover`, `--colorBrandBackgroundPressed`, `--colorBrandForeground1`, `--colorBrandForeground2`, `--colorBrandStroke1`, `--colorBrandStroke2`, `--colorNeutralBackground1Hover`, `--colorNeutralBackground1Pressed`, `--colorNeutralBackground2`, `--colorNeutralBackground3`, `--colorNeutralBackground6`, `--colorNeutralBackgroundOverlay`, `--colorNeutralForeground1`, `--colorNeutralForeground2`, `--colorNeutralForeground3`, `--colorNeutralForegroundOnBrand`, `--colorNeutralStroke1`, `--colorNeutralStroke1Hover`, `--colorNeutralStroke2`, `--colorNeutralStroke3`, `--colorNeutralStrokeAccessible`, `--colorNeutralStrokeAccessibleHover`, `--colorPaletteRedForeground1`, `--fontFamilyMonospace`, `--fontWeightSemibold`, `--spacingHorizontalMNudge`, `--spacingHorizontalXS`, `--spacingHorizontalXXS`, `--spacingVerticalS`, `--thaw-color-brand-background-2`, `--thaw-color-brand-foreground-1`, `--thaw-color-brand-foreground-2`, `--thaw-color-brand-stroke-1`, `--thaw-color-danger-background-1`, `--thaw-color-danger-border-1`, `--thaw-color-danger-foreground-1`, `--thaw-color-neutral-background-1`, `--thaw-color-neutral-background-2`, `--thaw-color-neutral-background-3`, `--thaw-color-neutral-foreground-1`, `--thaw-color-neutral-foreground-2`, `--thaw-color-neutral-foreground-3`, `--thaw-color-neutral-stroke-1`, `--thaw-color-palette-red-background-2`, `--thaw-color-palette-red-background-3`, `--thaw-color-palette-red-border-1`, `--thaw-color-palette-red-border-2`, `--thaw-color-palette-red-foreground-1`, `--thaw-color-palette-red-foreground-2`, `--thaw-color-success-border-1`, `--thaw-color-success-foreground-1`, `--thaw-color-warning-foreground-1`

### Theme drift

A token defined by some themes but not others resolves to whatever the
previous theme left behind. Tokens carrying a base value in
`themes/core/variables.css` are excluded - there a theme file is an
override, and not overriding is a legitimate choice (e.g. the strict dark
theme deliberately skips the fancy `--glass-filter` / `--scrim-*` system).

| Theme | Tokens | Missing vs union |
|-------|--------|------------------|
| dark | 171 | 0 |
| forest | 170 | 0 |
| light | 168 | 0 |

## Hardcode by file

Colours belong in `static/themes/*/`; spacing and sizes belong in tokens.
Theme files are excluded from the hex count - defining colours is their job.

| File | Layer | Lines | Classes | Hex | Raw px |
|------|-------|-------|---------|-----|--------|
| static/themes/core/components.css | core | 7897 | 868 | 191 | 720 |
| static/themes/core/layout.css | core | 2163 | 214 | 30 | 165 |
| static/pages/plugins.css | page | 1094 | 123 | 28 | 160 |
| static/pages/d406_wb_sales_funnel.css | page | 380 | 50 | 31 | 61 |
| static/themes/core/general_ledger_dimensions.css | core | 632 | 56 | 3 | 84 |
| static/themes/core/navigator.css | core | 714 | 60 | 32 | 52 |
| static/pages/dom_inspector.css | page | 272 | 42 | 33 | 46 |
| static/pages/sys_datasets.css | page | 508 | 68 | 0 | 61 |
| static/pages/d407_llm_quality.css | page | 255 | 38 | 7 | 45 |
| static/pages/llm_tools.css | page | 216 | 24 | 1 | 50 |
| static/pages/sys_tasks.css | page | 256 | 32 | 2 | 46 |
| static/pages/a018_chat_workspace.css | page | 296 | 35 | 0 | 44 |
| static/pages/llm_skills.css | page | 230 | 31 | 0 | 42 |
| static/universal_dashboard.css | feature | 284 | 43 | 0 | 34 |
| static/pages/general_ledger_matrix.css | page | 167 | 24 | 5 | 23 |
| static/pages/sys_tickets.css | page | 694 | 48 | 6 | 20 |
| static/themes/core/app-shell.css | core | 613 | 51 | 0 | 23 |
| static/pages/sys_style_guide.css | page | 274 | 37 | 1 | 21 |
| static/pages/knowledge_base.css | page | 113 | 17 | 1 | 18 |
| static/plugin-sdk.css | feature | 315 | 28 | 0 | 19 |
| static/ai_chat_menu.css | feature | 80 | 7 | 0 | 19 |
| static/condition_editor.css | feature | 137 | 17 | 3 | 14 |
| static/pages/sys_raw_storage.css | page | 173 | 22 | 0 | 17 |
| static/pages/maintenance.css | page | 118 | 13 | 1 | 12 |
| static/themes/core/dashboards/d405_metadata_dashboard.css | core | 154 | 26 | 0 | 13 |
| static/pages/sys_roles_matrix.css | page | 122 | 16 | 0 | 9 |
| static/themes/core/fields.css | core | 80 | 11 | 3 | 6 |
| static/pages/sys_task_type_registry.css | page | 110 | 15 | 0 | 7 |
| static/pages/login.css | page | 286 | 19 | 0 | 7 |
| static/themes/core/thaw-patches.css | core | 220 | 23 | 1 | 5 |
| static/themes/core/general_ledger_details.css | core | 61 | 5 | 2 | 1 |
| static/pages/sys_processes.css | page | 351 | 46 | 0 | 2 |
| static/themes/core/utilities.css | core | 369 | 58 | 0 | 2 |
| static/pages/a037_wb_product_snapshot.css | page | 27 | 2 | 0 | 2 |
| static/pages/p907_ym_payment_report.css | page | 29 | 4 | 0 | 2 |
| static/pages/quality_check_details.css | page | 19 | 2 | 0 | 1 |

## Dead candidates

### Unlinked stylesheets

Not linked from `index.html`, not reached by any `@import`, and their filename
appears in no `.rs` or asset `.html`. Nothing loads these.

None.

### Classes with no Rust reference (373)

Conservative: a class counts as used if it appears as a whole token in any Rust
string literal, or if some `format!` stem is a prefix of it. Still verify before
deleting - a class may be referenced from an asset `.html` or a plugin bundle.

| Block | Dead classes |
|-------|--------------|
| `plugin-host` | `plugin-host__code-block` `plugin-host__code-label` `plugin-host__code-panel` `plugin-host__code-section` `plugin-host__code-toggle` `plugin-host__context` `plugin-host__context-field` `plugin-host__context-hint` `plugin-host__editor` `plugin-host__editor-block` `plugin-host__field` `plugin-host__field-label` `plugin-host__hidden` `plugin-host__input` `plugin-host__pane--frame` `plugin-host__run--active` |
| `mp-picker` | `mp-picker` `mp-picker__checkbox` `mp-picker__clear-btn` `mp-picker__error` `mp-picker__footer` `mp-picker__label` `mp-picker__list` `mp-picker__list-empty` `mp-picker__loading` `mp-picker__row` `mp-picker__row--selected` `mp-picker__search` `mp-picker__summary` `mp-picker__summary-text` `mp-picker__summary-text--active` |
| `navigator-mp` | `navigator-mp__badge` `navigator-mp__badge--aggregate` `navigator-mp__badge--projection` `navigator-mp__badge--usecase` `navigator-mp__card-meta` `navigator-mp__card-tabkey` `navigator-mp__cell--empty` `navigator-mp__chip` `navigator-mp__mp-dash` `navigator-mp__mp-link` `navigator-mp__sort-indicator--active` `navigator-mp__td--mp` `navigator-mp__th--center` `navigator-mp__th-mp-logo` |
| `scheduled-task-details` | `scheduled-task-details__checkbox-row` `scheduled-task-details__delete-btn` `scheduled-task-details__grid` `scheduled-task-details__header` `scheduled-task-details__json-body` `scheduled-task-details__logs` `scheduled-task-details__progress` `scheduled-task-details__progress-bar` `scheduled-task-details__progress-bar-fill` `scheduled-task-details__progress-current` `scheduled-task-details__progress-meta` `scheduled-task-details__progress-title` `scheduled-task-details__title` |
| `table` | `table__cell--highlight` `table__cell--highlight-alt` `table__cell--sticky` `table__row--cancelled` `table__row--selected` `table__row--warning` `table__sort-icon` `table__sort-icon--active` `table__totals-header` `table__tree-label` `table__tree-placeholder` `table__tree-toggle` |
| `mp-ms` | `mp-ms` `mp-ms__badge` `mp-ms__badge--all` `mp-ms__badges` `mp-ms__badge--selected` `mp-ms__clear` `mp-ms__state` `mp-ms__state--error` `mp-ms__summary` `mp-ms__toolbar` |
| `activity-item` | `activity-item` `activity-item__details` `activity-item__icon` `activity-item__icon--error` `activity-item__icon--success` `activity-item__icon--warning` `activity-item__text` `activity-item__time` |
| `stat-card` | `stat-card__change` `stat-card__change--down` `stat-card__change--flat` `stat-card__change--up` `stat-card__subtitle` `stat-card--error` `stat-card--success` |
| `app-panel-activity` | `app-panel-activity` `app-panel-activity__content` `app-panel-activity__header` `app-panel-activity__section` `app-panel-activity__section-body` `app-panel-activity__section-header` `app-panel-activity__title` |
| `proj-list-source` | `proj-list-source` `proj-list-source__empty` `proj-list-source__main` `proj-list-source__meta` `proj-list-source__role` `proj-list-source__row` `proj-list-source__title` |
| `dashboard-filter` | `dashboard-filter` `dashboard-filter__checkboxes` `dashboard-filter__checkbox-label` `dashboard-filter__checkbox-row` `dashboard-filter__label` `dashboard-filter--daterange` `dashboard-filter--multiselect` |
| `dom-tree-node` | `dom-tree-node__class--special` `dom-tree-node__data-attr--hidden` `dom-tree-node__header--clickable` `dom-tree-node__tag--page` `dom-tree-node__tag--panel-left` `dom-tree-node__tag--right-panel` `dom-tree-node__tag--table` |
| `schema-test-result` | `schema-test-result` `schema-test-result__content` `schema-test-result__rows` `schema-test-result__status` `schema-test-result__time` `schema-test-result--error` `schema-test-result--success` |
| `sys-ticket-details` | `sys-ticket-details__attachment-item--linked` `sys-ticket-details__attachment-item--selected` `sys-ticket-details__attachment-name--image` `sys-ticket-details__attachments-list--dragover` `sys-ticket-details__comment-attachments` `sys-ticket-details__comment--linked` `sys-ticket-details__comment--selected` |
| `indicator-set` | `indicator-set` `indicator-set__grid` `indicator-set__grid--cols-2` `indicator-set__grid--cols-3` `indicator-set__grid--cols-4` `indicator-set__title` |
| `app-panel` | `app-panel__info-item` `app-panel__info-label` `app-panel__info-value` `app-panel--hidden` `app-panel--resizing` |
| `stat` | `stat` `stat__label` `stat__value` `stat--bad` `stat--ok` |
| `app-sidebar` | `app-sidebar__chevron--expanded` `app-sidebar__collapse--open` `app-sidebar__header` `app-sidebar__item--active` `app-sidebar__title` |
| `doc-list` | `doc-list__progress` `doc-list__summary` `doc-list__title` `doc-list__toolbar` |
| `bi-style-option` | `bi-style-option` `bi-style-option__desc` `bi-style-option__label` `bi-style-option--selected` |
| `json-viewer` | `json-viewer` `json-viewer__body` `json-viewer__content` `json-viewer__footer` |
| `checkbox-list` | `checkbox-list` `checkbox-list__item` `checkbox-list__item--mono` |
| `bi-indicator-general` | `bi-indicator-general__group--checkbox` `bi-indicator-general__group--full` `bi-indicator-general__group--wide` |
| `d401-tree` | `d401-tree` `d401-tree__btn--active` `d401-tree__toggle--disabled` |
| `proj-detail` | `proj-detail__section--full` `proj-detail__source-item--muted` `proj-detail__value--muted` |
| `plugins-dot` | `plugins-dot` `plugins-dot--off` `plugins-dot--on` |
| `a007-link-card` | `a007-link-card__status` `a007-link-card__status--empty` `a007-link-card__status--linked` |
| `field-flag` | `field-flag` `field-flag--no` `field-flag--yes` |
| `datasets` | `datasets__job-bar--pulsing` `datasets__row--disabled` `datasets__snapshot--own` |
| `indicator-detail` | `indicator-detail__about` `indicator-detail__value` `indicator-detail__value-row` |
| `badge` | `badge__icon` `badge__text` |
| `drilldown-drawer` | `drilldown-drawer__filters` `drilldown-drawer__footer` |
| `data-matrix-wrapper` | `data-matrix-wrapper--framed` `data-matrix-wrapper--tall` |
| `bi-viewspec` | `bi-viewspec__preview-frame` `bi-viewspec__style-grid` |
| `favorite-list` | `favorite-list__editor` `favorite-list__editor-actions` |
| `login` | `login__footer-text` `login--maintenance` |
| `chat-tree` | `chat-tree__dir--active` `chat-tree__row--open` |
| `picker` | `picker__item--selected` `picker__row--selected` |
| `thaw-table-cell-layout` | `thaw-table-cell-layout` `thaw-table-cell-layout__content` |
| `page` | `page__tab--active` `page--narrow` |
| `app-header` | `app-header__center` `app-header__icon-btn` |
| `dsc-dim-toggle` | `dsc-dim-toggle` `dsc-dim-toggle--active` |
| `indicator-dashboard` | `indicator-dashboard__filters` `indicator-dashboard__sets` |
| `input-actions` | `input-actions` `input-actions--single` |
| `schema-chip` | `schema-chip` `schema-chip--active` |
| `thaw-table-header-cell` | `thaw-table-header-cell` `thaw-table-header-cell__button` |
| `status` | `status--error` `status--ok` |
| `cm-scroller` | `cm-scroller` |
| `ind-picker` | `ind-picker__row--already` |
| `bi-llm-panel` | `bi-llm-panel--open` |
| `field-id` | `field-id` |
| `bg-secondary` | `bg-secondary` |
| `thaw-table-header` | `thaw-table-header` |
| `form-grid` | `form-grid--simple` |
| `chat-workspace` | `chat-workspace__plan-step--done` |
| `schema-fields-tab` | `schema-fields-tab` |
| `function-select` | `function-select` |
| `summary-item` | `summary-item` |
| `gap-md` | `gap-md` |
| `num` | `num` |
| `schema-table-name` | `schema-table-name` |
| `json-preview` | `json-preview` |
| `task-type-registry` | `task-type-registry__chevron--expanded` |
| `th-w-14p` | `th-w-14p` |
| `cm-editor` | `cm-editor` |
| `th-w-15p` | `th-w-15p` |
| `schema-browser-main` | `schema-browser-main` |
| `thaw-config-provider` | `thaw-config-provider` |
| `bi-indicator-dataspec` | `bi-indicator-dataspec__stats` |
| `input-with-actions` | `input-with-actions` |
| `schema-browser-side` | `schema-browser-side` |
| `validation-summary` | `validation-summary` |
| `schema-browser-content` | `schema-browser-content` |
| `validation-card` | `validation-card` |
| `d406-field` | `d406-field--grow` |
| `raw-json-content` | `raw-json-content` |
| `card-body` | `card-body` |
| `schema-browser` | `schema-browser` |
| `card-title` | `card-title` |
| `td-w-14p` | `td-w-14p` |
| `th-w-46p` | `th-w-46p` |
| `card-header` | `card-header` |
| `raw-json-header` | `raw-json-header` |
| `col-mp` | `col-mp` |
| `error-banner` | `error-banner` |
| `schema-id` | `schema-id` |
| `thaw-button` | `thaw-button` |
| `thaw-table-row` | `thaw-table-row` |
| `drill-th` | `drill-th--sortable` |
| `schema-sql-tab` | `schema-sql-tab` |
| `header-left` | `header-left` |
| `badge-group` | `badge-group` |
| `error-item` | `error-item` |
| `field-row` | `field-row` |
| `field` | `field__label` |
| `validation-panel` | `validation-panel` |
| `schema-browser-header` | `schema-browser-header` |
| `placeholder` | `placeholder__title` |
| `llm-tools` | `llm-tools__row--active` |
| `text-warning` | `text-warning` |
| `card-errors` | `card-errors` |
| `field-name` | `field-name` |
| `badge-auto` | `badge-auto` |
| `config-created` | `config-created` |
| `validation-empty` | `validation-empty` |
| `bi-timeline` | `bi-timeline__panel` |
| `thaw-table` | `thaw-table` |
| `spec-list` | `spec-list--compact` |
| `doc-filters` | `doc-filters` |
| `text-negative` | `text-negative` |
| `thaw-card` | `thaw-card` |
| `gap-lg` | `gap-lg` |
| `badge-custom` | `badge-custom` |
| `header-right` | `header-right` |
| `form-actions-center` | `form-actions-center` |
| `thaw-input` | `thaw-input__input` |
| `date-input` | `date-input` |
| `card-warnings` | `card-warnings` |
| `bg-surface` | `bg-surface` |
| `thaw-tab` | `thaw-tab` |
| `schema-test-tab` | `schema-test-tab` |
| `modal-actions-top` | `modal-actions-top` |
| `filters-row` | `filters-row` |
| `schema-picker-select` | `schema-picker-select` |
| `raw-storage` | `raw-storage__list-value--warn` |
| `config-updated` | `config-updated` |
| `sql-header-actions` | `sql-header-actions` |
| `totals-row` | `totals-row` |
| `noise-pattern` | `noise-pattern` |
| `windows-list` | `windows-list__item--active` |
| `td-w-35p` | `td-w-35p` |
| `field-type` | `field-type` |
| `status-invalid` | `status-invalid` |
| `modal-header` | `modal-header__left` |
| `card-meta` | `card-meta` |
| `tabs-art` | `tabs-art__tab--hover` |
| `schema-name-link` | `schema-name-link` |
| `excel-importer` | `excel-importer__import-btn` |
| `warning-item` | `warning-item` |
| `text-error` | `text-error` |
| `thaw-textarea` | `thaw-textarea__textarea` |
| `td-w-15p` | `td-w-15p` |
| `details-grid` | `details-grid--2col` |
| `toolbar` | `toolbar` |
| `dv-drawer` | `dv-drawer__list` |
| `thaw-badge` | `thaw-badge` |
| `button` | `button--info` |
| `config-name` | `config-name` |
| `sg-type-sans` | `sg-type-sans` |
| `header-center` | `header-center` |
| `plugins-link` | `plugins-link--muted` |
| `bi-viewspec-editor` | `bi-viewspec-editor` |
| `sql-code` | `sql-code` |
| `font-semibold` | `font-semibold` |
| `status-valid` | `status-valid` |
| `sql-display` | `sql-display` |
| `loading-placeholder` | `loading-placeholder` |
| `td-w-46p` | `td-w-46p` |
| `btn` | `btn--ghost` |
| `nomenclature-picker` | `nomenclature-picker__row--selected` |
| `summary-box` | `summary-box` |
| `font-bold` | `font-bold` |
| `schema-list-header` | `schema-list-header` |
| `text-link` | `text-link` |
| `navigator` | `navigator__view-btn--active` |
| `schema-settings-tab` | `schema-settings-tab` |
| `th-w-35p` | `th-w-35p` |
| `th-w-10p` | `th-w-10p` |
| `input-action-btn` | `input-action-btn` |
| `general-info` | `general-info__grid` |
| `thaw-table-cell` | `thaw-table-cell` |
| `config-description` | `config-description` |
| `text-success` | `text-success` |
| `role-select` | `role-select` |
| `td-w-20p` | `td-w-20p` |
| `td-w-10p` | `td-w-10p` |
| `gap-sm` | `gap-sm` |
| `gldim-tree` | `gldim-tree__btn--active` |
| `month-selector` | `month-selector` |
| `cm-focused` | `cm-focused` |
| `data-matrix` | `data-matrix__num` |
| `dashboard-filters` | `dashboard-filters` |
| `card-status` | `card-status` |
| `bi-preview` | `bi-preview__empty-hint` |
| `plugin-data-mode` | `plugin-data-mode__button--active` |
| `info-message` | `info-message` |
| `field-db-column` | `field-db-column` |
| `d406-na` | `d406-na` |
| `modal` | `modal__actions` |

