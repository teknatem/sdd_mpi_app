//! Инструменты чата, объявленные срезом.
//!
//! **Зачем.** Реестр инструментов — такой же хаб-перечисление, как реестр
//! регистраторов или каталог Действий: `skills::tool_bundles()` собирает
//! определения, `tool_executor::ROUTING_TABLE` маршрутизирует вызов,
//! `skill_policy` знает, какому инструменту нужно право. Пока срез добавлял
//! инструмент правкой этих трёх мест, ядро чата знало имена прикладных
//! модулей: одна строка в бандлах, один `use` в исполнителе, одно имя в
//! списке мутирующих.
//!
//! Инструмент, принадлежащий срезу, объявляется здесь целиком — определения,
//! маршрут и требуемое право в одном месте. Состав реестра перечисляет
//! `composition::llm_tools`.
//!
//! **Ядро своих инструментов сюда не переносит.** Их пятнадцать наборов, они
//! часть механизма чата, и разворачивать их в провайдеры незачем: реестр
//! существует ради того, чтобы **прикладной** срез не правил ядро.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use serde_json::Value;

use super::tool_executor::ToolContext;
use super::types::ToolDefinition;

/// Набор инструментов, принадлежащий срезу.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Бандл навыка, в который попадают инструменты (`"funnel_repair"`).
    /// Совпадение с бандлом ядра допустимо — определения складываются.
    fn bundle(&self) -> &'static str;

    /// Имена инструментов. Единственный источник маршрутизации: имя, которого
    /// здесь нет, не будет ни вызвано, ни проверено.
    fn tool_names(&self) -> &'static [&'static str];

    fn definitions(&self) -> Vec<ToolDefinition>;

    /// Право, без которого инструмент не выполняется (`skill_policy::*`).
    /// `None` — достаточно активного набора.
    fn required_capability(&self, _tool_name: &str) -> Option<&'static str> {
        None
    }

    async fn execute(&self, name: &str, arguments: &str, cx: &ToolContext<'_>) -> Value;
}

static REGISTRY: OnceLock<Vec<Arc<dyn ToolProvider>>> = OnceLock::new();

/// Установить реестр. Зовётся один раз из `composition::install_all()`.
///
/// # Panics
/// При повторной установке и при конфликте имён инструментов — в том числе с
/// именами ядра: два набора под одним именем маршрутизировались бы по первому
/// совпадению, то есть молча.
pub fn install(providers: Vec<Arc<dyn ToolProvider>>) {
    let mut seen: HashSet<&'static str> = HashSet::new();
    for provider in &providers {
        for name in provider.tool_names() {
            if !seen.insert(name) {
                panic!("инструмент '{name}' объявлен двумя провайдерами");
            }
            if super::tool_executor::is_core_tool_name(name) {
                panic!("инструмент '{name}' уже объявлен ядром чата");
            }
        }
    }
    if REGISTRY.set(providers).is_err() {
        panic!("реестр инструментов срезов уже установлен");
    }
}

/// Все провайдеры в порядке установки.
///
/// # Panics
/// Если реестр не установлен. Пустой реестр по умолчанию был бы хуже паники:
/// инструмент среза просто исчез бы из выдачи, а чат ответил бы «инструмент не
/// активен» — то есть отказом, неотличимым от отсутствия права.
pub fn all() -> &'static [Arc<dyn ToolProvider>] {
    REGISTRY.get().map(Vec::as_slice).expect(
        "реестр инструментов срезов не установлен: composition::install_all() не был вызван",
    )
}

/// Провайдер, которому принадлежит инструмент.
pub fn find(tool_name: &str) -> Option<&'static Arc<dyn ToolProvider>> {
    all()
        .iter()
        .find(|provider| provider.tool_names().contains(&tool_name))
}

/// Бандлы срезов — в том же виде, в каком их складывает `skills::tool_bundles`.
pub fn bundles() -> Vec<(&'static str, Vec<ToolDefinition>)> {
    all()
        .iter()
        .map(|provider| (provider.bundle(), provider.definitions()))
        .collect()
}

/// Право, требуемое инструментом среза.
pub fn required_capability(tool_name: &str) -> Option<&'static str> {
    find(tool_name).and_then(|provider| provider.required_capability(tool_name))
}
