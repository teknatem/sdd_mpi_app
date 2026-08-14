use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_SYSTEM;
use leptos::prelude::*;

/// Живой гид по реально используемым стилям проекта.
///
/// Каждый пример снабжён копируемым идентификатором (его настоящее имя класса
/// или токена) — нажми на чип, чтобы скопировать, и используй его в чате как
/// ссылку на нужный элемент (напр. `badge--accent`, `filter-panel`, `--color-primary`).
/// Работает во всех темах (dark / light / forest) — значения берутся из токенов.
#[component]
pub fn StyleGuidePage() -> impl IntoView {
    view! {
        <PageFrame page_id="sys_style_guide--system" category=PAGE_CAT_SYSTEM>
            <h1 class="sg-page-title">"Гид по стилям"</h1>
            <p class="sg-page-sub">
                "Каждый пример имеет копируемый идентификатор (чип "<code>"📋"</code>
                ") — это настоящее имя класса или токена. Скопируй и вставь в чат, "
                "чтобы сослаться на элемент."
            </p>

            // ─── A. Токены и типографика ──────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"A · Токены и типографика"</h2>
                <div class="sg-items">
                    <SgSwatch token="--color-primary"/>
                    <SgSwatch token="--color-success"/>
                    <SgSwatch token="--color-warning"/>
                    <SgSwatch token="--color-error"/>
                    <SgSwatch token="--color-accent"/>
                    <SgSwatch token="--color-bg-secondary"/>
                    <SgSwatch token="--color-surface"/>
                    <SgSwatch token="--color-border"/>

                    <SgItem id="--font-family-mono" note="JetBrains Mono">
                        <span class="sg-type-mono">"a015 · p904 · GL 41.02"</span>
                    </SgItem>
                    <SgItem id="u-tech-label" note="mono · uppercase · tracking">
                        <span class="u-tech-label">"Артикул поставщика"</span>
                    </SgItem>
                    <SgItem id="code-box" note="блок кода/SQL">
                        <span class="code-box">"SELECT * FROM p904_sales_data"</span>
                    </SgItem>
                    <SgItem id="text-muted" note="приглушённый текст">
                        <span class="text-muted">"Обновлено 5 минут назад"</span>
                    </SgItem>
                </div>
            </section>

            // ─── B. Кнопки ────────────────────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"B · Кнопки"</h2>
                <p class="sg-section__note">
                    "В проекте два набора кнопок. "<code>".button"</code>" — кастомный BEM-класс "
                    "на токенах (радиус выровнен под Thaw, 4px): лёгкий, для собственной разметки — "
                    "тулбары, хедеры страниц, кастомные формы без Thaw-обвязки. Thaw "<code>"<Button>"</code>
                    " (Fluent-компонент) — там, где экран уже построен на Thaw-компонентах (диалоги, "
                    "Thaw-формы). Правило: на одном экране придерживайтесь одного набора."
                </p>
                <div class="sg-items">
                    <SgItem id="button--primary">
                        <button class="button button--primary">"Сохранить"</button>
                    </SgItem>
                    <SgItem id="button--secondary">
                        <button class="button button--secondary">"Отмена"</button>
                    </SgItem>
                    <SgItem id="button--ghost">
                        <button class="button button--ghost">"Подробнее"</button>
                    </SgItem>
                    <SgItem id="button--success">
                        <button class="button button--success">"Провести"</button>
                    </SgItem>
                    <SgItem id="button--warning">
                        <button class="button button--warning">"Перепровести"</button>
                    </SgItem>
                    <SgItem id="button--small">
                        <button class="button button--primary button--small">"Мелкая"</button>
                    </SgItem>
                </div>
            </section>

            // ─── C. Формы ─────────────────────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"C · Формы"</h2>
                <div class="sg-items">
                    <SgItem id="form__group" note="label + input">
                        <div class="form__group" style="width: 100%;">
                            <label class="form__label">"Наименование"</label>
                            <input class="form__input" placeholder="Введите значение..."/>
                        </div>
                    </SgItem>
                    <SgItem id="form__select">
                        <div class="form__group" style="width: 100%;">
                            <label class="form__label">"Маркетплейс"</label>
                            <select class="form__select">
                                <option>"Wildberries"</option>
                                <option>"Ozon"</option>
                                <option>"Yandex Market"</option>
                            </select>
                        </div>
                    </SgItem>
                    <SgItem id="form__label" note="подпись поля">
                        <label class="form__label">"Дата документа"</label>
                    </SgItem>
                </div>
            </section>

            // ─── D. Таблицы ───────────────────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"D · Таблицы"</h2>
                <div class="sg-items">
                    <SgItem id="table--striped" wide=true note="table · table__head · table__header-cell · table__row · table__cell">
                        <div class="table" style="width: 100%;">
                            <table class="table__data table--striped">
                                <thead class="table__head">
                                    <tr>
                                        <th class="table__header-cell">"Код"</th>
                                        <th class="table__header-cell">"Документ"</th>
                                        <th class="table__header-cell">"Сумма"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    <tr class="table__row">
                                        <td class="table__cell"><a class="table__link">"a015"</a></td>
                                        <td class="table__cell">"WB заказы"</td>
                                        <td class="table__cell table__cell--right">"128 400 ₽"</td>
                                    </tr>
                                    <tr class="table__row">
                                        <td class="table__cell"><a class="table__link">"a034"</a></td>
                                        <td class="table__cell">"YM реализация"</td>
                                        <td class="table__cell table__cell--right">"96 210 ₽"</td>
                                    </tr>
                                </tbody>
                            </table>
                        </div>
                    </SgItem>
                    <SgItem id="table__link" note="ссылка-drilldown в ячейке">
                        <a class="table__link">"Открыть документ"</a>
                    </SgItem>
                </div>
            </section>

            // ─── E. Бэйджи ────────────────────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"E · Бэйджи"</h2>
                <div class="sg-items">
                    <SgItem id="badge--primary"><span class="badge badge--primary">"primary"</span></SgItem>
                    <SgItem id="badge--success"><span class="badge badge--success">"Проведён"</span></SgItem>
                    <SgItem id="badge--warning"><span class="badge badge--warning">"Черновик"</span></SgItem>
                    <SgItem id="badge--error"><span class="badge badge--error">"Ошибка"</span></SgItem>
                    <SgItem id="badge--neutral"><span class="badge badge--neutral">"neutral"</span></SgItem>
                    <SgItem id="badge--accent" note="новый акцент"><span class="badge badge--accent">"accent"</span></SgItem>
                </div>
            </section>

            // ─── F. Алерты и блоки ────────────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"F · Алерты и блоки"</h2>
                <div class="sg-items">
                    <SgItem id="alert--error">
                        <div class="alert alert--error" style="width: 100%; margin: 0;">"Не удалось загрузить данные"</div>
                    </SgItem>
                    <SgItem id="alert--success">
                        <div class="alert alert--success" style="width: 100%; margin: 0;">"Импорт завершён успешно"</div>
                    </SgItem>
                    <SgItem id="warning-box--error" wide=true>
                        <div class="warning-box warning-box--error" style="width: 100%;">
                            <span class="warning-box__icon">"⚠"</span>
                            <span class="warning-box__text">"Проверьте маппинг перед репостом (u508)"</span>
                        </div>
                    </SgItem>
                </div>
            </section>

            // ─── G. Панель фильтров ───────────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"G · Панель фильтров"</h2>
                <div class="sg-items">
                    <SgItem id="filter-panel" wide=true note="filter-panel-header · filter-panel__title · filter-panel__badge · filter-panel-content">
                        <div class="filter-panel" style="width: 100%; border: 1px solid var(--color-border); border-radius: var(--radius-md); overflow: hidden;">
                            <div class="filter-panel-header">
                                <div class="filter-panel-header__left">
                                    <span class="filter-panel__title">"Фильтры"</span>
                                    <span class="filter-panel__badge">"3"</span>
                                </div>
                                <div class="filter-panel-header__right">
                                    <span class="filter-panel__count">"124 записи"</span>
                                </div>
                            </div>
                            <div class="filter-panel-content">
                                <div class="form__group">
                                    <label class="form__label">"Период"</label>
                                    <input class="form__input" placeholder="2026-07"/>
                                </div>
                            </div>
                        </div>
                    </SgItem>
                </div>
            </section>

            // ─── H. Детали ────────────────────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"H · Детали документа"</h2>
                <div class="sg-items">
                    <SgItem id="details-section__title">
                        <h3 class="details-section__title" style="width: 100%;">"Основные реквизиты"</h3>
                    </SgItem>
                    <SgItem id="detail-grid" wide=true note="detail-grid · detail-grid__col">
                        <div class="detail-grid" style="width: 100%;">
                            <div class="detail-grid__col">
                                <div class="form__group">
                                    <label class="form__label">"Номер"</label>
                                    <span class="form__value">"WB-000128"</span>
                                </div>
                            </div>
                            <div class="detail-grid__col">
                                <div class="form__group">
                                    <label class="form__label">"Статус"</label>
                                    <span class="badge badge--success">"Проведён"</span>
                                </div>
                            </div>
                        </div>
                    </SgItem>
                </div>
            </section>

            // ─── I. Структура страницы ────────────────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"I · Структура страницы"</h2>
                <div class="sg-items">
                    <SgItem id="page__header" wide=true note="page__header · page__title · page__header-right">
                        <div class="sg-mock" style="width: 100%;">
                            <div class="page__header">
                                <div class="page__header-left">
                                    <h1 class="page__title">"WB заказы"</h1>
                                </div>
                                <div class="page__header-right">
                                    <button class="button button--primary button--small">"Импорт"</button>
                                </div>
                            </div>
                        </div>
                    </SgItem>
                    <SgItem id="modal-header" wide=true note="modal-header · modal-title · modal-body">
                        <div style="width: 100%; border: 1px solid var(--color-border); border-radius: var(--radius-md); overflow: hidden;">
                            <div class="modal-header">
                                <h2 class="modal-title">"Подтверждение"</h2>
                            </div>
                            <div class="modal-body">"Провести документ в главную книгу?"</div>
                        </div>
                    </SgItem>
                </div>
            </section>

            // ─── J. Новые стили (из унификации) ───────────────────────
            <section class="sg-section">
                <h2 class="sg-section__title u-tech-label">"J · Новые стили"</h2>
                <div class="sg-items">
                    <SgItem id="sg-activity--success" note="статус-тинт success">
                        <div class="sg-activity sg-activity--success">"✓ Проведено успешно"</div>
                    </SgItem>
                    <SgItem id="sg-activity--warning" note="статус-тинт warning">
                        <div class="sg-activity sg-activity--warning">"⚠ Требует внимания"</div>
                    </SgItem>
                    <SgItem id="sg-activity--error" note="статус-тинт error">
                        <div class="sg-activity sg-activity--error">"✕ Ошибка импорта"</div>
                    </SgItem>
                    <SgItem id="sg-focal" wide=true note="свечение фокус-узла (как .gldim-hero)">
                        <div class="sg-focal">"🧭 GL измерение · День 2026-07-22"</div>
                    </SgItem>
                    <SgItem id="sg-mode-card--success" note="тинт success">
                        <div class="sg-mode-card sg-mode-card--success" style="width: 100%;">
                            <div class="sg-mode-card__icon">"✅"</div>
                            <div class="sg-mode-card__title">"Auto-Reply"</div>
                            <div class="sg-mode-card__desc">"Рутинные ответы без участия человека."</div>
                        </div>
                    </SgItem>
                    <SgItem id="sg-mode-card--warning" note="тинт warning">
                        <div class="sg-mode-card sg-mode-card--warning" style="width: 100%;">
                            <div class="sg-mode-card__icon">"🤝"</div>
                            <div class="sg-mode-card__title">"Human-in-the-Loop"</div>
                            <div class="sg-mode-card__desc">"Агент готовит, человек проверяет."</div>
                        </div>
                    </SgItem>
                    <SgItem id="sg-mode-card--accent" note="тинт accent">
                        <div class="sg-mode-card sg-mode-card--accent" style="width: 100%;">
                            <div class="sg-mode-card__icon">"🎯"</div>
                            <div class="sg-mode-card__title">"Full Delegation"</div>
                            <div class="sg-mode-card__desc">"Агент выполняет задачи сам."</div>
                        </div>
                    </SgItem>
                    <SgItem id="sg-login-preview" wide=true note="фон логина: grid + glow (--login-*)">
                        <div class="sg-login-preview">
                            <div class="sg-login-preview__tag">"grid + glow"</div>
                        </div>
                    </SgItem>
                </div>
            </section>
        </PageFrame>
    }
}

