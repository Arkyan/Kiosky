//! Conversion d'unités (longueur, masse, volume, données, durée, vitesse, surface, température).
//! Uniquement la bibliothèque standard → testable avec `cargo test`.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cat {
    Length,
    Mass,
    Volume,
    Data,
    Time,
    Speed,
    Area,
    Temp,
}

impl Cat {
    pub fn label(self) -> &'static str {
        match self {
            Cat::Length => "Longueur",
            Cat::Mass => "Masse",
            Cat::Volume => "Volume",
            Cat::Data => "Données",
            Cat::Time => "Durée",
            Cat::Speed => "Vitesse",
            Cat::Area => "Surface",
            Cat::Temp => "Température",
        }
    }

    /// Unités proposées quand on tape seulement « 10 km ».
    fn defaults(self) -> &'static [&'static str] {
        match self {
            Cat::Length => &["m", "km", "cm", "mi", "ft", "in"],
            Cat::Mass => &["kg", "g", "lb", "oz"],
            Cat::Volume => &["L", "mL", "gal", "fl oz"],
            Cat::Data => &["Ko", "Mo", "Go", "Mio", "Gio"],
            Cat::Time => &["s", "min", "h", "j", "sem."],
            Cat::Speed => &["km/h", "m/s", "mph", "nd"],
            Cat::Area => &["m²", "km²", "ha", "ft²"],
            Cat::Temp => &["°C", "°F", "K"],
        }
    }
}

pub struct Unit {
    pub names: &'static [&'static str],
    pub symbol: &'static str,
    pub cat: Cat,
    /// Facteur vers l'unité de base de la catégorie (m, kg, L, octet, s, m/s, m²).
    pub factor: f64,
}

const fn u(names: &'static [&'static str], symbol: &'static str, cat: Cat, factor: f64) -> Unit {
    Unit { names, symbol, cat, factor }
}

use Cat::*;

