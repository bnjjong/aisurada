//! 기준값 파일의 입력(진짜 노트의 머리와 손으로 만든 사례)을 무작위로 망가뜨려 시험 사례를 만듭니다.
//!
//! 사용: `cargo run -p ais-core --example yaml_fuzz_cases -- <ground-truth.jsonl> <건수> <시드> > fuzz-cases.txt`
//! 같은 시드는 늘 같은 사례를 냅니다. 결과는 `YamlGroundTruth --cases`에 넣어 Java 기준값을 뽑는다.

use std::{env, fs};

use serde::Deserialize;
use serde_json::Value as Json;

/// 외부 라이브러리 없이 쓰는 xorshift 난수입니다.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// YAML 문법이 민감하게 반응하는 글 조각입니다.
const TOKENS: &[&str] = &[
    "yes",
    "No",
    "on",
    "~",
    "null",
    "0x1F",
    "0o17",
    "010",
    "1:30",
    ".inf",
    ".nan",
    "1e3",
    "1_000",
    "'q'",
    "\"d\"",
    "|",
    "|-",
    ">",
    ">+",
    "&a",
    "*a",
    "!!str",
    "!!int",
    "!custom",
    "<<",
    "[a, b]",
    "{k: v}",
    "- x",
    "? k",
    ": v",
    "---",
    "...",
    "# c",
    "@",
    "`",
    "%",
    "\t",
    "  ",
    "é",
    "한글",
    "😀",
    "\\",
    "'",
    "\"",
    ": ",
    " #",
    "-",
    "?",
    "[",
    "]",
    "{",
    "}",
    ",",
    "&",
    "*",
    "!",
    "=",
    "2026-11-14",
    "2026-11-14T13:00:00Z",
    "'",
    "\"\"",
    "''",
];

fn mutate(rng: &mut Rng, seed: &str) -> String {
    let mut text: Vec<char> = seed.chars().collect();
    for _ in 0..=rng.below(3) {
        if text.is_empty() {
            text.extend(TOKENS[rng.below(TOKENS.len())].chars());
            continue;
        }
        match rng.below(10) {
            0 => {
                text.remove(rng.below(text.len()));
            }
            1 => {
                let at = rng.below(text.len() + 1);
                let token: Vec<char> = TOKENS[rng.below(TOKENS.len())].chars().collect();
                text.splice(at..at, token);
            }
            2 => replace_value(rng, &mut text),
            3..=6 => edit_lines(rng, &mut text),
            7 => text.truncate(rng.below(text.len() + 1)),
            8 => {
                let line: Vec<char> =
                    format!("k{}: {}\n", rng.below(9), TOKENS[rng.below(TOKENS.len())])
                        .chars()
                        .collect();
                text.splice(0..0, line);
            }
            _ => {
                let at = rng.below(text.len());
                text.insert(at, ['"', '\'', '\n', ' ', ':'][rng.below(5)]);
            }
        }
    }
    text.into_iter().collect()
}

/// 한 줄에서 `: ` 뒤를 글 조각으로 바꿉니다.
fn replace_value(rng: &mut Rng, text: &mut Vec<char>) {
    let source: String = text.iter().collect();
    let mut lines: Vec<String> = source.split('\n').map(str::to_string).collect();
    let i = rng.below(lines.len());
    if let Some(at) = lines[i].find(": ") {
        lines[i].truncate(at + 2);
        lines[i].push_str(TOKENS[rng.below(TOKENS.len())]);
    }
    *text = lines.join("\n").chars().collect();
}

/// 줄 단위로 복제, 삭제, 들여쓰기 바꾸기, 맞바꾸기를 합니다.
fn edit_lines(rng: &mut Rng, text: &mut Vec<char>) {
    let source: String = text.iter().collect();
    let mut lines: Vec<String> = source.split('\n').map(str::to_string).collect();
    let i = rng.below(lines.len());
    match rng.below(4) {
        0 => lines.insert(i, lines[i].clone()),
        1 => {
            lines.remove(i);
        }
        2 => {
            if rng.below(2) == 0 {
                lines[i] = format!("{}{}", " ".repeat(1 + rng.below(3)), lines[i]);
            } else {
                let strip = lines[i]
                    .chars()
                    .take_while(|c| *c == ' ')
                    .count()
                    .min(1 + rng.below(3));
                lines[i] = lines[i][strip..].to_string();
            }
        }
        _ => {
            let j = rng.below(lines.len());
            lines.swap(i, j);
        }
    }
    *text = lines.join("\n").chars().collect();
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 3 {
        eprintln!("사용법: yaml_fuzz_cases <ground-truth.jsonl> <건수> <시드>");
        std::process::exit(2);
    }
    let count: usize = args[1].parse().expect("건수는 정수");
    let seed: u64 = args[2].parse().expect("시드는 정수");
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);

    let seeds: Vec<String> = fs::read_to_string(&args[0])
        .expect("기준값 파일을 읽지 못했다")
        .lines()
        .filter_map(|line| {
            let mut de = serde_json::Deserializer::from_str(line);
            de.disable_recursion_limit();
            Json::deserialize(&mut de).ok()
        })
        .filter(|row| {
            row["parse"]["kind"] == "map"
                && !row["name"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("case:depth-")
        })
        .filter_map(|row| row["input"].as_str().map(str::to_string))
        .collect();
    assert!(!seeds.is_empty(), "씨앗이 될 입력이 없다");

    let mut written = 0;
    while written < count {
        let seed_text = &seeds[rng.below(seeds.len())];
        let mutated = mutate(&mut rng, seed_text);
        // 사례 파일의 구획 표시와 줄 끝 처리에 걸리는 글은 건너뛴다
        if mutated.contains('\r')
            || mutated.contains('\0')
            || mutated.lines().any(|l| l.starts_with("#### "))
        {
            continue;
        }
        println!("#### fuzz-{written:05}");
        println!("{}", mutated.trim_end_matches('\n'));
        written += 1;
    }
}
