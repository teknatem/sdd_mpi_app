use super::repository;
use anyhow::Result;
use contracts::domain::a012_wb_sales::aggregate::WbSales;
use contracts::domain::common::AggregateId;
use std::collections::{BTreeSet, HashMap, HashSet};
use uuid::Uuid;

use crate::shared::marketplaces::wildberries::datetime::wb_business_date;

const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

#[derive(Default)]
pub struct PostingPreparationCache {
    connection_by_id:
        HashMap<String, Option<contracts::domain::a006_connection_mp::aggregate::ConnectionMP>>,
    acquiring_fee_rate_by_marketplace_id: HashMap<String, f64>,
    resolved_price_by_nom_and_date: HashMap<
        (String, String),
        Option<crate::projections::p906_nomenclature_prices::service::ResolvedPrice>,
    >,
    nomenclature_by_id:
        HashMap<String, Option<contracts::domain::a004_nomenclature::aggregate::Nomenclature>>,
    kit_variant_by_id:
        HashMap<String, Option<contracts::domain::a022_kit_variant::aggregate::KitVariant>>,
    kit_variant_by_owner_ref:
        HashMap<String, Option<contracts::domain::a022_kit_variant::aggregate::KitVariant>>,
    direct_cost_by_nom_and_date: HashMap<(String, String), Option<f64>>,
    resolved_prod_unit_cost_by_nom_and_date: HashMap<(String, String), Option<f64>>,
    /// Memoizes the a007 nomenclature_ref resolution, keyed by (connection_id, nm_id, article).
    /// Lets the import/batch path and unpost-refresh avoid repeating the same a007 lookups.
    nomenclature_ref_by_lookup: HashMap<(String, i64, String), Option<String>>,
}

impl PostingPreparationCache {
    async fn get_connection(
        &mut self,
        connection_id: &str,
    ) -> Result<Option<contracts::domain::a006_connection_mp::aggregate::ConnectionMP>> {
        if let Some(value) = self.connection_by_id.get(connection_id) {
            return Ok(value.clone());
        }

        let resolved = match Uuid::parse_str(connection_id) {
            Ok(id) => crate::domain::a006_connection_mp::service::get_by_id(id).await?,
            Err(_) => None,
        };
        self.connection_by_id
            .insert(connection_id.to_string(), resolved.clone());
        Ok(resolved)
    }

    async fn get_acquiring_fee_rate(&mut self, marketplace_id: &str) -> Result<f64> {
        if let Some(value) = self
            .acquiring_fee_rate_by_marketplace_id
            .get(marketplace_id)
        {
            return Ok(*value);
        }

        let rate = match Uuid::parse_str(marketplace_id) {
            Ok(id) => crate::domain::a005_marketplace::service::get_by_id(id)
                .await?
                .map(|marketplace| marketplace.acquiring_fee_pro)
                .unwrap_or(0.0),
            Err(_) => 0.0,
        };

        self.acquiring_fee_rate_by_marketplace_id
            .insert(marketplace_id.to_string(), rate);
        Ok(rate)
    }

    async fn resolve_price(
        &mut self,
        nomenclature_ref: &str,
        sale_date: &str,
    ) -> Result<Option<crate::projections::p906_nomenclature_prices::service::ResolvedPrice>> {
        let cache_key = (nomenclature_ref.to_string(), sale_date.to_string());
        if let Some(value) = self.resolved_price_by_nom_and_date.get(&cache_key) {
            return Ok(value.clone());
        }

        let resolved =
            crate::projections::p906_nomenclature_prices::service::resolve_price_for_nomenclature(
                nomenclature_ref,
                sale_date,
            )
            .await?;
        self.resolved_price_by_nom_and_date
            .insert(cache_key, resolved.clone());
        Ok(resolved)
    }

    async fn resolve_nomenclature_ref(
        &mut self,
        connection_id: &str,
        nm_id: i64,
        article: &str,
    ) -> Result<Option<String>> {
        let cache_key = (connection_id.to_string(), nm_id, article.to_string());
        if let Some(value) = self.nomenclature_ref_by_lookup.get(&cache_key) {
            return Ok(value.clone());
        }

        let resolved =
            crate::domain::a007_marketplace_product::service::resolve_wb_nomenclature_ref(
                connection_id,
                nm_id,
                Some(article),
            )
            .await?;
        self.nomenclature_ref_by_lookup
            .insert(cache_key, resolved.clone());
        Ok(resolved)
    }

    async fn get_nomenclature(
        &mut self,
        nomenclature_ref: &str,
    ) -> Result<Option<contracts::domain::a004_nomenclature::aggregate::Nomenclature>> {
        if let Some(value) = self.nomenclature_by_id.get(nomenclature_ref) {
            return Ok(value.clone());
        }

        let resolved = match Uuid::parse_str(nomenclature_ref) {
            Ok(id) => crate::domain::a004_nomenclature::repository::get_by_id(id).await?,
            Err(_) => None,
        };
        self.nomenclature_by_id
            .insert(nomenclature_ref.to_string(), resolved.clone());
        Ok(resolved)
    }

    async fn get_kit_variant_by_owner_ref(
        &mut self,
        owner_ref: &str,
    ) -> Result<Option<contracts::domain::a022_kit_variant::aggregate::KitVariant>> {
        if let Some(value) = self.kit_variant_by_owner_ref.get(owner_ref) {
            return Ok(value.clone());
        }

        let resolved =
            crate::domain::a022_kit_variant::repository::get_main_by_owner_ref(owner_ref).await?;
        self.kit_variant_by_owner_ref
            .insert(owner_ref.to_string(), resolved.clone());
        Ok(resolved)
    }

