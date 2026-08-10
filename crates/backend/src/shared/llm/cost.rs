//! Стоимость прогона: накопление разбивки токенов и её пересчёт в деньги.
//!
//! Зачем отдельный модуль: до этого на сообщении хранился только `tokens_used`
//! (сумма), и вопрос «во сколько обошёлся ответ» не имел ответа — вход и выход
//! тарифицируются по разным ставкам. Числитель качества (вердикты) уже есть,
//! здесь появляется знаменатель.
//!
//! Деньги считаются в **микроединицах валюты** целыми числами: копить стоимость
//! в `f64` и суммировать её по тысячам сообщений — способ получить расхождение
//! в отчёте на ровном месте.

use super::types::LlmResponse;

/// Накопитель разбивки токенов за один ответ пользователю.
///
/// Один ответ — это все итерации цикла инструментов плюс вызов классификатора:
/// пользователю виден один пузырь, платим мы за все обращения к модели.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UsageTotals {
    pub prompt: i64,
    pub completion: i64,
    /// Часть `prompt`, обслуженная кэшем провайдера. Это подмножество `prompt`,
    /// а не добавка к нему (семантика OpenAI `prompt_tokens_details.cached_tokens`).
    pub cached_prompt: i64,
}

impl UsageTotals {
    pub fn add_response(&mut self, response: &LlmResponse) {
        self.add(
            response.prompt_tokens.unwrap_or(0),
            response.completion_tokens.unwrap_or(0),
            response.cached_prompt_tokens.unwrap_or(0),
        );
    }

    pub fn add(&mut self, prompt: i32, completion: i32, cached_prompt: i32) {
        self.prompt += prompt.max(0) as i64;
        self.completion += completion.max(0) as i64;
        self.cached_prompt += cached_prompt.max(0) as i64;
    }

    pub fn is_empty(&self) -> bool {
        self.prompt == 0 && self.completion == 0
    }

    fn clamp_i32(value: i64) -> Option<i32> {
        (value > 0).then(|| value.min(i32::MAX as i64) as i32)
    }

    pub fn prompt_i32(&self) -> Option<i32> {
        Self::clamp_i32(self.prompt)
    }

    pub fn completion_i32(&self) -> Option<i32> {
        Self::clamp_i32(self.completion)
    }

    pub fn cached_prompt_i32(&self) -> Option<i32> {
        Self::clamp_i32(self.cached_prompt)
    }
}

/// Прайс подключения: ставки за миллион токенов в валюте подключения.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    /// Ставка за кэшированный вход. `None` означает «скидки нет» — считаем по
    /// входной ставке. Занижать стоимость по умолчанию нельзя: незаполненное поле
    /// не должно выглядеть как бесплатный кэш.
    pub cached_input_per_mtok: Option<f64>,
}

impl Pricing {
    /// Собрать прайс из полей подключения. `None`, если не заполнено ничего —
    /// тогда стоимость честно не считается, а не записывается нулём.
    pub fn from_fields(
        input_per_mtok: Option<f64>,
        output_per_mtok: Option<f64>,
        cached_input_per_mtok: Option<f64>,
    ) -> Option<Self> {
        if input_per_mtok.is_none() && output_per_mtok.is_none() {
            return None;
        }
        Some(Self {
            input_per_mtok: input_per_mtok.unwrap_or(0.0).max(0.0),
            output_per_mtok: output_per_mtok.unwrap_or(0.0).max(0.0),
            cached_input_per_mtok: cached_input_per_mtok.filter(|v| *v >= 0.0),
        })
    }
}

/// Стоимость в микроединицах валюты (1e-6).
///
/// Тождество вывода: `цена = (токены / 1_000_000) × ставка_за_миллион`, а
/// микроединиц в этом ровно `токены × ставка_за_миллион` — поэтому деление
/// не нужно и точность не теряется на нём.
pub fn cost_micro(usage: &UsageTotals, pricing: Option<Pricing>) -> Option<i64> {
    let pricing = pricing?;
    if usage.is_empty() {
        return None;
    }

    // Кэшированные токены — часть prompt: по полной ставке идёт только остаток.
    let cached = usage.cached_prompt.min(usage.prompt);
    let fresh_prompt = usage.prompt - cached;
    let cached_rate = pricing
        .cached_input_per_mtok
        .unwrap_or(pricing.input_per_mtok);

    let micro = fresh_prompt as f64 * pricing.input_per_mtok
        + cached as f64 * cached_rate
        + usage.completion as f64 * pricing.output_per_mtok;

    Some(micro.round().max(0.0) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(prompt: i64, completion: i64, cached: i64) -> UsageTotals {
        UsageTotals {
            prompt,
            completion,
            cached_prompt: cached,
        }
    }

    #[test]
    fn no_pricing_means_no_cost_not_zero_cost() {
        assert_eq!(cost_micro(&usage(1000, 500, 0), None), None);
        assert_eq!(Pricing::from_fields(None, None, None), None);
    }

    #[test]
    fn zero_price_is_a_real_price() {
        // Локальная модель бесплатна — это ноль, а не «не посчитано».
        let pricing = Pricing::from_fields(Some(0.0), Some(0.0), None);
        assert_eq!(cost_micro(&usage(1000, 500, 0), pricing), Some(0));
    }

    #[test]
    fn million_tokens_costs_exactly_the_rate() {
        let pricing = Pricing::from_fields(Some(3.0), Some(15.0), None);
        // 1M входа по 3.0 = 3.0 валюты = 3_000_000 микроединиц.
        assert_eq!(
            cost_micro(&usage(1_000_000, 0, 0), pricing),
            Some(3_000_000)
        );
        assert_eq!(
            cost_micro(&usage(0, 1_000_000, 0), pricing),
            Some(15_000_000)
        );
        assert_eq!(cost_micro(&usage(1000, 500, 0), pricing), Some(3000 + 7500));
    }

    #[test]
    fn cached_tokens_are_a_subset_of_prompt_and_discounted() {
        let pricing = Pricing::from_fields(Some(3.0), Some(15.0), Some(0.3));
        // 1000 prompt, из них 800 из кэша: 200×3.0 + 800×0.3 = 600 + 240.
        assert_eq!(cost_micro(&usage(1000, 0, 800), pricing), Some(840));
    }

    #[test]
    fn missing_cached_rate_does_not_discount() {
        let pricing = Pricing::from_fields(Some(3.0), Some(15.0), None);
        assert_eq!(
            cost_micro(&usage(1000, 0, 800), pricing),
            cost_micro(&usage(1000, 0, 0), pricing)
        );
    }

    #[test]
    fn cached_over_prompt_is_clamped() {
        // Провайдер прислал несогласованную разбивку — стоимость не должна уйти в минус.
        let pricing = Pricing::from_fields(Some(3.0), Some(15.0), Some(0.3));
        assert_eq!(cost_micro(&usage(100, 0, 5000), pricing), Some(30));
    }

    #[test]
    fn totals_accumulate_across_iterations_and_ignore_negatives() {
        let mut totals = UsageTotals::default();
        totals.add(100, 20, 10);
        totals.add(-5, 30, 0);
        assert_eq!(totals, usage(100, 50, 10));
        assert_eq!(totals.prompt_i32(), Some(100));
        assert_eq!(UsageTotals::default().prompt_i32(), None);
    }
}
