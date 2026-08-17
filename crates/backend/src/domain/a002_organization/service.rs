use super::repository;
use contracts::domain::a002_organization::aggregate::{Organization, OrganizationDto};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

/// Создание новой организации
pub async fn create(db: &DatabaseConnection, dto: OrganizationDto) -> anyhow::Result<Uuid> {
    let code = dto
        .code
        .clone()
        .unwrap_or_else(|| format!("ORG-{}", Uuid::new_v4()));
    let mut aggregate = Organization::new_for_insert(
        code,
        dto.description,
        dto.full_name,
        dto.inn,
        dto.kpp,
        dto.comment,
    );

    // Валидация
    aggregate
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;

    // Before write
    aggregate.before_write();

    // Сохранение через repository
    repository::insert(db, &aggregate).await
}

/// Обновление существующей организации
pub async fn update(db: &DatabaseConnection, dto: OrganizationDto) -> anyhow::Result<()> {
    let id = dto
        .id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| anyhow::anyhow!("Invalid ID"))?;

    let mut aggregate = repository::get_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Not found"))?;

    aggregate.update(&dto);

    // Валидация
    aggregate
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;

    // Before write
    aggregate.before_write();

    // Сохранение
    repository::update(db, &aggregate).await
}

/// Мягкое удаление организации
pub async fn delete(db: &DatabaseConnection, id: Uuid) -> anyhow::Result<bool> {
    repository::soft_delete(db, id).await
}

/// Получение организации по ID
pub async fn get_by_id(db: &DatabaseConnection, id: Uuid) -> anyhow::Result<Option<Organization>> {
    repository::get_by_id(db, id).await
}

/// Получение списка всех организаций
pub async fn list_all(db: &DatabaseConnection) -> anyhow::Result<Vec<Organization>> {
    repository::list_all(db).await
}

/// Вставка тестовых данных
pub async fn insert_test_data(db: &DatabaseConnection) -> anyhow::Result<()> {
    let data = vec![
        OrganizationDto {
            id: None,
            code: Some("ORG-001".into()),
            description: "ООО \"Рога и Копыта\"".into(),
            full_name: "Общество с ограниченной ответственностью \"Рога и Копыта\"".into(),
            inn: "7701234567".into(),
            kpp: "770101001".into(),
            comment: Some("Основная организация для тестирования".into()),
            entity_ref: None,
        },
        OrganizationDto {
            id: None,
            code: Some("ORG-002".into()),
            description: "ИП Иванов И.И.".into(),
            full_name: "Индивидуальный предприниматель Иванов Иван Иванович".into(),
            inn: "771234567890".into(),
            kpp: "".into(),
            comment: Some("Индивидуальный предприниматель".into()),
            entity_ref: None,
        },
        OrganizationDto {
            id: None,
            code: Some("ORG-003".into()),
            description: "АО \"Ромашка\"".into(),
            full_name: "Акционерное общество \"Ромашка\"".into(),
            inn: "7702345678".into(),
            kpp: "770201001".into(),
            comment: None,
            entity_ref: None,
        },
    ];

    for dto in data {
        create(db, dto).await?;
    }

    Ok(())
}