    async fn get_kit_variant_by_id(
        &mut self,
        kit_variant_ref: &str,
    ) -> Result<Option<contracts::domain::a022_kit_variant::aggregate::KitVariant>> {
        if let Some(value) = self.kit_variant_by_id.get(kit_variant_ref) {
            return Ok(value.clone());
        }

        let resolved = match Uuid::parse_str(kit_variant_ref) {
            Ok(id) => crate::domain::a022_kit_variant::repository::get_by_id(id).await?,
            Err(_) => None,
        };
        self.kit_variant_by_id
            .insert(kit_variant_ref.to_string(), resolved.clone());
        Ok(resolved)
    }

    async fn resolve_direct_cost(
        &mut self,
        nomenclature_ref: &str,
        sale_date: &str,
    ) -> Result<Option<f64>> {
        let cache_key = (nomenclature_ref.to_string(), sale_date.to_string());
        if let Some(value) = self.direct_cost_by_nom_and_date.get(&cache_key) {
            return Ok(*value);
        }

        let resolved =
            crate::projections::p912_nomenclature_costs::service::resolve_latest_cost_before_date(
                nomenclature_ref,
                sale_date,
            )
            .await?
            .map(|record| record.cost)
            .filter(|cost| *cost > 0.0);
        self.direct_cost_by_nom_and_date.insert(cache_key, resolved);
        Ok(resolved)
    }

