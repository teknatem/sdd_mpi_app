use crate::shared::api_utils::api_base;
use contracts::domain::a004_nomenclature::aggregate::{Nomenclature, NomenclatureDto};
use contracts::domain::a004_nomenclature::orders_dto::{
    NomenclatureOrderRowDto, NomenclatureOrdersResponse,
};
use contracts::projections::p912_nomenclature_costs::dto::{
    NomenclatureCostDto, NomenclatureCostListResponse,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NomenclatureBarcodeDto {
    pub barcode: String,
    pub source: String,
    pub nomenclature_ref: Option<String>,
    pub nomenclature_name: Option<String>,
    pub article: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarcodesByNomenclatureResponse {
    pub nomenclature_ref: String,
    pub barcodes: Vec<NomenclatureBarcodeDto>,
    pub total_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionValuesResponse {
    pub dim1_category: Vec<String>,
    pub dim2_line: Vec<String>,
    pub dim3_model: Vec<String>,
    pub dim4_format: Vec<String>,
    pub dim5_sink: Vec<String>,
    pub dim6_size: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseNomenclatureInfo {
    pub name: String,
    pub article: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitVariantInfo {
    pub id: String,
    pub code: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitComponentInfo {
    pub nomenclature_ref: String,
    pub quantity: f64,
    pub description: String,
    pub article: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DealerPriceDto {
    pub period: String,
    pub price: f64,
    pub source: String, // "Текущая" или "Базовая"
    pub nomenclature_ref: String,
}

pub async fn fetch_by_id(id: String) -> Result<Nomenclature, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/nomenclature/{}", api_base(), id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if resp.status() == 404 {
        return Err("Not found".to_string());
    }
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let data: Nomenclature = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(data)
}

pub async fn save_form(dto: NomenclatureDto) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let body = serde_json::to_string(&dto).map_err(|e| format!("{e}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body));

    let url = format!("{}/api/nomenclature", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let fetch_promise = window.fetch_with_request(&request);
    let resp_value = wasm_bindgen_futures::JsFuture::from(fetch_promise)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;

    if !resp.ok() {
        // Попытка извлечь детали ошибки из ответа
        #[derive(serde::Deserialize)]
        struct ErrorResponse {
            error: Option<String>,
        }
        if let Ok(error_data) = serde_json::from_str::<ErrorResponse>(&text) {
            if let Some(error_msg) = error_data.error {
                return Err(format!("Ошибка сохранения: {}", error_msg));
            }
        }
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }

    #[derive(serde::Deserialize)]
    struct SaveResponse {
        id: String,
    }
    let data: SaveResponse = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(data.id)
}

#[allow(dead_code)]
pub async fn delete_by_id(id: String) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("DELETE");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/nomenclature/{}", api_base(), id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if resp.status() == 404 {
        return Err("Not found".to_string());
    }
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

pub async fn fetch_barcodes_by_nomenclature(
    nomenclature_ref: String,
) -> Result<BarcodesByNomenclatureResponse, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!(
        "{}/api/p901/nomenclature/{}/barcodes?include_inactive=false",
        api_base(),
        nomenclature_ref
    );
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if resp.status() == 404 {
        return Ok(BarcodesByNomenclatureResponse {
            nomenclature_ref,
            barcodes: vec![],
            total_count: 0,
        });
    }
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let data: BarcodesByNomenclatureResponse =
        serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(data)
}

/// Загрузить только количество штрихкодов (без самих данных)
pub async fn fetch_barcodes_count(nomenclature_ref: String) -> Result<usize, String> {
    match fetch_barcodes_by_nomenclature(nomenclature_ref).await {
        Ok(response) => Ok(response.total_count),
        Err(_) => Ok(0),
    }
}

pub async fn fetch_dimension_values() -> Result<DimensionValuesResponse, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/nomenclature/dimensions", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let data: DimensionValuesResponse = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(data)
}

/// Загрузить информацию о базовой номенклатуре (артикул и название)
pub async fn fetch_base_nomenclature_info(id: &str) -> Result<BaseNomenclatureInfo, String> {
    if id.is_empty() {
        return Ok(BaseNomenclatureInfo {
            name: String::new(),
            article: String::new(),
        });
    }

    match fetch_by_id(id.to_string()).await {
        Ok(nomenclature) => Ok(BaseNomenclatureInfo {
            name: nomenclature.base.description.clone(),
            article: nomenclature.article.clone(),
        }),
        Err(_e) => {
            // Failed to fetch, return ID instead
            Ok(BaseNomenclatureInfo {
                name: format!("[{}]", id),
                article: String::new(),
            })
        }
    }
}

pub async fn fetch_kit_variant_info(id: &str) -> Result<KitVariantInfo, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/a022/kit-variant/{}", api_base(), id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;

    Ok(KitVariantInfo {
        id: json
            .get("base")
            .and_then(|b| b.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string(),
        code: json
            .get("base")
            .and_then(|b| b.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        description: json
            .get("base")
            .and_then(|b| b.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

pub async fn fetch_kit_components(id: &str) -> Result<Vec<KitComponentInfo>, String> {
    let url = format!("{}/api/a022/kit-variant/{}", api_base(), id);
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch kit variant: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    let kit_variant: contracts::domain::a022_kit_variant::aggregate::KitVariant = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse kit variant: {}", e))?;

    let mut components = Vec::new();
    for item in kit_variant.parse_goods() {
        let nom_url = format!("{}/api/nomenclature/{}", api_base(), item.nomenclature_ref);
        let nom_response = gloo_net::http::Request::get(&nom_url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch component nomenclature: {}", e))?;

        if nom_response.status() == 200 {
            let nomenclature: Nomenclature = nom_response
                .json()
                .await
                .map_err(|e| format!("Failed to parse component nomenclature: {}", e))?;
            components.push(KitComponentInfo {
                nomenclature_ref: nomenclature.base.id.0.to_string(),
                quantity: item.quantity,
                description: nomenclature.base.description,
                article: nomenclature.article,
            });
        } else {
            components.push(KitComponentInfo {
                nomenclature_ref: item.nomenclature_ref,
                quantity: item.quantity,
                description: String::new(),
                article: String::new(),
            });
        }
    }

    Ok(components)
}

pub async fn fetch_production_costs(
    nomenclature_ref: &str,
) -> Result<Vec<NomenclatureCostDto>, String> {
    let url = format!(
        "{}/api/p912/nomenclature-costs?nomenclature_ref={}&limit=1000",
        api_base(),
        nomenclature_ref
    );

    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch production costs: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    let mut items = response
        .json::<NomenclatureCostListResponse>()
        .await
        .map_err(|e| format!("Failed to parse production costs: {}", e))?
        .items;

    items.sort_by(|a, b| {
        a.period
            .cmp(&b.period)
            .then(a.registrator_type.cmp(&b.registrator_type))
            .then(a.registrator_ref.cmp(&b.registrator_ref))
            .then(a.line_no.cmp(&b.line_no))
    });

    Ok(items)
}

pub async fn fetch_nomenclature_orders(
    nomenclature_ref: &str,
    days: u32,
) -> Result<Vec<NomenclatureOrderRowDto>, String> {
    let url = format!(
        "{}/api/nomenclature/{}/orders?days={}",
        api_base(),
        nomenclature_ref,
        days
    );

    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch orders: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    Ok(response
        .json::<NomenclatureOrdersResponse>()
        .await
        .map_err(|e| format!("Failed to parse orders: {}", e))?
        .items)
}

/// Загрузить только количество дилерских цен (без самих данных)
pub async fn fetch_dealer_prices_count(
    nomenclature_ref: String,
    base_ref: Option<String>,
) -> Result<usize, String> {
    match fetch_dealer_prices_by_nomenclature(nomenclature_ref, base_ref).await {
        Ok(prices) => Ok(prices.len()),
        Err(_) => Ok(0),
    }
}

/// Загрузить дилерские цены для номенклатуры (и базовой, если есть)
pub async fn fetch_dealer_prices_by_nomenclature(
    nomenclature_ref: String,
    base_ref: Option<String>,
) -> Result<Vec<DealerPriceDto>, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    #[derive(serde::Deserialize)]
    struct PriceWithNomenclature {
        period: String,
        price: f64,
        nomenclature_ref: String,
    }

    #[derive(serde::Deserialize)]
    struct PricesResponse {
        items: Vec<PriceWithNomenclature>,
    }

    let mut all_prices: Vec<DealerPriceDto> = Vec::new();

    // Загрузить цены текущей номенклатуры
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!(
        "{}/api/p906/nomenclature-prices?nomenclature_ref={}&limit=1000",
        api_base(),
        nomenclature_ref
    );
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if resp.ok() {
        let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
            .await
            .map_err(|e| format!("{e:?}"))?;
        let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
        if let Ok(data) = serde_json::from_str::<PricesResponse>(&text) {
            for item in data.items {
                all_prices.push(DealerPriceDto {
                    period: item.period,
                    price: item.price,
                    source: "Текущая".to_string(),
                    nomenclature_ref: item.nomenclature_ref,
                });
            }
        }
    }

    // Загрузить цены базовой номенклатуры (если есть)
    if let Some(base_ref_id) = base_ref {
        if !base_ref_id.is_empty() {
            let opts = RequestInit::new();
            opts.set_method("GET");
            opts.set_mode(RequestMode::Cors);

            let url = format!(
                "{}/api/p906/nomenclature-prices?nomenclature_ref={}&limit=1000",
                api_base(),
                base_ref_id
            );
            let request =
                Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
            request
                .headers()
                .set("Accept", "application/json")
                .map_err(|e| format!("{e:?}"))?;

            let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
            let resp_value =
                wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
                    .await
                    .map_err(|e| format!("{e:?}"))?;
            let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

            if resp.ok() {
                let text = wasm_bindgen_futures::JsFuture::from(
                    resp.text().map_err(|e| format!("{e:?}"))?,
                )
                .await
                .map_err(|e| format!("{e:?}"))?;
                let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
                if let Ok(data) = serde_json::from_str::<PricesResponse>(&text) {
                    for item in data.items {
                        all_prices.push(DealerPriceDto {
                            period: item.period,
                            price: item.price,
                            source: "Базовая".to_string(),
                            nomenclature_ref: item.nomenclature_ref,
                        });
                    }
                }
            }
        }
    }

    // Сортировать по дате (ASC - старые сверху)
    all_prices.sort_by(|a, b| a.period.cmp(&b.period));

    Ok(all_prices)
}
