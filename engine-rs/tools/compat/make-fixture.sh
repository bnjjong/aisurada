#!/usr/bin/env bash
# 읽기 호환 시험용 합성 vault를 만든다. 진짜 vault의 내용은 하나도 들어 있지 않다(저장소가 공개라서).
#
# 사용: engine-rs/tools/compat/make-fixture.sh <빈 폴더>
#
# 만드는 것: 아젠다 4개와 노트 40여 개. JSON·HTML 노트, 확장자 폴더 노트, 깊은 후속 노트, 충돌 사본, 심볼릭 링크,
# 깨진 파일, 크기 초과 파일, `created` 변형, 태그·링크·평가의 경계 값, 월별 폴더 배치와 그 밖의 배치를 담는다.
set -euo pipefail

ROOT="${1:?만들 폴더를 주세요}"
if [[ -e "$ROOT" && -n "$(ls -A "$ROOT" 2>/dev/null)" ]]; then
    echo "빈 폴더가 아니다: $ROOT" >&2
    exit 1
fi
mkdir -p "$ROOT/.aisurada/agendas" "$ROOT/notes"
cd "$ROOT"

# write_file <경로>: 표준 입력을 그 경로에 쓴다(폴더는 만든다)
write_file() {
    mkdir -p "$(dirname "$1")"
    cat > "$1"
}

# ---------- 아젠다 ----------
write_file .aisurada/agendas/demo.md <<'EOF'
---
id: agfx0001-0000000a
slug: demo
title: 데모 아젠다
hue: teal
repo: example/demo
project: demo-project
projectRepo: web
---
# 데모 아젠다 소개

여러 줄 본문이다.
EOF
write_file .aisurada/agendas/work.md <<'EOF'
---
id: agfx0002-0000000b
hue: not-a-color
---

# 업무 노트

EOF
write_file .aisurada/agendas/기록.md <<'EOF'
---
id: agfx0003-0000000c
slug: 기록
title:
---
제목이 비었으면 본문 첫 줄을 쓴다
EOF
write_file .aisurada/agendas/empty-title.md <<'EOF'
---
id: agfx0004-0000000d
slug: empty-title
---
EOF
# 건너뛰는 아젠다: id 없음, 안전하지 않은 slug, 프론트매터 없음
write_file .aisurada/agendas/no-id.md <<'EOF'
---
slug: no-id
title: id 없음
---
EOF
write_file .aisurada/agendas/unsafe.md <<'EOF'
---
id: agfx0005-0000000e
slug: ../escape
---
EOF
write_file .aisurada/agendas/plain.md <<'EOF'
머리가 없는 파일
EOF

# 노트 머리를 만드는 도우미: note_front <id> <created> [추가 줄...]
note() {
    local path="$1" id="$2" created="$3"; shift 3
    {
        echo "---"
        echo "id: $id"
        echo "type: free"
        echo "title: 제목 $id"
        echo "created: '$created'"
        for line in "$@"; do echo "$line"; done
        echo "---"
        echo ""
        echo "# 본문 $id"
        echo "내용 한 줄."
    } > "$path"
}

# ---------- 월별 폴더 배치(demo) ----------
mkdir -p notes/demo/2026-11/first
write_file notes/demo/2026-11/first/note.md <<'EOF'
---
id: ndfx0001-00000001
agenda: demo
type: plan
title: 모든 필드가 있는 노트
created: '2026-11-05T16:08:00+09:00'
sessionId: 11111111-2222-3333-4444-555555555555
sessionAgent: Gemini
tier: reading
editedBy: tool
editedAt: '2026-11-05T17:00:00+09:00'
editedSessionId: aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee
editedSessionAgent: codex
tags:
  - 하나
  - ' 둘 '
  - 하나
  - ''
  - 셋
links:
  - to: ndfx0002-00000002
    kind: direct
  - to: ndfx0002-00000002
    kind: direct
  - to: ndfx0001-00000001
    kind: direct
  - to: ndfx0003-00000003
    kind: weird
  - to: not-an-id
    kind: direct
  - to: ndfx0003-00000003
    kind: attach
favorite: true
favoriteCategory: 중요
template: decision-1
evaluation:
  score: 99.6
  note: '  좋은 노트  '
  at: '2026-11-06T00:00:00Z'
---

# 모든 필드가 있는 노트

본문이다.
EOF