    async fn resolve_simple_prod_unit_cost(
        &mut self,
        nomenclature_ref: &str,
        sale_date: &str,
    ) -> Result<Option<f64>> {
        let root_key = (nomenclature_ref.to_string(), sale_date.to_string());
        if let Some(value) = self.resolved_prod_unit_cost_by_nom_and_date.get(&root_key) {
            return Ok(*value);
        }

        let mut stack = vec![(nomenclature_ref.to_string(), false)];
        let mut visiting = HashSet::new();

        while let Some((current_ref, expanded)) = stack.pop() {
            let cache_key = (current_ref.clone(), sale_date.to_string());
            if self
                .resolved_prod_unit_cost_by_nom_and_date
                .contains_key(&cache_key)
            {
                if expanded {
                    visiting.remove(&current_ref);
                }
                continue;
            }

            if expanded {
                visiting.remove(&current_ref);

                let mut resolved = self.resolve_direct_cost(&current_ref, sale_date).await?;
                if resolved.is_none() {
                    if let Some(nomenclature) = self.get_nomenclature(&current_ref).await? {
                        for fallback_ref in collect_cost_fallback_refs(&nomenclature, &current_ref)
                        {
                            let fallback_key = (fallback_ref.clone(), sale_date.to_string());
                            if let Some(cost) = self
                                .resolved_prod_unit_cost_by_nom_and_date
                                .get(&fallback_key)
                                .copied()
                                .flatten()
                            {
                                resolved = Some(cost);
                                break;
                            }
                        }
                    }
                }

                self.resolved_prod_unit_cost_by_nom_and_date
                    .insert(cache_key, resolved);
                continue;
            }

            if !visiting.insert(current_ref.clone()) {
                continue;
            }

            stack.push((current_ref.clone(), true));

            if let Some(nomenclature) = self.get_nomenclature(&current_ref).await? {
                let mut fallback_refs = collect_cost_fallback_refs(&nomenclature, &current_ref);
                fallback_refs.reverse();

                for fallback_ref in fallback_refs {
                    let fallback_key = (fallback_ref.clone(), sale_date.to_string());
                    if self
                        .resolved_prod_unit_cost_by_nom_and_date
                        .contains_key(&fallback_key)
                        || visiting.contains(&fallback_ref)
                    {
                        continue;
                    }
                    stack.push((fallback_ref, false));
                }
            }
        }

        Ok(self
            .resolved_prod_unit_cost_by_nom_and_date
            .get(&root_key)
            .copied()
            .flatten())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProdCostResolution {
    pub total: Option<f64>,
    pub status: &'static str,
    pub problem_message: Option<String>,
}

impl ProdCostResolution {
    fn ok(total: f64) -> Self {
        Self {
            total: Some(total),
            status: "ok",
            problem_message: None,
        }
    }

    fn problem(status: &'static str, message: impl Into<String>) -> Self {
        Self {
            total: None,
            status,
            problem_message: Some(message.into()),
        }
    }

    pub fn has_problem(&self) -> bool {
        self.problem_message.is_some()
    }
}

fn set_if_changed<T: PartialEq>(slot: &mut Option<T>, next: Option<T>) -> bool {
    if *slot != next {
        *slot = next;
        true
    } else {
        false
    }
}

fn clear_fact_fields(document: &mut WbSales) -> bool {
    let mut changed = false;
    changed |= set_if_changed(&mut document.line.is_fact, Some(false));
    changed |= set_if_changed(&mut document.line.sell_out_fact, None);
    changed |= set_if_changed(&mut document.line.acquiring_fee_fact, None);
    changed |= set_if_changed(&mut document.line.other_fee_fact, None);
    changed |= set_if_changed(&mut document.line.supplier_payout_fact, None);
    changed |= set_if_changed(&mut document.line.profit_fact, None);
    changed |= set_if_changed(&mut document.line.commission_fact, None);
    changed
}

fn set_scalar_if_changed<T: PartialEq>(slot: &mut T, next: T) -> bool {
    if *slot != next {
        *slot = next;
        true
    } else {
        false
    }
}

pub fn apply_prod_cost_diagnostics(
    document: &mut WbSales,
    resolution: &ProdCostResolution,
) -> bool {
    let mut changed = false;
    changed |= set_scalar_if_changed(&mut document.prod_cost_problem, resolution.has_problem());
    changed |= set_if_changed(
        &mut document.prod_cost_status,
        Some(resolution.status.to_string()),
    );
    changed |= set_if_changed(
        &mut document.prod_cost_problem_message,
        resolution.problem_message.clone(),
    );
    changed |= set_if_changed(&mut document.prod_cost_resolved_total, resolution.total);
    changed
}

fn valid_ref(value: Option<String>, current_ref: &str) -> Option<String> {
    value.filter(|raw| {
        let trimmed = raw.trim();
        !trimmed.is_empty() && trimmed != ZERO_UUID && trimmed != current_ref
    })
}

fn collect_cost_fallback_refs(
    nomenclature: &contracts::domain::a004_nomenclature::aggregate::Nomenclature,
    current_ref: &str,
) -> Vec<String> {
    let mut refs = Vec::new();

    if let Some(base_ref) = valid_ref(nomenclature.base_nomenclature_ref.clone(), current_ref) {
        refs.push(base_ref);
    }

    if let Some(alternative_ref) = valid_ref(
        nomenclature.alternative_cost_source_ref.clone(),
        current_ref,
    ) {
        if !refs.iter().any(|existing| existing == &alternative_ref) {
            refs.push(alternative_ref);
        }
    }

    refs
}

async fn ensure_missing_cost_entries_best_effort(target_date: &str, nomenclature_refs: &[String]) {
    if nomenclature_refs.is_empty() {
        return;
    }

    if let Err(error) =
        crate::domain::a028_missing_cost_registry::service::ensure_missing_cost_entries(
            target_date,
            nomenclature_refs,
        )
        .await
    {
        tracing::error!(
            "Failed to ensure a028 missing cost entries for {} refs at {}: {}",
            nomenclature_refs.len(),
            target_date,
            error
        );
    }
}

async fn preload_nomenclature_graph(
    cache: &mut PostingPreparationCache,
    initial_refs: &BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    let mut all_refs = initial_refs.clone();
    let mut pending_refs = initial_refs.clone();

    while !pending_refs.is_empty() {
        let refs_batch = pending_refs.iter().cloned().collect::<Vec<_>>();
        pending_refs.clear();

        for nomenclature in
            crate::domain::a004_nomenclature::repository::list_by_ids(&refs_batch).await?
        {
            let nomenclature_id = nomenclature.base.id.as_string();
            let fallback_refs = collect_cost_fallback_refs(&nomenclature, &nomenclature_id);
            cache
                .nomenclature_by_id
                .insert(nomenclature_id.clone(), Some(nomenclature));

            for fallback_ref in fallback_refs {
                if all_refs.insert(fallback_ref.clone()) {
                    pending_refs.insert(fallback_ref);
                }
            }
        }
    }

    Ok(all_refs)
}

pub async fn preload_prod_cost_context_for_documents(
    cache: &mut PostingPreparationCache,
    documents: &[WbSales],
) -> Result<()> {
    if documents.is_empty() {
        return Ok(());
    }

    let sale_date = wb_business_date(&documents[0].state.sale_dt).to_string();
    let document_nom_refs: BTreeSet<String> = documents
        .iter()
        .filter_map(|document| document.nomenclature_ref.clone())
        .filter(|value| !value.trim().is_empty())
        .collect();

    if document_nom_refs.is_empty() {
        return Ok(());
    }

    let mut refs_to_resolve = preload_nomenclature_graph(cache, &document_nom_refs).await?;

    let explicit_kit_variant_refs = document_nom_refs
        .iter()
        .filter_map(|nomenclature_ref| {
            cache
                .nomenclature_by_id
                .get(nomenclature_ref.as_str())
                .and_then(|item| item.as_ref())
                .and_then(|nomenclature| valid_ref(nomenclature.kit_variant_ref.clone(), ""))
        })
        .collect::<Vec<_>>();

    if !explicit_kit_variant_refs.is_empty() {
        let variants =
            crate::domain::a022_kit_variant::repository::list_by_ids(&explicit_kit_variant_refs)
                .await?;
        let variants_by_id = variants
            .into_iter()
            .map(|variant| (variant.base.id.as_string(), variant))
            .collect::<HashMap<_, _>>();

        for kit_variant_ref in &explicit_kit_variant_refs {
            let variant = variants_by_id.get(kit_variant_ref).cloned();
            cache
                .kit_variant_by_id
                .insert(kit_variant_ref.clone(), variant.clone());
            if let Some(variant) = variant {
                if let Some(owner_ref) = variant.owner_ref.clone() {
                    cache
                        .kit_variant_by_owner_ref
                        .entry(owner_ref)
                        .or_insert_with(|| Some(variant));
                }
            }
        }
    }

    let owner_refs = document_nom_refs
        .iter()
        .filter(|owner_ref| {
            cache
                .nomenclature_by_id
                .get(owner_ref.as_str())
                .and_then(|item| item.as_ref())
                .is_some_and(|nomenclature| {
                    nomenclature.is_assembly
                        || nomenclature
                            .kit_variant_ref
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty())
                })
        })
        .cloned()
        .collect::<Vec<_>>();

    if !owner_refs.is_empty() {
        let variants =
            crate::domain::a022_kit_variant::repository::list_main_by_owner_refs(&owner_refs)
                .await?;
        for owner_ref in &owner_refs {
            cache
                .kit_variant_by_owner_ref
                .insert(owner_ref.clone(), variants.get(owner_ref).cloned());
        }
    }

    let component_refs: BTreeSet<String> = owner_refs
        .iter()
        .filter_map(|owner_ref| cache.kit_variant_by_owner_ref.get(owner_ref))
        .filter_map(|variant| variant.as_ref())
        .chain(
            explicit_kit_variant_refs
                .iter()
                .filter_map(|kit_variant_ref| cache.kit_variant_by_id.get(kit_variant_ref))
                .filter_map(|variant| variant.as_ref()),
        )
        .flat_map(|variant| variant.parse_goods().into_iter())
        .map(|item| item.nomenclature_ref)
        .filter(|value| !value.trim().is_empty())
        .collect();

    if !component_refs.is_empty() {
        refs_to_resolve.extend(preload_nomenclature_graph(cache, &component_refs).await?);
    }

    if refs_to_resolve.is_empty() {
        return Ok(());
    }

    let direct_costs =
        crate::projections::p912_nomenclature_costs::service::resolve_latest_costs_before_date(
            &refs_to_resolve.iter().cloned().collect::<Vec<_>>(),
            &sale_date,
        )
        .await?;

    for nomenclature_ref in &refs_to_resolve {
        let cache_key = (nomenclature_ref.clone(), sale_date.clone());
        let direct_cost = direct_costs
            .get(nomenclature_ref)
            .map(|record| record.cost)
            .filter(|cost| *cost > 0.0);
        cache
            .direct_cost_by_nom_and_date
            .insert(cache_key.clone(), direct_cost);
        if let Some(cost) = direct_cost {
            cache
                .resolved_prod_unit_cost_by_nom_and_date
                .insert(cache_key, Some(cost));
        }
    }

    for nomenclature_ref in refs_to_resolve.iter().cloned().collect::<Vec<_>>() {
        let _ = cache
            .resolve_simple_prod_unit_cost(&nomenclature_ref, &sale_date)
            .await?;
    }

    Ok(())
}

pub async fn resolve_prod_cost_cached(
    document: &WbSales,
    cache: &mut PostingPreparationCache,
) -> Result<ProdCostResolution> {
    let Some(nomenclature_ref) = document.nomenclature_ref.as_deref() else {
        return Ok(ProdCostResolution::problem(
            "missing_nomenclature_ref",
            format!(
                "Не задана номенклатура для prod-себестоимости. Документ {}, article {}",
                document.base.id.as_string(),
                document.line.supplier_article
            ),
        ));
    };
    if nomenclature_ref.trim().is_empty() {
        return Ok(ProdCostResolution::problem(
            "missing_nomenclature_ref",
            format!(
                "Пустая ссылка на номенклатуру для prod-себестоимости. Документ {}, article {}",
                document.base.id.as_string(),
                document.line.supplier_article
            ),
        ));
    }

    let sale_date = wb_business_date(&document.state.sale_dt).to_string();
    let Some(nomenclature) = cache.get_nomenclature(nomenclature_ref).await? else {
        let message = format!(
            "Номенклатура {} не найдена. Prod-себестоимость не рассчитана на дату {}",
            nomenclature_ref, sale_date
        );
        tracing::warn!(
            "Skip prod item_cost for WB Sales {}: {}",
            document.base.id.as_string(),
            message
        );
        return Ok(ProdCostResolution::problem(
            "nomenclature_not_found",
            message,
        ));
    };

    let has_kit_link = nomenclature
        .kit_variant_ref
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());

