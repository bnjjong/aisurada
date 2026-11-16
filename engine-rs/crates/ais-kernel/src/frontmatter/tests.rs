//! Java 엔진 없이 도는 단위 시험입니다. 기대값은 Java 엔진(SnakeYAML)이 실제로 낸 결과를 `tools/yaml-spike`로 확인한 것입니다.

use super::{Value, dump_general_map, parse};

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn int(text: &str) -> Value {
    Value::Int(text.to_string())
}

/// 읽은 맵에서 문자열 키의 값을 꺼냅니다.
fn get<'a>(map: &'a [(Value, Value)], key: &str) -> &'a Value {
    &map.iter()
        .find(|(k, _)| *k == s(key))
        .unwrap_or_else(|| panic!("키가 없다: {key}"))
        .1
}

fn keys(map: &[(Value, Value)]) -> Vec<String> {
    map.iter().map(|(k, _)| format!("{k:?}")).collect()
}

#[test]
fn reads_a_standard_note_header() {
    let map = parse(
        "id: ndabc-00000001\nagenda: demo\ntype: free\ntitle: 제목\ncreated: '2026-11-14T15:59:00+09:00'\ntags:\n  - a\n  - b\nfavorite: true\nlinks:\n  - to: x\n    kind: direct\n",
    )
    .unwrap();
    assert_eq!(get(&map, "id"), &s("ndabc-00000001"));
    assert_eq!(get(&map, "favorite"), &Value::Bool(true));
    assert_eq!(get(&map, "tags"), &Value::List(vec![s("a"), s("b")]));
    assert_eq!(
        get(&map, "links"),
        &Value::List(vec![Value::Map(vec![
            (s("to"), s("x")),
            (s("kind"), s("direct"))
        ])])
    );
}

#[test]
fn follows_yaml_1_1_types_like_snakeyaml() {
    let map = parse(
        "a: yes\nb: No\nc: on\nd: y\ne: n\nf: 010\ng: 0x1F\nh: 1:30\ni: 1_000\nj: 0o17\nk: 09\n",
    )
    .unwrap();
    assert_eq!(get(&map, "a"), &Value::Bool(true));
    assert_eq!(get(&map, "b"), &Value::Bool(false));
    assert_eq!(get(&map, "c"), &Value::Bool(true));
    // y와 n은 SnakeYAML이 불리언으로 읽지 않는다
    assert_eq!(get(&map, "d"), &s("y"));
    assert_eq!(get(&map, "e"), &s("n"));
    assert_eq!(get(&map, "f"), &int("8"));
    assert_eq!(get(&map, "g"), &int("31"));
    assert_eq!(get(&map, "h"), &int("90"));
    assert_eq!(get(&map, "i"), &int("1000"));
    // 8진수 표기가 아닌 글은 문자열이다
    assert_eq!(get(&map, "j"), &s("0o17"));
    assert_eq!(get(&map, "k"), &s("09"));
}

#[test]
fn keeps_timestamps_and_quoted_scalars_as_strings() {
    let map = parse("a: 2026-11-14\nb: 2026-11-14T13:00:00Z\nc: '123'\nd: \"yes\"\n").unwrap();
    assert_eq!(get(&map, "a"), &s("2026-11-14"));
    assert_eq!(get(&map, "b"), &s("2026-11-14T13:00:00Z"));
    assert_eq!(get(&map, "c"), &s("123"));
    assert_eq!(get(&map, "d"), &s("yes"));
}

#[test]
fn null_forms() {
    let map = parse("a: ~\nb: null\nc:\nd: ''\n").unwrap();
    assert_eq!(get(&map, "a"), &Value::Null);
    assert_eq!(get(&map, "b"), &Value::Null);
    assert_eq!(get(&map, "c"), &Value::Null);
    assert_eq!(get(&map, "d"), &s(""));
}

#[test]
fn duplicate_key_moves_to_the_last_position() {
    let map = parse("a: 1\nb: 2\na: 3\n").unwrap();
    assert_eq!(map, vec![(s("b"), int("2")), (s("a"), int("3"))]);
}

#[test]
fn merge_key_keeps_the_first_source_and_lets_explicit_pairs_win() {
    let map = parse("b1: &b1 {x: 1}\nb2: &b2 {y: 2, x: 9}\nc:\n  <<: [*b1, *b2]\n  z: 3\nd:\n  <<: *b1\n  x: 7\n").unwrap();
    assert_eq!(
        get(&map, "c"),
        &Value::Map(vec![
            (s("x"), int("1")),
            (s("y"), int("2")),
            (s("z"), int("3"))
        ])
    );
    assert_eq!(get(&map, "d"), &Value::Map(vec![(s("x"), int("7"))]));
}

#[test]
fn keeps_non_string_keys() {
    let map = parse("1: a\ntrue: b\nnull: c\n").unwrap();
    assert_eq!(
        keys(&map),
        vec![
            format!("{:?}", int("1")),
            format!("{:?}", Value::Bool(true)),
            format!("{:?}", Value::Null)
        ]
    );
}

#[test]
fn treats_unreadable_headers_as_absent() {
    assert!(parse("").is_none());
    assert!(parse("# only a comment\n").is_none());
    assert!(parse("- a\n- b\n").is_none(), "최상위가 맵이 아님");
    assert!(parse("just text\n").is_none());
    assert!(parse("a: 1\n---\nb: 2\n").is_none(), "문서가 둘");
    assert!(parse("a: 'oops\n").is_none(), "문법 오류");
    assert!(parse("a: !custom x\n").is_none(), "모르는 태그");
    assert!(parse("a: *nope\n").is_none(), "정의되지 않은 별칭");
    assert!(parse("a: &x [*x]\n").is_none(), "순환");
}

