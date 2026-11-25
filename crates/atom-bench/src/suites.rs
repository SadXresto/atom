//! Benchmark suites. Each case has a deterministic, checkable answer so results
//! are reproducible rather than vibes-based.

use crate::score::Scoring;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuiteInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseDef {
    pub id: &'static str,
    pub suite: &'static str,
    pub title: &'static str,
    pub prompt: &'static str,
    pub scoring: Scoring,
    pub note: &'static str,
}

pub fn suites() -> Vec<SuiteInfo> {
    vec![
        SuiteInfo {
            id: "instruction",
            title: "Instruction following",
            description: "Can the model obey explicit output-format constraints?",
            kind: "quality",
        },
        SuiteInfo {
            id: "knowledge",
            title: "General knowledge",
            description: "Short factual questions with unambiguous answers.",
            kind: "quality",
        },
        SuiteInfo {
            id: "reasoning",
            title: "Basic reasoning",
            description: "Simple deduction and sequence completion.",
            kind: "quality",
        },
        SuiteInfo {
            id: "math",
            title: "Arithmetic",
            description: "Multi-digit arithmetic and one-step algebra.",
            kind: "quality",
        },
        SuiteInfo {
            id: "coding",
            title: "Coding",
            description: "Small code and complexity questions with string checks.",
            kind: "quality",
        },
        SuiteInfo {
            id: "consistency",
            title: "Output consistency",
            description: "The same prompt three times; measures agreement, not correctness.",
            kind: "consistency",
        },
    ]
}

pub const CONSISTENCY_PROMPT: &str = "Name one primary color. Reply with a single word.";

pub fn all_cases() -> Vec<CaseDef> {
    vec![
        // Instruction following
        CaseDef {
            id: "ins-1",
            suite: "instruction",
            title: "Exact single word",
            prompt: "Reply with exactly the word ATOM and nothing else.",
            scoring: Scoring::ExactNormalized("atom".into()),
            note: "Format compliance under a strict constraint.",
        },
        CaseDef {
            id: "ins-2",
            suite: "instruction",
            title: "JSON shape",
            prompt: "Output only a JSON object with keys \"status\" and \"count\", where status is \"ok\" and count is 3. No prose, no code fences.",
            scoring: Scoring::JsonEquals(serde_json::json!({"status": "ok", "count": 3})),
            note: "Structured output without markdown wrapping.",
        },
        CaseDef {
            id: "ins-3",
            suite: "instruction",
            title: "Fixed line count",
            prompt: "List exactly three colors, one per line, with no numbering, no bullets and no extra text.",
            scoring: Scoring::LineCount(3),
            note: "Counting constraints break easily after quantization.",
        },
        // Knowledge
        CaseDef {
            id: "kno-1",
            suite: "knowledge",
            title: "Capital city",
            prompt: "What is the capital of France? Answer in one short sentence.",
            scoring: Scoring::ContainsAll(vec!["paris".into()]),
            note: "",
        },
        CaseDef {
            id: "kno-2",
            suite: "knowledge",
            title: "Acronym expansion",
            prompt: "What does HTTP stand for? Give the full expansion.",
            scoring: Scoring::ContainsAll(vec![
                "hypertext".into(),
                "transfer".into(),
                "protocol".into(),
            ]),
            note: "",
        },
        CaseDef {
            id: "kno-3",
            suite: "knowledge",
            title: "Single-fact recall",
            prompt: "Which planet is known as the Red Planet?",
            scoring: Scoring::ContainsAll(vec!["mars".into()]),
            note: "",
        },
        // Reasoning
        CaseDef {
            id: "rea-1",
            suite: "reasoning",
            title: "Transitive deduction",
            prompt: "If all bloops are razzies and all razzies are lazzies, are all bloops lazzies? Answer yes or no, then one sentence of explanation.",
            scoring: Scoring::ContainsAll(vec!["yes".into()]),
            note: "The answer must also be correct; 'yes' is the only right word.",
        },
        CaseDef {
            id: "rea-2",
            suite: "reasoning",
            title: "Classic word problem",
            prompt: "A farmer has 17 sheep. All but 9 run away. How many sheep are left? Reply with the number only.",
            scoring: Scoring::NumericEquals { value: 9.0, tolerance: 0.0 },
            note: "",
        },
        CaseDef {
            id: "rea-3",
            suite: "reasoning",
            title: "Sequence completion",
            prompt: "What number comes next in the sequence 2, 4, 8, 16? Reply with the number only.",
            scoring: Scoring::NumericEquals { value: 32.0, tolerance: 0.0 },
            note: "",
        },
        // Math
        CaseDef {
            id: "mat-1",
            suite: "math",
            title: "Multiplication",
            prompt: "What is 47 multiplied by 19? Reply with the number only.",
            scoring: Scoring::NumericEquals { value: 893.0, tolerance: 0.0 },
            note: "",
        },
        CaseDef {
            id: "mat-2",
            suite: "math",
            title: "Linear equation",
            prompt: "Solve for x: 3x + 12 = 45. Reply with the number only.",
            scoring: Scoring::NumericEquals { value: 11.0, tolerance: 0.0 },
            note: "",
        },
        CaseDef {
            id: "mat-3",
            suite: "math",
            title: "Percentage",
            prompt: "What is 15% of 240? Reply with the number only.",
            scoring: Scoring::NumericEquals { value: 36.0, tolerance: 0.0 },
            note: "",
        },
        // Coding
        CaseDef {
            id: "cod-1",
            suite: "coding",
            title: "Function definition",
            prompt: "Write a Python function named add that returns the sum of its two arguments. Output code only.",
            scoring: Scoring::ContainsAll(vec!["def add".into(), "return".into(), "+".into()]),
            note: "",
        },
        CaseDef {
            id: "cod-2",
            suite: "coding",
            title: "Complexity",
            prompt: "What is the time complexity of binary search on a sorted array of n elements? Answer with the big-O expression.",
            scoring: Scoring::ContainsAll(vec!["log".into()]),
            note: "",
        },
        CaseDef {
            id: "cod-3",
            suite: "coding",
            title: "SQL statement",
            prompt: "Write a SQL query that selects all columns from a table named users where active is true. Output SQL only.",
            scoring: Scoring::AllOf(vec![
                Scoring::ContainsAll(vec!["select".into()]),
                Scoring::ContainsAll(vec!["from".into(), "users".into()]),
            ]),
            note: "",
        },
    ]
}
