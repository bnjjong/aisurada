#!/usr/bin/env bash
# 합성 vault에 git 저장소와 노트의 판 이력을 만든다(`history` 시험용). make-fixture.sh로 vault를 만든 뒤 부른다.
# 진짜 vault의 내용은 들어 있지 않다. 끝나면 작업 트리는 이 스크립트를 부르기 전과 같다(파일 내용이 되돌아온다).
#
# 사용: engine-rs/tools/compat/add-git-history.sh <make-fixture.sh로 만든 폴더>
#
# 만드는 이력(커밋 시각은 모두 고정):
#   ndfx0002-00000002 부모 노트: 수정 3번 뒤 처음 내용으로 되돌림, 옛 스냅숏 참조 둘(하나는 한 커밋과 내용이 같다)
#   ndfx0001-00000001 모든 필드 노트: 수정 한 번과 되돌림(편집 정보가 머리에 있다)
#   ndfx0003-00000003 이름이 바뀌었다 돌아온 노트(내용이 같아 판이 하나로 접힌다)
#   ndfx0040-00000040, ndfx0041-00000041 지금은 없는 노트: 커밋 메시지 꼬리 줄로만 찾는다
set -euo pipefail

ROOT="${1:?make-fixture.sh로 만든 폴더를 주세요}"
[[ -d "$ROOT/notes/demo" ]] || { echo "합성 vault가 아니다: $ROOT" >&2; exit 1; }
[[ ! -e "$ROOT/.git" ]] || { echo "이미 git 저장소다: $ROOT" >&2; exit 1; }
cd "$ROOT"

# 사용자의 git 설정과 상관없이 같은 결과가 나오게 한다
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
G() { git -c core.hooksPath=/dev/null -c commit.gpgsign=false -c core.quotePath=false "$@"; }