pub static UNITS: &[Unit] = &[
    // Longueur (m)
    u(&["mm", "millimetre", "millimètre"], "mm", Length, 0.001),
    u(&["cm", "centimetre", "centimètre"], "cm", Length, 0.01),
    u(&["dm", "decimetre", "décimètre"], "dm", Length, 0.1),
    u(&["m", "metre", "mètre", "meter"], "m", Length, 1.0),
    u(&["km", "kilometre", "kilomètre", "kilometer"], "km", Length, 1000.0),
    u(&["in", "inch", "inches", "pouce", "\""], "in", Length, 0.0254),
    u(&["ft", "foot", "feet", "pied", "'"], "ft", Length, 0.3048),
    u(&["yd", "yard"], "yd", Length, 0.9144),
    u(&["mi", "mile"], "mi", Length, 1609.344),
    u(&["nmi", "mille marin"], "nmi", Length, 1852.0),
    // Masse (kg)
    u(&["mg", "milligramme"], "mg", Mass, 1e-6),
    u(&["g", "gramme", "gram"], "g", Mass, 0.001),
    u(&["kg", "kilo", "kilogramme", "kilogram"], "kg", Mass, 1.0),
    u(&["t", "tonne", "ton"], "t", Mass, 1000.0),
    u(&["oz", "once", "ounce"], "oz", Mass, 0.028_349_523_125),
    u(&["lb", "lbs", "livre", "pound"], "lb", Mass, 0.453_592_37),
    u(&["st", "stone"], "st", Mass, 6.350_293_18),
    // Volume (L)
    u(&["ml", "millilitre"], "mL", Volume, 0.001),
    u(&["cl", "centilitre"], "cL", Volume, 0.01),
    u(&["dl", "decilitre", "décilitre"], "dL", Volume, 0.1),
    u(&["l", "litre", "liter"], "L", Volume, 1.0),
    u(&["m3", "m³"], "m³", Volume, 1000.0),
    u(&["gal", "gallon"], "gal", Volume, 3.785_411_784),
    u(&["qt", "quart"], "qt", Volume, 0.946_352_946),
    u(&["pt", "pint", "pinte"], "pt", Volume, 0.473_176_473),
    u(&["cup", "tasse"], "cup", Volume, 0.236_588_236_5),
    u(&["floz", "fl oz", "fl.oz"], "fl oz", Volume, 0.029_573_529_562_5),
    // Données (octet)
    u(&["bit", "bits"], "bit", Data, 0.125),
    u(&["o", "octet", "b", "byte"], "o", Data, 1.0),
    u(&["ko", "kb", "kilooctet"], "Ko", Data, 1e3),
    u(&["mo", "mb", "megaoctet", "mégaoctet"], "Mo", Data, 1e6),
    u(&["go", "gb", "gigaoctet"], "Go", Data, 1e9),
    u(&["to", "tb", "teraoctet", "téraoctet"], "To", Data, 1e12),
    u(&["kio", "kib"], "Kio", Data, 1024.0),
    u(&["mio", "mib"], "Mio", Data, 1_048_576.0),
    u(&["gio", "gib"], "Gio", Data, 1_073_741_824.0),
    u(&["tio", "tib"], "Tio", Data, 1_099_511_627_776.0),
    // Durée (s)
    u(&["ms", "milliseconde"], "ms", Time, 0.001),
    u(&["s", "sec", "seconde", "second"], "s", Time, 1.0),
    u(&["min", "minute"], "min", Time, 60.0),
    u(&["h", "heure", "hour", "hr"], "h", Time, 3600.0),
    u(&["j", "jour", "d", "day"], "j", Time, 86_400.0),
    u(&["sem", "semaine", "week", "wk"], "sem.", Time, 604_800.0),
    u(&["mois", "month"], "mois", Time, 2_629_746.0),
    u(&["an", "année", "annee", "year", "yr"], "an", Time, 31_556_952.0),
    // Vitesse (m/s)
    u(&["m/s", "mps"], "m/s", Speed, 1.0),
    u(&["km/h", "kmh", "kph"], "km/h", Speed, 1.0 / 3.6),
    u(&["mph"], "mph", Speed, 0.447_04),
    u(&["nd", "noeud", "nœud", "kn", "knot"], "nd", Speed, 0.514_444),
    u(&["ft/s", "fps"], "ft/s", Speed, 0.3048),
    // Surface (m²)
    u(&["cm2", "cm²"], "cm²", Area, 1e-4),
    u(&["m2", "m²"], "m²", Area, 1.0),
    u(&["km2", "km²"], "km²", Area, 1e6),
    u(&["ha", "hectare"], "ha", Area, 1e4),
    u(&["a", "are"], "a", Area, 100.0),
    u(&["ft2", "ft²", "sqft"], "ft²", Area, 0.092_903_04),
    u(&["acre"], "acre", Area, 4_046.856_422_4),
    // Température (cas particulier, facteur ignoré)
    u(&["c", "°c", "celsius", "degc"], "°C", Temp, 1.0),
    u(&["f", "°f", "fahrenheit", "degf"], "°F", Temp, 1.0),
    u(&["k", "kelvin"], "K", Temp, 1.0),
];

pub fn find(name: &str) -> Option<&'static Unit> {
    let n = name.trim().trim_end_matches('.').to_lowercase();
    let lookup = |s: &str| {
        UNITS.iter().find(|u| u.names.contains(&s) || u.symbol.to_lowercase() == s)
    };
    lookup(&n).or_else(|| {
        // Pluriels : « kilomètres », « miles », « heures »…
        n.strip_suffix('s').filter(|s| s.len() > 1).and_then(lookup)
    })
}

fn find_symbol(symbol: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.symbol == symbol)
}

pub fn convert(value: f64, from: &Unit, to: &Unit) -> Option<f64> {
    if from.cat != to.cat {
        return None;
    }
    if from.cat == Temp {
        let kelvin = match from.symbol {
            "°C" => value + 273.15,
            "°F" => (value - 32.0) * 5.0 / 9.0 + 273.15,
            _ => value,
        };
        return Some(match to.symbol {
            "°C" => kelvin - 273.15,
            "°F" => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
            _ => kelvin,
        });
    }
    Some(value * from.factor / to.factor)
}

/// Mots qui séparent la source de la cible : « 10 km en miles ».
pub const SEPARATORS: &[&str] = &["en", "to", "in", "vers", "->", "=>", "→", "=", "as"];

/// Lit un nombre au début de la chaîne (virgule ou point) et renvoie le reste.
pub fn split_number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim_start();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        let ok = c.is_ascii_digit() || ((c == '.' || c == ',') && i > 0) || (c == '-' && i == 0);
        if !ok {
            break;
        }
        end = i + c.len_utf8();
    }
    let num = s[..end].trim_end_matches(['.', ',']);
    if num.is_empty() || num == "-" {
        return None;
    }
    let value: f64 = num.replace(',', ".").parse().ok()?;
    Some((value, s[num.len()..].trim_start()))
}

