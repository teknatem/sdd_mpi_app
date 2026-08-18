//! dv006 - DataView: ratio of two BI indicators.
//!
//! Required params:
//!   numerator_indicator_code   = code of the numerator indicator
//!   denominator_indicator_code = code of the denominator indicator
//! Optional params:
//!   metric = ratio_percent (default) | ratio | ratio_percent_complement

use anyhow::{anyhow, Result};
use contracts::shared::analytics::{
    IndicatorContext, IndicatorId, IndicatorStatus, IndicatorValue,
};
use contracts::shared::data_view::ViewContext;
use contracts::shared::drilldown::DrilldownResponse;

use crate::domain::a024_bi_indicator::repository;
use crate::shared::data::db::get_connection;

const VIEW_ID: &str = "dv006_indicator_ratio_percent";
const DEFAULT_METRIC_ID: &str = "ratio_percent";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMetric {
    Ratio,
    RatioPercent,
    RatioPercentComplement,
}

fn resolve_metric(ctx: &ViewContext) -> Result<ViewMetric> {
    let metric = ctx
        .params
        .get("metric")
        .map(String::as_str)
        .unwrap_or(DEFAULT_METRIC_ID);
    match metric {
        "ratio" => Ok(ViewMetric::Ratio),
        "ratio_percent" => Ok(ViewMetric::RatioPercent),
        "ratio_percent_complement" => Ok(ViewMetric::RatioPercentComplement),
        _ => Err(anyhow!(
            "Unsupported metric '{}' for {}. Expected 'ratio' | 'ratio_percent' | 'ratio_percent_complement'",
            metric,
            VIEW_ID
        )),
    }
}

fn required_param<'a>(ctx: &'a ViewContext, key: &str) -> Result<&'a str> {
    ctx.params
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Missing required param '{}' for {}", key, VIEW_ID))
}

fn ratio_value(
    numerator: Option<f64>,
    denominator: Option<f64>,
    metric: ViewMetric,
) -> Option<f64> {
    match (numerator, denominator) {
        (Some(numerator), Some(denominator)) if denominator.abs() >= 0.000_001 => {
            let ratio = numerator / denominator;
            Some(match metric {
                ViewMetric::Ratio => ratio,
                ViewMetric::RatioPercent => ratio * 100.0,
                ViewMetric::RatioPercentComplement => 100.0 - (ratio * 100.0),
            })
        }
        _ => None,
    }
}

fn pct_change(cur: Option<f64>, prev: Option<f64>) -> Option<f64> {
    match (cur, prev) {
        (Some(cur), Some(prev)) if prev.abs() >= 0.01 => Some(((cur - prev) / prev.abs()) * 100.0),
        _ => None,
    }
}

fn build_child_ctx(ctx: &ViewContext) -> IndicatorContext {
    let mut extra = ctx.params.clone();
    extra.remove("metric");
    extra.remove("numerator_indicator_code");
    extra.remove("denominator_indicator_code");

    if let Some(period2_from) = &ctx.period2_from {
        extra.insert("period2_from".to_string(), period2_from.clone());
    }
    if let Some(period2_to) = &ctx.period2_to {
        extra.insert("period2_to".to_string(), period2_to.clone());
    }

    IndicatorContext {
        date_from: ctx.date_from.clone(),
        date_to: ctx.date_to.clone(),
        organization_ref: None,
        marketplace: None,
        connection_mp_refs: ctx.connection_mp_refs.clone(),
        extra,
    }
}

fn build_spark_points(numerator: &[f64], denominator: &[f64], metric: ViewMetric) -> Vec<f64> {
    numerator
        .iter()
        .zip(denominator.iter())
        .filter_map(|(num, den)| ratio_value(Some(*num), Some(*den), metric))
        .collect()
}

fn format_value(value: Option<f64>) -> String {
    match value {
        Some(value) if (value.round() - value).abs() < 0.000_001 => format!("{}", value as i64),
        Some(value) => format!("{value:.2}"),
        None => "—".to_string(),
    }
}

fn display_indicator_name(
    indicator: Option<&contracts::domain::a024_bi_indicator::aggregate::BiIndicator>,
    code: &str,
) -> String {
    indicator
        .map(|item| item.base.description.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| code.to_string())
}

fn formula_text(metric: ViewMetric) -> &'static str {
    match metric {
        ViewMetric::Ratio => "Формула: числитель / знаменатель",
        ViewMetric::RatioPercent => "Формула: числитель / знаменатель * 100",
        ViewMetric::RatioPercentComplement => "Формула: 100 - (числитель / знаменатель * 100)",
    }
}