# commit <시각> <작성자> <메시지> [add할 경로...]: 경로가 없으면 빈 커밋. 삭제는 호출 전에 git rm으로 한다
commit() {
    local when="$1" author="$2" message="$3"; shift 3
    [[ $# -eq 0 ]] || G add -A -- "$@"
    GIT_AUTHOR_NAME="$author" GIT_COMMITTER_NAME="$author" \
    GIT_AUTHOR_EMAIL=noreply@example.invalid GIT_COMMITTER_EMAIL=noreply@example.invalid \
    GIT_AUTHOR_DATE="$when" GIT_COMMITTER_DATE="$when" \
        G commit -q --allow-empty -m "$message"
}

# snapshot <id> <시각> <파일> [부모 커밋]: 옛 스냅숏 참조에 커밋을 하나 더한다. 부모 없이 부르면 새 줄기다
snapshot() {
    local id="$1" when="$2" file="$3" parent="${4:-}"
    local blob tree args=()
    blob=$(G hash-object -w -- "$file")
    tree=$(printf '100644 blob %s\tnote.md\n' "$blob" | G mktree)
    [[ -z "$parent" ]] || args=(-p "$parent")
    GIT_AUTHOR_NAME=aisurada-auto GIT_COMMITTER_NAME=aisurada-auto \
    GIT_AUTHOR_EMAIL=noreply@aisurada.local GIT_COMMITTER_EMAIL=noreply@aisurada.local \
    GIT_AUTHOR_DATE="$when" GIT_COMMITTER_DATE="$when" \
        G commit-tree "$tree" ${args[@]+"${args[@]}"} -m "aisurada-auto: $id — notes/demo/2026-11/parent/note.md"
}

P=notes/demo/2026-11/parent/note.md
F=notes/demo/2026-11/first/note.md
L=notes/demo/loose
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cp "$P" "$TMP/p.orig"
cp "$F" "$TMP/f.orig"

G init -q -b main
commit '2026-11-01T09:00:00+09:00' aisurada-auto '초기 커밋' .

# ---- 부모 노트: 수정, 수정, 수정, 되돌림 ----
write_p() { # 머리의 추가 줄은 인자로, 본문은 표준 입력으로
    {
        echo "---"; echo "id: ndfx0002-00000002"; echo "type: free"; echo "title: 제목 ndfx0002-00000002"
        echo "created: '2026-11-05T09:00'"; echo "tags: [상위, 폴더]"
        for line in "$@"; do echo "$line"; done
        echo "---"; echo ""; cat
    } > "$P"
}
printf '# 본문 ndfx0002-00000002\n내용 한 줄.\n수정 하나 Keyword\n' \
    | write_p "editedBy: tool" "editedAt: '2026-11-02T09:00:00+09:00'" "editedSessionId: sess-1111" "editedSessionAgent: Codex"
commit '2026-11-02T09:00:00+09:00' aisurada-auto '노트 고침' "$P"
cp "$P" "$TMP/p.v2"
printf '# 본문 ndfx0002-00000002\n수정 둘 한글 검색어\n' \
    | write_p "editedBy: user" "editedAt: '2026-11-03T09:00:00+09:00'" "editedSessionId: ''" "editedSessionAgent: claude"
commit '2026-11-03T09:00:00+09:00' Jane '사람이 고침' "$P"
printf '# 본문 ndfx0002-00000002\n수정 셋 \n' | write_p "editedBy: '  '" "editedAt: '2026-11-03T20:00:00+09:00'"
commit '2026-11-03T20:00:00+09:00' aisurada-auto '또 고침' "$P"
cp "$TMP/p.orig" "$P"
commit '2026-11-04T09:00:00+09:00' aisurada-auto '처음으로 되돌림' "$P"

# 옛 스냅숏: s1은 11-02 고침(v2)과 내용이 같고, s2는 스냅숏에만 있는 판
S1=$(snapshot ndfx0002-00000002 '2026-11-02T12:00:00+09:00' "$TMP/p.v2")
printf '# 본문 ndfx0002-00000002\n스냅숏에만 있는 판\n' > "$TMP/p.s2"
S2=$(snapshot ndfx0002-00000002 '2026-11-03T12:00:00+09:00' "$TMP/p.s2" "$S1")
G update-ref refs/aisurada/snapshots/ndfx0002-00000002 "$S2"

# ---- 모든 필드 노트: 수정 한 번과 되돌림 ----
printf '\n본문 끝에 덧붙인 줄\n' >> "$F"
commit '2026-11-05T09:00:00+09:00' Jane '덧붙임' "$F"
cp "$TMP/f.orig" "$F"
commit '2026-11-06T09:00:00+09:00' aisurada-auto '되돌림' "$F"

# ---- 이름이 바뀌었다 돌아온 노트 ----
G mv "$L" "${L}2"
commit '2026-11-07T09:00:00+09:00' aisurada-auto '폴더 이름 바꿈'
G mv "${L}2" "$L"
commit '2026-11-08T09:00:00+09:00' aisurada-auto '폴더 이름 되돌림'

# ---- 지금은 없는 노트: 메시지 꼬리 줄로만 찾는다 ----
mkdir -p notes/demo/ghost notes/demo/ghost2
write_ghost() { # write_ghost <경로> <id>
    printf -- '---\nid: %s\ntype: free\ntitle: 유령 %s\ncreated: '"'"'2026-11-09T09:00:00+09:00'"'"'\n---\n\n유령 본문 %s\n' "$2" "$2" "$2" > "$1"
}
write_ghost notes/demo/ghost/note.md ndfx0040-00000040
write_ghost notes/demo/ghost2/note.md ndfx0041-00000041
commit '2026-11-09T09:00:00+09:00' aisurada-auto $'유령 노트를 더함\n\nAisurada-Notes: ndfx0040-00000040 ndfx0041-00000041' notes/demo/ghost notes/demo/ghost2
commit '2026-11-09T10:00:00+09:00' Jane 'ndfx0040-00000040 를 말로만 언급함(꼬리 줄 아님)'
G rm -q -r -- notes/demo/ghost notes/demo/ghost2
commit '2026-11-10T09:00:00+09:00' Jane '유령 노트를 지움'

# ---- main이 아닌 가지의 커밋은 목록에 오르지 않는다 ----
G switch -q -c side
printf '가지에서만 고침\n' >> "$P"
commit '2026-11-11T09:00:00+09:00' Jane '가지의 고침' "$P"
G switch -q main
G status --porcelain | grep -q . && { echo "작업 트리가 되돌아오지 않았다" >&2; G status --short >&2; exit 1; }
echo "git 이력을 더했다: $ROOT"
