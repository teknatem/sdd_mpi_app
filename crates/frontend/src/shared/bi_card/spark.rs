//! Геометрия спарклайна: система координат, путь линии, шаг сетки.
//!
//! Раньше формула перевода значения в координату жила здесь и ещё раз — в
//! компоненте `Sparkline`, который рисовал по ней маркер. Две копии одной
//! математики расходятся молча: маркер просто перестаёт попадать на линию.
//! Поэтому шкала вынесена в `SparkScale`, а оба потребителя считают по ней.

/// Система координат спарклайна: `100 x 30`, отступ 2 единицы сверху и снизу.
///
/// Наружу график тянется через `width: 100%` при `preserveAspectRatio="none"`,
/// то есть по горизонтали координаты растягиваются произвольно. Всё, что должно
/// сохранить форму (точки, толщина линий), рисуется обводкой с
/// `vector-effect: non-scaling-stroke`, а не заливкой.
pub const VIEW_W: f64 = 100.0;
pub const VIEW_H: f64 = 30.0;

/// Верх и низ области данных внутри `VIEW_H`.
const PLOT_TOP: f64 = 4.0;
const PLOT_BOTTOM: f64 = 28.0;

/// Шкала ряда: связывает значения с координатами viewBox.
#[derive(Debug, Clone, Copy)]
pub struct SparkScale {
    pub min: f64,
    pub max: f64,
    /// Всегда > 0: у плоского ряда искусственно взят 1.0, иначе деление на ноль.
    pub range: f64,
    pub count: usize,
}

impl SparkScale {
    pub fn of(points: &[f64]) -> Self {
        let min = points.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = points.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let (min, max) = if points.is_empty() {
            (0.0, 0.0)
        } else {
            (min, max)
        };
        let range = if (max - min).abs() < 1e-9 {
            1.0
        } else {
            max - min
        };
        Self {
            min,
            max,
            range,
            count: points.len(),
        }
    }

    /// Значение → координата Y. Минимум ряда ложится на низ области данных,
    /// максимум — на верх; ось Y в SVG растёт вниз, отсюда вычитание.
    pub fn y_of(&self, value: f64) -> f64 {
        PLOT_BOTTOM - (value - self.min) / self.range * (PLOT_BOTTOM - PLOT_TOP)
    }

    /// Индекс точки → координата X. Единственная точка ставится в середину:
    /// у ряда из одного значения нет направления, и прижимать его к краю значит
    /// показывать несуществующий тренд.
    pub fn x_of(&self, index: usize) -> f64 {
        if self.count <= 1 {
            VIEW_W / 2.0
        } else {
            index as f64 / (self.count - 1) as f64 * VIEW_W
        }
    }

    /// Расширить диапазон так, чтобы значение попало в область данных.
    ///
    /// Нужно порогам: линия порога, лежащего выше максимума ряда, иначе ушла бы
    /// за край viewBox — а именно расстояние до порога и есть то, ради чего
    /// крупный график рисуется.
    pub fn including(mut self, value: f64) -> Self {
        if !value.is_finite() {
            return self;
        }
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.range = if (self.max - self.min).abs() < 1e-9 {
            1.0
        } else {
            self.max - self.min
        };
        self
    }

    /// Доля высоты области данных сверху — для HTML-подписей поверх графика.
    /// SVG `<text>` здесь непригоден: `preserveAspectRatio="none"` растягивает
    /// глифы вместе с координатами.
    pub fn y_percent(&self, value: f64) -> f64 {
        self.y_of(value) / VIEW_H * 100.0
    }
}

/// «Круглый» шаг сетки из ряда 1 / 2 / 5 · 10ⁿ, покрывающий диапазон примерно
/// за `target_lines` линий.
///
/// Смысл в подписях: 0 / 5 / 10 / 15 читается с одного взгляда, а
/// 0 / 4,7 / 9,4 / 14,1 приходится расшифровывать — при том что линий столько же.
pub fn nice_step(range: f64, target_lines: usize) -> f64 {
    let lines = target_lines.max(1) as f64;
    if !range.is_finite() || range <= 0.0 {
        return 1.0;
    }
    let rough = range / lines;
    let magnitude = 10_f64.powf(rough.log10().floor());
    let normalized = rough / magnitude;
    // Пороги 1,5 / 3 / 7 округляют к ближайшей «круглой» ступени, а не вверх:
    // округление вверх даёт вдвое меньше линий, чем просили, и сетка редеет.
    let factor = if normalized < 1.5 {
        1.0
    } else if normalized < 3.0 {
        2.0
    } else if normalized < 7.0 {
        5.0
    } else {
        10.0
    };
    factor * magnitude
}

