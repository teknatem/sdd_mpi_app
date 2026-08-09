pub mod d400_monthly_summary;
pub mod d401_wb_finance;
pub mod d402_wb_order_flow;
pub mod d403_ym_order_flow;
pub mod d404_wb_advert_report;
pub mod d405_metadata_dashboard;
pub mod d406_wb_sales_funnel;
pub mod d407_llm_quality;

pub use d400_monthly_summary::ui::MonthlySummaryDashboard;
pub use d401_wb_finance::ui::D401WbFinanceDashboard;
pub use d402_wb_order_flow::ui::WbOrderFlowDashboard;
pub use d403_ym_order_flow::ui::YmOrderFlowDashboard;
pub use d404_wb_advert_report::ui::WbAdvertReportDashboard;
pub use d405_metadata_dashboard::ui::MetadataDashboard;
pub use d406_wb_sales_funnel::ui::WbSalesFunnelDashboard;
pub use d407_llm_quality::ui::LlmQualityDashboard;