/// Копирует текст в системный буфер обмена (браузерный Clipboard API).
fn copy_to_clipboard(text: String) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(window) = web_sys::window() {
            let nav = window.navigator().clipboard();
            let _ = nav.write_text(&text);
        }
    });
}

/// Один пример с копируемым идентификатором.
/// `id` — настоящее имя класса/токена; клик по чипу копирует его в буфер.
#[component]
fn SgItem(
    id: &'static str,
    /// Короткая подпись справа от чипа (например, входящие в пример под-классы).
    #[prop(optional)]
    note: &'static str,
    /// Растянуть карточку на всю ширину сетки (для широких примеров).
    #[prop(optional)]
    wide: bool,
    children: Children,
) -> impl IntoView {
    let copied = RwSignal::new(false);
    let item_class = if wide {
        "sg-item sg-item--wide"
    } else {
        "sg-item"
    };

    view! {
        <div class=item_class>
            <div class="sg-item__bar">
                <button
                    class=move || if copied.get() { "sg-item__id sg-item__id--copied" } else { "sg-item__id" }
                    title="Скопировать идентификатор"
                    on:click=move |_| {
                        copy_to_clipboard(id.to_string());
                        copied.set(true);
                    }
                >
                    <span class="sg-item__id-text">{id}</span>
                    <span class="sg-item__copy">{move || if copied.get() { "✓" } else { "📋" }}</span>
                </button>
                {(!note.is_empty()).then(|| view! { <span class="sg-item__note">{note}</span> })}
            </div>
            <div class="sg-item__demo">{children()}</div>
        </div>
    }
}

/// Плитка-образец цвета: показывает сам цвет и копируемое имя токена.
#[component]
fn SgSwatch(token: &'static str) -> impl IntoView {
    view! {
        <SgItem id=token>
            <div class="sg-swatch">
                <div class="sg-swatch__chip" style=format!("background: var({token});")></div>
            </div>
        </SgItem>
    }
}
