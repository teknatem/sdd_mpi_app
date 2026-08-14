# Tab Registry Reference

> Источник правды: `crates/frontend/src/layout/tabs/registry.rs`
> Обновлять этот файл при добавлении новых табов.

## Соглашение по ключам


| Тип страницы          | Формат ключа                                                                | Пример                          |
| --------------------- | --------------------------------------------------------------------------- | ------------------------------- |
| Список                | `{index}_{name}`                                                            | `a001_connection_1c`            |
| Детали (существующий) | `{index}_{name}_details_{id}`                                               | `a012_wb_sales_details_UUID`    |
| Детали (новый)        | `{index}_{name}_details_new` или `{index}_{name}_new`                       | `a024_bi_indicator_details_new` |
| General Ledger        | `general_ledger`, `general_ledger_turnovers`, `general_ledger_details_{id}` | —                               |
| Проекции              | `p9NN_{name}`                                                               | `p904_sales_data`               |
| Использование кейсов  | `u5NN_{name}`                                                               | `u501_import_from_ut`           |
| Дашборды              | `d4NN_{name}`                                                               | `d400_monthly_summary`          |
| Система               | `sys_{name}`                                                                | `sys_users`, `sys_roles`        |
| DataView              | `data_view`, `data_view_details_{view_id}`                                  | —                               |
| Drilldown             | `drilldown__{session_id}` или `drilldown__new`                              | —                               |


## Все зарегистрированные табы

### Domain Aggregates


| Ключ                                    | Страница                          |
| --------------------------------------- | --------------------------------- |
| `a001_connection_1c`                    | Connection1CList                  |
| `a002_organization`                     | OrganizationList                  |
| `a002_organization_details_{id}`        | OrganizationDetails               |
| `a003_counterparty`                     | CounterpartyTree                  |
| `a004_nomenclature`                     | NomenclatureTree                  |
| `a004_nomenclature_list`                | NomenclatureList                  |
| `a004_nomenclature_details_{id}`        | NomenclatureDetails               |
| `a005_marketplace`                      | MarketplaceList                   |
| `a005_marketplace_details_{id}`         | MarketplaceDetails                |
| `a006_connection_mp`                    | ConnectionMPList                  |
| `a006_connection_mp_details`            | ConnectionMPDetail (новый)        |
| `a006_connection_mp_details_{id}`       | ConnectionMPDetail                |
| `a007_marketplace_product`              | MarketplaceProductList            |
| `a007_marketplace_product_details_{id}` | MarketplaceProductDetails         |
| `a007_marketplace_product_new`          | MarketplaceProductDetails (новый) |
| `a008_marketplace_sales`                | MarketplaceSalesList              |
| `a009_ozon_returns`                     | OzonReturnsList                   |
| `a009_ozon_returns_details_{id}`        | OzonReturnsDetail                 |
| `a010_ozon_fbs_posting`                 | OzonFbsPostingList                |
| `a011_ozon_fbo_posting`                 | OzonFboPostingList                |
| `a012_wb_sales`                         | WbSalesList                       |
| `a012_wb_sales_details_{id}`            | WbSalesDetail                     |
| `a013_ym_order`                         | YmOrderList                       |
| `a013_ym_order_details_{id}`            | YmOrderDetail                     |
| `a014_ozon_transactions`                | OzonTransactionsList              |
| `a014_ozon_transactions_details_{id}`   | OzonTransactionsDetail            |
| `a015_wb_orders`                        | WbOrdersList                      |
| `a015_wb_orders_details_{id}`           | WbOrdersDetails                   |
| `a016_ym_returns`                       | YmReturnsList                     |
| `a016_ym_returns_details_{id}`          | YmReturnDetail                    |
| `a017_llm_agent`                        | LlmAgentList                      |
| `a018_llm_chat`                         | LlmChatList                       |
| `a018_llm_chat_details_{id}`            | LlmChatDetails                    |
| `a019_llm_artifact`                     | LlmArtifactList                   |
| `a019_llm_artifact_details_{id}`        | LlmArtifactDetails                |
| `a020_wb_promotion`                     | WbPromotionList                   |
| `a020_wb_promotion_details_{id}`        | WbPromotionDetail                 |
| `a021_production_output`                | ProductionOutputList              |
| `a021_production_output_details_{id}`   | ProductionOutputDetail            |
| `a022_kit_variant`                      | KitVariantList                    |
| `a022_kit_variant_details_{id}`         | KitVariantDetail                  |
| `a023_purchase_of_goods`                | PurchaseOfGoodsList               |
| `a023_purchase_of_goods_details_{id}`   | PurchaseOfGoodsDetail             |
| `a024_bi_indicator`                     | BiIndicatorList                   |
| `a024_bi_indicator_details_{id|new}`    | BiIndicatorDetails                |
| `a025_bi_dashboard`                     | BiDashboardList                   |
| `a025_bi_dashboard_details_{id|new}`    | BiDashboardDetails                |
| `a025_bi_dashboard_view_{id}`           | BiDashboardView                   |
| `a026_wb_advert_daily`                  | WbAdvertDailyList                 |
| `a026_wb_advert_daily_details_{id}`     | WbAdvertDailyDetail               |


