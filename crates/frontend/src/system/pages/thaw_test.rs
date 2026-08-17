use crate::shared::components::date_range_picker_exp::DateRangePickerExp;
use crate::shared::components::date_range_picker_v2::DateRangePickerV2;
use crate::shared::components::date_range_picker_v3::DateRangePickerV3;
use crate::shared::components::date_range_picker_v4::DateRangePickerV4;
use crate::shared::page_frame::PageFrame;
use leptos::prelude::*;
use std::collections::HashSet;
use thaw::*;

/// Тестовая страница для проверки совместимости компонентов Thaw UI
#[component]
pub fn ThawTestPage() -> impl IntoView {
    let count = RwSignal::new(0);
    let text = RwSignal::new(String::new());
    let selected = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let checked = RwSignal::new(false);
    let checkbox_group = RwSignal::new(HashSet::new());
    let switch_value = RwSignal::new(false);
    let exp_from = RwSignal::new(String::new());
    let exp_to = RwSignal::new(String::new());
    let v2_from = RwSignal::new(String::new());
    let v2_to = RwSignal::new(String::new());
    let v3_from = RwSignal::new(String::new());
    let v3_to = RwSignal::new(String::new());
    let v4_from = RwSignal::new(String::new());
    let v4_to = RwSignal::new(String::new());

    view! {
        <PageFrame page_id="sys_thaw_test--system" category="system">
            <h1 style="margin-bottom: 20px; font-size: 24px; font-weight: bold;">
                "Тестирование компонентов Thaw UI"
            </h1>

            // Секция кнопок
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "1. Кнопки (Buttons)"
                </h2>

                <div style="display: flex; gap: 10px; flex-wrap: wrap; margin-bottom: 15px;">
                    <Button on_click=move |_| {
                        count.update(|n| *n += 1);
                    }>
                        {move || format!("Счётчик: {}", count.get())}
                    </Button>

                    <Button
                        appearance=ButtonAppearance::Primary
                        on_click=move |_| {
                            count.set(0);
                        }
                    >
                        "Сбросить"
                    </Button>

                    <Button
                        appearance=ButtonAppearance::Subtle
                        on_click=move |_| {
                            loading.update(|l| *l = !*l);
                        }
                    >
                        {move || if loading.get() { "Остановить" } else { "Запустить" }}
                    </Button>

                    <Button
                        appearance=ButtonAppearance::Transparent
                        disabled=Signal::from(loading)
                    >
                        "Transparent"
                    </Button>

                    <Button
                        shape=ButtonShape::Circular
                    >
                        "+"
                    </Button>

                    <Button
                        shape=ButtonShape::Square
                    >
                        "■"
                    </Button>
                </div>

                <div style="padding: 10px; background-color: var(--color-bg-secondary); border-radius: 4px;">
                    <p><strong>"Результат:"</strong> {move || format!("Счётчик = {}, Загрузка = {}", count.get(), loading.get())}</p>
                </div>
            </div>

            // Секция полей ввода
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "2. Поля ввода (Input)"
                </h2>

                <div style="max-width: 400px;">
                    <Input
                        value=text
                        placeholder="Введите текст..."
                    />
                </div>

                <div style="margin-top: 10px; padding: 10px; background-color: var(--color-bg-secondary); border-radius: 4px;">
                    <p><strong>"Введённый текст:"</strong> {move || text.get()}</p>
                </div>
            </div>

            // Секция выбора
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "3. Выпадающий список (Select)"
                </h2>

                <div style="max-width: 400px;">
                    <Select value=selected>
                        <option value="">"-- Выберите --"</option>
                        <option value="option1">"Опция 1"</option>
                        <option value="option2">"Опция 2"</option>
                        <option value="option3">"Опция 3"</option>
                    </Select>
                </div>

                <div style="margin-top: 10px; padding: 10px; background-color: var(--color-bg-secondary); border-radius: 4px;">
                    <p><strong>"Выбрано:"</strong> {move || {
                        let val = selected.get();
                        if val.is_empty() { "Ничего не выбрано".to_string() } else { val }
                    }}</p>
                </div>
            </div>

            // Секция индикаторов
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "4. Индикаторы загрузки"
                </h2>

                <Show when=move || loading.get()>
                    <div style="display: flex; gap: 20px; align-items: center;">
                        <Spinner />
                        <span>"Загрузка..."</span>
                    </div>
                </Show>

                <Show when=move || !loading.get()>
                    <p style="color: var(--color-text-secondary);">"Нажмите 'Запустить' чтобы увидеть спиннер"</p>
                </Show>
            </div>

            // Секция Checkbox и Switch
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "5. Checkbox и Switch"
                </h2>

                <div style="margin-bottom: 20px;">
                    <Checkbox checked=checked label="Checkbox с привязкой"/>
                    <div style="margin-top: 5px; padding: 5px; background-color: var(--color-bg-secondary); border-radius: 4px; display: inline-block;">
                        <span>{move || if checked.get() { "Выбрано ✓" } else { "Не выбрано" }}</span>
                    </div>
                </div>

                <div style="margin-bottom: 20px;">
                    <h3 style="margin: 10px 0; font-size: 14px;">"Группа Checkbox:"</h3>
                    <CheckboxGroup value=checkbox_group>
                        <div style="display: flex; flex-direction: column; gap: 10px;">
                            <Checkbox label="Option A" value="a"/>
                            <Checkbox label="Option B" value="b"/>
                            <Checkbox label="Option C" value="c"/>
                        </div>
                    </CheckboxGroup>
                    <div style="margin-top: 10px; padding: 10px; background-color: var(--color-bg-secondary); border-radius: 4px;">
                        <p><strong>"Выбрано:"</strong> {move || format!("{:?}", checkbox_group.get())}</p>
                    </div>
                </div>

                <div style="margin-bottom: 20px;">
                    <h3 style="margin: 10px 0; font-size: 14px;">"Switch:"</h3>
                    <div>
                        <Switch checked=switch_value label="Toggle Switch"/>
                    </div>
                    <div style="margin-top: 5px; padding: 5px; background-color: var(--color-bg-secondary); border-radius: 4px; display: inline-block;">
                        <span>{move || if switch_value.get() { "Включено" } else { "Выключено" }}</span>
                    </div>
                </div>
            </div>

            // Секция Badge
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "6. Badge (Значки)"
                </h2>

                <div style="margin-bottom: 20px;">
                    <h3 style="margin-bottom: 10px; font-size: 14px;">"Разные стили:"</h3>
                    <Flex gap=FlexGap::Large>
                        <Badge appearance=BadgeAppearance::Filled>"Filled"</Badge>
                        <Badge appearance=BadgeAppearance::Ghost>"Ghost"</Badge>
                        <Badge appearance=BadgeAppearance::Outline>"Outline"</Badge>
                        <Badge appearance=BadgeAppearance::Tint>"Tint"</Badge>
                    </Flex>
                </div>

                <div style="margin-bottom: 20px;">
                    <h3 style="margin: 10px 0; font-size: 14px;">"Цвета:"</h3>
                    <Flex gap=FlexGap::Large>
                        <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Brand>"Brand"</Badge>
                        <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Danger>"Danger"</Badge>
                        <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Success>"Success"</Badge>
                        <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Warning>"Warning"</Badge>
                        <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Important>"Important"</Badge>
                    </Flex>
                </div>

                <div style="margin-bottom: 20px;">
                    <h3 style="margin: 10px 0; font-size: 14px;">"С счётчиком:"</h3>
                    <Flex gap=FlexGap::Large>
                        <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Danger>
                            {move || count.get().to_string()}
                        </Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Brand>
                            {move || format!("{}+", count.get())}
                        </Badge>
                    </Flex>
                </div>
            </div>

            // Секция Layout (Space и Flex)
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "7. Layout компоненты (Space & Flex)"
                </h2>

                <div style="margin-bottom: 20px;">
                    <h3 style="margin-bottom: 10px; font-size: 14px;">"Flex Justify:"</h3>
                    <div style="border: 1px dashed var(--color-border); padding: 10px; margin-bottom: 10px;">
                        <Flex justify=FlexJustify::SpaceAround>
                            <Badge>"1"</Badge>
                            <Badge>"2"</Badge>
                            <Badge>"3"</Badge>
                        </Flex>
                    </div>
                    <div style="border: 1px dashed var(--color-border); padding: 10px; margin-bottom: 10px;">
                        <Flex justify=FlexJustify::Center>
                            <Badge>"Center"</Badge>
                            <Badge>"Center"</Badge>
                        </Flex>
                    </div>
                    <div style="border: 1px dashed var(--color-border); padding: 10px;">
                        <Flex justify=FlexJustify::End>
                            <Badge>"End"</Badge>
                            <Badge>"End"</Badge>
                        </Flex>
                    </div>
                </div>

                <div style="margin-bottom: 20px;">
                    <h3 style="margin: 10px 0; font-size: 14px;">"Space с разными gap:"</h3>
                    <Space gap=SpaceGap::Large>
                        <Button>"Large Gap"</Button>
                        <Button>"Button 2"</Button>
                        <Button>"Button 3"</Button>
                    </Space>
                </div>
            </div>

            // DateRangePickerExp
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "8. DateRangePickerExp (экспериментальный)"
                </h2>
                <p style="margin-bottom: 12px; color: var(--color-text-secondary); font-size: 14px;">
                    "Режимы Г/М/Н/Д, текстовый ввод dd.mm.yyyy (01022026), интервал кликом начало→конец в диалоге."
                </p>
                <DateRangePickerExp
                    date_from=Signal::derive(move || exp_from.get())
                    date_to=Signal::derive(move || exp_to.get())
                    on_change=Callback::new(move |(from, to)| {
                        exp_from.set(from);
                        exp_to.set(to);
                    })
                    label="Период:".to_string()
                />
                <div style="margin-top: 12px; padding: 10px; background-color: var(--color-bg-secondary); border-radius: 4px;">
                    <p>
                        <strong>"ISO from → to: "</strong>
                        {move || format!("{} → {}", exp_from.get(), exp_to.get())}
                    </p>
                </div>
            </div>

            // DateRangePickerV2
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "9. DateRangePickerV2 (экспериментальный, поколение 2)"
                </h2>
                <p style="margin-bottom: 12px; color: var(--color-text-secondary); font-size: 14px;">
                    "Ввод без масок: 01022026 / 1.2.26 / 0102 (год сам) / 5 (день). Enter или 8-я цифра — переход во второе поле, ↑↓ — ±день (с Shift ±месяц). Чип справа открывает пресеты и выбор интервала двумя кликами."
                </p>
                <DateRangePickerV2
                    date_from=Signal::derive(move || v2_from.get())
                    date_to=Signal::derive(move || v2_to.get())
                    on_change=Callback::new(move |(from, to)| {
                        v2_from.set(from);
                        v2_to.set(to);
                    })
                    label="Период:".to_string()
                />
                <div style="margin-top: 12px; padding: 10px; background-color: var(--color-bg-secondary); border-radius: 4px;">
                    <p>
                        <strong>"ISO from → to: "</strong>
                        {move || format!("{} → {}", v2_from.get(), v2_to.get())}
                    </p>
                </div>
            </div>

            // DateRangePickerV3
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "10. DateRangePickerV3 (экспериментальный, поколение 3)"
                </h2>
                <p style="margin-bottom: 12px; color: var(--color-text-secondary); font-size: 14px;">
                    "Явные режимы Год/Мес/Нед/День. Можно набрать 01022026 с автопереходом, короткую дату 0102 с автоматическим годом или вставить сразу диапазон 01.02.2026 — 28.02.2026. Выбор интервала — пресетом либо двумя кликами."
                </p>
                <DateRangePickerV3
                    date_from=Signal::derive(move || v3_from.get())
                    date_to=Signal::derive(move || v3_to.get())
                    on_change=Callback::new(move |(from, to)| {
                        v3_from.set(from);
                        v3_to.set(to);
                    })
                    label="Период:".to_string()
                />
                <div style="margin-top: 12px; padding: 10px; background-color: var(--color-bg-secondary); border-radius: 4px;">
                    <p>
                        <strong>"ISO from → to: "</strong>
                        {move || format!("{} → {}", v3_from.get(), v3_to.get())}
                    </p>
                </div>
            </div>

            // DateRangePickerV4
            <div style="margin-bottom: 30px; padding: 20px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--card-bg);">
                <h2 style="margin-bottom: 15px; font-size: 18px; font-weight: 600;">
                    "11. DateRangePickerV4 (компактная полоса + большой поповер)"
                </h2>
                <p style="margin-bottom: 12px; color: var(--color-text-secondary); font-size: 14px;">
                    "На полосе только период и ←/→. Пиктограмма линейки открывает большой поповер: единица, пресеты в один клик, интервал двумя кликами."
                </p>
                <DateRangePickerV4
                    date_from=Signal::derive(move || v4_from.get())
                    date_to=Signal::derive(move || v4_to.get())
                    on_change=Callback::new(move |(from, to)| {
                        v4_from.set(from);
                        v4_to.set(to);
                    })
                    label="Период:".to_string()
                />
                <div style="margin-top: 12px; padding: 10px; background-color: var(--color-bg-secondary); border-radius: 4px;">
                    <p>
                        <strong>"ISO from → to: "</strong>
                        {move || format!("{} → {}", v4_from.get(), v4_to.get())}
                    </p>
                </div>
            </div>
            <div style="margin-top: 30px; padding: 20px; background: var(--activity-success-bg); border: 1px solid var(--color-success); border-radius: 8px;">
                <h2 style="margin-bottom: 10px; font-size: 18px; font-weight: 600; color: var(--color-success);">
                    "✅ Статус совместимости"
                </h2>
                <p style="margin: 5px 0;"><strong>"Leptos версия:"</strong> " 0.8"</p>
                <p style="margin: 5px 0;"><strong>"Thaw версия:"</strong> " 0.5.0-beta"</p>
                <p style="margin: 5px 0; color: var(--color-success);">
                    "Все компоненты загружены успешно. Библиотека Thaw совместима с проектом!"
                </p>
                <div style="margin-top: 15px; padding: 10px; background: var(--color-bg-secondary); border-radius: 4px;">
                    <p style="font-size: 14px; margin: 5px 0;">
                        <strong>"Протестированные компоненты:"</strong>
                    </p>
                    <Flex gap=FlexGap::Large style="margin-top: 10px; flex-wrap: wrap;">
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Button"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Input"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Select"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Spinner"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Checkbox"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Switch"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Badge"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Space"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"Flex"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"DateRangePickerExp"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"DateRangePickerV2"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"DateRangePickerV3"</Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Success>"DateRangePickerV4"</Badge>
                    </Flex>
                </div>
            </div>
        </PageFrame>
    }
}
