//! Вкладка «Диагностика»: карточка текущего экземпляра и замеченные странности.
//!
//! Это ответ на вопрос «куда я вообще смотрю»: при работе с двумя одинаковыми
//! на вид копиями приложения перепутать рабочий инстанс с тестовым — самая
//! дорогая из возможных ошибок на этой странице. Вкладка собирает всё, что
//! описывает сам экземпляр: БД, пути, сборку, S3 и аномалии раскладки.

use contracts::system::datasets::{AnomalySeverity, DatasetsStatusDto, InstanceEnv};
use leptos::prelude::*;

use crate::shared::date_utils::format_bytes_compact;
use crate::shared::icons::icon;
use crate::system::maintenance::MaintenanceToggle;

pub fn env_badge_class(env: InstanceEnv) -> &'static str {
    match env {
        InstanceEnv::Production => "badge badge--error",
        InstanceEnv::Staging => "badge badge--warning",
        InstanceEnv::Dev => "badge badge--info",
        InstanceEnv::Unknown => "badge badge--neutral",
    }
}

#[component]
pub fn DiagnosticsTab(status: RwSignal<Option<DatasetsStatusDto>>) -> impl IntoView {
    view! {
        // Режим обслуживания живёт здесь: он про этот же экземпляр и нужен
        // ровно тем же операциям переноса, что и остальная страница.
        <MaintenanceToggle />
        <section class="datasets__section">
            {move || {
                let Some(status) = status.get() else {
                    return view! {
                        <div class="text-muted">"Состояние экземпляра ещё не загружено."</div>
                    }.into_any();
                };
                let instance = status.instance;
                let anomalies = status.anomalies;
                let sets = status.sets;

                // Итог по локальным наборам: сколько данных вообще лежит под
                // корнем — цифра, за которой чаще всего и приходят в диагностику.
                let (set_files, set_bytes) = sets
                    .iter()
                    .filter(|set| set.exists)
                    .fold((0_u64, 0_u64), |(files, bytes), set| {
                        (files + set.file_count, bytes + set.total_bytes)
                    });
                let skipped: u64 = sets.iter().map(|set| set.skipped_count).sum();

                let s3_line = if instance.s3_ready {
                    format!(
                        "{} · {}",
                        instance.s3_bucket.clone().unwrap_or_default(),
                        instance.s3_endpoint.clone().unwrap_or_default()
                    )
                } else {
                    instance.s3_error.clone().unwrap_or_else(|| "не настроено".to_string())
                };

                view! {
                    <div class="datasets__instance-head">
                        <span class="datasets__instance-label">{instance.instance_label.clone()}</span>
                        <span class=env_badge_class(instance.instance_env)>
                            {instance.instance_env.label_ru()}
                        </span>
                        <span class="datasets__mono text-muted">{instance.instance_id.clone()}</span>
                    </div>

                    <div class="datasets__diag-group">
                        <h3 class="datasets__diag-title">"Экземпляр"</h3>
                        <dl class="datasets__facts">
                            <Fact label="Машина" value=instance.hostname.clone() />
                            <Fact label="Операционная система" value=instance.os.clone() />
                            <Fact
                                label="Сборка"
                                value=format!(
                                    "{} · {} · {}",
                                    instance.app_version, instance.git_commit, instance.build_profile,
                                )
                            />
                            <Fact label="Файл конфигурации" value=instance.config_path.clone() />
                        </dl>
                    </div>

                    <div class="datasets__diag-group">
                        <h3 class="datasets__diag-title">"База данных"</h3>
                        <dl class="datasets__facts">
                            <Fact label="Файл базы" value=instance.database_path.clone() />
                            <Fact label="Версия схемы" value=instance.schema_version.to_string() />
                        </dl>
                    </div>

                    <div class="datasets__diag-group">
                        <h3 class="datasets__diag-title">"Файловые данные"</h3>
                        <dl class="datasets__facts">
                            <Fact
                                label="Корень данных"
                                value=instance.data_root.clone().unwrap_or_else(|| "— не задан —".to_string())
                            />
                            <Fact
                                label="Наборов на диске"
                                value=format!(
                                    "{} из {}",
                                    sets.iter().filter(|set| set.exists).count(),
                                    sets.len(),
                                )
                            />
                            <Fact
                                label="Всего файлов"
                                value=format!("{} · {}", set_files, format_bytes_compact(set_bytes))
                            />
                            {(skipped > 0).then(|| view! {
                                <Fact label="Пропущено при сканировании" value=skipped.to_string() />
                            })}
                        </dl>
                    </div>

                    <div class="datasets__diag-group">
                        <h3 class="datasets__diag-title">"Хранилище S3"</h3>
                        <dl class="datasets__facts">
                            <div class="datasets__fact">
                                <dt>"Состояние"</dt>
                                <dd class="datasets__mono">
                                    <span class=if instance.s3_ready {
                                        "badge badge--success"
                                    } else {
                                        "badge badge--neutral"
                                    }>
                                        {if instance.s3_ready { "готово" } else { "недоступно" }}
                                    </span>
                                    " "
                                    {s3_line}
                                </dd>
                            </div>
                        </dl>
                    </div>

                    {(!anomalies.is_empty()).then(|| view! {
                        <div class="datasets__diag-group">
                            <h3 class="datasets__diag-title">"Замечания по раскладке"</h3>
                            <div class="datasets__anomalies">
                                {anomalies.iter().map(|anomaly| {
                                    let class = match anomaly.severity {
                                        AnomalySeverity::Warning => "alert alert--warning",
                                        AnomalySeverity::Info => "alert alert--info",
                                    };
                                    let icon_name = match anomaly.severity {
                                        AnomalySeverity::Warning => "alert-triangle",
                                        AnomalySeverity::Info => "info",
                                    };
                                    view! {
                                        <div class=class>
                                            {icon(icon_name)}
                                            <div class="datasets__anomaly-body">
                                                <div>{anomaly.message.clone()}</div>
                                                {anomaly.path.clone().map(|path| view! {
                                                    <div class="datasets__mono text-muted">{path}</div>
                                                })}
                                                {anomaly.hint.clone().map(|hint| view! {
                                                    <div class="text-muted">{hint}</div>
                                                })}
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    })}
                }.into_any()
            }}
        </section>
    }
}

#[component]
fn Fact(label: &'static str, #[prop(into)] value: String) -> impl IntoView {
    view! {
        <div class="datasets__fact">
            <dt>{label}</dt>
            <dd class="datasets__mono">{value}</dd>
        </div>
    }
}