    if !nomenclature.is_assembly && !has_kit_link {
        let resolved = cache
            .resolve_simple_prod_unit_cost(nomenclature_ref, &sale_date)
            .await?;
        return match resolved {
            Some(unit_cost) => Ok(ProdCostResolution::ok(unit_cost * document.line.qty.abs())),
            None => {
                ensure_missing_cost_entries_best_effort(
                    &sale_date,
                    &[nomenclature_ref.to_string()],
                )
                .await;
                let alternative_hint = valid_ref(
                    nomenclature.alternative_cost_source_ref.clone(),
                    nomenclature_ref,
                )
                .map(|alternative_ref| format!(", альтернативный источник {}", alternative_ref))
                .unwrap_or_default();
                let message = format!(
                    "Не найдена себестоимость в p912 для номенклатуры {} ({}){} на дату {}",
                    nomenclature.base.code, nomenclature.article, alternative_hint, sale_date
                );
                tracing::warn!(
                    "Skip prod item_cost for WB Sales {}: {}",
                    document.base.id.as_string(),
                    message
                );
                Ok(ProdCostResolution::problem("missing_p912_cost", message))
            }
        };
    }

    let kit_variant = if let Some(kit_variant_ref) = nomenclature.kit_variant_ref.as_deref() {
        cache.get_kit_variant_by_id(kit_variant_ref).await?
    } else {
        cache.get_kit_variant_by_owner_ref(nomenclature_ref).await?
    };

    let Some(kit_variant) = kit_variant else {
        let message = format!(
            "Для комплекта {} ({}) не найден вариант комплектации a022",
            nomenclature.base.code, nomenclature.article
        );
        tracing::warn!(
            "Skip prod item_cost for WB Sales {}: {}",
            document.base.id.as_string(),
            message
        );
        return Ok(ProdCostResolution::problem(
            "kit_variant_not_found",
            message,
        ));
    };

    let goods = kit_variant.parse_goods();
    if goods.is_empty() {
        let message = format!(
            "Для комплекта {} ({}) состав a022 пустой",
            nomenclature.base.code, nomenclature.article
        );
        tracing::warn!(
            "Skip prod item_cost for WB Sales {}: {}",
            document.base.id.as_string(),
            message
        );
        return Ok(ProdCostResolution::problem("empty_kit", message));
    }

    let mut unit_cost = 0.0;
    let mut missing_components = Vec::new();
    let mut missing_component_refs = Vec::new();
    for component in goods {
        match cache
            .resolve_simple_prod_unit_cost(&component.nomenclature_ref, &sale_date)
            .await?
        {
            Some(component_cost) => unit_cost += component_cost * component.quantity,
            None => {
                missing_component_refs.push(component.nomenclature_ref.clone());
                let label = cache
                    .get_nomenclature(&component.nomenclature_ref)
                    .await?
                    .map(|nom| {
                        let article = nom.article.trim().to_string();
                        let code = nom.base.code.trim().to_string();
                        if article.is_empty() {
                            code
                        } else if code.is_empty() {
                            article
                        } else {
                            format!("{code}/{article}")
                        }
                    })
                    .filter(|label| !label.trim().is_empty())
                    .unwrap_or(component.nomenclature_ref.clone());
                missing_components.push(label);
            }
        }
    }

    if !missing_components.is_empty() {
        ensure_missing_cost_entries_best_effort(&sale_date, &missing_component_refs).await;
        let message = format!(
            "Не найдена себестоимость в p912 для компонентов комплекта {} ({}): {}. Дата {}",
            nomenclature.base.code,
            nomenclature.article,
            missing_components.join(", "),
            sale_date
        );
        tracing::warn!(
            "Skip prod item_cost for WB Sales {}: {}",
            document.base.id.as_string(),
            message
        );
        return Ok(ProdCostResolution::problem(
            "missing_component_costs",
            message,
        ));
    }