#[test]
fn nesting_depth_limit_matches_the_java_engine() {
    let nested_map = |levels: usize| {
        let mut text = String::new();
        for i in 0..levels {
            text.push_str(&format!("{}k:\n", "  ".repeat(i)));
        }
        text.push_str(&format!("{}v: 1\n", "  ".repeat(levels)));
        text
    };
    assert!(parse(&nested_map(49)).is_some());
    assert!(parse(&nested_map(50)).is_none());
}

#[test]
fn alias_count_limit_matches_the_java_engine() {
    let with_aliases = |count: usize| {
        let mut text = String::from("a: &a [1]\n");
        for i in 0..count {
            text.push_str(&format!("b{i}: *a\n"));
        }
        text
    };
    assert!(parse(&with_aliases(50)).is_some());
    assert!(parse(&with_aliases(51)).is_none());
}

#[test]
fn self_merge_does_not_loop_forever() {
    let map = parse("base: &base\n  k1: v1\n  <<: *base\nchild:\n  k2: v2\n").unwrap();
    assert_eq!(get(&map, "base"), &Value::Map(vec![(s("k1"), s("v1"))]));
}

#[test]
fn dump_quotes_strings_that_would_read_back_as_other_types() {
    let map = parse(
        "a: '123'\nb: 'yes'\nc: '1:30'\nd: 'null'\ne: '2026-11-14'\nf: ' lead'\ng: 'x #y'\nh: 'a: b'\ni: plain text\nj: ''\n",
    )
    .unwrap();
    assert_eq!(
        dump_general_map(&map).unwrap(),
        "a: '123'\nb: 'yes'\nc: '1:30'\nd: 'null'\ne: '2026-11-14'\nf: ' lead'\ng: 'x #y'\nh: 'a: b'\ni: plain text\nj: ''\n"
    );
}

#[test]
fn dump_writes_blocks_and_nested_structures() {
    let map = parse(
        "a: |\n  line1\n  line2\nb: >-\n  strip\nl:\n  - x\n  - k: v\n    k2: w\ne1: []\ne2: {}\n",
    )
    .unwrap();
    assert_eq!(
        dump_general_map(&map).unwrap(),
        "a: |\n  line1\n  line2\nb: strip\nl:\n  - x\n  - k: v\n    k2: w\ne1: []\ne2: {}\n"
    );
}

#[test]
fn dump_folds_long_lines_at_the_width_limit() {
    let long = "aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll mmmm nnnn oooo pppp qqqq rrrr ssss tttt uuuu vvvv wwww";
    let map = vec![(s("t"), s(long))];
    let dumped = dump_general_map(&map).unwrap();
    assert!(dumped.starts_with("t: >-\n"), "{dumped}");
    // 접은 글을 다시 읽으면 원래 문자열이 된다
    assert_eq!(parse(&dumped).unwrap(), map);
}

#[test]
fn dump_escapes_control_characters_in_double_quotes() {
    let map = vec![
        (s("a"), s("ctrl\u{1}char")),
        (s("b"), s("tab\tinside")),
        (s("c"), s("nbsp\u{a0}here")),
    ];
    assert_eq!(
        dump_general_map(&map).unwrap(),
        "a: \"ctrl\\x01char\"\nb: \"tab\\tinside\"\nc: \"nbsp\\_here\"\n"
    );
}

#[test]
fn dump_round_trips_what_it_wrote() {
    let samples = [
        "id: a\ntitle: 한글 제목\ntags:\n  - x\n  - 'yes'\nevaluation:\n  score: 80\n  note: |\n    메모\n    둘째 줄\n",
        "m:\n  inner:\n    - a\n    - b: 1\n      c: 2\nlist:\n  - - nested\n    - 2\n",
    ];
    for sample in samples {
        let map = parse(sample).unwrap();
        let dumped = dump_general_map(&map).unwrap();
        assert_eq!(
            parse(&dumped).unwrap(),
            map,
            "다시 읽은 값이 달라졌다:\n{dumped}"
        );
        // 두 번 써도 글이 같다
        assert_eq!(dump_general_map(&parse(&dumped).unwrap()).unwrap(), dumped);
    }
}

#[test]
fn dump_refuses_values_the_header_cannot_hold() {
    assert!(
        dump_general_map(&parse("a: 1.5\n").unwrap()).is_err(),
        "실수"
    );
    assert!(
        dump_general_map(&parse("a: 12345678901234567890\n").unwrap()).is_err(),
        "i64를 넘는 정수"
    );
    assert!(
        dump_general_map(&parse("a: !!binary aGVsbG8=\n").unwrap()).is_err(),
        "명시 태그 값"
    );
    assert!(
        dump_general_map(&parse("l:\n  - ? [x]\n    : y\n").unwrap()).is_err(),
        "문자열이 아닌 안쪽 키"
    );
}

#[test]
fn dump_skips_non_string_top_level_keys() {
    let map = parse("1: a\nb: 2\n").unwrap();
    assert_eq!(dump_general_map(&map).unwrap(), "b: 2\n");
}
