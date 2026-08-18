use crate::domain::a020_wb_promotion::ui::details::api::PromotionNomenclatureDto;
use crate::domain::a020_wb_promotion::ui::details::view_model::WbPromotionDetailsVm;
use leptos::prelude::*;
use thaw::*;

#[component]
pub fn NomenclaturesTab(vm: WbPromotionDetailsVm) -> impl IntoView {
    let nomenclatures: RwSignal<Vec<PromotionNomenclatureDto>> = RwSignal::new(vec![]);

    Effect::new({
        let promotion = vm.promotion;
        move || {
            if let Some(promo) = promotion.get() {
                nomenclatures.set(promo.nomenclatures);
            }
        }
    });

    view! {
        <div style="padding: var(--spacing-lg);">
            <div style="margin-bottom: var(--spacing-md); display: flex; align-items: center; gap: 8px;">
                <span style="font-size: 13px; font-weight: 600;">"Товары в акции"</span>
                <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Informative>
                    {move || nomenclatures.get().len().to_string()}
                </Badge>
            </div>

            <Show
                when=move || !nomenclatures.get().is_empty()
                fallback=|| view! {
                    <div style="padding: var(--spacing-lg); color: var(--colorNeutralForeground3); text-align: center;">
                        "Нет данных о товарах"
                    </div>
                }
            >
                <div style="max-height: 500px; overflow-y: auto; border: 1px solid var(--colorNeutralStroke1); border-radius: var(--borderRadiusMedium);">
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHeaderCell>"#"</TableHeaderCell>
                                <TableHeaderCell>"nmId (Wildberries)"</TableHeaderCell>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            <For
                                each=move || nomenclatures.get().into_iter().enumerate()
                                key=|(i, _)| *i
                                children=move |(idx, item)| {
                                    view! {
                                        <TableRow>
                                            <TableCell>
                                                <TableCellLayout>
                                                    {(idx + 1).to_string()}
                                                </TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout>
                                                    {item.nm_id.to_string()}
                                                </TableCellLayout>
                                            </TableCell>
                                        </TableRow>
                                    }
                                }
                            />
                        </TableBody>
                    </Table>
                </div>
            </Show>
        </div>
    }
}