    Ok(ProdCostResolution::ok(unit_cost * document.line.qty.abs()))
}

pub async fn sync_organization_from_connection_cached(
    document: &mut WbSales,
    cache: &mut PostingPreparationCache,
) -> Result<bool> {
    let should_sync = document.header.organization_id.trim().is_empty()
        || match cache.get_connection(&document.header.connection_id).await? {
            Some(connection) => {
                let organization_ref = connection.organization_ref.trim().trim_matches('"');
                organization_ref != document.header.organization_id
            }
            None => false,
        };

    if !should_sync {
        return Ok(false);
    }

    let Some(connection) = cache.get_connection(&document.header.connection_id).await? else {
        tracing::warn!(
            "Skip organization sync for WB Sales {}: connection not found, connection_id={}",
            document.base.id.value(),
            document.header.connection_id
        );
        return Ok(false);
    };

    let organization_ref = connection.organization_ref.trim().trim_matches('"');
    let organization_uuid = match Uuid::parse_str(organization_ref) {
        Ok(uuid) => uuid,
        Err(_) => {
            tracing::warn!(
                "Skip organization sync for WB Sales {}: invalid organization_ref={}",
                document.base.id.value(),
                connection.organization_ref
            );
            return Ok(false);
        }
    };

    if crate::domain::a002_organization::service::get_by_id(
        crate::shared::data::db::get_connection(),
        organization_uuid,
    )
    .await?
    .is_none()
    {
        tracing::warn!(
            "Skip organization sync for WB Sales {}: organization_ref not found={}",
            document.base.id.value(),
            connection.organization_ref
        );
        return Ok(false);
    }

    let resolved_org_id = organization_uuid.to_string();
    if document.header.organization_id != resolved_org_id {
        document.header.organization_id = resolved_org_id;
        return Ok(true);
    }

    Ok(false)
}

/// Принудительно разрешает и заполняет ссылочные реквизиты (marketplace_product_ref,
/// nomenclature_ref), всегда зеркаля актуальное состояние a007. Используется при импорте
/// (первое проведение) и при unpost (вариант b: чтобы непроведённый документ оставался
/// корректным для отображения).
pub async fn refresh_references_cached(
    document: &mut WbSales,
    cache: &mut PostingPreparationCache,
) -> Result<bool> {
    let mut changed = false;

    if document.marketplace_product_ref.is_none() {
        if let Some(marketplace_sku) =
            crate::domain::a007_marketplace_product::service::wb_marketplace_sku(
                document.line.nm_id,
            )
        {
            let title = if document.line.name.trim().is_empty() {
                format!("Артикул: {}", document.line.supplier_article)
            } else {
                document.line.name.clone()
            };

            let mp_id = crate::domain::a007_marketplace_product::service::find_or_create_for_sale(
                crate::domain::a007_marketplace_product::service::FindOrCreateParams {
                    marketplace_ref: document.header.marketplace_id.clone(),
                    connection_mp_ref: document.header.connection_id.clone(),
                    marketplace_sku,
                    article: Some(document.line.supplier_article.clone()),
                    barcode: Some(document.line.barcode.clone()),
                    title,
                },
            )
            .await?;

            document.marketplace_product_ref = Some(mp_id.to_string());
            changed = true;
        }
    }

    let resolved = cache
        .resolve_nomenclature_ref(
            &document.header.connection_id,
            document.line.nm_id,
            &document.line.supplier_article,
        )
        .await?;
    if document.nomenclature_ref != resolved {
        document.nomenclature_ref = resolved;
        changed = true;
    }

    Ok(changed)
}

/// Дешёвый путь проведения: ссылочные реквизиты меняются крайне редко, поэтому если оба
/// реквизита уже заполнены — пропускаем обращения к a007 целиком (предложение 2). На свежем
/// документе (пустые ссылки) откатываемся к полному резолву. Принудительное обновление
/// делается на unpost через refresh_references_cached.
pub async fn auto_fill_references_cached(
    document: &mut WbSales,
    cache: &mut PostingPreparationCache,
) -> Result<bool> {
    if document.marketplace_product_ref.is_some()
        && document
            .nomenclature_ref
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(false);
    }

    refresh_references_cached(document, cache).await
}

pub async fn fill_dealer_price_resolved_cached(
    document: &mut WbSales,
    cache: &mut PostingPreparationCache,
) -> Result<bool> {
    let Some(nom_ref) = document.nomenclature_ref.as_deref() else {
        let mut changed = false;
        changed |= set_if_changed(&mut document.line.dealer_price_ut, None);
        changed |= set_if_changed(&mut document.line.cost_of_production, None);
        return Ok(changed);
    };

    if document.line.cost_of_production.unwrap_or(0.0) > 0.0
        && document.line.dealer_price_ut.unwrap_or(0.0) > 0.0
    {
        return Ok(false);
    }

    let sale_date = wb_business_date(&document.state.sale_dt).to_string();
    let resolved = cache.resolve_price(nom_ref, &sale_date).await?;

    if let Some(ref resolved_price) = resolved {
        tracing::info!(
            "Filled dealer_price_ut = {:?} for document {} (from {})",
            resolved_price.price,
            document.base.id.as_string(),
            resolved_price.describe(&sale_date)
        );
    } else {
        tracing::warn!(
            "Could not find dealer_price_ut for document {} (nomenclature: {})",
            document.base.id.as_string(),
            nom_ref
        );
    }

    let resolved_price = resolved.map(|value| value.price);
    let mut changed = false;

    if document.line.dealer_price_ut.unwrap_or(0.0) <= 0.0 {
        changed |= set_if_changed(&mut document.line.dealer_price_ut, resolved_price);
    }
    if document.line.cost_of_production.unwrap_or(0.0) <= 0.0 {
        changed |= set_if_changed(&mut document.line.cost_of_production, resolved_price);
    }

    Ok(changed)
}

