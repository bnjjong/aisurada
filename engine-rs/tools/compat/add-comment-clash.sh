#!/usr/bin/env bash
# 합성 vault에 코멘트 id가 파일 사이에서 겹치는 노트를 더한다. 노트 폴더를 통째로 복사해 둔 상황을 흉내 낸다.
# 이런 vault에서 읽기 명령은 처음 겹친 노트부터 뒤의 노트를 목록에서 빼고, 쓰기 명령은 일을 하지 않고 실패한다.
#
# 사용: engine-rs/tools/compat/add-comment-clash.sh <make-fixture.sh로 만든 폴더>
set -euo pipefail

ROOT="${1:?make-fixture.sh로 만든 폴더를 주세요}"
SRC="$ROOT/notes/demo/2026-11/parent/comments.json"
[[ -f "$SRC" ]] || { echo "합성 vault가 아니다: $ROOT" >&2; exit 1; }
mkdir -p "$ROOT/notes/demo/zcopy"
cat > "$ROOT/notes/demo/zcopy/note.md" <<'EOF'
---
id: ndcc0001-0000cc01
agenda: demo
type: free
title: 복사한 노트
created: '2026-11-20T09:00:00+09:00'
---

복사본
EOF
cp "$SRC" "$ROOT/notes/demo/zcopy/comments.json"
echo "코멘트 id가 겹치는 노트를 더했다: $ROOT"