mkdir -p notes/demo/2026-11/parent/sub notes/demo/2026-11/parent/attach
note notes/demo/2026-11/parent/note.md ndfx0002-00000002 '2026-11-05T09:00' 'tags: [상위, 폴더]'
note notes/demo/2026-11/parent/sub/child1.md snfx0001-00000011 '2026-11-05T10:00:00+09:00'
mkdir -p notes/demo/2026-11/parent/sub/child2/sub/grand notes/demo/2026-11/parent/sub/child2/sub/deep/sub/leaf
note notes/demo/2026-11/parent/sub/child2/note.md snfx0002-00000012 '2026-11-05T11:00:00+09:00' 'links:' '  - to: ndfx0002-00000002' '    kind: direct' '  - to: ndfx0001-00000001' '    kind: indirect'
note notes/demo/2026-11/parent/sub/child2/sub/grand/note.md snfx0003-00000013 '2026-11-05T12:00:00+09:00'
note notes/demo/2026-11/parent/sub/child2/sub/ignored.md snfx0009-00000019 '2026-11-05T12:30:00+09:00'
note notes/demo/2026-11/parent/sub/child2/sub/deep/note.md snfx0004-00000014 '2026-11-05T13:00:00+09:00'
note notes/demo/2026-11/parent/sub/child2/sub/deep/sub/leaf/note.md snfx0005-00000015 '2026-11-05T14:00:00+09:00'
# 후속 노트 건너뜀: 프론트매터 없는 폴더 후속 노트(아래 후손도 함께 건너뜀), 중복 id, 잘못된 id
mkdir -p notes/demo/2026-11/parent/sub/nofront/sub/inner
echo "머리 없음" > notes/demo/2026-11/parent/sub/nofront/note.md
note notes/demo/2026-11/parent/sub/nofront/sub/inner/note.md snfx0006-00000016 '2026-11-05T15:00:00+09:00'
note notes/demo/2026-11/parent/sub/dup.md snfx0001-00000011 '2026-11-05T16:00:00+09:00'
note notes/demo/2026-11/parent/sub/badid.md ndfx0000-00000000 '2026-11-05T17:00:00+09:00'
mkdir -p notes/demo/2026-11/parent/sub/attach notes/demo/2026-11/parent/sub/sub-comments notes/demo/2026-11/parent/sub/chats
echo '{"comments":[]}' > notes/demo/2026-11/parent/comments.json
# HTML 첨부(attachText): 이름 차례로 잇는다. 스크립트와 스타일은 뺀다
write_file notes/demo/2026-11/parent/attach/b-second.html <<'EOF'
<html><head><style>p{color:red}</style></head><body><p>둘째 &amp; 문서</p><script>alert(1)</script></body></html>
EOF
write_file notes/demo/2026-11/parent/attach/a-first.htm <<'EOF'
<p>첫째   문서&nbsp;끝</p>
EOF
echo "무시되는 텍스트" > notes/demo/2026-11/parent/attach/readme.txt

# ---------- 월별 폴더가 아닌 자리(demo, work) ----------
mkdir -p notes/demo/loose
note notes/demo/loose/note.md ndfx0003-00000003 '2026-11-06T08:00:00Z' 'favorite: true' 'favoriteCategory: 보관'
write_file notes/work/single.md <<'EOF'
---
id: ndfx0004-00000004
agenda: work
created: '2026-11-06'
title: '   '
type: Plan
tier: ' reading '
---
한 파일 노트
EOF
write_file notes/work/page.html <<'EOF'
<!--aisurada
id: ndfx0005-00000005
type: report
title: HTML 노트
created: '2026-11-07T09:00:00Z'
-->
<h1>HTML 본문</h1>
EOF
mkdir -p notes/work/jsonnote
write_file notes/work/jsonnote/note.json <<'EOF'
---
id: ndfx0006-00000006
type: research
title: JSON 노트
created: '2026-11-07t09:00:00.123456-05:30'
sessionId: should-be-hidden
editedBy: tool
editedAt: '2026-11-07T10:00:00Z'
tags: [json]
---
{"cells": []}
EOF
mkdir -p notes/work/ext.md
note notes/work/ext.md/note.md ndfx0007-00000007 '0000-01-01' 'sessionId: hidden-too'
mkdir -p notes/work/rec
note notes/work/rec/note.md ndfx0008-00000008 '2026-11-08T08:00:00+09:00' 'tier: record' 'tags: [기록]'
note notes/work/odd-tier.md ndfx0009-00000009 '2026-11-08T08:00:00+09:00' 'tier: somethingelse'
mkdir -p notes/work/archive/2025/old
note notes/work/archive/2025/old/note.md ndfx000a-0000000a '2025-12-31T23:59:59-12:00' 'tags: [오래됨]'
# 깊이 상한: 폴더 10겹 아래의 노트는 건너뛴다
deep=notes/work/d1/d2/d3/d4/d5/d6/d7/d8/d9/d10
mkdir -p "$deep"
note "$deep/note.md" ndfx000b-0000000b '2026-11-09T09:00:00+09:00'
mkdir -p notes/work/d1/d2/d3/ok
note notes/work/d1/d2/d3/ok/note.md ndfx000c-0000000c '2026-11-09T10:00:00+09:00'