pub async fn calculate_plan_fields_cached(
    document: &mut WbSales,
    cache: &mut PostingPreparationCache,
) -> Result<bool> {
    let acquiring_fee_pro = cache
        .get_acquiring_fee_rate(&document.header.marketplace_id)
        .await?;

    let finished_price = document.line.finished_price.unwrap_or(0.0);
    let amount_line = document.line.amount_line.unwrap_or(0.0);
    let cost_of_production = document.line.cost_of_production.unwrap_or(0.0);

    let acquiring_fee_plan = acquiring_fee_pro * finished_price / 100.0;
    let commission_plan = finished_price - amount_line;
    let other_fee_plan = 0.0;
    let supplier_payout_plan = amount_line - acquiring_fee_plan;
    let profit_plan =
        finished_price - acquiring_fee_plan - commission_plan - other_fee_plan - cost_of_production;

    let mut changed = clear_fact_fields(document);
    changed |= set_if_changed(&mut document.line.sell_out_plan, Some(finished_price));
    changed |= set_if_changed(
        &mut document.line.acquiring_fee_plan,
        Some(acquiring_fee_plan),
    );
    changed |= set_if_changed(&mut document.line.other_fee_plan, Some(other_fee_plan));
    changed |= set_if_changed(
        &mut document.line.supplier_payout_plan,
        Some(supplier_payout_plan),
    );
    changed |= set_if_changed(&mut document.line.commission_plan, Some(commission_plan));
    changed |= set_if_changed(&mut document.line.profit_plan, Some(profit_plan));
    Ok(changed)
}

pub async fn prepare_document_for_posting_cached(
    document: &mut WbSales,
    cache: &mut PostingPreparationCache,
) -> Result<bool> {
    let mut changed = false;
    changed |= sync_organization_from_connection_cached(document, cache).await?;
    changed |= auto_fill_references_cached(document, cache).await?;
    changed |= fill_dealer_price_resolved_cached(document, cache).await?;
    changed |= calculate_plan_fields_cached(document, cache).await?;
    Ok(changed)
}

/// Принудительно обновляет ВСЕ ссылочные реквизиты (организация, ссылки a007, дилерская цена /
/// себестоимость, плановые поля) независимо от текущих значений. Используется при unpost
/// (вариант b), чтобы выполнялся инвариант: unpost + post = полное обновление всех реквизитов,
/// при этом непроведённый документ остаётся корректным для отображения. В отличие от
/// `prepare_document_for_posting_cached`, обходит «дешёвые» пропуски (заполненные ссылки и
/// дилерская цена принудительно перерасчитываются).
pub async fn force_refresh_requisites_cached(
    document: &mut WbSales,
    cache: &mut PostingPreparationCache,
) -> Result<()> {
    sync_organization_from_connection_cached(document, cache).await?;
    refresh_references_cached(document, cache).await?;
    // Принудительно сбрасываем и перезаполняем дилерскую цену/себестоимость (на post они
    // пропускаются, если заполнены), чтобы unpost действительно обновил их.
    document.line.dealer_price_ut = None;
    document.line.cost_of_production = None;
    fill_dealer_price_resolved_cached(document, cache).await?;
    calculate_plan_fields_cached(document, cache).await?;
    Ok(())
}

pub async fn calculate_plan_fields(document: &mut WbSales) -> Result<()> {
    let mut cache = PostingPreparationCache::default();
    calculate_plan_fields_cached(document, &mut cache).await?;
    Ok(())
}

pub async fn auto_fill_references(document: &mut WbSales) -> Result<()> {
    let mut cache = PostingPreparationCache::default();
    auto_fill_references_cached(document, &mut cache).await?;
    Ok(())
}

pub async fn fill_dealer_price(document: &mut WbSales) -> Result<()> {
    fill_dealer_price_resolved(document).await
}

pub async fn fill_dealer_price_resolved(document: &mut WbSales) -> Result<()> {
    let mut cache = PostingPreparationCache::default();
    fill_dealer_price_resolved_cached(document, &mut cache).await?;
    Ok(())
}

pub async fn prepare_document_for_posting(document: &mut WbSales) -> Result<bool> {
    let mut cache = PostingPreparationCache::default();
    prepare_document_for_posting_cached(document, &mut cache).await
}

pub async fn store_document_with_raw(mut document: WbSales, raw_json: &str) -> Result<Uuid> {
    let mut cache = PostingPreparationCache::default();
    let (id, _is_new) =
        store_document_with_raw_shared_cache(&mut document, raw_json, &mut cache, None).await?;
    Ok(id)
}

/// Hot-path variant used by the bulk import loop.  Accepts a shared
/// `PostingPreparationCache` (avoids repeated lookups for the same products /
/// organisations / prices) and a pre-loaded `existing_uuid` (avoids an extra
/// SELECT per row).  Returns `(uuid, is_new)`.
pub async fn store_document_with_raw_shared_cache(
    document: &mut WbSales,
    raw_json: &str,
    cache: &mut PostingPreparationCache,
    existing_uuid: Option<uuid::Uuid>,
) -> Result<(uuid::Uuid, bool)> {
    let is_new = existing_uuid.is_none();

    let raw_ref = crate::shared::data::raw_storage::save_raw_json(
        "WB",
        "WB_Sales",
        &document.header.document_no,
        raw_json,
        document.source_meta.fetched_at,
    )
    .await?;

    if let Some(raw_ref) = raw_ref {
        document.source_meta.raw_payload_ref = raw_ref;
    }
    prepare_document_for_posting_cached(document, cache).await?;
    let prod_cost_resolution = resolve_prod_cost_cached(document, cache).await?;
    apply_prod_cost_diagnostics(document, &prod_cost_resolution);

    document
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;
    document.before_write();

    let id = repository::upsert_document_knowing_existence(document, existing_uuid).await?;

    if document.is_posted {
        if let Err(e) = super::posting::post_document(id).await {
            tracing::error!("Failed to post WB Sales document: {}", e);
        }
    } else {
        if let Err(e) = crate::projections::p900_mp_sales_register::service::delete_by_registrator(
            &id.to_string(),
        )
        .await
        {
            tracing::error!("Failed to delete projections for WB Sales document: {}", e);
        }
        if let Err(e) =
            crate::projections::p904_sales_data::repository::delete_by_registrator(&id.to_string())
                .await
        {
            tracing::error!(
                "Failed to delete P904 projections for WB Sales document: {}",
                e
            );
        }
    }

    Ok((id, is_new))
}