/// Отметки сетки с «круглым» шагом внутри `[min, max]`.
pub fn grid_ticks(min: f64, max: f64, target_lines: usize) -> Vec<f64> {
    let step = nice_step(max - min, target_lines);
    if !step.is_finite() || step <= 0.0 {
        return Vec::new();
    }
    let first = (min / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = first;
    // Ограничение сверху — защита от зацикливания на вырожденных данных
    // (например, шаг, потерявший точность рядом с f64::MAX).
    while value <= max + step * 1e-9 && ticks.len() < 12 {
        ticks.push(value);
        value += step;
    }
    ticks
}

/// Generate SVG `d` attribute strings for a sparkline.
///
/// Returns `(line_d, fill_d)` where:
/// - `line_d` is a polyline path (M … L … L …)
/// - `fill_d` is the same with the bottom closed (for a filled area)
///
/// The output is normalised to fit in a `100 x 30` viewBox.
pub fn points_to_svg_path(points: &[f64]) -> (String, String) {
    if points.is_empty() {
        let line = "M0 15 L100 15".to_string();
        let fill = "M0 15 L100 15 L100 30 L0 30 Z".to_string();
        return (line, fill);
    }

    path_with_scale(points, &SparkScale::of(points))
}

/// Тот же путь, но по заранее подготовленной шкале — когда диапазон расширен
/// порогами и не совпадает с диапазоном самого ряда.
pub fn path_with_scale(points: &[f64], scale: &SparkScale) -> (String, String) {
    if points.is_empty() {
        let line = "M0 15 L100 15".to_string();
        let fill = "M0 15 L100 15 L100 30 L0 30 Z".to_string();
        return (line, fill);
    }

    let coords: Vec<(f64, f64)> = points
        .iter()
        .enumerate()
        .map(|(i, &v)| (scale.x_of(i), scale.y_of(v)))
        .collect();

    let mut line = String::new();
    for (i, (x, y)) in coords.iter().enumerate() {
        if i == 0 {
            line.push_str(&format!("M{:.1} {:.1}", x, y));
        } else {
            line.push_str(&format!(" L{:.1} {:.1}", x, y));
        }
    }

    let fill = format!(
        "{} L{:.1} 30 L{:.1} 30 Z",
        line,
        coords.last().map(|p| p.0).unwrap_or(100.0),
        coords.first().map(|p| p.0).unwrap_or(0.0),
    );

    (line, fill)
}

/// Default demo sparkline data (8 points, upward trend with noise).
pub fn demo_spark_points() -> Vec<f64> {
    vec![23.0, 22.0, 18.0, 19.0, 14.0, 12.0, 9.0, 7.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_pins_min_to_bottom_and_max_to_top() {
        let scale = SparkScale::of(&[5.0, 10.0, 15.0]);
        assert!((scale.y_of(5.0) - PLOT_BOTTOM).abs() < 1e-9);
        assert!((scale.y_of(15.0) - PLOT_TOP).abs() < 1e-9);
        assert!((scale.y_of(10.0) - 16.0).abs() < 1e-9);
    }

    #[test]
    fn flat_series_does_not_divide_by_zero() {
        let scale = SparkScale::of(&[7.0, 7.0, 7.0]);
        assert_eq!(scale.range, 1.0);
        assert!(scale.y_of(7.0).is_finite());
    }

    #[test]
    fn single_point_sits_in_the_middle() {
        let scale = SparkScale::of(&[3.0]);
        assert_eq!(scale.x_of(0), VIEW_W / 2.0);
    }

    #[test]
    fn x_spans_the_full_width() {
        let scale = SparkScale::of(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(scale.x_of(0), 0.0);
        assert_eq!(scale.x_of(3), VIEW_W);
    }

    #[test]
    fn nice_step_stays_on_the_1_2_5_ladder() {
        assert_eq!(nice_step(10.0, 4), 2.0);
        assert_eq!(nice_step(100.0, 4), 20.0);
        assert_eq!(nice_step(1.0, 4), 0.2);
        assert_eq!(nice_step(37.0, 4), 10.0);
        // Вырожденный диапазон не должен давать нулевой или бесконечный шаг.
        assert_eq!(nice_step(0.0, 4), 1.0);
    }

    #[test]
    fn grid_ticks_land_on_round_values_inside_the_range() {
        let ticks = grid_ticks(3.0, 17.0, 4);
        assert_eq!(ticks, vec![5.0, 10.0, 15.0]);
        assert!(ticks.iter().all(|t| *t >= 3.0 && *t <= 17.0));
    }

    #[test]
    fn path_generator_matches_the_shared_scale() {
        let points = vec![1.0, 5.0, 3.0];
        let (line, _) = points_to_svg_path(&points);
        let scale = SparkScale::of(&points);
        assert!(line.starts_with(&format!("M{:.1} {:.1}", scale.x_of(0), scale.y_of(1.0))));
    }
}
