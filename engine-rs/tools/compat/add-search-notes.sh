#!/usr/bin/env bash
# 합성 vault에 검색 시험용 노트를 덧붙인다. 먼저 make-fixture.sh로 vault를 만든 뒤 부른다. 진짜 vault의 내용은 들어 있지 않다.
#
# 사용: engine-rs/tools/compat/add-search-notes.sh <make-fixture.sh로 만든 폴더>
#
# 더하는 노트: 순위(제목·메타·본문·기록 층), 대소문자, 마크다운 서식 제거, 긴 문단의 맥락 글 자르기,
# 서로게이트 쌍(이모지) 뒤의 위치, 같은 시각의 동점, HTML·JSON 노트의 본문.
set -euo pipefail

ROOT="${1:?make-fixture.sh로 만든 폴더를 주세요}"
[[ -d "$ROOT/notes/demo" ]] || { echo "합성 vault가 아니다: $ROOT" >&2; exit 1; }
cd "$ROOT"

# search_note <경로> <id> <created> <제목> [추가 머리 줄...]: 본문은 표준 입력
search_note() {
    local path="$1" id="$2" created="$3" title="$4"; shift 4
    mkdir -p "$(dirname "$path")"
    {
        echo "---"
        echo "id: $id"
        echo "agenda: demo"
        echo "type: free"
        echo "title: $title"
        echo "created: '$created'"
        for line in "$@"; do echo "$line"; done
        echo "---"
        echo ""
        cat
    } > "$path"
}

D=notes/demo/search

# 순위: alpha가 제목, 메타(태그), 본문, 기록 층 노트에 각각 걸린다
echo "mkS1 본문은 평범하다" | search_note $D/rank-title.md ndsr0001-000000a1 '2026-11-08T01:00:00+09:00' 'Alpha 순위 제목' 'tags: [공통, Beta]'
echo "본문에는 없다" | search_note $D/rank-meta.md ndsr0002-000000a2 '2026-11-08T02:00:00+09:00' '순위 메타' 'tags: [Alpha, 알파]'
echo "그냥 alpha 본문에 있다" | search_note $D/rank-body.md ndsr0003-000000a3 '2026-11-08T03:00:00+09:00' '순위 본문' 'tags: [공통, beta, Beta]'
echo "기록 층 노트다" | search_note $D/rank-record.md ndsr0004-000000a4 '2026-11-08T04:00:00+09:00' 'Alpha 기록 층' 'tier: record' 'tags: [alpha]'
echo "mixedcase body" | search_note $D/mixed.md ndsr0005-000000a5 '2026-11-08T05:00:00+09:00' 'MiXeD CaSe Title' 'tags: [Search, 검색태그, 공통]'

# 마크다운 서식: 표지 낱말(mkA, mkB)로 찾으면 맥락 글에 서식을 걷어 낸 모양이 보인다
search_note $D/markup-a.md ndsr0006-000000a6 '2026-11-08T06:00:00+09:00' '서식 가' <<'MD'
mkA <!-- hidden word --> 앞글

```
codeonly line
```

중간 `inlinecode` 뒤 ![alt text](img.png) 그림과 [link label](http://example.com/a) 링크 끝
MD
search_note $D/markup-b.md ndsr0007-000000a7 '2026-11-08T07:00:00+09:00' '서식 나' <<'MD'
mkB
## Heading Text
> quoted words
- item one
* item two
1. numbered one
| col a | col b |
|-------|-------|
| cell1 | cell2 |

---

**bold** _it_ ~~del~~ __under__ 끝
MD

# 긴 문단: 찾는 낱말이 가운데라 맥락 글 양쪽에 말줄임표가 붙는다. 앞쪽에 가깝게 놓은 것은 앞 말줄임표가 없다
{
    printf 'mkC '
    for _ in $(seq 1 60); do printf '가나다라 '; done
    printf 'needleword '
    for _ in $(seq 1 60); do printf '마바사아 '; done
    echo
} | search_note $D/long.md ndsr0008-000000a8 '2026-11-08T08:00:00+09:00' '긴 문단'
echo "시작 needlehead 뒤로 마바사아 마바사아 마바사아 마바사아 마바사아 마바사아 마바사아 마바사아 마바사아 마바사아 끝" | search_note $D/head.md ndsr0009-000000a9 '2026-11-08T09:00:00+09:00' '앞쪽 낱말'

# 서로게이트 쌍: 이모지(UTF-16으로 2칸) 뒤의 위치
echo "😀😀😀 이모지 뒤 needleemoji 단어 뒤에도 글이 이어진다 😀 마지막" | search_note $D/emoji.md ndsr000a-000000aa '2026-11-08T10:00:00+09:00' '이모지 본문'

# 같은 시각(표기는 다름) 동점: 만든 시각이 같으면 id가 큰 쪽이 먼저
echo "tiedword 하나" | search_note $D/tie-a.md ndsr000b-000000ab '2026-11-09T09:00:00+09:00' '동점 가'
echo "tiedword 둘" | search_note $D/tie-b.md ndsr000c-000000ac '2026-11-09T00:00:00Z' '동점 나'
echo "tiedword 셋" | search_note $D/tie-c.md ndsr000d-000000ad '2026-11-08T19:00:00-05:00' '동점 다'

# 만든 시각 표기까지 같으면 id가 큰 쪽이 먼저(위의 tie-a와 같은 표기)
echo "tiedword 넷" | search_note $D/tie-d.md ndsr0011-000000b1 '2026-11-09T09:00:00+09:00' '동점 라'
echo "tiedword 다섯" | search_note $D/tie-e.md ndsr0012-000000b2 '2026-11-09T09:00:00+09:00' '동점 마'

# 공백·BOM
printf '회의 안건 정리\xef\xbb\xbf 끝\n' | search_note $D/meeting.md ndsr000e-000000ae '2026-11-09T10:00:00+09:00' '회의록'

# HTML과 JSON 노트
mkdir -p $D
cat > $D/html-note.html <<'HTML'
<!--aisurada
id: ndsr000f-000000af
agenda: demo
type: report
title: 검색 HTML 노트
created: '2026-11-09T11:00:00+09:00'
-->
<h1>제목</h1><p>htmlword <b>굵게</b> &amp; 끝</p><script>scriptword()</script>
HTML
cat > $D/json-note.json <<'JSON'
---
id: ndsr0010-000000b0
agenda: demo
type: data
title: 검색 JSON 노트
created: '2026-11-09T12:00:00+09:00'
---
{"jsonword": "값"}
JSON

echo "검색용 노트를 더했다: $ROOT"