pub async fn get_by_id(id: Uuid) -> Result<Option<WbSales>> {
    repository::get_by_id(id).await
}

pub async fn get_by_document_no(document_no: &str) -> Result<Option<WbSales>> {
    repository::get_by_document_no(document_no).await
}

pub async fn get_by_sale_id(sale_id: &str) -> Result<Option<WbSales>> {
    repository::get_by_sale_id(sale_id).await
}

pub async fn list_all() -> Result<Vec<WbSales>> {
    repository::list_all().await
}

pub async fn list_by_sale_date_range(
    date_from: Option<chrono::NaiveDate>,
    date_to: Option<chrono::NaiveDate>,
) -> Result<Vec<WbSales>> {
    repository::list_by_sale_date_range(date_from, date_to).await
}

pub async fn delete(id: Uuid) -> Result<bool> {
    repository::soft_delete(id).await
}

pub async fn refresh_dealer_price(id: Uuid) -> Result<()> {
    let mut document = get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    document.line.dealer_price_ut = None;
    document.line.cost_of_production = None;
    let mut cache = PostingPreparationCache::default();
    fill_dealer_price_resolved_cached(&mut document, &mut cache).await?;
    calculate_plan_fields_cached(&mut document, &mut cache).await?;
    let prod_cost_resolution = resolve_prod_cost_cached(&document, &mut cache).await?;
    apply_prod_cost_diagnostics(&mut document, &prod_cost_resolution);
    repository::upsert_document(&document).await?;

    tracing::info!(
        "Refreshed dealer_price_ut for document {}: {:?}",
        id,
        document.line.dealer_price_ut
    );

    // Сигнал клиентам обновить открытые списки a012 (изменились dealer_price/prod_cost).
    super::change_token::TOKEN.bump();

    Ok(())
}

/// Перепроведение a012 за период с кэшом по дню продажи.
///
/// Общий путь движка (`ids_in_period` + конкурентный прогон) здесь проигрывает:
/// `post_document` собирает `PostingPreparationCache` заново на каждый документ,
/// а набор за месяц — сотни тысяч строк. Поэтому документы группируются по дню
/// продажи, кэш прогревается один раз на день, а прогресс отчитывается чанками
/// «день × кабинет» — так пользователь видит, где именно идёт долгий прогон.
/// Ключ агрегата — он же имя таблицы и каталога.
const KIND: &str = "a012_wb_sales";

pub struct BulkRepost;

#[async_trait::async_trait]
impl crate::usecases::u508_repost_documents::AggregateBulkRepost for BulkRepost {
    fn key(&self) -> &'static str {
        "a012_wb_sales"
    }

    async fn repost(
        &self,
        tracker: &std::sync::Arc<crate::usecases::u508_repost_documents::ProgressTracker>,
        session_id: &str,
        request: &contracts::usecases::u508_repost_documents::aggregate_request::AggregateRepostRequest,
    ) -> anyhow::Result<()> {
        let connection_labels: std::collections::HashMap<String, String> =
            crate::domain::a006_connection_mp::service::list_all()
                .await?
                .into_iter()
                .map(|connection| {
                    let label = if connection.base.description.trim().is_empty() {
                        connection.base.code.clone()
                    } else {
                        connection.base.description.clone()
                    };
                    (connection.base.id.as_string(), label)
                })
                .collect();

        let chunks = super::repository::list_repost_chunks_by_sale_date_range(
            &request.date_from,
            &request.date_to,
            request.only_posted,
            &request.connection_mp_refs,
        )
        .await?;

        let mut prepared_chunks = Vec::with_capacity(chunks.len());
        let mut total_documents = 0_i32;

        for chunk in chunks {
            let ids = super::repository::list_ids_by_sale_date_and_connection(
                &chunk.sale_date,
                &chunk.connection_mp_ref,
                request.only_posted,
            )
            .await?;
            total_documents += ids.len() as i32;
            prepared_chunks.push((chunk, ids));
        }

        tracker.set_total(session_id, total_documents);
        tracker.set_chunks_total(session_id, prepared_chunks.len() as i32);

        let mut processed = 0_i32;
        let mut reposted = 0_i32;
        let mut chunks_processed = 0_i32;
        let mut day_start = 0_usize;
        while day_start < prepared_chunks.len() {
            let current_day = prepared_chunks[day_start].0.sale_date.clone();
            let mut day_end = day_start;
            while day_end < prepared_chunks.len()
                && prepared_chunks[day_end].0.sale_date == current_day
            {
                day_end += 1;
            }

            let mut posting_cache = PostingPreparationCache::default();
            let day_document_ids = prepared_chunks[day_start..day_end]
                .iter()
                .flat_map(|(_, ids)| ids.iter().cloned())
                .collect::<Vec<_>>();
            if !day_document_ids.is_empty() {
                let day_documents = super::repository::list_by_ids(&day_document_ids).await?;
                preload_prod_cost_context_for_documents(&mut posting_cache, &day_documents).await?;
            }

            for (chunk, ids) in &prepared_chunks[day_start..day_end] {
                let cabinet_label = connection_labels
                    .get(&chunk.connection_mp_ref)
                    .cloned()
                    .unwrap_or_else(|| chunk.connection_mp_ref.clone());
                let chunk_label = format!("{} | {}", chunk.sale_date, cabinet_label);
                tracker.update_chunk_progress(
                    session_id,
                    chunks_processed,
                    Some(chunk.sale_date.clone()),
                    Some(chunk.connection_mp_ref.clone()),
                    Some(chunk_label.clone()),
                );

                for document_id in ids {
                    let current_item = format!("{} {}", KIND, document_id);
                    tracker.update_progress(
                        session_id,
                        processed,
                        reposted,
                        Some(current_item.clone()),
                    );

                    let aggregate_id = match Uuid::parse_str(document_id) {
                        Ok(id) => id,
                        Err(error) => {
                            processed += 1;
                            tracker.add_error(
                                session_id,
                                format!("Invalid aggregate id {}: {}", document_id, error),
                            );
                            tracker.update_progress(
                                session_id,
                                processed,
                                reposted,
                                Some(current_item),
                            );
                            continue;
                        }
                    };

                    let post_start = std::time::Instant::now();
                    let post_result =
                        super::posting::post_document_with_cache(aggregate_id, &mut posting_cache)
                            .await;
                    let elapsed_ms = post_start.elapsed().as_millis() as i64;
                    tracker.record_post_timing(session_id, document_id, elapsed_ms);
                    // Порог предупреждения о медленном проведении (диагностика динамики).
                    const SLOW_DOC_MS: i64 = 500;
                    if elapsed_ms > SLOW_DOC_MS {
                        tracing::warn!(
                            "Slow {} post: {} took {} ms",
                            KIND,
                            aggregate_id,
                            elapsed_ms
                        );
                    }
                    match post_result {
                        Ok(()) => reposted += 1,
                        Err(error) => tracker.add_error(
                            session_id,
                            format!("Failed to repost {} {}: {}", KIND, aggregate_id, error),
                        ),
                    }

                    processed += 1;
                    tracker.update_progress(session_id, processed, reposted, Some(current_item));
                }

                chunks_processed += 1;
                tracker.update_chunk_progress(
                    session_id,
                    chunks_processed,
                    Some(chunk.sale_date.clone()),
                    Some(chunk.connection_mp_ref.clone()),
                    Some(chunk_label),
                );
            }

            day_start = day_end;
        }

        tracker.update_progress(session_id, processed, reposted, None);

        let final_status = if tracker
            .get_progress(session_id)
            .map(|progress| progress.errors > 0)
            .unwrap_or(false)
        {
            contracts::usecases::u508_repost_documents::progress::RepostStatus::CompletedWithErrors
        } else {
            contracts::usecases::u508_repost_documents::progress::RepostStatus::Completed
        };

        tracker.complete_session(session_id, final_status);

        Ok(())
    }
}

