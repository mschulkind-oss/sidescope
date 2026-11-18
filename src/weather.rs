//! Explicit-location, non-commercial Open-Meteo adapters. Air quality is modeled,
//! not PurpleAir or a nearby sensor; credentials are never requested or stored.
use crate::{
    Card,
    dashboard::{card, metric, unavailable},
};
use serde::Deserialize;

pub const REFRESH_SECS: u32 = 1800;
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Celsius,
    Fahrenheit,
}
impl Units {
    fn query(self) -> &'static str {
        match self {
            Self::Celsius => "celsius",
            Self::Fahrenheit => "fahrenheit",
        }
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Celsius => "°C",
            Self::Fahrenheit => "°F",
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherConfig {
    pub latitude: f64,
    pub longitude: f64,
    pub place: String,
    #[serde(default)]
    pub units: Units,
}
impl WeatherConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.latitude)
            || !self.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
        {
            return Err("Weather latitude/longitude out of range".into());
        }
        if self.place.trim().is_empty()
            || self.place.chars().count() > 80
            || self.place.chars().any(char::is_control)
        {
            return Err("Weather place must be a nonempty label of at most 80 characters".into());
        }
        Ok(())
    }
    pub fn forecast_url(&self) -> String {
        format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code&hourly=temperature_2m,weather_code&daily=temperature_2m_max,temperature_2m_min,weather_code&forecast_days=5&timezone=auto&temperature_unit={}",
            self.latitude,
            self.longitude,
            self.units.query()
        )
    }
    pub fn air_url(&self) -> String {
        format!(
            "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}&current=us_aqi,pm2_5&timezone=auto&forecast_days=1",
            self.latitude, self.longitude
        )
    }
}

#[derive(Deserialize)]
struct Forecast {
    current: CurrentWeather,
    current_units: TemperatureUnit,
    hourly: Hourly,
    daily: Daily,
    timezone: String,
    #[serde(default)]
    utc_offset_seconds: i64,
}
#[derive(Deserialize)]
struct TemperatureUnit {
    temperature_2m: String,
}
#[derive(Deserialize)]
struct CurrentWeather {
    time: String,
    temperature_2m: Option<f64>,
    weather_code: Option<u16>,
}
#[derive(Deserialize)]
struct Hourly {
    time: Vec<String>,
    temperature_2m: Vec<Option<f64>>,
    weather_code: Vec<Option<u16>>,
}
#[derive(Deserialize)]
struct Daily {
    time: Vec<String>,
    temperature_2m_max: Vec<Option<f64>>,
    temperature_2m_min: Vec<Option<f64>>,
    weather_code: Vec<Option<u16>>,
}

