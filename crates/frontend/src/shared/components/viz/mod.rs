//! viz — примитивы отображения чисел.
//!
//! Плитка показателя, дельта, метр и спарклайн: то, что до сих пор каждая
//! страница писала заново. Стили лежат одним файлом в
//! `static/themes/core/viz.css`, геометрия — токенами в `variables.css`.
//!
//! Компоненты ничего не знают ни про источник данных, ни про каталог метрик:
//! на вход идут уже посчитанные значения и готовые подписи. Благодаря этому их
//! можно ставить и на дашборд индикаторов, и в список, и в системную страницу.

pub mod delta_chip;
pub mod meter;
pub mod sparkline;
pub mod stat_tile;

pub use delta_chip::DeltaChip;
pub use meter::Meter;
pub use sparkline::Sparkline;
pub use stat_tile::StatTile;