### General Ledger (независимая система)


| Ключ                          | Страница                   |
| ----------------------------- | -------------------------- |
| `general_ledger`              | GeneralLedgerPage          |
| `general_ledger_turnovers`    | GeneralLedgerTurnoversPage |
| `general_ledger_details_{id}` | GeneralLedgerDetailsPage   |


### Use Cases


| Ключ                           | Страница                     |
| ------------------------------ | ---------------------------- |
| `u501_import_from_ut`          | ImportWidget (1С)            |
| `u502_import_from_ozon`        | ImportWidget (Ozon)          |
| `u503_import_from_yandex`      | ImportWidget (Яндекс.Маркет) |
| `u504_import_from_wildberries` | ImportWidget (WB)            |
| `u505_match_nomenclature`      | MatchNomenclatureView        |
| `u506_import_from_lemanapro`   | ImportWidget (LemanaPro)     |
| `u507_import_from_erp`         | ImportWidget (ERP)           |
| `u508_repost_documents`        | RepostDocumentsWidget        |


### Projections


| Ключ                                     | Страница                         |
| ---------------------------------------- | -------------------------------- |
| `p900_sales_register`                    | SalesRegisterList                |
| `p901_barcodes`                          | BarcodesList                     |
| `p902_ozon_finance_realization`          | OzonFinanceRealizationList       |
| `p903_wb_finance_report`                 | WbFinanceReportList              |
| `p903_wb_finance_report_details_id_{id}` | WbFinanceReportDetail            |
| `p904_sales_data`                        | SalesDataList                    |
| `p905_commission_history`                | CommissionHistoryList            |
| `p905-commission/{id}`                   | CommissionHistoryDetails         |
| `p905-commission-new`                    | CommissionHistoryDetails (новый) |
| `p906_nomenclature_prices`               | NomenclaturePricesList           |
| `p907_ym_payment_report`                 | YmPaymentReportList              |
| `p907_ym_payment_report_details_{key}`   | YmPaymentReportDetail            |
| `p908_wb_goods_prices`                   | WbGoodsPricesList                |


### DataView


| Ключ                          | Страница                           |
| ----------------------------- | ---------------------------------- |
| `data_view`                   | DataViewList                       |
| `filter_registry`             | FilterRegistryPage                 |
| `data_view_details_{view_id}` | DataViewDetail                     |
| `drilldown__new`              | DrilldownReportPage (ручной режим) |
| `drilldown__{session_id}`     | DrilldownReportPage (сессия)       |


### Дашборды


| Ключ                                                          | Страница                         |
| ------------------------------------------------------------- | -------------------------------- |
| `d400_monthly_summary`                                        | MonthlySummaryDashboard          |
| `d401_metadata_dashboard`                                     | MetadataDashboard                |
| `d401_wb_finance`                                             | D401WbFinanceDashboard (legacy)  |
| `universal_dashboard`                                         | UniversalDashboard               |
| `schema_browser`                                              | SchemaBrowser                    |
| `all_reports`                                                 | AllReportsList                   |
| `all_reports_details_{config_id}`                             | AllReportsDetails                |
| `schema_details_{schema_id}`                                  | SchemaDetails                    |
| `universal_dashboard_report_{uuid}__{schema_id}__{config_id}` | UniversalDashboard с параметрами |


### Система


| Ключ                    | Страница                     |
| ----------------------- | ---------------------------- |
| `sys_users`             | UsersListPage                |
| `sys_user_new`          | CreateUserPage               |
| `sys_user_details_{id}` | UserDetailsPage              |
| `sys_roles`             | RolesListPage                |
| `sys_roles_matrix`      | RoleMatrixPage               |
| `sys_role_details_{id}` | RoleDetailsPage              |
| `sys_tasks`             | ScheduledTaskList            |
| `sys_task_details`      | ScheduledTaskDetails (новый) |
| `sys_task_details_{id}` | ScheduledTaskDetails         |
| `sys_thaw_test`         | ThawTestPage                 |
| `dom_inspector`         | DomInspectorPage             |


## Как добавить новый таб

1. Создать компонент в `crates/frontend/src/domain/aNNN_xxx/ui/list/mod.rs` (или `ui/details/mod.rs`)
2. В `registry.rs` добавить import в начало файла
3. В `registry.rs` добавить match-arm в нужную секцию:
  ```rust
   "aNNN_xxx" => view! { <XxxList /> }.into_any(),
   k if k.starts_with("aNNN_xxx_details_") => {
       let id = k.strip_prefix("aNNN_xxx_details_").unwrap().to_string();
       view! { <XxxDetails id=id on_close=... /> }.into_any()
   }
  ```
4. В меню навигации (sidebar/nav) добавить кнопку которая вызывает `tabs_store.open_tab("aNNN_xxx")`