/// Découpe « km en miles » en (« km », Some(« miles »)).
/// Gère les unités en deux mots (« fl oz ») et le cas ambigu « in » (pouce ou séparateur).
pub fn split_units(rest: &str) -> Option<(String, Option<String>)> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    match tokens.len() {
        0 => None,
        1 => Some((tokens[0].into(), None)),
        2 => {
            let joined = tokens.join(" ");
            if find(&joined).is_some() {
                return Some((joined, None));
            }
            Some((tokens[0].into(), Some(tokens[1].into())))
        }
        _ => {
            // Cherche un séparateur qui laisse une unité valide de chaque côté.
            for i in 1..tokens.len() - 1 {
                if SEPARATORS.contains(&tokens[i].to_lowercase().as_str()) {
                    let a = tokens[..i].join(" ");
                    let b = tokens[i + 1..].join(" ");
                    if find(&a).is_some() && find(&b).is_some() {
                        return Some((a, Some(b)));
                    }
                }
            }
            None
        }
    }
}

pub struct UnitResult {
    pub category: &'static str,
    pub value: f64,
    pub symbol: &'static str,
    pub source: String,
}

/// Point d'entrée : « 10 km en miles », « 72 °F », « 5 go en mio »…
pub fn parse_and_convert(input: &str) -> Vec<UnitResult> {
    let Some((value, rest)) = split_number(input) else { return vec![] };
    let Some((from_name, to_name)) = split_units(rest) else { return vec![] };
    let Some(from) = find(&from_name) else { return vec![] };
    let source = format!("{} {}", crate_fmt(value), from.symbol);

    match to_name {
        Some(t) => {
            let Some(to) = find(&t) else { return vec![] };
            match convert(value, from, to) {
                Some(v) => vec![UnitResult { category: from.cat.label(), value: v, symbol: to.symbol, source }],
                None => vec![],
            }
        }
        None => from
            .cat
            .defaults()
            .iter()
            .filter(|s| **s != from.symbol)
            .filter_map(|s| find_symbol(s))
            .take(5)
            .filter_map(|to| {
                convert(value, from, to).map(|v| UnitResult {
                    category: from.cat.label(),
                    value: v,
                    symbol: to.symbol,
                    source: source.clone(),
                })
            })
            .collect(),
    }
}

fn crate_fmt(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}").replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(input: &str) -> (f64, &'static str) {
        let r = parse_and_convert(input);
        assert_eq!(r.len(), 1, "résultat attendu pour « {input} »");
        (r[0].value, r[0].symbol)
    }

    #[test]
    fn conversions() {
        let (v, s) = one("10 km en miles");
        assert!((v - 6.213_711_922).abs() < 1e-6 && s == "mi");
        let (v, _) = one("100 c en f");
        assert!((v - 212.0).abs() < 1e-9);
        let (v, _) = one("2,5 kg to lb");
        assert!((v - 5.511_556_555).abs() < 1e-6);
        let (v, s) = one("5 to en go");
        assert!((v - 5000.0).abs() < 1e-9 && s == "Go");
        let (v, _) = one("12 in in cm");
        assert!((v - 30.48).abs() < 1e-9);
        let (v, _) = one("90km/h mph");
        assert!((v - 55.923_407_7).abs() < 1e-6);
        let (v, _) = one("1 fl oz en ml");
        assert!((v - 29.573_529_562_5).abs() < 1e-9);
        let (v, _) = one("3 heures en minutes");
        assert!((v - 180.0).abs() < 1e-9);
    }

    #[test]
    fn listing_and_rejects() {
        assert!(parse_and_convert("10 km").len() >= 4);
        assert!(parse_and_convert("10 km en kg").is_empty());
        assert!(parse_and_convert("bonjour").is_empty());
        assert!(parse_and_convert("14h tokyo").is_empty());
        assert!(parse_and_convert("50 eur usd").is_empty());
    }

    #[test]
    fn numbers() {
        assert_eq!(split_number("1,5kg"), Some((1.5, "kg")));
        assert_eq!(split_number("-3 c"), Some((-3.0, "c")));
        assert_eq!(split_number("abc"), None);
    }
}
