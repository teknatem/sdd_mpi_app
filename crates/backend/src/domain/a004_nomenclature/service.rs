use super::repository;
use contracts::domain::a004_nomenclature::aggregate::{Nomenclature, NomenclatureDto};
use contracts::domain::a004_nomenclature::orders_dto::NomenclatureOrderRowDto;
use uuid::Uuid;

fn normalize_validation_error(message: &str) -> String {
    match message {
        "Формат не должен превышать 20 символов" => {
            "Поле «Формат» в блоке измерений не должно превышать 20 символов".to_string()
        }
        "Р Р°Р·РјРµСЂ не должен превышать 20 символов" => {
            "Поле В«Р Р°Р·РјРµСЂВ» в блоке измерений не должно превышать 20 символов".to_string()
        }
        "Категория не должна превышать 40 символов" => {
            "Поле «Категория» в блоке измерений не должно превышать 40 символов".to_string()
        }
        "Линейка не должна превышать 40 символов" => {
            "Поле «Линейка» в блоке измерений не должно превышать 40 символов".to_string()
        }
        "Модель не должна превышать 80 символов" => {
            "Поле «Модель» в блоке измерений не должно превышать 80 символов".to_string()
        }
        "Р Р°РєРѕРІРёРЅР° не должна превышать 40 символов" => {
            "Поле В«Р Р°РєРѕРІРёРЅР°В» в блоке измерений не должно превышать 40 символов"
                .to_string()
        }
        _ => message.to_string(),
    }
}

pub async fn create(dto: NomenclatureDto) -> anyhow::Result<Uuid> {
    let code = dto
        .code
        .clone()
        .unwrap_or_else(|| format!("NOM-{}", Uuid::new_v4()));
    let mut aggregate = Nomenclature::new_for_insert(
        code,
        dto.description,
        dto.full_description.unwrap_or_default(),
        dto.is_folder,
        dto.parent_id,
        dto.article.unwrap_or_default(),
        dto.comment,
    );
    aggregate.alternative_cost_source_ref = dto.alternative_cost_source_ref.clone();
    aggregate.base_nomenclature_ref = dto.base_nomenclature_ref.clone();
    aggregate.kit_variant_ref = dto.kit_variant_ref.clone();
    aggregate.is_assembly = dto.is_assembly.unwrap_or(false);
    aggregate.is_derivative = aggregate.compute_is_derivative();

    aggregate
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", normalize_validation_error(&e)))?;
    aggregate.before_write();

    repository::insert(&aggregate).await
}

pub async fn update(dto: NomenclatureDto) -> anyhow::Result<()> {
    let id = dto
        .id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| anyhow::anyhow!("Invalid ID"))?;

    let mut aggregate = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Not found"))?;

    aggregate.update(&dto);

    aggregate
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", normalize_validation_error(&e)))?;
    aggregate.before_write();

    repository::update(&aggregate).await
}

pub async fn delete(id: Uuid) -> anyhow::Result<bool> {
    repository::soft_delete(id).await
}

pub async fn get_by_id(id: Uuid) -> anyhow::Result<Option<Nomenclature>> {
    repository::get_by_id(id).await
}

pub async fn list_all() -> anyhow::Result<Vec<Nomenclature>> {
    repository::list_all().await
}

/// Поиск номенклатуры по штрихкоду через проекцию p901_nomenclature_barcodes.
///
/// Штрихкод резолвится в `nomenclature_ref` (источник 1C приоритетнее), после чего
/// загружаются соответствующие карточки номенклатуры. Возвращает 0/1/N позиций —
/// фронтенд обрабатывает их так же, как автоподбор по артикулу.
pub async fn find_by_barcode(barcode: &str) -> anyhow::Result<Vec<Nomenclature>> {
    let refs =
        crate::projections::p901_nomenclature_barcodes::service::find_nomenclature_refs_by_barcode(
            barcode,
        )
        .await?;

    let mut result = Vec::new();
    for nom_ref in refs {
        let Ok(uuid) = Uuid::parse_str(&nom_ref) else {
            continue;
        };
        if let Some(nomenclature) = repository::get_by_id(uuid).await? {
            result.push(nomenclature);
        }
    }

    Ok(result)
}

pub async fn list_paginated(
    limit: u64,
    offset: u64,
    sort_by: &str,
    sort_desc: bool,
    q: &str,
    only_mp: bool,
    no_analytics: bool,
) -> anyhow::Result<(Vec<Nomenclature>, u64)> {
    repository::list_paginated(limit, offset, sort_by, sort_desc, q, only_mp, no_analytics).await
}

pub async fn sync_kit_variant_links(
) -> anyhow::Result<super::kit_variant_link_sync::KitVariantLinkSyncStats> {
    super::kit_variant_link_sync::sync_links().await
}

/// Объединённый список заказов WB + YM за последние `days` дней, относящихся
/// к номенклатуре `nomenclature_ref` (напрямую или через её деривативы,
/// у которых base_nomenclature_ref указывает на неё). Для вкладки «Заказы»
/// на карточке номенклатуры.
pub async fn list_related_orders(
    nomenclature_ref: &str,
    days: u32,
) -> anyhow::Result<Vec<NomenclatureOrderRowDto>> {
    let date_from = (chrono::Utc::now().date_naive() - chrono::Duration::days(days as i64))
        .format("%Y-%m-%d")
        .to_string();

    let wb_rows = crate::domain::a015_wb_orders::repository::list_orders_for_nomenclature(
        nomenclature_ref,
        &date_from,
    )
    .await?;
    let ym_rows = crate::domain::a013_ym_order::repository::list_lines_for_nomenclature(
        nomenclature_ref,
        &date_from,
    )
    .await?;

    let mut items: Vec<NomenclatureOrderRowDto> = Vec::with_capacity(wb_rows.len() + ym_rows.len());

    for r in wb_rows {
        items.push(NomenclatureOrderRowDto {
            id: r.id,
            marketplace: "WB".to_string(),
            document_no: r.document_no,
            order_date: r.document_date,
            is_cancel: r.is_cancel,
            is_supply: r.is_supply,
            is_realization: r.is_realization,
            line_status: None,
            status_norm: None,
            qty: r.qty.unwrap_or(1.0),
            price_before_discount: r.total_price.or(r.price),
            price_after_discount: r.price_with_disc.or(r.finished_price),
            final_buyer_price: r.finished_price,
            dealer_price_ut: r.dealer_price_ut,
            margin_pro: r.margin_pro,
        });
    }

    for r in ym_rows {
        // Маржа заказа относится ко всему заказу целиком, поэтому переносим её
        // на строку, только если заказ однострочный — иначе некорректно.
        let margin_pro = match r.order_lines_count {
            Some(1) => r.order_margin_pro,
            _ => None,
        };
        items.push(NomenclatureOrderRowDto {
            id: r.order_id,
            marketplace: "YM".to_string(),
            document_no: r.document_no,
            order_date: r.creation_date,
            is_cancel: None,
            is_supply: None,
            is_realization: None,
            line_status: r.line_status,
            status_norm: r.status_norm,
            qty: r.qty,
            price_before_discount: r.price_list,
            price_after_discount: r.price_effective,
            final_buyer_price: r.buyer_price,
            dealer_price_ut: r.dealer_price_ut,
            margin_pro,
        });
    }

    items.sort_by(|a, b| b.order_date.cmp(&a.order_date));

    Ok(items)
}