# 태그 50개 상한과 평가 경계
{
    echo "---"
    echo "id: ndfx000d-0000000d"
    echo "created: '2026-11-10T09:00:00+09:00'"
    echo "evaluation: {score: 150, note: 위로 넘침}"
    echo "tags:"
    for i in $(seq 1 60); do echo "  - tag$i"; done
    echo "---"
    echo "태그가 많다"
} > notes/work/many-tags.md
write_file notes/work/eval-low.md <<'EOF'
---
id: ndfx000e-0000000e
created: '2026-11-10T10:00:00+09:00'
evaluation:
  score: -3
---
EOF
write_file notes/work/eval-half.md <<'EOF'
---
id: ndfx000f-0000000f
created: '2026-11-10T11:00:00+09:00'
evaluation:
  score: 49.5
  at: 'not a date'
---
EOF
write_file notes/work/eval-string.md <<'EOF'
---
id: ndfx0010-00000010
created: '2026-11-10T12:00:00+09:00'
evaluation:
  score: '80'
---
EOF
write_file notes/work/bad-template.md <<'EOF'
---
id: ndfx0011-00000011
created: '2026-11-10T13:00:00+09:00'
template: 'has space'
favorite: yes
favoriteCategory: ''
---
EOF

# ---------- 건너뛰거나 충돌로 보는 것 ----------
echo "머리가 없는 노트" > notes/work/nofront.md
note notes/work/badid.md not-a-note-id '2026-11-11T09:00:00+09:00'
note notes/work/bad-created.md ndfx0012-00000012 '2026-02-30T09:00:00+09:00'
note notes/work/bad-created2.md ndfx0013-00000013 '2026-11-11 09:00'
note notes/work/unknown-agenda.md ndfx0014-00000014 '2026-11-11T09:00:00+09:00' 'agenda: nope'
note notes/work/dup-a.md ndfx0015-00000015 '2026-11-12T09:00:00+09:00'
note notes/work/dup-b.md ndfx0015-00000015 '2026-11-12T10:00:00+09:00'
printf '\xff\xfe\x00broken' > notes/work/not-utf8.md
head -c 1100000 /dev/zero | tr '\0' 'a' > notes/work/too-big.md
note "notes/work/single (conflicted copy).md" ndfx0016-00000016 '2026-11-12T11:00:00+09:00'
mkdir -p "notes/work/충돌 사본 폴더"
note "notes/work/충돌 사본 폴더/note.md" ndfx0017-00000017 '2026-11-12T12:00:00+09:00'
mkdir -p notes/work/attach notes/work/sub
note notes/work/attach/note.md ndfx0018-00000018 '2026-11-12T13:00:00+09:00'
ln -s ../demo/loose notes/work/linked
ln -s single.md notes/work/linked.md
echo "{}" > notes/work/comments.json
echo "{}" > notes/work/ask.json

# ---------- 한글 slug 아젠다와 이름 차례(UTF-16) ----------
mkdir -p "notes/기록/z-last" "notes/기록/😀emoji" "notes/기록/ｆullwidth"
note "notes/기록/z-last/note.md" ndfx0019-00000019 '2026-11-13T09:00:00+09:00'
note "notes/기록/😀emoji/note.md" ndfx001a-0000001a '2026-11-13T09:00:00+09:00'
note "notes/기록/ｆullwidth/note.md" ndfx001b-0000001b '2026-11-13T09:00:00+09:00'
mkdir -p notes/empty-title notes/no-agenda-file
note notes/no-agenda-file/x.md ndfx001c-0000001c '2026-11-13T10:00:00+09:00'

echo "합성 vault를 만들었다: $ROOT"
