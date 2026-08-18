// Временный файл для нового метода import_nomenclature
// Этот код будет скопирован в executor.rs

use super::{progress_tracker::ProgressTracker, ut_api_client::UtODataClient};
use crate::domain::a004_nomenclature;
use anyhow::Result;

/// Импорт номенклатуры из УТ (ОПТИМИЗИРОВАННАЯ ВЕРСИЯ)
pub async fn import_nomenclature(
    odata_client: &UtODataClient,
    progress_tracker: &ProgressTracker,
    session_id: &str,
    connection: &contracts::domain::a001_connection_1c::aggregate::Connection1CDatabase,
) -> Result<()> {
    use a004_nomenclature::u501_import_from_ut::UtNomenclatureListResponse;

    tracing::info!("Importing nomenclature for session: {}", session_id);

    let aggregate_index = "a004_nomenclature";
    let page_size = 100;
    let mut total_processed = 0;
    let mut total_inserted = 0;
    let mut total_updated = 0;

    // Pass 1: Загружаем ТОЛЬКО папки с фильтром IsFolder eq true
    tracing::info!("Nomenclature import pass 1/2: folders only");

    let folders_count = odata_client
        .get_collection_count_with_filter(connection, "Catalog_Номенклатура", Some("IsFolder eq true"))
        .await
        .ok()
        .flatten();

    tracing::info!("Folders count: {:?}", folders_count);

    let mut skip = 0;
    loop {
        let response: UtNomenclatureListResponse = odata_client
            .fetch_collection_with_filter(
                connection,
                "Catalog_Номенклатура",
                Some(page_size),
                Some(skip),
                Some("IsFolder eq true"),
            )
            .await?;

        if response.value.is_empty() {
            break;
        }

        let batch_size = response.value.len();

        for odata_item in response.value {
            progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!("[Папка] {} - {}", odata_item.code, odata_item.description)),
            );

            match process_nomenclature(&odata_item).await {
                Ok(is_new) => {
                    total_processed += 1;
                    if is_new {
                        total_inserted += 1;
                    } else {
                        total_updated += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to process folder {}: {}", odata_item.code, e);
                    progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Failed to process folder {}", odata_item.code),
                        Some(e.to_string()),
                    );
                }
            }

            progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                total_processed,
                folders_count,
                total_inserted,
                total_updated,
            );
        }

        skip += page_size;
        if batch_size < page_size as usize {
            break;
        }
    }

    tracing::info!("Folders imported: processed={}, inserted={}, updated={}",
        total_processed, total_inserted, total_updated);

    // Pass 2: Загружаем ТОЛЬКО элементы с фильтром IsFolder eq false
    tracing::info!("Nomenclature import pass 2/2: items only");

    let items_count = odata_client
        .get_collection_count_with_filter(connection, "Catalog_Номенклатура", Some("IsFolder eq false"))
        .await
        .ok()
        .flatten();

    tracing::info!("Items count: {:?}", items_count);

    // Вычисляем общее количество для прогресса
    let total_count = match (folders_count, items_count) {
        (Some(f), Some(i)) => Some(f + i),
        _ => None,
    };

    skip = 0;
    loop {
        let response: UtNomenclatureListResponse = odata_client
            .fetch_collection_with_filter(
                connection,
                "Catalog_Номенклатура",
                Some(page_size),
                Some(skip),
                Some("IsFolder eq false"),
            )
            .await?;

        if response.value.is_empty() {
            break;
        }

        let batch_size = response.value.len();

        for odata_item in response.value {
            progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!("{} - {}", odata_item.code, odata_item.description)),
            );

            match process_nomenclature(&odata_item).await {
                Ok(is_new) => {
                    total_processed += 1;
                    if is_new {
                        total_inserted += 1;
                    } else {
                        total_updated += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to process item {}: {}", odata_item.code, e);
                    progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Failed to process item {}", odata_item.code),
                        Some(e.to_string()),
                    );
                }
            }

            progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                total_processed,
                total_count,
                total_inserted,
                total_updated,
            );
        }

        skip += page_size;
        if batch_size < page_size as usize {
            break;
        }
    }

    // Очистить текущий элемент после завершения
    progress_tracker
        .set_current_item(session_id, aggregate_index, None);

    progress_tracker
        .complete_aggregate(session_id, aggregate_index);

    tracing::info!(
        "Nomenclature import completed: total_processed={}, inserted={}, updated={}",
        total_processed,
        total_inserted,
        total_updated
    );

    Ok(())
}

async fn process_nomenclature(
    odata: &a004_nomenclature::u501_import_from_ut::UtNomenclatureOData,
) -> Result<bool> {
    use uuid::Uuid;

    let existing = if !odata.ref_key.is_empty() {
        if let Ok(uuid) = Uuid::parse_str(&odata.ref_key) {
            a004_nomenclature::repository::get_by_id(uuid).await?
        } else {
            None
        }
    } else {
        None
    };

    if let Some(mut existing_item) = existing {
        existing_item.base.code = odata.code.clone();
        existing_item.base.description = odata.description.clone();
        existing_item.full_description = odata.full_description.clone().unwrap_or_default();
        existing_item.is_folder = odata.is_folder;
        existing_item.parent_id = odata
            .parent_key
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(|u| u.to_string());
        existing_item.article = odata.article.clone().unwrap_or_default();
        existing_item.base.metadata.is_deleted = odata.deletion_mark;
        existing_item.before_write();

        a004_nomenclature::repository::update(&existing_item).await?;
        Ok(false)
    } else {
        let mut new_item = odata.to_aggregate().map_err(|e| anyhow::anyhow!(e))?;
        new_item.before_write();

        match a004_nomenclature::repository::insert(&new_item).await {
            Ok(_) => Ok(true),
            Err(e) => Err(e),
        }
    }
}