pub fn conditions(code: u16) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 | 77 => "Snow",
        80..=82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Unknown conditions",
    }
}
/// Temperatures in tenths of the configured unit keep the presentation Eq/testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherDisplay {
    pub units: Units,
    pub temperature: i32,
    pub code: u16,
    pub hours: Vec<HourForecast>,
    pub days: Vec<DayForecast>,
}
impl WeatherDisplay {
    pub fn hourly_description(&self) -> String {
        self.hours
            .iter()
            .map(|h| {
                format!(
                    "{} {} · {}",
                    &h.time[11..16],
                    h.temperature
                        .map(|n| temperature_text(Some(n), self.units))
                        .unwrap_or_else(|| "Temperature unavailable".into()),
                    h.code.map(conditions).unwrap_or("Conditions unavailable")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourForecast {
    pub time: String,
    pub temperature: Option<i32>,
    pub code: Option<u16>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayForecast {
    pub date: String,
    pub low: Option<i32>,
    pub high: Option<i32>,
    pub code: Option<u16>,
}
fn tenths(value: Option<f64>) -> Option<i32> {
    value
        .filter(|n| n.is_finite() && (n * 10.0).abs() <= f64::from(i32::MAX))
        .map(|n| (n * 10.0).round() as i32)
}
pub fn temperature_text(value: Option<i32>, units: Units) -> String {
    value
        .map(|n| format!("{:.0}{}", f64::from(n) / 10.0, units.symbol()))
        .unwrap_or_else(|| "—".into())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Sun,
    PartlyCloudy,
    Cloud,
    Fog,
    Rain,
    Snow,
    Storm,
    Unknown,
}
pub fn glyph(code: Option<u16>) -> Glyph {
    match code {
        Some(0) => Glyph::Sun,
        Some(1 | 2) => Glyph::PartlyCloudy,
        Some(3) => Glyph::Cloud,
        Some(45 | 48) => Glyph::Fog,
        Some(51 | 53 | 55 | 56 | 57 | 61 | 63 | 65 | 66 | 67 | 80..=82) => Glyph::Rain,
        Some(71 | 73 | 75 | 77 | 85 | 86) => Glyph::Snow,
        Some(95 | 96 | 99) => Glyph::Storm,
        _ => Glyph::Unknown,
    }
}
fn valid_time(time: &str) -> bool {
    time.len() == 16
        && time.as_bytes()[10] == b'T'
        && gtk::glib::DateTime::from_iso8601(&format!("{time}:00Z"), None).is_ok()
}
fn valid_date(date: &str) -> bool {
    date.len() == 10 && valid_time(&format!("{date}T00:00"))
}

pub fn forecast_card(json: &str, config: &WeatherConfig) -> Result<Card, String> {
    config.validate()?;
    let f: Forecast =
        serde_json::from_str(json).map_err(|_| "Invalid Open-Meteo weather response")?;
    if f.current_units.temperature_2m != config.units.symbol()
        || !valid_time(&f.current.time)
        || f.timezone.len() > 80
    {
        return Err("Invalid weather units or time".into());
    }
    let current = tenths(f.current.temperature_2m).ok_or("Current temperature unavailable")?;
    let code = f
        .current
        .weather_code
        .ok_or("Current conditions unavailable")?;
    let h = &f.hourly;
    let d = &f.daily;
    if h.time.len() != h.temperature_2m.len()
        || h.time.len() != h.weather_code.len()
        || d.time.len() != d.temperature_2m_min.len()
        || d.time.len() != d.temperature_2m_max.len()
        || d.time.len() != d.weather_code.len()
        || d.time.len() < 5
        || h.time.iter().any(|t| !valid_time(t))
        || d.time.iter().any(|t| !valid_date(t))
        || h.time.windows(2).any(|p| p[0] >= p[1])
        || d.time.windows(2).any(|p| p[0] >= p[1])
    {
        return Err("Incomplete or unordered weather forecast".into());
    }
    let mut c = card(
        "weather",
        "Weather",
        &format!("{} · {}", config.place, f.timezone),
        vec![],
    );
    let hours: Vec<_> = h
        .time
        .iter()
        .enumerate()
        .filter(|(_, time)| time.as_str() >= f.current.time.as_str())
        .take(12)
        .collect();
    if hours.len() < 12 {
        return Err("Hourly forecast unavailable".into());
    }
    c.weather = Some(WeatherDisplay {
        units: config.units,
        temperature: current,
        code,
        hours: hours
            .into_iter()
            .map(|(i, time)| HourForecast {
                time: time.clone(),
                temperature: tenths(h.temperature_2m[i]),
                code: h.weather_code[i],
            })
            .collect(),
        days: d
            .time
            .iter()
            .take(5)
            .enumerate()
            .map(|(i, date)| {
                let (low, high) = (
                    tenths(d.temperature_2m_min[i]),
                    tenths(d.temperature_2m_max[i]),
                );
                let valid = !matches!((low, high), (Some(lo), Some(hi)) if lo > hi);
                DayForecast {
                    date: date.clone(),
                    low: low.filter(|_| valid),
                    high: high.filter(|_| valid),
                    code: d.weather_code[i],
                }
            })
            .collect(),
    });
    c.note = format!(
        "Open-Meteo · CC BY 4.0\nModel {} · auto 30 min",
        crate::freshness_text(
            crate::current_unix_time(),
            &f.current.time,
            f.utc_offset_seconds
        )
    );
    Ok(c)
}

#[derive(Deserialize)]
struct Air {
    current: CurrentAir,
    #[serde(default)]
    utc_offset_seconds: i64,
}
#[derive(Deserialize)]
struct CurrentAir {
    time: String,
    us_aqi: Option<f64>,
    pm2_5: Option<f64>,
}
pub fn air_card(json: &str, config: &WeatherConfig) -> Result<Card, String> {
    config.validate()?;
    let f: Air = serde_json::from_str(json).map_err(|_| "Invalid Open-Meteo air response")?;
    if !valid_time(&f.current.time) {
        return Err("Invalid air quality time".into());
    }
    let aqi = f
        .current
        .us_aqi
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or("Modeled AQI unavailable")?;
    let pm = f
        .current
        .pm2_5
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or("Modeled PM2.5 unavailable")?;
    let category = match aqi.ceil() as u64 {
        0..=50 => "Good",
        51..=100 => "Moderate",
        101..=150 => "Unhealthy for sensitive groups",
        151..=200 => "Unhealthy",
        201..=300 => "Very unhealthy",
        _ => "Hazardous",
    };
    let mut c = card(
        "air",
        "Air quality",
        &format!("{} · modeled, not a sensor", config.place),
        vec![
            metric("US AQI", format!("{aqi:.0}"), category),
            metric(
                "PM2.5",
                format!("{pm:.1} µg/m³"),
                "CAMS forecast · not PurpleAir",
            ),
        ],
    );
    c.note = format!(
        "Open-Meteo / CAMS ENSEMBLE\nCC BY 4.0 · model {}\nAuto 30 min · not a health advisory",
        crate::freshness_text(
            crate::current_unix_time(),
            &f.current.time,
            f.utc_offset_seconds
        )
    );
    Ok(c)
}
pub fn unconfigured_cards() -> Vec<Card> {
    vec![
        unavailable(
            "weather",
            "Weather",
            "Set [weather] latitude, longitude, place and units in local config.toml. No location is guessed.",
        ),
        unavailable(
            "air",
            "Air quality",
            "Configure an explicit weather location to enable Open-Meteo/CAMS modeled air quality (not PurpleAir).",
        ),
    ]
}
pub fn demo_cards() -> Vec<Card> {
    let config = WeatherConfig {
        latitude: 0.0,
        longitude: 0.0,
        place: "Sample place (synthetic)".into(),
        units: Units::Celsius,
    };
    vec![
        forecast_card(include_str!("../tests/fixtures/weather.json"), &config)
            .expect("synthetic forecast fixture"),
        air_card(include_str!("../tests/fixtures/air.json"), &config)
            .expect("synthetic air fixture"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> WeatherConfig {
        WeatherConfig {
            latitude: 40.0,
            longitude: -74.0,
            place: "Test place".into(),
            units: Units::Celsius,
        }
    }
    #[test]
    fn explicit_location_and_units_are_validated_without_tracking() {
        let mut c = config();
        assert!(c.validate().is_ok());
        c.latitude = f64::NAN;
        assert!(c.validate().is_err());
        c.latitude = 91.0;
        assert!(c.validate().is_err());
        c.latitude = 40.0;
        c.place.clear();
        assert!(c.validate().is_err());
        let c = crate::parse_config(
            "[weather]\nlatitude=40.0\nlongitude=-74.0\nplace='Example'\nunits='fahrenheit'",
        )
        .unwrap()
        .weather
        .unwrap();
        assert!(c.forecast_url().contains("temperature_unit=fahrenheit"));
        assert!(
            c.air_url()
                .starts_with("https://air-quality-api.open-meteo.com/")
        );
        assert!(crate::parse_config("[weather]\nlatitude=40.0\nlongitude=-74.0").is_err());
        assert!(
            crate::parse_config(
                "[weather]\nlatitude=40.0\nlongitude=-74.0\nplace='Example'\nunits='kelvin'"
            )
            .is_err()
        );
    }
    #[test]
    fn forecast_contains_current_twelve_hours_five_days_and_attribution() {
        let c = forecast_card(include_str!("../tests/fixtures/weather.json"), &config()).unwrap();
        assert!(
            c.metrics.is_empty(),
            "forecast must use a bounded structured view, not 18 metric blocks"
        );
        let display = c.weather.as_ref().unwrap();
        assert_eq!(
            temperature_text(Some(display.temperature), display.units),
            "18°C"
        );
        assert_eq!(display.hours.len(), 12);
        assert_eq!(display.days.len(), 5);
        assert_eq!(display.hours[0].time, "2026-10-05T12:00");
        assert_eq!(display.days[0].date, "2026-10-05");
        assert!(c.note.contains("Open-Meteo"));
        assert_eq!(conditions(999), "Unknown conditions");
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/weather.json")).unwrap();
        v["current"]["temperature_2m"] = serde_json::Value::Null;
        assert!(forecast_card(&v.to_string(), &config()).is_err());
        v["current"]["temperature_2m"] = 18.into();
        v["hourly"]["time"][0] = "not a time".into();
        assert!(forecast_card(&v.to_string(), &config()).is_err());
        assert!(forecast_card("{}", &config()).is_err());
        let mut c = config();
        c.units = Units::Fahrenheit;
        assert!(forecast_card(include_str!("../tests/fixtures/weather.json"), &c).is_err());
    }
    #[test]
    fn fahrenheit_and_missing_hourly_values_are_honest() {
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/weather.json")).unwrap();
        v["current_units"]["temperature_2m"] = "°F".into();
        v["current"]["temperature_2m"] = 65.into();
        v["hourly"]["temperature_2m"][12] = serde_json::Value::Null;
        v["hourly"]["weather_code"][12] = serde_json::Value::Null;
        let mut cfg = config();
        cfg.units = Units::Fahrenheit;
        let c = forecast_card(&v.to_string(), &cfg).unwrap();
        let display = c.weather.as_ref().unwrap();
        assert_eq!(
            temperature_text(Some(display.temperature), display.units),
            "65°F"
        );
        assert_eq!(display.hours[0].temperature, None);
        assert_eq!(
            temperature_text(display.hours[0].temperature, display.units),
            "—"
        );
        assert_eq!(display.hours[0].code, None);
        assert!(
            display
                .hourly_description()
                .starts_with("12:00 Temperature unavailable · Conditions unavailable")
        );
        v["daily"]["temperature_2m_min"][0] = 100.into();
        v["daily"]["temperature_2m_max"][0] = 10.into();
        let c = forecast_card(&v.to_string(), &cfg).unwrap();
        let day = &c.weather.unwrap().days[0];
        assert_eq!((day.low, day.high), (None, None));
        v["daily"]["time"] = serde_json::json!([]);
        assert!(forecast_card(&v.to_string(), &cfg).is_err());
        assert!(crate::AppConfig::default().weather.is_none());
    }

    #[test]
    fn weather_glyphs_group_supported_codes_and_leave_unknowns_honest() {
        assert_eq!(glyph(Some(0)), Glyph::Sun);
        assert_eq!(glyph(Some(2)), Glyph::PartlyCloudy);
        assert_eq!(glyph(Some(3)), Glyph::Cloud);
        assert_eq!(glyph(Some(45)), Glyph::Fog);
        assert_eq!(glyph(Some(61)), Glyph::Rain);
        assert_eq!(glyph(Some(73)), Glyph::Snow);
        assert_eq!(glyph(Some(99)), Glyph::Storm);
        for code in [None, Some(58), Some(72), Some(999)] {
            assert_eq!(glyph(code), Glyph::Unknown);
        }
    }

    #[test]
    fn air_is_explicitly_modeled_and_missing_data_never_becomes_zero() {
        let c = air_card(include_str!("../tests/fixtures/air.json"), &config()).unwrap();
        assert_eq!(c.metrics[0].value, "42");
        assert_eq!(c.metrics[0].detail, "Good");
        assert!(c.subtitle.contains("modeled, not a sensor"));
        assert!(c.note.contains("CAMS ENSEMBLE"));
        assert!(
            air_card(
                r#"{"current":{"time":"2026-10-05T12:00","us_aqi":null,"pm2_5":5}}"#,
                &config()
            )
            .is_err()
        );
        assert!(
            air_card(
                r#"{"current":{"time":"bad","us_aqi":-1,"pm2_5":5}}"#,
                &config()
            )
            .is_err()
        );
        assert_eq!(unconfigured_cards().len(), 2);
        assert_eq!(demo_cards(), demo_cards());
    }
}