pub async fn compute_scalar(ctx: &ViewContext) -> Result<IndicatorValue> {
    let metric = resolve_metric(ctx)?;
    let numerator_code = required_param(ctx, "numerator_indicator_code")?.to_string();
    let denominator_code = required_param(ctx, "denominator_indicator_code")?.to_string();
    let child_ctx = build_child_ctx(ctx);
    let db = get_connection();

    let (numerator_result, denominator_result, numerator_def_result, denominator_def_result) = tokio::join!(
        crate::domain::a024_bi_indicator::service::compute_indicator_by_code(
            &numerator_code,
            &child_ctx
        ),
        crate::domain::a024_bi_indicator::service::compute_indicator_by_code(
            &denominator_code,
            &child_ctx
        ),
        repository::find_by_code(db, &numerator_code),
        repository::find_by_code(db, &denominator_code),
    );

    let numerator = numerator_result?;
    let denominator = denominator_result?;
    let numerator_def = numerator_def_result?;
    let denominator_def = denominator_def_result?;
    let current = ratio_value(numerator.value, denominator.value, metric);
    let previous = ratio_value(numerator.previous_value, denominator.previous_value, metric);
    let details = vec![
        format!(
            "Числитель: {} ({}) = {}",
            display_indicator_name(numerator_def.as_ref(), &numerator_code),
            numerator_code,
            format_value(numerator.value)
        ),
        format!(
            "Знаменатель: {} ({}) = {}",
            display_indicator_name(denominator_def.as_ref(), &denominator_code),
            denominator_code,
            format_value(denominator.value)
        ),
        formula_text(metric).to_string(),
    ];

    Ok(IndicatorValue {
        id: IndicatorId::new(VIEW_ID),
        value: current,
        previous_value: previous,
        change_percent: pct_change(current, previous),
        status: IndicatorStatus::Neutral,
        subtitle: Some(format!("{numerator_code} / {denominator_code}")),
        details,
        spark_points: build_spark_points(
            &numerator.spark_points,
            &denominator.spark_points,
            metric,
        ),
    })
}

pub async fn compute_drilldown_multi(
    _ctx: &ViewContext,
    _group_by: &str,
    _metric_ids: &[String],
) -> Result<DrilldownResponse> {
    Err(anyhow!(
        "{} does not support drilldown. Use drilldown on source indicators instead.",
        VIEW_ID
    ))
}

#[cfg(test)]
mod tests {
    use super::{build_spark_points, pct_change, ratio_value, ViewMetric};

    #[test]
    fn ratio_percent_multiplies_by_hundred() {
        assert_eq!(
            ratio_value(Some(80.0), Some(100.0), ViewMetric::RatioPercent),
            Some(80.0)
        );
        assert_eq!(
            ratio_value(Some(25.0), Some(50.0), ViewMetric::RatioPercent),
            Some(50.0)
        );
    }

    #[test]
    fn ratio_returns_raw_quotient() {
        assert_eq!(
            ratio_value(Some(80.0), Some(100.0), ViewMetric::Ratio),
            Some(0.8)
        );
        assert_eq!(
            ratio_value(Some(25.0), Some(50.0), ViewMetric::Ratio),
            Some(0.5)
        );
    }

    #[test]
    fn ratio_percent_complement_subtracts_from_hundred() {
        assert_eq!(
            ratio_value(Some(80.0), Some(100.0), ViewMetric::RatioPercentComplement),
            Some(20.0)
        );
        assert_eq!(
            ratio_value(Some(25.0), Some(50.0), ViewMetric::RatioPercentComplement),
            Some(50.0)
        );
    }

    #[test]
    fn ratio_value_returns_none_on_zero_denominator() {
        assert_eq!(ratio_value(Some(10.0), Some(0.0), ViewMetric::Ratio), None);
        assert_eq!(
            ratio_value(Some(10.0), None, ViewMetric::RatioPercent),
            None
        );
        assert_eq!(
            ratio_value(Some(10.0), Some(0.0), ViewMetric::RatioPercentComplement),
            None
        );
    }

    #[test]
    fn spark_points_are_built_pairwise() {
        assert_eq!(
            build_spark_points(
                &[50.0, 80.0, 30.0],
                &[100.0, 160.0, 60.0],
                ViewMetric::RatioPercent
            ),
            vec![50.0, 50.0, 50.0]
        );
    }

    #[test]
    fn change_is_calculated_from_ratio_values() {
        assert_eq!(pct_change(Some(80.0), Some(100.0)), Some(-20.0));
    }
}

pub fn meta() -> contracts::shared::data_view::DataViewMeta {
    const JSON: &str = include_str!("metadata.json");
    serde_json::from_str(JSON).expect("dv006/metadata.json parse error")
}
