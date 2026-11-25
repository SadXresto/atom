//! Deterministic output scoring.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Scoring {
    ExactNormalized(String),
    ContainsAll(Vec<String>),
    NumericEquals { value: f64, tolerance: f64 },
    LineCount(usize),
    JsonEquals(serde_json::Value),
    AllOf(Vec<Scoring>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreOutcome {
    pub passed: bool,
    pub detail: String,
}

pub fn normalize(text: &str) -> String {
    let lowered = text.to_lowercase();
    let stripped: String = lowered
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect();
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn extract_json(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

pub fn extract_first_number(text: &str) -> Option<f64> {
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let is_number_start = c.is_ascii_digit()
            || (c == '-' && bytes.get(i + 1).map(|n| n.is_ascii_digit()).unwrap_or(false));
        if is_number_start {
            let mut token = String::new();
            token.push(c);
            let mut j = i + 1;
            while j < bytes.len() {
                let ch = bytes[j];
                if ch.is_ascii_digit() || ch == '.' || ch == ',' || ch == ' ' {
                    // Keep spaces only between digits thousands separators like "1 000".
                    if ch == ' ' {
                        if bytes.get(j + 1).map(|n| n.is_ascii_digit()).unwrap_or(false)
                            && token.chars().last().map(|p| p.is_ascii_digit()).unwrap_or(false)
                        {
                            j += 1;
                            continue;
                        }
                        break;
                    }
                    token.push(ch);
                    j += 1;
                } else {
                    break;
                }
            }
            let cleaned = token.replace([',', ' '], "");
            if let Ok(v) = cleaned.parse::<f64>() {
                return Some(v);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

pub fn score_output(scoring: &Scoring, output: &str) -> ScoreOutcome {
    match scoring {
        Scoring::ExactNormalized(expected) => {
            let actual = normalize(output);
            let want = normalize(expected);
            let passed = actual == want;
            ScoreOutcome {
                passed,
                detail: if passed {
                    "matched exactly".into()
                } else {
                    format!("expected \"{want}\", got \"{}\"", truncate(&actual, 120))
                },
            }
        }
        Scoring::ContainsAll(needles) => {
            let haystack = output.to_lowercase();
            let missing: Vec<&String> = needles.iter().filter(|n| !haystack.contains(&n.to_lowercase())).collect();
            ScoreOutcome {
                passed: missing.is_empty(),
                detail: if missing.is_empty() {
                    format!("all {} required tokens present", needles.len())
                } else {
                    format!(
                        "missing: {}",
                        missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                    )
                },
            }
        }
        Scoring::NumericEquals { value, tolerance } => match extract_first_number(output) {
            Some(found) => {
                let passed = (found - value).abs() <= tolerance.max(f64::EPSILON);
                ScoreOutcome {
                    passed,
                    detail: format!("expected {value}, first number found was {}", trim_float(found)),
                }
            }
            None => ScoreOutcome {
                passed: false,
                detail: "no number found in the output".into(),
            },
        },
        Scoring::LineCount(expected) => {
            let lines: Vec<&str> = output.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            let passed = lines.len() == *expected;
            ScoreOutcome {
                passed,
                detail: format!("expected {} non-empty lines, found {}", expected, lines.len()),
            }
        }
        Scoring::JsonEquals(expected) => match extract_json(output) {
            Some(value) => {
                let passed = json_equals(&value, expected);
                ScoreOutcome {
                    passed,
                    detail: if passed {
                        "JSON matched the required shape".into()
                    } else {
                        format!("JSON did not match: {}", truncate(&value.to_string(), 160))
                    }
                }
            }
            None => ScoreOutcome {
                passed: false,
                detail: "no JSON object found in the output".into(),
            },
        },
        Scoring::AllOf(parts) => {
            let outcomes: Vec<ScoreOutcome> = parts.iter().map(|p| score_output(p, output)).collect();
            let passed = outcomes.iter().all(|o| o.passed);
            ScoreOutcome {
                passed,
                detail: outcomes
                    .iter()
                    .map(|o| o.detail.clone())
                    .collect::<Vec<_>>()
                    .join(" | "),
            }
        }
    }
}

fn json_equals(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Number(x), serde_json::Value::Number(y)) => {
            x.as_f64().zip(y.as_f64()).map(|(m, n)| (m - n).abs() < 1e-9).unwrap_or(false)
        }
        (serde_json::Value::String(x), serde_json::Value::String(y)) => x.trim().eq_ignore_ascii_case(y.trim()),
        (serde_json::Value::Object(x), serde_json::Value::Object(y)) => {
            y.iter().all(|(k, v)| x.get(k).map(|xv| json_equals(xv, v)).unwrap_or(false))
        }
        (serde_json::Value::Array(x), serde_json::Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(m, n)| json_equals(m, n))
        }
        _ => a == b,
    }
}

fn trim_float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn truncate(text: &str, max: usize) -> String {
    let t: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        format!("{t}…")
    } else {
        t
    }
}

/// Token-set agreement between repeated answers, in `[0, 1]`.
pub fn agreement(samples: &[String]) -> f64 {
    if samples.len() < 2 {
        return 1.0;
    }
    let mut pairs = 0usize;
    let mut same = 0usize;
    for i in 0..samples.len() {
        for j in (i + 1)..samples.len() {
            pairs += 1;
            if normalize(&samples[i]) == normalize(&samples[j]) {
                same += 1;
            }
        }
    }
    if pairs == 0 {
        1.0
    } else {
        same as f64 / pairs as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_normalized_ignores_case_and_punctuation() {
        let s = Scoring::ExactNormalized("atom".into());
        assert!(score_output(&s, "  ATOM! ").passed);
        assert!(!score_output(&s, "ATOM is a project").passed);
    }

    #[test]
    fn numeric_extraction_handles_symbols_and_commas() {
        assert_eq!(extract_first_number("The answer is 1,024."), Some(1024.0));
        assert_eq!(extract_first_number("x = -12"), Some(-12.0));
        assert_eq!(extract_first_number("no digits here"), None);
        let s = Scoring::NumericEquals { value: 893.0, tolerance: 0.0 };
        assert!(score_output(&s, "893").passed);
        assert!(score_output(&s, "The product is 893.").passed);
        assert!(!score_output(&s, "892").passed);
    }

    #[test]
    fn json_scoring_is_shape_tolerant() {
        let s = Scoring::JsonEquals(serde_json::json!({"status":"ok","count":3}));
        assert!(score_output(&s, "```json\n{\"status\":\"OK\",\"count\":3}\n```").passed);
        assert!(!score_output(&s, "{\"status\":\"ok\",\"count\":4}").passed);
        assert!(!score_output(&s, "status ok count 3").passed);
    }

    #[test]
    fn line_count_ignores_blank_lines() {
        let s = Scoring::LineCount(3);
        assert!(score_output(&s, "red\n\ngreen\nblue\n").passed);
        assert!(!score_output(&s, "red\ngreen").passed);
    }

    #[test]
    fn agreement_detects_instability() {
        let samples = vec!["red".into(), "red".into(), "blue".into()];
        let score = agreement(&samples);
        assert!((score - 1.0 / 3.0).abs() < 1e-9, "{score}");
        assert_eq!(agreement(&["a".into(), "A".into()]), 1.0);
    }

    #[test]
    fn all_of_requires_every_part() {
        let s = Scoring::AllOf(vec![
            Scoring::ContainsAll(vec!["select".into()]),
            Scoring::ContainsAll(vec!["from".into()]),
        ]);
        assert!(score_output(&s, "SELECT * FROM users").passed);
        assert!(!score_output(&s, "SELECT * users").passed);
    }
}