/// Перепроведение набора a012 с кэшом по дню продажи.
///
/// Отдельно от `BulkRepost`: тому период задают датами и кабинетами, а сюда
/// приходит готовый список id — так его зовёт пересбор воронки, где набор
/// отобран когортой заказов, а не календарём. Общее у них — причина не
/// пользоваться общим путём движка: `post_document` собирал бы
/// `PostingPreparationCache` заново на каждый документ.
pub async fn repost_ids_with_daily_cache(
    tracker: &std::sync::Arc<crate::usecases::u508_repost_documents::ProgressTracker>,
    session_id: &str,
    a012_ids: Vec<String>,
    processed: &std::sync::Arc<std::sync::atomic::AtomicI32>,
    reposted: &std::sync::Arc<std::sync::atomic::AtomicI32>,
) -> anyhow::Result<()> {
    if a012_ids.is_empty() {
        return Ok(());
    }

    let id_days = super::repository::list_id_sale_days_by_ids(&a012_ids).await?;

    // Группируем id по дню продажи (id_days отсортирован по дню, затем по id).
    let mut covered: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut day_groups: Vec<(String, Vec<String>)> = Vec::new();
    for (id, day) in id_days {
        covered.insert(id.clone());
        match day_groups.last_mut() {
            Some((current_day, ids)) if *current_day == day => ids.push(id),
            _ => day_groups.push((day, vec![id])),
        }
    }

    for (day, day_ids) in day_groups {
        let mut posting_cache = PostingPreparationCache::default();
        let day_documents = super::repository::list_by_ids(&day_ids).await?;
        preload_prod_cost_context_for_documents(&mut posting_cache, &day_documents).await?;

        for id_str in &day_ids {
            let current_item = format!("a012 {} | {}", id_str, day);
            match uuid::Uuid::parse_str(id_str) {
                Ok(id) => {
                    let post_start = std::time::Instant::now();
                    let post_result =
                        super::posting::post_document_with_cache(id, &mut posting_cache).await;
                    let elapsed_ms = post_start.elapsed().as_millis() as i64;
                    tracker.record_post_timing(session_id, id_str, elapsed_ms);
                    const SLOW_DOC_MS: i64 = 500;
                    if elapsed_ms > SLOW_DOC_MS {
                        tracing::warn!("Slow {} post: {} took {} ms", KIND, id, elapsed_ms);
                    }
                    match post_result {
                        Ok(()) => {
                            reposted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Err(error) => {
                            tracker.add_error(session_id, format!("a012 {}: {}", id_str, error))
                        }
                    }
                }
                Err(error) => {
                    tracker.add_error(session_id, format!("Invalid a012 id {}: {}", id_str, error))
                }
            }
            let p = processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let r = reposted.load(std::sync::atomic::Ordering::Relaxed);
            tracker.update_progress(session_id, p, r, Some(current_item));
        }
    }

    // id, пропавшие из выборки между отбором и пересбором (soft-delete/удаление), —
    // отмечаем ошибкой и учитываем в прогрессе, чтобы processed совпадал с числом
    // отобранных.
    for id_str in &a012_ids {
        if !covered.contains(id_str) {
            tracker.add_error(
                session_id,
                format!("a012 {}: документ не найден при пересборе", id_str),
            );
            let p = processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let r = reposted.load(std::sync::atomic::Ordering::Relaxed);
            tracker.update_progress(session_id, p, r, None);
        }
    }

    Ok(())
}
