use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// Лёгковесный счётчик изменений для одного домена.
/// Инкрементируется при любом изменении, клиент сравнивает с запомненным значением.
pub struct ChangeToken(AtomicU64);

impl ChangeToken {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn bump(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Реестр токенов, которые отдаёт `GET /api/sys/change-tokens`.
///
/// Раньше хендлер перечислял домены сам и потому знал имена агрегатов —
/// ровно та зависимость ядра от прикладного слоя, из-за которой срез нельзя
/// вынести в отдельный крейт. Теперь состав объявляет composition root.
static REGISTRY: OnceLock<Vec<(&'static str, &'static ChangeToken)>> = OnceLock::new();

/// Установить состав реестра. Зовётся один раз из `composition::install_all()`.
///
/// # Panics
/// При повторной установке: два разных состава означали бы, что фронт видит
/// не все токены и не обновляет часть списков.
pub fn install(tokens: Vec<(&'static str, &'static ChangeToken)>) {
    if REGISTRY.set(tokens).is_err() {
        panic!("реестр токенов изменений уже установлен");
    }
}

/// Снимок всех токенов: имя домена → текущее значение счётчика.
///
/// До установки реестра — пусто. Это не ошибка: единственный потребитель —
/// поллинг фронта, которому пустой ответ говорит «изменений нет».
pub fn snapshot() -> Vec<(&'static str, u64)> {
    REGISTRY
        .get()
        .map(|tokens| {
            tokens
                .iter()
                .map(|(name, token)| (*name, token.get()))
                .collect()
        })
        .unwrap_or_default()
}
