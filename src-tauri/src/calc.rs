//! Mini-calculatrice : + - * / % ^, parenthèses, fonctions et constantes.
//! Accepte la virgule décimale française (« 2,5 * 4 »).

use std::f64::consts::{E, PI};

pub fn eval(src: &str) -> Option<f64> {
    let normalized = normalize(src);
    let mut p = Parser { c: normalized.chars().collect(), i: 0 };
    let v = p.expr()?;
    p.ws();
    if p.i != p.c.len() || !v.is_finite() {
        return None;
    }
    Some(v)
}

/// Vrai si l'expression contient au moins une opération (évite d'afficher « 42 = 42 »).
pub fn looks_like_math(src: &str) -> bool {
    let s = src.trim();
    let has_op = s.chars().skip(1).any(|c| "+-*/^%×÷(".contains(c)) || s.starts_with('(');
    let has_fn = ["sqrt", "abs", "round", "floor", "ceil", "sin", "cos", "tan", "ln", "log", "pi"]
        .iter()
        .any(|f| s.to_lowercase().contains(f));
    has_op || has_fn
}

fn normalize(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    for (i, &c) in chars.iter().enumerate() {
        let prev_digit = i > 0 && chars[i - 1].is_ascii_digit();
        let next_digit = chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        match c {
            ',' if prev_digit && next_digit => out.push('.'),
            '×' => out.push('*'),
            '÷' => out.push('/'),
            '²' => out.push_str("^2"),
            '³' => out.push_str("^3"),
            _ => out.push(c),
        }
    }
    out.to_lowercase()
}

struct Parser {
    c: Vec<char>,
    i: usize,
}

impl Parser {
    fn ws(&mut self) {
        while self.c.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.ws();
        self.c.get(self.i).copied()
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.i += 1;
                    v += self.term()?;
                }
                Some('-') => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => return Some(v),
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            match self.peek() {
                Some('*') => {
                    self.i += 1;
                    v *= self.unary()?;
                }
                Some('/') => {
                    self.i += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return None;
                    }
                    v /= d;
                }
                Some('%') => {
                    self.i += 1;
                    v %= self.unary()?;
                }
                _ => return Some(v),
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some('-') => {
                self.i += 1;
                Some(-self.unary()?)
            }
            Some('+') => {
                self.i += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.atom()?;
        if self.peek() == Some('^') {
            self.i += 1;
            let exp = self.unary()?; // associatif à droite : 2^3^2 = 2^9
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn atom(&mut self) -> Option<f64> {
        let c = self.peek()?;
        if c == '(' {
            self.i += 1;
            let v = self.expr()?;
            if self.peek()? != ')' {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        if c.is_ascii_digit() || c == '.' {
            let start = self.i;
            while self.c.get(self.i).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
                self.i += 1;
            }
            let s: String = self.c[start..self.i].iter().collect();
            return s.parse().ok();
        }
        if c.is_alphabetic() {
            let start = self.i;
            while self.c.get(self.i).is_some_and(|c| c.is_alphanumeric()) {
                self.i += 1;
            }
            let name: String = self.c[start..self.i].iter().collect();
            match name.as_str() {
                "pi" => return Some(PI),
                "e" => return Some(E),
                _ => {}
            }
            let arg = self.atom()?; // sqrt(2) ou sqrt 2
            return match name.as_str() {
                "sqrt" | "racine" => Some(arg.sqrt()),
                "abs" => Some(arg.abs()),
                "round" | "arrondi" => Some(arg.round()),
                "floor" => Some(arg.floor()),
                "ceil" => Some(arg.ceil()),
                "sin" => Some(arg.sin()),
                "cos" => Some(arg.cos()),
                "tan" => Some(arg.tan()),
                "ln" => Some(arg.ln()),
                "log" => Some(arg.log10()),
                "exp" => Some(arg.exp()),
                _ => None,
            };
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        assert_eq!(eval("1 + 2 * 3"), Some(7.0));
        assert_eq!(eval("(1 + 2) * 3"), Some(9.0));
        assert_eq!(eval("2^10"), Some(1024.0));
        assert_eq!(eval("-2^2"), Some(-4.0));
        assert_eq!(eval("2,5 * 4"), Some(10.0));
        assert_eq!(eval("10 % 3"), Some(1.0));
        assert_eq!(eval("sqrt(16) + 1"), Some(5.0));
        assert_eq!(eval("3²"), Some(9.0));
        assert_eq!(eval("1 / 0"), None);
        assert_eq!(eval("2 +"), None);
        assert_eq!(eval("bonjour"), None);
    }

    #[test]
    fn detection() {
        assert!(looks_like_math("2+2"));
        assert!(!looks_like_math("42"));
        assert!(!looks_like_math("-5"));
        assert!(looks_like_math("sqrt 2"));
    }
}
